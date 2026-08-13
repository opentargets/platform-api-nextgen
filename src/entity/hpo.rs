//! `Human Phenotype Ontology (HPO)` entity: core annotation for HPO.

use std::cmp::Ordering;

use async_graphql::{Context, Enum, InputObject, Object, SimpleObject};
use clickhouse::{Row, error::Result};
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, execute,
        filter::{Filter, StringFilter},
        paginate::Page,
        search::Searchable,
        sort::{SortDirection, SortKey},
    },
    schema::Paged,
};

// ---- model ----

/// Entity trait: how do we get the unique indentifier of a disease:
impl Entity for Hpo {
    fn id(&self) -> &str { &self.id }
}

/// `HPO` entity: represents the core annotation for a HPO.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Hpo {
    id: String,
    name: String,
    description: Option<String>,
    namespace: Vec<String>,
}

/// HPO filter: which fields can we filter HPO by?
#[derive(Debug, InputObject)]
pub struct HpoFilter {
    pub id: Option<StringFilter>,
}

impl Filter<Hpo> for HpoFilter {
    fn matches(&self, d: &Hpo) -> bool { self.id.as_ref().is_none_or(|f| f.matches(Some(&d.id))) }
}

/// Hpo sort fields: which fields can we sort diseases by?
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Default)]
pub enum HpoSortField {
    #[default]
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

/// Searchable implementation for Hpo: searches by ID, name, and description.
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

// ---- retriever ----

#[tracing::instrument(skip_all, fields(n = ids.len()))]
async fn fetch_by_ids(ch: &ClickHouse, ids: &[String]) -> Result<Vec<Hpo>> {
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
        filter: Option<HpoFilter>,
        search: Option<String>,
        sort_by: Option<HpoSortField>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Hpo>> {
        let items = fetch_by_ids(ctx.data::<ClickHouse>()?, &ids).await?;
        Ok(execute(
            items,
            filter.as_ref(),
            search.as_deref(),
            &sort_by,
            SortDirection::Asc,
            page,
        ))
    }
}
