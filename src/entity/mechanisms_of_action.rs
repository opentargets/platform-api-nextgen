use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::target::{Target, load_targets},
    query::cache::{CachedLoader, entity_cache},
};

// ---- models ----

/// Reference information supporting the drug mechanisms of action.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    /// List of reference identifiers.
    ids: Vec<String>,
    /// Source of the reference (e.g., PubMed, FDA, package inserts).
    source: String,
    /// List of URLs linking to the reference.
    urls: Vec<String>,
}

/// Collection of mechanisms of action for a drug molecule.
#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
pub struct MechanismsOfAction {
    #[graphql(skip)]
    chembl_id: String,
    /// List of mechanism of action entries.
    rows: Vec<MechanismOfActionRow>,
    ///Unique list of action types across all mechanisms.
    unique_action_types: Vec<String>,
    ///Unique list of target types across all mechanisms.
    unique_target_types: Vec<String>,
}

/// Mechanism of action information for a drug.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct MechanismOfActionRow {
    /// Description of the mechanism of action.
    mechanism_of_action: String,
    /// Classification of how the drug interacts with its target (e.g., ACTIVATOR, INHIBITOR).
    action_type: Option<String>,
    /// Name of the target molecule.
    target_name: Option<String>,
    /// List of on-target (genes or proteins) involved in the drug or clinical candidate mechanism
    /// of action.
    #[graphql(skip)]
    targets: Vec<String>,
    /// Reference information supporting the mechanism of action.
    references: Vec<Reference>,
}

#[ComplexObject]
impl MechanismOfActionRow {
    /// List of on-target (genes or proteins) involved in the drug or clinical candidate
    /// mechanism of action.
    async fn targets(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Target>> {
        load_targets(ctx, &self.targets).await
    }
}

// ---- loaders ----

pub type MechanismsOfActionCache = Cache<String, Option<MechanismsOfAction>>;
static MECHANISMS_OF_ACTION_CACHE: LazyLock<MechanismsOfActionCache> = LazyLock::new(entity_cache);

#[derive(From)]
pub struct MechanismsOfActionLoader {
    ch: ClickHouse,
}

impl CachedLoader for MechanismsOfActionLoader {
    type Key = String;
    type Value = MechanismsOfAction;

    fn cache(&self) -> &MechanismsOfActionCache { &MECHANISMS_OF_ACTION_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.chembl_id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM mechanism_of_action WHERE chemblId IN ?")
            .bind(misses)
            .fetch_all::<MechanismsOfAction>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for MechanismsOfActionLoader {
    type Value = MechanismsOfAction;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[String],
    ) -> Result<HashMap<String, MechanismsOfAction>, Self::Error> {
        self.load_cached(keys).await
    }
}

/// Loads the mechanisms of action recorded for a single drug by its ChEMBL id.
///
/// # Returns
/// The mechanisms of action, or None if the drug has none.
/// # Errors
/// Returns an error if the mechanisms of action could not be loaded.
pub async fn load_mechanisms_of_action(
    ctx: &Context<'_>,
    chembl_id: String,
) -> async_graphql::Result<Option<MechanismsOfAction>> {
    ctx.data_unchecked::<DataLoader<MechanismsOfActionLoader>>()
        .load_one(chembl_id)
        .await
}
