use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::cache::{CachedLoader, entity_cache},
};

// ---- models ----

#[derive(Debug, Clone, Deserialize, SimpleObject)]
pub struct DrugWarningReference {
    id: String,
    source: String,
    url: String,
}

/// Blackbox and withdrawn information for drugs molecules included in ChEMBL database.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
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
    id: Option<u32>,
    /// List of sources supporting the warning information.
    references: Vec<DrugWarningReference>,
    /// Classification of action taken (drug is withdrawn or has a black box warning).
    warning_type: String,
    /// Year when the warning was issued.
    year: Option<u16>,
    /// List of disease labels.
    efo_term: Option<String>,
    /// List of disease identifiers.
    efo_id: Option<String>,
    /// Disease identifier categorising the type of warning.
    efo_id_for_warning_class: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
pub struct DrugWarnings {
    chembl_id: String,
    drug_warnings: Vec<DrugWarning>,
}

// ---- loaders ----

pub type DrugWarningCache = Cache<String, Option<DrugWarnings>>;
static DRUG_WARNING_CACHE: LazyLock<DrugWarningCache> = LazyLock::new(entity_cache);

pub struct DrugWarningLoader {
    ch: ClickHouse,
}

impl DrugWarningLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for DrugWarningLoader {
    type Key = String;
    type Value = DrugWarnings;

    fn cache(&self) -> &DrugWarningCache { &DRUG_WARNING_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.chembl_id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM drug_warnings WHERE chemblId IN ?")
            .bind(misses)
            .fetch_all::<DrugWarnings>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for DrugWarningLoader {
    type Value = DrugWarnings;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, DrugWarnings>, Self::Error> {
        self.load_cached(keys).await
    }
}

/// Loads the warnings recorded for a single drug by its ChEMBL id.
///
/// # Returns
/// The drug's warnings, or an empty `Vec` if it has none.
/// # Errors
/// Returns an error if the warnings could not be loaded.
pub async fn load_drug_warnings(
    ctx: &Context<'_>,
    chembl_id: &str,
) -> async_graphql::Result<Vec<DrugWarning>> {
    Ok(ctx
        .data_unchecked::<DataLoader<DrugWarningLoader>>()
        .load_one(chembl_id.to_owned())
        .await?
        .map(|w| w.drug_warnings)
        .unwrap_or_default())
}
