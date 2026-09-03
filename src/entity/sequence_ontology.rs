use std::{char::CharTryFromError, cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;
use serde_repr::Deserialize_repr;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        // search::Searchable,
        // sort::{Sort, SortKey},
    },
};

// ---- models ----
///Sequence ontology term identifier and name.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject, Default)]
#[serde(rename_all = "camelCase")]
pub struct SequenceOntologyTerm {
    /// Sequence ontology term label (e.g. 'missense_variant').
    label: String,
    /// Sequence ontology term identifier [bioregistry:so].
    id: String
}

// ---- loaders ----

pub type SequenceOntologyTermCache = Cache<String, Option<SequenceOntologyTerm>>;
static SO_CACHE: LazyLock<SequenceOntologyTermCache> = LazyLock::new(entity_cache);

pub struct SequenceOntologyTermLoader {
    ch: ClickHouse,
}

impl SequenceOntologyTermLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
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
    ) -> Result<HashMap<String, SequenceOntologyTerm>, async_graphql::Error> {
        self.load_cached(keys).await
    }
}
