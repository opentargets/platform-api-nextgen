//! intOGen evidence: genes found to be cancer drivers in tumour cohorts by intOGen's driver
//! detection methods.

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

const DATASOURCE: &str = "intogen";

// ---- model ----

/// Mutation counts for one functional consequence, across the cohort's samples.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(name = "EvidenceIntogenMutatedSamples")]
pub struct MutationCounts {
    /// Sequence Ontology identifier of the functional consequence.
    functional_consequence_id: Option<String>,
    /// Sequence Ontology label of the functional consequence.
    functional_consequence_label: Option<String>,
    /// Number of cohort samples in which the target is mutated.
    number_mutated_samples: u32,
    /// Number of cohort samples tested.
    number_samples_tested: u32,
    /// Number of cohort samples with this type of mutation.
    number_samples_with_mutation_type: Option<u32>,
}

/// An intOGen evidence, as shown in the evidence page's intOGen table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/intogen.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceIntogen {
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
    /// Mutated and tested samples in the cohort.
    mutated_samples: Vec<MutationCounts>,
    /// Combined p-value of the cancer driver methods.
    resource_score: Option<f64>,
    /// Cancer driver methods that found the target significant.
    significant_driver_methods: Vec<String>,
    /// Identifier of the cohort.
    cohort_id: Option<String>,
    /// Short name of the cohort.
    cohort_short_name: Option<String>,
    /// Description of the cohort.
    cohort_description: Option<String>,
}

impl Table for EvidenceIntogen {
    type Col = EvidenceIntogenCol;
    const NAME: &'static str = include_str!("../../sql/evidence/intogen.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceIntogenCol {
    Id                => "id"                : Str,
    Score             => "score"             : F64,
    ResourceScore     => "resourceScore"     : F64,
    DiseaseName       => "diseaseName"       : Str,
    CohortShortName   => "cohortShortName"   : Str,
    CohortDescription => "cohortDescription" : Str,
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceIntogen>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceIntogenCol::Id, DATASOURCE)?,
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
#[graphql(name = "EvidenceIntogenSortField")]
pub enum SortField {
    /// Evidence score.
    Score,
    /// Combined p-value.
    PValue,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::Score,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceIntogenCol>> {
        use EvidenceIntogenCol as C;
        let cols: &[C] = match self {
            Self::Score => &[C::Score],
            Self::PValue => &[C::ResourceScore],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the text shown in the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceIntogenSearchField")]
    pub enum SearchField => EvidenceIntogenCol {
        DiseaseName,
        CohortShortName,
        CohortDescription,
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
    evidences: Paged<EvidenceIntogen>,
}

#[derive(From)]
pub struct EvidenceIntogenLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceIntogenLoader {
    type Value = Paged<EvidenceIntogen>;
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
pub async fn load_evidence_intogen(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceIntogen>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceIntogenLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
