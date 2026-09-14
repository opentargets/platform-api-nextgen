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
    target_id: String,
    pub is_essential: Option<bool>,
    pub dep_map_essentiality: Vec<DepMapEssentiality>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GeneEssentialityScreen {
    cell_line_name: Option<String>,
    depmap_id: Option<String>,
    disease_cell_line_id: Option<String>,
    disease_from_source: Option<String>,
    expression: Option<f64>,
    gene_effect: Option<f64>,
    mutation: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DepMapEssentiality {
    screens: Vec<GeneEssentialityScreen>,
    tissue_id: Option<String>,
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
