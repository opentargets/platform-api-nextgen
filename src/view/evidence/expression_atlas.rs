//! Expression Atlas evidence: genes differentially expressed between disease and normal samples
//! in Expression Atlas experiments.

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
    query::{
        paginate::Paged,
        sort::{Sort, SortDirection},
        sql::{Column, OrderBy, PageSpec, Table, ValueKind, build, column_subset, columns},
    },
};

const DATASOURCE: &str = "expression_atlas";

// ---- model ----

/// An Expression Atlas evidence, as shown in the evidence page's Expression Atlas table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/expression_atlas.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceExpressionAtlas {
    /// Identifier of the disease/target evidence.
    id: String,
    /// Score of the evidence reflecting the strength of the disease/target relationship.
    score: f64,
    /// Identifier of the disease for which the target is associated in this evidence.
    disease_id: String,
    /// Name of the disease. Empty if the disease isn't found.
    disease_name: String,
    /// Disease label from the original source.
    disease_from_source: Option<String>,
    /// Identifier of the target for which the disease is associated in this evidence.
    target_id: String,
    /// Approved symbol of the target. Empty if the target isn't found.
    target_approved_symbol: String,
    /// Identifier of the Expression Atlas experiment.
    study_id: Option<String>,
    /// Description of the experiment.
    study_overview: Option<String>,
    /// Experiment details: the conditions compared, e.g. `'disease' vs 'normal' in 'tissue'`.
    contrast: Option<String>,
    /// Experiment confidence: low, medium or high.
    confidence: Option<String>,
    /// Percentile rank of the fold change among the experiment's genes.
    log2_fold_change_percentile_rank: Option<i32>,
    /// P-value of the differential expression.
    resource_score: Option<f64>,
}

impl Table for EvidenceExpressionAtlas {
    type Col = EvidenceExpressionAtlasCol;
    const NAME: &'static str = include_str!("../../sql/evidence/expression_atlas.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceExpressionAtlasCol {
    Id                           => "id"                           : Str,
    Score                        => "score"                        : F64,
    DiseaseName                  => "diseaseName"                  : Str,
    StudyId                      => "studyId"                      : Str,
    Contrast                     => "contrast"                     : Str,
    Confidence                   => "confidence"                   : Str,
    Log2FoldChangePercentileRank => "log2FoldChangePercentileRank" : I64,
    ResourceScore                => "resourceScore"                : F64,
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceExpressionAtlas>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceExpressionAtlasCol::Id, DATASOURCE)?,
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
#[graphql(name = "EvidenceExpressionAtlasSortField")]
pub enum SortField {
    /// Evidence score.
    Score,
    /// Percentile rank of the fold change.
    Percentile,
    /// P-value.
    PValue,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::Score,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceExpressionAtlasCol>> {
        use EvidenceExpressionAtlasCol as C;
        let cols: &[C] = match self {
            Self::Score => &[C::Score],
            Self::Percentile => &[C::Log2FoldChangePercentileRank],
            Self::PValue => &[C::ResourceScore],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the text shown in the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceExpressionAtlasSearchField")]
    pub enum SearchField => EvidenceExpressionAtlasCol {
        DiseaseName,
        StudyId,
        Contrast,
        Confidence,
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
    evidences: Paged<EvidenceExpressionAtlas>,
}

#[derive(From)]
pub struct EvidenceExpressionAtlasLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceExpressionAtlasLoader {
    type Value = Paged<EvidenceExpressionAtlas>;
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
pub async fn load_evidence_expression_atlas(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceExpressionAtlas>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceExpressionAtlasLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
