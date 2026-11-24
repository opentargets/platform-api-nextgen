//! `UniProt` literature evidence: disease/target relationships curated by `UniProt` from the
//! literature.

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

const DATASOURCE: &str = "uniprot_literature";

// ---- model ----

/// A `UniProt` literature evidence, as shown in the evidence page's `UniProt` literature table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/uniprot_literature.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceUniprotLiterature {
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
    /// Reported protein: the `UniProt` accession of the target in the source.
    target_from_source_id: String,
    /// Identifier of the study generating the data.
    study_id: Option<String>,
    /// Confidence of the disease/target relationship reported by `UniProt`.
    confidence: Option<String>,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
}

impl Table for EvidenceUniprotLiterature {
    type Col = EvidenceUniprotLiteratureCol;
    const NAME: &'static str = include_str!("../../sql/evidence/uniprot_literature.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceUniprotLiteratureCol {
    Id                 => "id"                 : Str,
    Score              => "score"              : F64,
    DiseaseName        => "diseaseName"        : Str,
    DiseaseFromSource  => "diseaseFromSource"  : Str,
    TargetFromSourceId => "targetFromSourceId" : Str,
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceUniprotLiterature>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceUniprotLiteratureCol::Id, DATASOURCE)?,
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
#[graphql(name = "EvidenceUniprotLiteratureSortField")]
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

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceUniprotLiteratureCol>> {
        let cols: &[EvidenceUniprotLiteratureCol] = match self {
            Self::Score => &[EvidenceUniprotLiteratureCol::Score],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the disease/phenotype and reported
    /// protein columns of the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceUniprotLiteratureSearchField")]
    pub enum SearchField => EvidenceUniprotLiteratureCol {
        DiseaseName,
        DiseaseFromSource,
        TargetFromSourceId,
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
    evidences: Paged<EvidenceUniprotLiterature>,
}

#[derive(From)]
pub struct EvidenceUniprotLiteratureLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceUniprotLiteratureLoader {
    type Value = Paged<EvidenceUniprotLiterature>;
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
pub async fn load_evidence_uniprot_literature(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceUniprotLiterature>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceUniprotLiteratureLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
