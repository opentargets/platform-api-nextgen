use std::{cmp::Ordering, collections::HashMap};

use async_graphql::{Context, Enum, Object, SimpleObject, dataloader::Loader};
use clickhouse::Row;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, QueryExt,
        paginate::{Page, Paged},
        search::Searchable,
        sort::{Sort, SortKey},
    },
};

// ---- model ----

/// Human Phenotype Ontology subset of information included in the Platform.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Hpo {
    id: String,
    name: String,
    description: Option<String>,
    namespace: Vec<String>,
}

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

// ---- dataloaders ----

pub struct HpoLoader {
    pub ch: ClickHouse,
}
impl HpoLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}
impl Loader<String> for HpoLoader {
    type Value = Hpo;
    type Error = async_graphql::Error;
    async fn load(&self, keys: &[String]) -> Result<HashMap<String, Hpo>, Self::Error> {
        Ok(fetch_by_ids(&self.ch, keys)
            .await?
            .into_iter()
            .map(|h| (h.id.clone(), h))
            .collect())
    }
}

// ---- retriever ----

#[tracing::instrument(skip_all, fields(n = ids.len()))]
async fn fetch_by_ids(ch: &ClickHouse, ids: &[String]) -> clickhouse::error::Result<Vec<Hpo>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    ch.query("SELECT ?fields FROM hpo WHERE id IN ?")
        .bind(ids)
        .fetch_all::<Hpo>()
        .await
}

// ---- resolver ----

#[derive(Default)]
pub struct HpoQuery;

#[Object]
impl HpoQuery {
    /// Fetch HPOs by HPO ID. `ids` is the PK anchor and is required.
    async fn hpos(
        &self,
        ctx: &Context<'_>,
        ids: Vec<String>,
        search: Option<String>,
        sort: Option<Sort<HpoSortField>>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Hpo>> {
        let items = fetch_by_ids(ctx.data::<ClickHouse>()?, &ids).await?;
        Ok(items
            .query()
            .search(search.as_deref())
            .sort(sort.as_ref())
            .paginate(page))
    }
}
