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

/// A target prioritisation factor.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TargetPrioritisation {
    /// Key for the area of prioritisation.
    key: String,
    /// Score for the prioritisation factor.
    value: String,
}

/// Target-specific properties used to prioritise targets for further investigation. Prioritisation
/// factors cover several areas around clinical precedence, tractability, do-ability, and safety of
/// the target. Values range from -1 (unfavourable/deprioritised) to 1 (favourable/prioritised).
#[derive(Debug, Clone, Deserialize, SimpleObject, Row, Default)]
#[serde(rename_all = "camelCase")]
pub struct TargetPrioritisations {
    #[graphql(skip)]
    target_id: String,
    items: Vec<TargetPrioritisation>,
}

// ---- loaders ----

pub type TargetPrioritisationsCache = Cache<String, Option<TargetPrioritisations>>;
static TARGET_PRIORITISATIONS_CACHE: LazyLock<TargetPrioritisationsCache> =
    LazyLock::new(entity_cache);

pub struct TargetPrioritisationsLoader {
    ch: ClickHouse,
}

impl TargetPrioritisationsLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for TargetPrioritisationsLoader {
    type Key = String;
    type Value = TargetPrioritisations;

    fn cache(&self) -> &TargetPrioritisationsCache { &TARGET_PRIORITISATIONS_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.target_id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM target_prioritisation WHERE targetId IN ?")
            .bind(misses)
            .fetch_all::<TargetPrioritisations>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for TargetPrioritisationsLoader {
    type Value = TargetPrioritisations;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[String],
    ) -> Result<HashMap<String, TargetPrioritisations>, Self::Error> {
        self.load_cached(keys).await
    }
}

/// Loads the target prioritisations for a target by its Ensembl ID.
///
/// # Returns
/// The target prioritisations struct, or None.
/// # Errors
/// Returns an error if the target prioritisation could not be loaded.
pub async fn load_target_prioritisations(
    ctx: &Context<'_>,
    ensembl_id: &str,
) -> async_graphql::Result<TargetPrioritisations> {
    Ok(ctx
        .data_unchecked::<DataLoader<TargetPrioritisationsLoader>>()
        .load_one(ensembl_id.to_owned())
        .await?
        .unwrap_or_default())
}
