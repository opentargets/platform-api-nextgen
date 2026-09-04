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
    entity::hpo::HpoCache,
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
    },
};

// ---- models ----

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct BiologicalModels {
    allelic_composition: String,
    genetic_background: String,
    id: Option<String>,
    literature: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ModelPhenotypeClasses {
    id: String,
    label: String,
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct MousePhenotype {
    biological_models: Vec<BiologicalModels>,
    model_phenotype_classes: Vec<ModelPhenotypeClasses>,
    model_phenotype_id: String,
    model_phenotype_label: String,
    target_from_source_id: String,
    target_in_model: String,
    target_in_model_ensembl_id: Option<String>,
    target_in_model_mgi_id: String,
}

/// One row of the `mouse_phenotypes` table; wraps the `Array(Tuple(...))` column so that
/// `clickhouse-rs` decodes the row correctly with `with_validation(false)`.
#[derive(Debug, Row, Deserialize)]
struct MousePhenotypeRow {
    mouse_phenotypes: Vec<MousePhenotype>,
    targetFromSourceId: String,
}

// ---- loaders ----
pub struct MousePhenotypeLoader {
    ch: ClickHouse,
}

impl MousePhenotypeLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<String> for MousePhenotypeLoader {
    type Value = Vec<MousePhenotype>;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<MousePhenotypeRow> = self
            .ch
            .query("SELECT ?fields FROM mouse_phenotypes WHERE targetFromSourceId IN ?")
            .bind(key)
            .fetch_all()
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.targetFromSourceId, r.mouse_phenotypes))
            .collect())
    }
}

/// Loads Mouse Phenotypes by the target id from the cache or database.
///
/// # Returns
/// A `Vec` of `MousePhenotype` objects corresponding to the given target IDs.
/// # Errors
/// Returns an error if the Mouse Phenotypes could not be loaded.
pub async fn load_mouse_phenotype_by_target(
    ctx: &Context<'_>,
    id: &String,
    page: Page,
) -> async_graphql::Result<Paged<MousePhenotype>> {
    let items = ctx
        .data_unchecked::<DataLoader<MousePhenotypeLoader>>()
        .load_one(id.clone())
        .await?
        .unwrap_or_default();
    Ok(items.query().paginate(page))
}
