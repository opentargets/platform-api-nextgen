use std::collections::HashMap;

use async_graphql::{
    Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        QueryExt,
        paginate::{Page, Paged},
    },
};

// ---- models ----

/// Container for all biological model-related attributes.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct BiologicalModels {
    /// The specific allelic composition of the mouse model.
    allelic_composition: String,
    /// The genetic background strain of the mouse model.
    genetic_background: String,
    /// Unique identifier for the biological model [bioregistry:mgi].
    id: Option<String>,
    /// References related to the mouse model [bioregistry:pubmed].
    literature: Vec<String>,
}

/// Container for phenotype class-related attributes.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ModelPhenotypeClasses {
    /// Unique identifier for the phenotype class [bioregistry:mp].
    id: String,
    /// Descriptive label for the phenotype class.
    label: String,
}

/// Mouse phenotype information linking human targets to observed phenotypes in mouse models.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct MousePhenotype {
    /// Container for all biological model-related attributes.
    biological_models: Vec<BiologicalModels>,
    /// Container for phenotype class-related attributes.
    model_phenotype_classes: Vec<ModelPhenotypeClasses>,
    /// Identifier for the specific phenotype observed in the model [bioregistry:mp].
    model_phenotype_id: String,
    /// Human-readable label describing the observed phenotype.
    model_phenotype_label: String,
    #[graphql(skip)]
    /// Identifier for the human target as provided by the data source.
    target_from_source_id: String,
    /// Name of the target gene as represented in the mouse model.
    target_in_model: String,
    /// Ensembl identifier for the target gene in the mouse model.
    target_in_model_ensembl_id: Option<String>,
    /// MGI identifier for the target gene in the mouse model [bioregistry:mgi].
    target_in_model_mgi_id: String,
}

/// One row of the `mouse_phenotypes` table; wraps the `Array(Tuple(...))` column so that
/// `clickhouse-rs` decodes the row correctly with `with_validation(false)`. Each row
/// contains all phenotypes observed across the biological models for a given target.
#[derive(Debug, Row, Deserialize)]
struct MousePhenotypeRow {
    mouse_phenotypes: Vec<MousePhenotype>,
    #[serde(rename = "targetFromSourceId")]
    /// Identifier for the human target as provided by the data source.
    target_from_source_id: String,
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
            .map(|r| (r.target_from_source_id, r.mouse_phenotypes))
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
    id: String,
    page: Page,
) -> async_graphql::Result<Paged<MousePhenotype>> {
    let items = ctx
        .data_unchecked::<DataLoader<MousePhenotypeLoader>>()
        .load_one(id)
        .await?
        .unwrap_or_default();
    Ok(items.query().paginate(page))
}
