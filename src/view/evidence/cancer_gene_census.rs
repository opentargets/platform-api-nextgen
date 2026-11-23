//! Cancer Gene Census evidence: somatic mutations in cancer genes curated by COSMIC.

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

const DATASOURCE: &str = "cancer_gene_census";

// ---- model ----

/// One mutation type observed for a disease/target pair, with its sample counts.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCancerGeneCensusMutation {
    /// Sequence Ontology identifier of the functional consequence.
    functional_consequence_id: Option<String>,
    /// Label of the functional consequence, the mutation type.
    functional_consequence_label: Option<String>,
    /// Number of samples with a mutation in the target.
    number_mutated_samples: u32,
    /// Number of samples tested.
    number_samples_tested: u32,
    /// Number of mutated samples with this mutation type.
    number_samples_with_mutation_type: Option<u32>,
}

/// A Cancer Gene Census evidence, as shown in the evidence page's Cancer Gene Census table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/cancer_gene_census.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCancerGeneCensus {
    /// Identifier of the disease/target evidence.
    id: String,
    /// Score of the evidence reflecting the strength of the disease/target relationship.
    score: f64,
    /// Identifier of the disease for which the target is associated in this evidence.
    disease_id: String,
    /// Name of the disease. Empty if the disease isn't found.
    disease_name: String,
    /// Identifier of the target for which the disease is associated in this evidence.
    target_id: String,
    /// Approved symbol of the target. Empty if the target isn't found.
    target_approved_symbol: String,
    /// Mutation types with their sample counts.
    mutated_samples: Vec<EvidenceCancerGeneCensusMutation>,
    /// Fraction of tested samples that are mutated, 0 to 1. 0 if the totals are unknown.
    mutated_samples_ratio: f64,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
}

impl Table for EvidenceCancerGeneCensus {
    type Col = EvidenceCancerGeneCensusCol;
    const NAME: &'static str = include_str!("../../sql/evidence/cancer_gene_census.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceCancerGeneCensusCol {
    Id                  => "id"                                 : Str,
    DiseaseName         => "diseaseName"                        : Str,
    MutationTypes       => "arrayMap(m -> m.2, mutatedSamples)" : StrArray,
    MutatedSamplesRatio => "mutatedSamplesRatio"                : F64,
    Literature          => "literature"                         : StrArray,
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceCancerGeneCensus>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceCancerGeneCensusCol::Id, DATASOURCE)?,
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

/// Fields available for sorting. The table only sorts by the mutated over total samples.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Hash)]
#[graphql(name = "EvidenceCancerGeneCensusSortField")]
pub enum SortField {
    /// Mutated over total samples.
    MutatedSamples,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::MutatedSamples,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceCancerGeneCensusCol>> {
        use EvidenceCancerGeneCensusCol as C;
        let cols: &[C] = match self {
            Self::MutatedSamples => &[C::MutatedSamplesRatio],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the disease/phenotype, mutation type and
    /// literature columns of the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceCancerGeneCensusSearchField")]
    pub enum SearchField => EvidenceCancerGeneCensusCol {
        DiseaseName,
        MutationTypes,
        Literature,
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
    evidences: Paged<EvidenceCancerGeneCensus>,
}

#[derive(From)]
pub struct EvidenceCancerGeneCensusLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceCancerGeneCensusLoader {
    type Value = Paged<EvidenceCancerGeneCensus>;
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
pub async fn load_evidence_cancer_gene_census(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceCancerGeneCensus>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceCancerGeneCensusLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
