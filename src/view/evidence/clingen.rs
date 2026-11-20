//! ClinGen evidence: gene-disease validity classifications curated by ClinGen.

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

const DATASOURCE: &str = "clingen";

// ---- model ----

/// An ClinGen evidence, as shown in the evidence page's ClinGen table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/clingen.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceClinGen {
    /// Identifier of the disease/target evidence.
    id: String,
    /// Score of the evidence reflecting the strength of the disease/target relationship.
    score: f64,
    /// Identifier of the disease for which the target is associated in this evidence.
    disease_id: String,
    /// Name of the disease. Empty if the disease isn't found.
    disease_name: String,
    /// Disease/phenotype label from ClinGen.
    disease_from_source: Option<String>,
    /// Identifier of the target for which the disease is associated in this evidence.
    target_id: String,
    /// Allelic requirements, e.g. AD (autosomal dominant) or AR (autosomal recessive).
    allelic_requirements: Vec<String>,
    /// ClinGen classification of the gene-disease validity, e.g. Definitive or Limited.
    confidence: Option<String>,
}

impl Table for EvidenceClinGen {
    type Col = EvidenceClinGenCol;
    const NAME: &'static str = include_str!("../../sql/evidence/clingen.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceClinGenCol {
    Id                 => "id"                 : Str,
    Score              => "score"              : F64,
    DiseaseFromSource  => "diseaseFromSource"  : Str,
    AllelicRequirements => "allelicRequirements" : StrArray,
    Confidence         => "confidence"         : Str,
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceClinGen>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceClinGenCol::Id, DATASOURCE)?,
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

/// Fields available for sorting. The table isn't sortable in the UI, so only the score.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Hash)]
#[graphql(name = "EvidenceClinGenSortField")]
pub enum SortField {
    /// Evidence score.
    Score,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::Score,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceClinGenCol>> {
        use EvidenceClinGenCol as C;
        let cols: &[C] = match self {
            Self::Score => &[C::Score],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the disease/phenotype, allelic requirement
    /// and classification columns of the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceClinGenSearchField")]
    pub enum SearchField => EvidenceClinGenCol {
        DiseaseFromSource,
        AllelicRequirements,
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
    evidences: Paged<EvidenceClinGen>,
}

#[derive(From)]
pub struct EvidenceClinGenLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceClinGenLoader {
    type Value = Paged<EvidenceClinGen>;
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
pub async fn load_evidence_clingen(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceClinGen>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceClinGenLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
