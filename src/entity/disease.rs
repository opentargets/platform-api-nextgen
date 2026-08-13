use std::{cmp::Ordering, collections::HashMap};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::disease_hpo::{DiseasePhenotype, DiseasePhenotypeLoader},
    query::{
        Entity, QueryExt,
        paginate::{Page, Paged},
        search::Searchable,
        sort::{Sort, SortKey},
    },
};

// ---- model ----

/// List of synonymous disease labels.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DiseaseSynonym {
    relation: String,
    terms: Vec<Option<String>>,
}

/// Core annotation for diseases or phenotypes. A disease or phenotype in the Platform is understood as any disease, phenotype, biological process or measurement that might have any type of causality relationship with a human target. The EMBL-EBI Experimental Factor Ontology (EFO) (slim version) is used as scaffold for the disease or phenotype entity
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Disease {
    id: String,
    name: String,
    description: Option<String>,
    is_therapeutic_area: bool,
    therapeutic_areas: Vec<String>,
    #[graphql(skip)]
    parents: Vec<String>,
    #[graphql(skip)]
    children: Vec<String>,
    ancestors: Vec<String>,
    descendants: Vec<String>,
    synonyms: Vec<DiseaseSynonym>,
    obsolete_terms: Vec<String>,
    db_x_refs: Vec<String>,
    direct_location_ids: Vec<String>,
    indirect_location_ids: Vec<String>,
}

impl Entity for Disease {
    fn id(&self) -> &str { &self.id }
}

/// Contains the fields available for sorting diseases.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
pub enum DiseaseSortField {
    Id,
    Name,
    IsTherapeuticArea,
}

impl SortKey<Disease> for DiseaseSortField {
    fn compare(&self, a: &Disease, b: &Disease) -> Ordering {
        match self {
            Self::Id => a.id.cmp(&b.id),
            Self::Name => a.name.cmp(&b.name),
            Self::IsTherapeuticArea => a.is_therapeutic_area.cmp(&b.is_therapeutic_area),
        }
    }
}

impl Searchable for Disease {
    fn matches_search(&self, needle: &str) -> bool {
        self.id.to_lowercase().contains(needle)
            || self.name.to_lowercase().contains(needle)
            || self
                .description
                .as_deref()
                .is_some_and(|d| d.to_lowercase().contains(needle))
            || self.synonyms.iter().flat_map(|s| &s.terms).any(|t| {
                t.as_deref()
                    .is_some_and(|f| f.to_lowercase().contains(needle))
            })
    }
}

// ---- loaders ----

pub struct DiseaseLoader {
    pub ch: ClickHouse,
}

impl DiseaseLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<String> for DiseaseLoader {
    type Value = Disease;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, Disease>, Self::Error> {
        let rows = fetch_by_ids(&self.ch, keys).await?;
        Ok(rows.into_iter().map(|d| (d.id.clone(), d)).collect())
    }
}

// ---- retriever ----

#[tracing::instrument(skip_all, fields(n = ids.len()))]
async fn fetch_by_ids(ch: &ClickHouse, ids: &[String]) -> clickhouse::error::Result<Vec<Disease>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    ch.query("SELECT ?fields FROM disease WHERE id IN ?")
        .bind(ids)
        .fetch_all::<Disease>()
        .await
}

#[tracing::instrument(skip(ch), fields(id = %id))]
async fn fetch_by_id(ch: &ClickHouse, id: &str) -> clickhouse::error::Result<Option<Disease>> {
    ch.query("SELECT ?fields FROM disease WHERE id = ? LIMIT 1")
        .bind(id)
        .fetch_optional::<Disease>()
        .await
}

// ---- resolver ----

#[derive(Default)]
pub struct DiseaseQuery;

#[Object]
impl DiseaseQuery {
    /// Fetch diseases by ontology ID.
    async fn diseases(
        &self,
        ctx: &Context<'_>,
        ids: Vec<String>,
        search: Option<String>,
        sort: Option<Sort<DiseaseSortField>>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Disease>> {
        let items = fetch_by_ids(ctx.data::<ClickHouse>()?, &ids).await?;
        Ok(items
            .query()
            .search(search.as_deref())
            .sort(sort.as_ref())
            .paginate(page))
    }

    async fn disease(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> async_graphql::Result<Option<Disease>> {
        Ok(fetch_by_id(ctx.data::<ClickHouse>()?, &id).await?)
    }
}

#[ComplexObject]
impl Disease {
    /// Direct parent terms in the disease ontology.
    async fn parents(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Disease>> {
        load_diseases(ctx, &self.parents).await
    }

    /// Direct child terms in the disease ontology.
    async fn children(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Disease>> {
        load_diseases(ctx, &self.children).await
    }

    async fn phenotypes(
        &self,
        ctx: &Context<'_>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<DiseasePhenotype>> {
        let items = ctx
            .data_unchecked::<DataLoader<DiseasePhenotypeLoader>>()
            .load_one(self.id.clone())
            .await?
            .unwrap_or_default();
        Ok(items.query().paginate(page))
    }
}

async fn load_diseases(ctx: &Context<'_>, ids: &[String]) -> async_graphql::Result<Vec<Disease>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let loader = ctx.data_unchecked::<DataLoader<DiseaseLoader>>();
    let mut found = loader.load_many(ids.iter().cloned()).await?;
    Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
}
