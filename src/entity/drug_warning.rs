use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    Context, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache_i64},
        load_ordered,
        paginate::{Page, Paged},
    },
};

// ---- models ----

#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct DrugWarningReference {
    id: String,
    source: String,
    url: String,
}

/// Blackbox and withdrawn information for drugs molecules included in ChEMBL database.
#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
pub struct DrugWarning {
    /// Classification of toxicity type associated with the drug.
    toxicity_class: Option<String>,
    /// List of Open Targets molecule identifiers.
    chembl_ids: Vec<String>,
    /// Country where the warning was issued.
    country: Option<String>,
    /// Description of the drug adverse effect.
    description: Option<String>,
    /// Internal identifier for the drug warning record.
    id: Option<i64>,
    /// List of sources supporting the warning information.
    references: Vec<DrugWarningReference>,
    /// Classification of action taken (drug is withdrawn or has a black box warning).
    warning_type: String,
    /// Year when the warning was issued.
    year: Option<i32>,
    /// List of disease labels.
    efo_term: Option<String>,
    /// List of disease identifiers.
    efo_id: Option<String>,
    /// Disease identifier categorising the type of warning.
    efo_id_for_warning_class: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DrugWarnings {
    chembl_id: String,
    drug_warnings: Vec<DrugWarning>,
}

// ---- loaders ----

pub type DrugWarningCache = Cache<i64, Option<DrugWarning>>;
static DRUG_WARNING_CACHE: LazyLock<DrugWarningCache> = LazyLock::new(entity_cache_i64);

pub struct DrugWarningLoader {
    ch: ClickHouse,
}

impl DrugWarningLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for DrugWarningLoader {
    type Key = i64;
    type Value = DrugWarning;

    fn cache(&self) -> &DrugWarningCache { &DRUG_WARNING_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM drug_warnings WHERE id IN ?")
            .bind(misses)
            .fetch_all::<DrugWarning>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<i64> for DrugWarningLoader {
    type Value = DrugWarning;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[i64]) -> Result<HashMap<i64, DrugWarning>, Self::Error> {
        self.load_cached(keys).await
    }
}

/// Loads DrugWarnings by their IDs from the cache or database.
///
/// # Returns
/// A `Vec` of `DrugWarning` objects corresponding to the given IDs.
/// # Errors
/// Returns an error if the DrugWarnings could not be loaded.
pub async fn load_drug_warnings(
    ctx: &Context<'_>,
    ids: &[i64],
) -> async_graphql::Result<Vec<DrugWarning>> {
    load_ordered(ctx.data_unchecked::<DataLoader<DrugWarningLoader>>(), ids).await
}
