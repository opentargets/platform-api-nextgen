use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    Context, Enum, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        search::Searchable,
        sort::SortKey,
    },
};

// ---- models ----

/// A term from the Gene Ontology (GO) TODO DESCRIPTION
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GeneOntology {
    /// Gene Ontology term identifier [bioregistry:so].
    id: String,
    /// Human-readable label for the term (e.g. `missense_variant`).
    label: String,
}

// ---- query utilities ----

impl Entity for GeneOntology {
    fn id(&self) -> &str { &self.id }
}

/// Contains the fields available for sorting gene ontology terms.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
pub enum GeneOntologySortField {
    Id,
    Label,
}

impl SortKey<GeneOntology> for GeneOntologySortField {
    fn compare(&self, a: &GeneOntology, b: &GeneOntology) -> Ordering {
        match self {
            Self::Id => a.id.cmp(&b.id),
            Self::Label => a.label.cmp(&b.label),
        }
    }
}

impl Searchable for GeneOntology {
    fn matches_search(&self, needle: &str) -> bool {
        self.id.to_lowercase().contains(needle) || self.label.to_lowercase().contains(needle)
    }
}

// ---- loaders ----

pub type GeneOntologyCache = Cache<String, Option<GeneOntology>>;
static SO_CACHE: LazyLock<GeneOntologyCache> = LazyLock::new(entity_cache);

pub struct GeneOntologyLoader {
    ch: ClickHouse,
}

impl GeneOntologyLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for GeneOntologyLoader {
    type Key = String;
    type Value = GeneOntology;

    fn cache(&self) -> &GeneOntologyCache { &SO_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM gene_ontology WHERE id IN ?")
            .bind(misses)
            .fetch_all::<GeneOntology>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for GeneOntologyLoader {
    type Value = GeneOntology;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, GeneOntology>, Self::Error> {
        self.load_cached(keys).await
    }
}

/// Loads gene ontology terms by their IDs from the cache or database.
///
/// Replaces `_` with `:` in the IDs to match the gene ontology format.
///
/// # Returns
/// A `Vec` of `GeneOntology` objects corresponding to the given IDs.
/// # Errors
/// Returns an error if the terms could not be loaded.
pub async fn load_gene_ontology_many(
    ctx: &Context<'_>,
    ids: &[String],
) -> async_graphql::Result<Vec<GeneOntology>> {
    let ids: Vec<String> = ids.iter().map(|id| id.replacen('_', ":", 1)).collect();
    load_ordered(ctx.data_unchecked::<DataLoader<GeneOntologyLoader>>(), &ids).await
}

/// Loads a single gene ontology term by its ID.
///
/// Replaces `_` with `:` in the ID to match the gene ontology format.
///
/// # Errors
/// Returns an error if the term could not be loaded.
pub async fn load_gene_ontology_one(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<GeneOntology>> {
    ctx.data_unchecked::<DataLoader<GeneOntologyLoader>>()
        .load_one(id.replacen('_', ":", 1))
        .await
}
