use std::cmp::Ordering;

use async_graphql::{Context, Enum, Object, SimpleObject};
use clickhouse::Row;
use serde::Deserialize;
use serde_repr::Deserialize_repr;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, QueryExt,
        paginate::{Page, Paged},
        search::Searchable,
        sort::{Sort, SortKey, nulls_last},
    },
};

// ---- models ----

/// Field specifying if study contains phenotype/disease or molecular genetic associations.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Enum, Deserialize_repr)]
#[repr(i8)]
#[graphql(rename_items = "lowercase")]
pub enum StudyType {
    Tuqtl = 1,
    Pqtl = 2,
    Eqtl = 3,
    Sqtl = 4,
    Sctuqtl = 5,
    Scpqtl = 6,
    Sceqtl = 7,
    Scsqtl = 8,
    Gwas = 9,
}

/// Represents a sample of biological material.
#[derive(Debug, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
// #[serde(from = "(String, u32)")]
pub struct Sample {
    ancestry: String,
    sample_size: u32,
}

/// Collection of populations referenced by the study.
#[derive(Debug, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
// #[serde(from = "(String, Option<f64>)")]
pub struct LdPopulationStructure {
    ld_population: String,
    relative_sample_size: Option<f64>,
}

/// Mapping of quality control flags.
#[derive(Debug, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
// #[serde(from = "(String, f64)")]
pub struct SumStatQC {
    #[graphql(name = "QCCheckName")]
    qc_check_name: String,
    #[graphql(name = "QCCheckValue")]
    qc_check_value: f64,
}

/// Metadata for all complex trait GWAS and molecular QTL studies in the Platform. The dataset includes study metadata, phenotype information, sample sizes, publication information and more. Molecular QTL studies are split by their target trait (e.g. gene, splice junction, etc), biosample (tissue, cell type or cell line) and condition (e.g. stimulation, time period, etc), potentially leading to tens of thousands of studies derived from the same publication
#[allow(clippy::struct_field_names)]
#[derive(Debug, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Study {
    #[graphql(name = "id")]
    study_id: String,
    condition: Option<String>,
    project_id: String,
    study_type: StudyType,
    trait_from_source: String,
    n_samples: Option<u32>,
    summarystats_location: Option<String>,
    has_sumstats: Option<bool>,
    cohorts: Vec<String>,
    initial_sample_size: Option<String>,
    trait_from_source_mapped_ids: Vec<String>,
    publication_journal: Option<String>,
    publication_date: Option<String>,
    ld_population_structure: Vec<LdPopulationStructure>,
    quality_controls: Vec<String>,
    replication_samples: Vec<Sample>,
    n_controls: Option<u32>,
    pubmed_id: Option<String>,
    publication_first_author: Option<String>,
    publication_title: Option<String>,
    discovery_samples: Vec<Sample>,
    n_cases: Option<u32>,
    analysis_flags: Vec<String>,
    #[serde(rename = "sumstatQCValues")]
    #[graphql(name = "SumstatQCValues")]
    sumstat_qc_values: Vec<SumStatQC>,
}

impl Entity for Study {
    fn id(&self) -> &str { &self.study_id }
}

/// Contains the fields available for sorting studies.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
pub enum StudySortField {
    StudyId,
    ProjectId,
    TraitFromSource,
    NSamples,
    NCases,
    NControls,
    PublicationDate,
    StudyType,
}

impl SortKey<Study> for StudySortField {
    fn compare(&self, a: &Study, b: &Study) -> Ordering {
        match self {
            Self::StudyId => a.study_id.cmp(&b.study_id),
            Self::ProjectId => a.project_id.cmp(&b.project_id),
            Self::TraitFromSource => a.trait_from_source.cmp(&b.trait_from_source),
            Self::NSamples => nulls_last(&a.n_samples, &b.n_samples),
            Self::NCases => nulls_last(&a.n_cases, &b.n_cases),
            Self::NControls => nulls_last(&a.n_controls, &b.n_controls),
            Self::PublicationDate => nulls_last(&a.publication_date, &b.publication_date),
            Self::StudyType => a.study_type.cmp(&b.study_type),
        }
    }
}

impl Searchable for Study {
    fn matches_search(&self, needle: &str) -> bool {
        self.study_id.to_lowercase().contains(needle)
            || self.trait_from_source.to_lowercase().contains(needle)
            || self.project_id.to_lowercase().contains(needle)
    }
}

// ---- retrievers ----

async fn fetch_studies(
    ch: &ClickHouse,
    ids: Option<&[String]>,
    disease_ids: Option<&[String]>,
    indirect: bool,
) -> clickhouse::error::Result<Vec<Study>> {
    let mut clauses: Vec<&'static str> = Vec::new();
    if ids.is_some() {
        clauses.push("studyId IN ?");
    }
    if disease_ids.is_some() {
        clauses.push(if indirect {
            "studyId IN (SELECT arrayJoin(arrayUnion(studyIds, indirectStudyIds)) \
             FROM disease WHERE id IN ?)"
        } else {
            "studyId IN (SELECT arrayJoin(studyIds) FROM disease WHERE id IN ?)"
        });
    }
    let sql = format!(
        "SELECT ?fields FROM studies WHERE {}",
        clauses.join(" AND ")
    );

    let mut q = ch.query(&sql);
    if let Some(v) = ids {
        q = q.bind(v);
    }
    if let Some(v) = disease_ids {
        q = q.bind(v);
    }
    q.fetch_all::<Study>().await
}

async fn fetch_by_id(ch: &ClickHouse, study_id: &str) -> clickhouse::error::Result<Option<Study>> {
    ch.query("SELECT ?fields FROM studies WHERE studyId = ? LIMIT 1")
        .bind(study_id)
        .fetch_optional::<Study>()
        .await
}

// ---- resolvers ----

#[derive(Default)]
pub struct StudyQuery;

#[Object]
impl StudyQuery {
    async fn studies(
        &self,
        ctx: &Context<'_>,
        ids: Option<Vec<String>>,
        disease_ids: Option<Vec<String>>,
        #[graphql(default)] enable_indirect: bool,
        search: Option<String>,
        sort: Option<Sort<StudySortField>>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Study>> {
        if ids.is_none() && disease_ids.is_none() {
            return Err("one of ids or diseaseIds is required".into());
        }
        let items = fetch_studies(
            ctx.data::<ClickHouse>()?,
            ids.as_deref(),
            disease_ids.as_deref(),
            enable_indirect,
        )
        .await?;
        Ok(items
            .query()
            .search(search.as_deref())
            .sort(sort.as_ref())
            .paginate(page))
    }

    async fn study(
        &self,
        ctx: &Context<'_>,
        study_id: String,
    ) -> async_graphql::Result<Option<Study>> {
        Ok(fetch_by_id(ctx.data::<ClickHouse>()?, &study_id).await?)
    }
}
