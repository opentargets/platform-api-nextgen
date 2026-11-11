use std::collections::HashMap;

use async_graphql::{
    Context, Enum, Error, InputObject, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use strum::{EnumIter, IntoEnumIterator};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::study::LdPopulationStructure,
    query::{
        paginate::Paged,
        sort::{Sort, SortDirection},
        sql::{
            Column, Filter, OrderBy, PageSpec, Table, Value, ValueKind, build, column_subset,
            columns,
        },
    },
};

// ---- model ----

/// A study linked to a disease, as shown in the disease page's GWAS studies table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `DiseaseStudy::NAME`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DiseaseStudy {
    /// The GWAS or molQTL study identifier (e.g. GCST004132).
    #[graphql(name = "id")]
    study_id: String,
    /// Identifier of the source project collection that the study information is derived from.
    project_id: String,
    /// Molecular or phenotypic trait, derived from source, analysed in the study.
    trait_from_source: String,
    /// First name and initials of the author of the publication that references the study.
    publication_first_author: Option<String>,
    /// Date of the publication that references the study.
    publication_date: Option<String>,
    /// Abbreviated journal name where the publication referencing study was published.
    publication_journal: Option<String>,
    /// The number of samples tested in GWAS analysis.
    n_samples: Option<u32>,
    /// List of cohort(s) represented in the discovery sample.
    cohorts: Vec<String>,
    /// PubMed identifier of the publication that references the study [bioregistry:pubmed].
    pubmed_id: Option<String>,
    /// Collection of populations referenced by the study.
    ld_population_structure: Vec<LdPopulationStructure>,
    /// Number of credible sets identified in the study.
    credible_sets_count: u64,
}

impl Table for DiseaseStudy {
    type Col = DiseaseStudyCol;
    // `studies` is a key-value table keyed by `studyId`, so it must only be read through a
    // `studyId IN (...)` filter (see `QueryParams::to_spec`); anything else is a full scan.
    const NAME: &'static str = "(
        SELECT
            studyId,
            projectId,
            traitFromSource,
            publicationFirstAuthor,
            publicationDate,
            publicationJournal,
            nSamples,
            cohorts,
            pubmedId,
            ldPopulationStructure,
            length(studyLocusIds) AS credibleSetsCount
        FROM studies
    )";
    const TIE_BREAKER: &'static str = "studyId";
}

columns!(DiseaseStudyCol {
    StudyId                => "studyId"                : Str,
    ProjectId              => "projectId"              : Str,
    TraitFromSource        => "traitFromSource"        : Str,
    PublicationFirstAuthor => "publicationFirstAuthor" : Str,
    PublicationDate        => "publicationDate"        : Str,
    PublicationJournal     => "publicationJournal"     : Str,
    NSamples               => "nSamples"               : U64,
    Cohorts                => "cohorts"                : StrArray,
    PubmedId               => "pubmedId"               : Str,
    CredibleSetsCount      => "credibleSetsCount"      : U64,
});

/// The disease's study ids, read from the `disease` row so `studies` is looked up by key.
const DIRECT_STUDY_IDS: &str = "SELECT arrayJoin(studyIds) FROM disease WHERE id = ?";
/// As [`DIRECT_STUDY_IDS`], plus the studies of the disease's ontology descendants.
const ALL_STUDY_IDS: &str =
    "SELECT arrayJoin(arrayConcat(studyIds, indirectStudyIds)) FROM disease WHERE id = ?";

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    disease_id: String,
    indirect: bool,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        disease_id: String,
        indirect: bool,
        sort: Vec<Sort<SortField>>,
        search: Option<Search>,
        index: u32,
        size: u32,
    ) -> Self {
        Self {
            disease_id,
            indirect,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<DiseaseStudy>> {
        let study_ids = if self.indirect {
            ALL_STUDY_IDS
        } else {
            DIRECT_STUDY_IDS
        };
        let mut filters = vec![
            Filter::in_subquery(
                DiseaseStudyCol::StudyId,
                study_ids,
                Value::Str(self.disease_id.clone()),
            )
            .map_err(Error::new)?,
        ];
        filters.extend(self.search.as_ref().map(Search::to_filter).transpose()?);
        Ok(PageSpec {
            filters,
            order_by: self
                .sort
                .iter()
                .flat_map(|s| s.key.as_order_by(s.direction))
                .collect(),
            offset: u64::from(self.index) * u64::from(self.size),
            limit: u64::from(self.size),
        })
    }
}

// ---- sort ----

/// Fields available for sorting.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Hash)]
#[graphql(name = "DiseaseStudiesSortField")]
pub enum SortField {
    /// Number of samples tested.
    NSamples,
    /// Number of credible sets identified in the study.
    CredibleSetsCount,
    /// By publication date.
    PublicationDate,
    /// Alphabetically.
    TraitFromSource,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::NSamples,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<DiseaseStudyCol>> {
        use DiseaseStudyCol as C;
        let cols: &[C] = match self {
            Self::NSamples => &[C::NSamples],
            Self::CredibleSetsCount => &[C::CredibleSetsCount],
            Self::PublicationDate => &[C::PublicationDate],
            Self::TraitFromSource => &[C::TraitFromSource],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "DiseaseStudiesSearchField")]
    pub enum SearchField => DiseaseStudyCol {
        StudyId,
        ProjectId,
        TraitFromSource,
        PublicationFirstAuthor,
        PublicationDate,
        PublicationJournal,
        Cohorts,
        PubmedId,
    }
}

impl SearchField {
    /// Every searchable field; the default when the client doesn't pick any.
    fn all() -> Vec<Self> { Self::iter().collect() }
}

/// Case-insensitive text search over the studies.
#[derive(Debug, Clone, Eq, PartialEq, Hash, InputObject)]
#[graphql(name = "DiseaseStudiesSearch")]
pub struct Search {
    /// Text to look for. Matches if any of `fields` contains it.
    #[graphql(validator(chars_max_length = 200))]
    text: String,
    /// Fields to search in. Defaults to all of them.
    #[graphql(default_with = "SearchField::all()", validator(min_items = 1))]
    fields: Vec<SearchField>,
}

impl Search {
    /// Trims the text and sorts/dedupes the fields, so equivalent searches share
    /// a `DataLoader` key. A blank search becomes `None`, the same as no search.
    fn normalise(mut self) -> Option<Self> {
        self.text = self.text.trim().to_owned();
        if self.text.is_empty() {
            return None;
        }
        self.fields.sort_unstable();
        self.fields.dedup();
        Some(self)
    }

    fn to_filter(&self) -> Result<Filter<DiseaseStudyCol>> {
        let cols = self.fields.iter().map(|f| f.as_col()).collect();
        Filter::search(cols, self.text.clone()).map_err(Error::new)
    }
}

// ---- loader ----

/// One row of the batched query: the page of results for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRow {
    key: usize,
    studies: Paged<DiseaseStudy>,
}

#[derive(From)]
pub struct DiseaseStudiesLoader {
    ch: ClickHouse,
}

impl DiseaseStudiesLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for DiseaseStudiesLoader {
    type Value = Paged<DiseaseStudy>;
    type Error = Error;

    async fn load(&self, keys: &[QueryParams]) -> Result<HashMap<QueryParams, Self::Value>> {
        let rendered = keys
            .iter()
            .map(|k| k.to_spec().map(|spec| spec.render()))
            .collect::<Result<Vec<_>>>()?;

        let rows = build(self.ch.client(), &rendered)
            .fetch_all::<BatchRow>()
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (keys[row.key].clone(), row.studies))
            .collect())
    }
}

/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_disease_studies(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<DiseaseStudy>> {
    Ok(ctx
        .data_unchecked::<DataLoader<DiseaseStudiesLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
