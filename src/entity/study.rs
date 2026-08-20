use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;
use serde_repr::Deserialize_repr;

use crate::{
    config::DEFAULT_CACHE_CAPACITY,
    datasource::clickhouse::ClickHouse,
    entity::disease::{Disease, DiseaseLoader, load_diseases},
    query::{
        Entity, QueryExt,
        cache::CachedLoader,
        load_ordered,
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
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// Sample ancestry name.
    ancestry: String,
    /// Sample size.
    sample_size: u32,
}

/// Collection of populations.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct LdPopulationStructure {
    /// Population identifier.
    ld_population: String,
    /// Fraction of the total sample represented by the population.
    relative_sample_size: Option<f64>,
}

/// Mapping of quality control flags.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct SumStatQC {
    #[graphql(name = "QCCheckName")]
    /// Quality control metric identifier.
    qc_check_name: String,
    /// Quality control metric value.
    #[graphql(name = "QCCheckValue")]
    qc_check_value: f64,
}

/// Metadata for all complex trait GWAS and molecular QTL studies in the Platform. The dataset
/// includes study metadata, phenotype information, sample sizes, publication information and more.
/// Molecular QTL studies are split by their target trait (e.g. gene, splice junction, etc),
/// biosample (tissue, cell type or cell line) and condition (e.g. stimulation, time period, etc),
/// potentially leading to tens of thousands of studies derived from the same publication.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Study {
    // Identity
    #[graphql(name = "id")]
    study_id: String,
    /// Identifier of the source project collection that the study information is derived from.
    project_id: String,
    /// Field specifying if study contains phenotype/disease or molecular genetic associations.
    study_type: StudyType,

    // Trait
    /// Molecular or phenotypic trait, derived from source, analysed in the study.
    trait_from_source: String,
    /// Phenotypic trait IDs that map to the analysed trait reported by study.
    trait_from_source_mapped_ids: Vec<String>,
    /// Reported sample conditions.
    condition: Option<String>,

    // Sample composition
    /// Study initial sample size.
    initial_sample_size: Option<String>,
    /// The number of samples tested in GWAS analysis.
    n_samples: Option<u32>,
    /// The number of cases in this broad ancestry group.
    n_cases: Option<u32>,
    /// The number of controls in this broad ancestry group.
    n_controls: Option<u32>,
    /// List of cohort(s) represented in the discovery sample.
    cohorts: Vec<String>,
    /// Collection of ancestries reported by the study discovery phase.
    discovery_samples: Vec<Sample>,
    /// Collection of ancestries reported by the study replication phase.
    replication_samples: Vec<Sample>,
    /// Collection of populations referenced by the study.
    ld_population_structure: Vec<LdPopulationStructure>,

    // Summary statistics
    /// Indication whether the summary statistics exist in the source.
    has_sumstats: Option<bool>,
    /// Path to the source study summary statistics (if exists at the source).
    summarystats_location: Option<String>,

    // Analysis & QC
    /// Collection of flags indicating the type of the analysis conducted in the association study.
    analysis_flags: Vec<String>,
    /// Control metrics refining study validation.
    quality_controls: Vec<String>,
    /// Mapping of quality control flags.
    #[serde(rename = "sumstatQCValues")]
    #[graphql(name = "SumstatQCValues")]
    sumstat_qc_values: Vec<SumStatQC>,

    // Publication
    /// PubMed identifier of the publication that references the study [bioregistry:pubmed].
    pubmed_id: Option<String>,
    /// First name and initials of the author of the publication that references the study.
    publication_first_author: Option<String>,
    /// Title of the publication that references the study.
    publication_title: Option<String>,
    /// Abbreviated journal name where the publication referencing study was published.
    publication_journal: Option<String>,
    /// Date of the publication that references the study.
    publication_date: Option<String>,

    // embedded fields
    /// Disease associated with a studied trait.
    #[graphql(skip)]
    disease_ids: Vec<String>,
}

// ---- query utilities ----

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

// ---- loaders ----

pub type StudyCache = Cache<String, Option<Study>>;

#[must_use]
pub fn study_cache() -> StudyCache {
    Cache::builder()
        .max_capacity(DEFAULT_CACHE_CAPACITY)
        .build()
}

pub struct StudyLoader {
    ch: ClickHouse,
    cache: StudyCache,
}

impl StudyLoader {
    #[must_use]
    pub fn new(ch: ClickHouse, cache: StudyCache) -> Self { Self { ch, cache } }
}

impl CachedLoader for StudyLoader {
    type Key = String;
    type Value = Study;

    fn cache(&self) -> &StudyCache { &self.cache }
    fn key_of(v: &Self::Value) -> Self::Key { v.study_id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM studies WHERE studyId IN ?")
            .bind(misses)
            .fetch_all::<Study>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for StudyLoader {
    type Value = Study;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, Study>, Self::Error> {
        self.load_cached(keys).await
    }
}

async fn load_studies(
    ctx: &Context<'_>,
    study_ids: Option<&[String]>,
    disease_ids: Option<&[String]>,
    indirect: bool,
) -> async_graphql::Result<Vec<Study>> {
    let study_loader = ctx.data_unchecked::<DataLoader<StudyLoader>>();

    // no disease_ids: just fetch with study_ids
    let Some(disease_ids) = disease_ids else {
        return load_ordered(study_loader, study_ids.unwrap_or_default()).await;
    };

    // disease_ids: first get their related study_ids
    let disease_loader = ctx.data_unchecked::<DataLoader<DiseaseLoader>>();
    let mut study_ids_from_diseases: Vec<String> = disease_loader
        .load_many(disease_ids.iter().cloned())
        .await?
        .into_values()
        .flat_map(|d| d.into_study_ids(indirect))
        .collect();
    study_ids_from_diseases.sort_unstable();
    study_ids_from_diseases.dedup();

    // then filter if there are study_ids
    if let Some(study_ids) = study_ids {
        // convert to a HashSet to make membership test O(1)
        let keep: HashSet<&String> = study_ids.iter().collect();
        study_ids_from_diseases.retain(|id| keep.contains(id));
    }

    load_ordered(study_loader, &study_ids_from_diseases).await
}

// ---- resolvers ----

#[derive(Default)]
pub struct StudyQuery;

#[Object]
impl StudyQuery {
    async fn studies(
        &self,
        ctx: &Context<'_>,
        study_ids: Option<Vec<String>>,
        disease_ids: Option<Vec<String>>,
        #[graphql(default)] enable_indirect: bool,
        search: Option<String>,
        sort: Option<Sort<StudySortField>>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Study>> {
        if study_ids.is_none() && disease_ids.is_none() {
            return Err("one of studyIds or diseaseIds is required".into());
        }
        let items = load_studies(
            ctx,
            study_ids.as_deref(),
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
        ctx.data_unchecked::<DataLoader<StudyLoader>>()
            .load_one(study_id)
            .await
    }
}

#[ComplexObject]
impl Study {
    /// Disease associated with a studied trait.
    async fn diseases(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Disease>> {
        load_diseases(ctx, &self.disease_ids).await
    }
}
