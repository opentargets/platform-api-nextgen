//! Orphanet evidence: rare-disease gene associations curated by Orphanet.

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

const DATASOURCE: &str = "orphanet";

// ---- model ----

/// An Orphanet evidence, as shown in the evidence page's Orphanet table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/orphanet.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceOrphanet {
    /// Identifier of the disease/target evidence.
    id: String,
    /// Score of the evidence reflecting the strength of the disease/target relationship.
    score: f64,
    /// Identifier of the disease for which the target is associated in this evidence.
    disease_id: String,
    /// Name of the disease. Empty if the disease isn't found.
    disease_name: String,
    /// Disease/phenotype label from Orphanet.
    disease_from_source: Option<String>,
    /// Identifier of the target for which the disease is associated in this evidence.
    target_id: String,
    /// Approved symbol of the target. Empty if the target isn't found.
    target_approved_symbol: String,
    /// Approved name of the target, the reported protein. Empty if the target isn't found.
    target_approved_name: String,
    /// Sequence Ontology identifier of the variant functional consequence.
    variant_functional_consequence_id: Option<String>,
    /// Label of the variant functional consequence.
    variant_functional_consequence_label: Option<String>,
    /// Gain or loss of function effect of the evidence on the target.
    direction_on_target: Option<String>,
    /// Predicted direction of effect on the trait.
    direction_on_trait: Option<String>,
    /// Allele origins, e.g. germline or somatic.
    allele_origins: Vec<String>,
    /// Whether Orphanet has assessed the association.
    confidence: Option<String>,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
}

impl Table for EvidenceOrphanet {
    type Col = EvidenceOrphanetCol;
    const NAME: &'static str = include_str!("../../sql/evidence/orphanet.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceOrphanetCol {
    Id                                => "id"                                : Str,
    Score                             => "score"                             : F64,
    DiseaseName                       => "diseaseName"                       : Str,
    DiseaseFromSource                 => "diseaseFromSource"                 : Str,
    TargetApprovedSymbol              => "targetApprovedSymbol"              : Str,
    TargetApprovedName                => "targetApprovedName"                : Str,
    VariantFunctionalConsequenceLabel => "variantFunctionalConsequenceLabel" : Str,
    DirectionOnTarget                 => "directionOnTarget"                 : Str,
    DirectionOnTrait                  => "directionOnTrait"                  : Str,
    AlleleOrigins                     => "alleleOrigins"                     : StrArray,
    Confidence                        => "confidence"                        : Str,
    Literature                        => "literature"                        : StrArray,
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceOrphanet>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceOrphanetCol::Id, DATASOURCE)?,
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
#[graphql(name = "EvidenceOrphanetSortField")]
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

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceOrphanetCol>> {
        use EvidenceOrphanetCol as C;
        let cols: &[C] = match self {
            Self::Score => &[C::Score],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the disease/phenotype, reported protein,
    /// functional consequence, direction of effect, allele origin, confidence and literature
    /// columns of the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceOrphanetSearchField")]
    pub enum SearchField => EvidenceOrphanetCol {
        DiseaseName,
        DiseaseFromSource,
        TargetApprovedSymbol,
        TargetApprovedName,
        VariantFunctionalConsequenceLabel,
        DirectionOnTarget,
        DirectionOnTrait,
        AlleleOrigins,
        Confidence,
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
    evidences: Paged<EvidenceOrphanet>,
}

#[derive(From)]
pub struct EvidenceOrphanetLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceOrphanetLoader {
    type Value = Paged<EvidenceOrphanet>;
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
pub async fn load_evidence_orphanet(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceOrphanet>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceOrphanetLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
