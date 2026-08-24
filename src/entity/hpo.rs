use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        search::Searchable,
        sort::{Sort, SortKey},
    },
};

// ---- models ----

/// Human Phenotype Ontology subset of information included in the Platform.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Hpo {
    /// Unique identifier for the disease in the Human Phenotype Ontology (HPO).
    id: String,
    /// Name of the disease (in HPO).
    name: String,
    /// Description of the disease.
    description: Option<String>,
    // DEPRECATED - This is always empty in the data.
    #[graphql(deprecation = "empty")]
    namespace: Vec<String>,
}

// ---- query utilities ----

impl Entity for Hpo {
    fn id(&self) -> &str { &self.id }
}

/// Contains the fields available for sorting hpos.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
pub enum HpoSortField {
    Id,
    Name,
}

impl SortKey<Hpo> for HpoSortField {
    fn compare(&self, a: &Hpo, b: &Hpo) -> Ordering {
        match self {
            Self::Id => a.id.cmp(&b.id),
            Self::Name => a.name.cmp(&b.name),
        }
    }
}

impl Searchable for Hpo {
    fn matches_search(&self, needle: &str) -> bool {
        self.id.to_lowercase().contains(needle)
            || self.name.to_lowercase().contains(needle)
            || self
                .description
                .as_deref()
                .is_some_and(|d| d.to_lowercase().contains(needle))
    }
}

// ---- loaders ----

pub type HpoCache = Cache<String, Option<Hpo>>;
static HPO_CACHE: LazyLock<HpoCache> = LazyLock::new(entity_cache);

pub struct HpoLoader {
    ch: ClickHouse,
}

impl HpoLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for HpoLoader {
    type Key = String;
    type Value = Hpo;

    fn cache(&self) -> &HpoCache { &HPO_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM hpo WHERE id IN ?")
            .bind(misses)
            .fetch_all::<Hpo>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for HpoLoader {
    type Value = Hpo;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, Hpo>, Self::Error> {
        self.load_cached(keys).await
    }
}

/// Loads HPOs by their IDs from the cache or database.
///
/// # Returns
/// A `Vec` of `Hpo` objects corresponding to the given IDs.
/// # Errors
/// Returns an error if the HPOs could not be loaded.
pub async fn load_hpos(ctx: &Context<'_>, ids: &[String]) -> async_graphql::Result<Vec<Hpo>> {
    load_ordered(ctx.data_unchecked::<DataLoader<HpoLoader>>(), ids).await
}

// ---- resolvers ----

#[derive(Default)]
pub struct HpoQuery;

#[Object]
impl HpoQuery {
    /// Fetch HPOs by HPO ID.
    async fn hpos(
        &self,
        ctx: &Context<'_>,
        ids: Vec<String>,
        search: Option<String>,
        sort: Option<Sort<HpoSortField>>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Hpo>> {
        let items = load_hpos(ctx, &ids).await?;
        Ok(items
            .query()
            .search(search.as_deref())
            .sort(sort.as_ref())
            .paginate(page))
    }
}
