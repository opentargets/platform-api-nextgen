//! `Disease HPO` entity: core annotation for Disease HPO.

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

/// `DiseaseHpoEvidences` entity: represents the core annotation for DiseaseHpoEvidences.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DiseaseHpoEvidences {
    aspect: Option<String>,
    bio_curation: Option<String>,
    disease_from_source_id: String,
    disease_from_source: String,
    disease_name: String,
    evidence_type: Option<String>,
    frequency: Option<String>,
    modifiers: Vec<String>,
    onset: Vec<String>,
    qualifier_not: bool,
    references: Vec<String>,
    sex: Option<String>,
    resource: String,
}

/// Entity trait: how do we get the unique indentifier of a DiseaseHpo:
impl Entity for DiseaseHpo {
    fn id(&self) -> &str { &self.phenotype }
}

/// `DiseaseHpo` entity: represents the core annotation for a DiseaseHpo.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DiseaseHpo {
    phenotype: String,
    disease: String,
    evidence: Vec<String>,
}

/// `DiseaseHpos` entity: represents the core annotation for a DiseaseHpos.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DiseaseHpos {
    count: i64,
    rows: Vec<DiseaseHpo>,
    // TODO: Implement default id = "" or not? Are we removing default id="" feature?
    id: String,
}

/// DiseaseHPO filter: which fields can we filter disease HPOs by?
#[derive(Debug, InputObject)]
pub struct DiseaseHpoFilter {
    pub phenotype: Option<StringFilter>,
}

impl Filter<DiseaseHpo> for DiseaseHpoFilter {
    fn matches(&self, d: &DiseaseHpo) -> bool {
        self.phenotype
            .as_ref()
            .is_none_or(|f| f.matches(Some(&d.phenotype)))
    }
}

/// DiseaseHpo sort fields: which fields can we sort disease HPOs by?
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Default)]
pub enum DiseaseHpoSortField {
    #[default]
    Phenotype,
    Disease,
}

impl SortKey<DiseaseHpo> for DiseaseHpoSortField {
    fn compare(&self, a: &DiseaseHpo, b: &DiseaseHpo) -> Ordering {
        match self {
            Self::Phenotype => a.phenotype.cmp(&b.phenotype),
            Self::Disease => a.disease.cmp(&b.disease),
        }
    }
}

/// Searchable implementation for DiseaseHpo: searches by phenotype and disease.
impl Searchable for DiseaseHpo {
    fn matches_search(&self, needle: &str) -> bool {
        self.phenotype.to_lowercase().contains(needle)
            || self.disease.to_lowercase().contains(needle)
    }
}

// ---- retriever ----

#[tracing::instrument(skip_all, fields(n = ids.len()))]
async fn fetch_by_ids(ch: &ClickHouse, ids: &[String]) -> Result<Vec<DiseaseHpo>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    // TODO: check table in k9s and fix query
    ch.query("SELECT ?fields FROM disease_hpo WHERE phenotype IN ?")
        .bind(ids)
        .fetch_all::<DiseaseHpo>()
        .await
}

// ---- resolver ----

#[derive(Default)]
pub struct DiseaseHpoQuery;

#[Object]
impl DiseaseHpoQuery {
    /// Fetch DiseaseHPOs by DiseaseHPO ID. `ids` is the PK anchor and is required.
    async fn disease_hpos(
        &self,
        ctx: &Context<'_>,
        ids: Vec<String>,
        filter: Option<DiseaseHpoFilter>,
        search: Option<String>,
        sort_by: Option<DiseaseHpoSortField>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<DiseaseHpo>> {
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
