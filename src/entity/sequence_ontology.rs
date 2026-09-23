use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    Context, Enum, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
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

/// A term from the Sequence Ontology (SO), a controlled vocabulary of sequence features and variant
/// consequences.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct SequenceOntologyTerm {
    /// Sequence Ontology term identifier [bioregistry:so].
    id: String,
    /// Human-readable label for the term (e.g. `missense_variant`).
    label: String,
}

// ---- query utilities ----

impl Entity for SequenceOntologyTerm {
    fn id(&self) -> &str { &self.id }
}

/// Contains the fields available for sorting sequence ontology terms.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
pub enum SequenceOntologyTermSortField {
    Id,
    Label,
}

impl SortKey<SequenceOntologyTerm> for SequenceOntologyTermSortField {
    fn compare(&self, a: &SequenceOntologyTerm, b: &SequenceOntologyTerm) -> Ordering {
        match self {
            Self::Id => a.id.cmp(&b.id),
            Self::Label => a.label.cmp(&b.label),
        }
    }
}

impl Searchable for SequenceOntologyTerm {
    fn matches_search(&self, needle: &str) -> bool {
        self.id.to_lowercase().contains(needle) || self.label.to_lowercase().contains(needle)
    }
}

// ---- loaders ----

pub type SequenceOntologyTermCache = Cache<String, Option<SequenceOntologyTerm>>;
static SO_CACHE: LazyLock<SequenceOntologyTermCache> = LazyLock::new(entity_cache);

#[derive(From)]
pub struct SequenceOntologyTermLoader {
    ch: ClickHouse,
}

impl CachedLoader for SequenceOntologyTermLoader {
    type Key = String;
    type Value = SequenceOntologyTerm;

    fn cache(&self) -> &SequenceOntologyTermCache { &SO_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM sequence_ontology WHERE id IN ?")
            .bind(misses)
            .fetch_all::<SequenceOntologyTerm>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for SequenceOntologyTermLoader {
    type Value = SequenceOntologyTerm;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[String],
    ) -> Result<HashMap<String, SequenceOntologyTerm>, Self::Error> {
        self.load_cached(keys).await
    }
}

/// Loads sequence ontology terms by their IDs from the cache or database.
///
/// Replaces `_` with `:` in the IDs to match the sequence ontology format.
///
/// # Returns
/// A `Vec` of `SequenceOntologyTerm` objects corresponding to the given IDs.
/// # Errors
/// Returns an error if the terms could not be loaded.
pub async fn load_sequence_ontology_many(
    ctx: &Context<'_>,
    ids: &[String],
) -> async_graphql::Result<Vec<SequenceOntologyTerm>> {
    let ids: Vec<String> = ids.iter().map(|id| id.replacen('_', ":", 1)).collect();
    load_ordered(
        ctx.data_unchecked::<DataLoader<SequenceOntologyTermLoader>>(),
        &ids,
    )
    .await
}

/// Loads a single sequence ontology term by its ID.
///
/// Replaces `_` with `:` in the ID to match the sequence ontology format.
///
/// # Errors
/// Returns an error if the term could not be loaded.
pub async fn load_sequence_ontology_one(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<SequenceOntologyTerm>> {
    ctx.data_unchecked::<DataLoader<SequenceOntologyTermLoader>>()
        .load_one(id.replacen('_', ":", 1))
        .await
}
