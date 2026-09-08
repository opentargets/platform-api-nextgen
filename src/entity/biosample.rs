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

/// Integration of biosample metadata about tissues or cell types derived from multiple ontologies including EFO, UBERON, CL, GO and others.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Biosample {
    /// Unique identifier for the biosample.
    biosample_id: String,
    /// Name of the biosample.
    biosample_name: String,
    /// Description of the biosample.
    description: Option<String>,
    /// Cross-reference IDs from other ontologies.
    xrefs: Vec<String>,
    /// List of synonymous names for the term.
    synonyms: Vec<String>,
    /// Direct parent biosample IDs in the ontology.
    parents: Vec<String>,
    /// List of ancestor biosample IDs in the ontology.
    ancestors: Vec<String>,
    /// Direct child biosample IDs in the ontology.
    children: Vec<String>,
    /// List of descendant biosample IDs in the ontology.
    descendants: Vec<String>,
}

// ---- loaders ----
pub struct BiosampleLoader {
    ch: ClickHouse,
}

impl BiosampleLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<String> for BiosampleLoader {
    type Value = Vec<Biosample>;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<Biosample> = self
            .ch
            .query(
                "SELECT ?fields \
                 FROM platform2606.biosample \
                 WHERE biosampleId IN ?",
            )
            .bind(key)
            .fetch_all()
            .await?;
        Ok(rows.into_iter().fold(
            key.iter().cloned().map(|k| (k, Vec::new())).collect(),
            |mut acc: HashMap<String, Vec<Biosample>>, row| {
                acc.entry(row.biosample_id.clone()).or_default().push(row);
                acc
            },
        ))
    }
}

/// Loads Biosample by the target id from the cache or database.
///
/// # Returns
/// A `Vec` of `Biosample` objects corresponding to the given target IDs.
/// # Errors
/// Returns an error if the Biosample could not be loaded.
pub async fn load_biosample_by_id(
    ctx: &Context<'_>,
    id: &String,
    page: Page,
) -> async_graphql::Result<Paged<Biosample>> {
    let items = ctx
        .data_unchecked::<DataLoader<BiosampleLoader>>()
        .load_one(id.clone())
        .await?
        .unwrap_or_default();
    Ok(items.query().paginate(page))
}
