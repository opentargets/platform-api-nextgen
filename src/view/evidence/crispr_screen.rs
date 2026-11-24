//! CRISPR screen evidence: genes whose perturbation changes a disease-relevant phenotype in
//! CRISPR screens (e.g. `CRISPRBrain`).

use std::collections::HashMap;

use async_graphql::{
    Context, Enum, Error, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use strum::EnumIter;

use super::EvidenceScope;
use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{association::Datasource, evidence::Datatype},
    query::{
        paginate::Paged,
        sort::{Sort, SortDirection},
        sql::{Column, OrderBy, PageSpec, Table, ValueKind, build, column_subset, columns},
    },
};

const DATASOURCE: &str = "crispr_screen";

// ---- model ----

/// A CRISPR screen evidence, as shown in the evidence page's CRISPR screen table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/crispr_screen.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCrisprScreen {
    /// Identifier of the disease/target evidence.
    id: String,
    /// Score of the evidence reflecting the strength of the disease/target relationship.
    score: f64,
    /// Identifier of the disease for which the target is associated in this evidence.
    disease_id: String,
    /// Name of the disease. Empty if the disease isn't found.
    disease_name: String,
    /// Reported disease: the disease label from the original source.
    disease_from_source: Option<String>,
    /// Identifier the source's disease label was mapped to.
    disease_from_source_mapped_id: String,
    /// Identifier of the target for which the disease is associated in this evidence.
    target_id: String,
    /// Approved symbol of the target. Empty if the target isn't found.
    target_approved_symbol: String,
    /// Target label from the original source.
    target_from_source_id: String,
    /// Identifier of the data source.
    datasource_id: Datasource,
    /// Identifier of the data type.
    datatype_id: Datatype,
    /// Source of the screen, e.g. `crispr_brain`.
    project_id: Option<String>,
    /// Identifier of the screen.
    study_id: Option<String>,
    /// Description of the screen.
    study_overview: Option<String>,
    /// Phenotype the screen compares.
    contrast: Option<String>,
    /// Cell type screened.
    cell_type: Option<String>,
    /// CRISPR library used in the screen.
    crispr_screen_library: Option<String>,
    /// Tail of the statistical test, e.g. `lower tail`.
    statistical_test_tail: Option<String>,
    /// Score of the phenotypic consequence of the target perturbation in the CRISPR screen.
    phenotypic_consequence_score: Option<f64>,
    /// Significance: the screen's p-value or FDR for the target.
    resource_score: Option<f64>,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
}

impl Table for EvidenceCrisprScreen {
    type Col = EvidenceCrisprScreenCol;
    const NAME: &'static str = include_str!("../../sql/evidence/crispr_screen.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceCrisprScreenCol {
    Id                => "id"                : Str,
    Score             => "score"             : F64,
    ResourceScore     => "resourceScore"     : F64,
    DiseaseFromSource => "diseaseFromSource" : Str,
    StudyId           => "studyId"           : Str,
    Contrast          => "contrast"          : Str,
    StudyOverview     => "studyOverview"     : Str,
    CellType          => "cellType"          : Str,
    // The id as stored (`crispr_brain`) and without underscores, so `CRISPRBrain`, the name the
    // table shows, matches too.
    Source            => "concat(projectId, ' ', replaceAll(projectId, '_', ''))" : Str,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    scope: EvidenceScope,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        scope: EvidenceScope,
        sort: Vec<Sort<SortField>>,
        search: Option<Search>,
        index: u32,
        size: u32,
    ) -> Self {
        Self {
            scope,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<EvidenceCrisprScreen>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceCrisprScreenCol::Id, DATASOURCE)?,
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
#[graphql(name = "EvidenceCrisprScreenSortField")]
pub enum SortField {
    /// Evidence score.
    Score,
    /// Significance (p-value or FDR).
    Significance,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::Score,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceCrisprScreenCol>> {
        use EvidenceCrisprScreenCol as C;
        let cols: &[C] = match self {
            Self::Score => &[C::Score],
            Self::Significance => &[C::ResourceScore],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the text shown in the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceCrisprScreenSearchField")]
    pub enum SearchField => EvidenceCrisprScreenCol {
        DiseaseFromSource,
        StudyId,
        Contrast,
        StudyOverview,
        CellType,
        Source,
    }
}

/// Case-insensitive text search over the `SearchField`s.
pub type Search = crate::query::search::Search<SearchField>;

// ---- loader ----

/// One row of the batched query: the page of results for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRow {
    key: usize,
    evidences: Paged<EvidenceCrisprScreen>,
}

#[derive(From)]
pub struct EvidenceCrisprScreenLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceCrisprScreenLoader {
    type Value = Paged<EvidenceCrisprScreen>;
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
            .map(|row| (keys[row.key].clone(), row.evidences))
            .collect())
    }
}

/// # Errors
/// If the `ClickHouse` query fails.
pub async fn load_evidence_crispr_screen(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceCrisprScreen>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceCrisprScreenLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
