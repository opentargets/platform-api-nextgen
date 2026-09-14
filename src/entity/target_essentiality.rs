use std::collections::HashMap;

use async_graphql::{
    Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use serde::Deserialize;

use crate::datasource::clickhouse::ClickHouse;

// ---- models ----

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TargetEssentiality {
    /// Target identifier the essentiality was measured on [bioregistry:ensembl].
    target_id: String,
    /// Boolean flag indicating whether the tested gene is considered essential in the given
    /// context.
    pub is_essential: Option<bool>,
    /// Essentiality measurements extraced from DepMap, stratified by tissue or anatomical units.
    pub dep_map_essentiality: Vec<DepMapEssentiality>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GeneEssentialityScreen {
    /// Name of the cancer cell line in which the gene essentiality was assessed.
    cell_line_name: Option<String>,
    /// Unique identifier of the assay in DepMap.
    depmap_id: Option<String>,
    /// Cell model passport identifier of a cell line modelling a disease.
    disease_cell_line_id: Option<String>,
    /// Disease associated with the cell line as reported in the source data.
    disease_from_source: Option<String>,
    /// Gene expression level in the corresponding cell line.
    expression: Option<f64>,
    /// Gene effect score indicating the impact of gene knockout.
    gene_effect: Option<f64>,
    /// Background mutation in the tested cell line.
    mutation: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DepMapEssentiality {
    /// List of CRISPR screening experiments supporting the essentiality assessment.
    screens: Vec<GeneEssentialityScreen>,
    /// Identifier of the tissue from where the cells were sampled for assay [bioregistry:uberon].
    tissue_id: Option<String>,
    /// Name of the tissue from where the cells were sampled for assay.
    tissue_name: Option<String>,
}

// ---- loaders ----

pub struct TargetEssentialityLoader {
    ch: ClickHouse,
}

impl TargetEssentialityLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<String> for TargetEssentialityLoader {
    type Value = TargetEssentiality;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        println!("start load");
        let rows: Vec<TargetEssentiality> = self
            .ch
            .query("SELECT ?fields FROM target_essentiality WHERE targetId IN ?")
            .bind(key)
            .clone()
            .fetch_all()
            .await?;

        println!("full query: {}", rows.len());

        Ok(rows.into_iter().fold(HashMap::new(), |mut acc, row| {
            acc.insert(row.target_id.clone(), row);
            acc
        }))
    }
}

/// Loads Target Essentiality by the target id from the cache or database.
///
/// # Returns
/// A `Vec` of `TargetEssentiality` objects corresponding to the given target IDs.
/// # Errors
/// Returns an error if the Target Essentiality could not be loaded.
pub async fn load_target_essentiality_by_target(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<TargetEssentiality>> {
    ctx.data_unchecked::<DataLoader<TargetEssentialityLoader>>()
        .load_one(id)
        .await
}
