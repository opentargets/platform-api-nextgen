//! Gene2Phenotype evidence: curated gene-disease relationships from the Gene2Phenotype panels.

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
    entity::evidence::{DirectionOnTarget, DirectionOnTrait},
    query::{
        paginate::Paged,
        sort::{Sort, SortDirection},
        sql::{Column, OrderBy, PageSpec, Table, ValueKind, build, column_subset, columns},
    },
};

const DATASOURCE: &str = "gene2phenotype";

// ---- model ----

/// A Gene2Phenotype evidence, as shown in the evidence page's Gene2Phenotype table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/gene2phenotype.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceGene2Phenotype {
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
    /// Target ID in resource of origin.
    target_from_source_id: String,
    /// Sequence Ontology identifier of the functional consequence.
    variant_functional_consequence_id: Option<String>,
    /// Sequence Ontology label of the functional consequence.
    variant_functional_consequence_label: Option<String>,
    /// Gain or loss of function effect of the evidence on the target.
    direction_on_target: Option<DirectionOnTarget>,
    /// Predicted direction of effect on the trait.
    direction_on_trait: Option<DirectionOnTrait>,
    /// Inheritance patterns.
    allelic_requirements: Vec<String>,
    /// Gene2Phenotype panels the relationship is curated in, e.g. `DD; Ear`.
    study_id: Option<String>,
    /// Gene2Phenotype confidence category: limited, moderate, strong or definitive.
    confidence: Option<String>,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
}

impl Table for EvidenceGene2Phenotype {
    type Col = EvidenceGene2PhenotypeCol;
    const NAME: &'static str = include_str!("../../sql/evidence/gene2phenotype.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceGene2PhenotypeCol {
    Id                                => "id"                                : Str,
    Score                             => "score"                             : F64,
    DiseaseName                       => "diseaseName"                       : Str,
    DiseaseFromSource                 => "diseaseFromSource"                 : Str,
    VariantFunctionalConsequenceLabel => "variantFunctionalConsequenceLabel" : Str,
    AllelicRequirements               => "allelicRequirements"               : StrArray,
    StudyId                           => "studyId"                           : Str,
    Confidence                        => "confidence"                        : Str,
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceGene2Phenotype>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceGene2PhenotypeCol::Id, DATASOURCE)?,
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
#[graphql(name = "EvidenceGene2PhenotypeSortField")]
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

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceGene2PhenotypeCol>> {
        let cols: &[EvidenceGene2PhenotypeCol] = match self {
            Self::Score => &[EvidenceGene2PhenotypeCol::Score],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the disease/phenotype, functional
    /// consequence, allelic requirement, panel and confidence category columns of the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceGene2PhenotypeSearchField")]
    pub enum SearchField => EvidenceGene2PhenotypeCol {
        DiseaseName,
        DiseaseFromSource,
        VariantFunctionalConsequenceLabel,
        AllelicRequirements,
        StudyId,
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
    evidences: Paged<EvidenceGene2Phenotype>,
}

#[derive(From)]
pub struct EvidenceGene2PhenotypeLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceGene2PhenotypeLoader {
    type Value = Paged<EvidenceGene2Phenotype>;
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
pub async fn load_evidence_gene2phenotype(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceGene2Phenotype>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceGene2PhenotypeLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
