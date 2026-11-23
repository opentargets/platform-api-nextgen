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

const DATASOURCE: &str = "gene_burden";

// ---- model ----

/// A gene burden evidence, as shown in the evidence page's gene burden table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/gene_burden.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceGeneBurden {
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
    /// Release version of the source data.
    release_version: Option<String>,
    /// URLs of linked external resources.
    urls: Vec<String>,
    /// Gain or loss of function effect of the evidence on the target.
    direction_on_target: Option<DirectionOnTarget>,
    /// Predicted direction of effect on the trait.
    direction_on_trait: Option<DirectionOnTrait>,
    /// Inheritance patterns.
    allelic_requirements: Vec<String>,
    /// Identifier of the study generating the data.
    study_id: Option<String>,
    /// Genetic origin of the population.
    ancestry: Option<String>,
    /// Identifier of the ancestry in the HANCESTRO ontology [bioregistry:hancestro].
    ancestry_id: Option<String>,
    /// Score provided by the datasource indicating the strength of the association.
    resource_score: Option<f64>,
    /// Identifier of the studied cohort.
    cohort_id: Option<String>,
    /// Identifier of the project that generated the data.
    project_id: Option<String>,
    /// Statistical method (model) used to calculate the association.
    statistical_method: Option<String>,
    /// Overview of the statistical method used to calculate the association.
    statistical_method_overview: Option<String>,
    /// Number of cases in the case-control study.
    study_cases: Option<u32>,
    /// Number of cases that carry at least one allele of the qualifying variant.
    study_cases_with_qualifying_variants: Option<u32>,
    /// Sample size of the study.
    study_sample_size: Option<u32>,
    /// Size of effect captured as odds ratio.
    odds_ratio: Option<f64>,
    /// Lower value of the confidence interval for the odds ratio.
    odds_ratio_confidence_interval_lower: Option<f64>,
    /// Upper value of the confidence interval for the odds ratio.
    odds_ratio_confidence_interval_upper: Option<f64>,
    /// Effect size of numeric traits.
    beta: Option<f64>,
    /// Lower value of the confidence interval for beta.
    beta_confidence_interval_lower: Option<f64>,
    /// Upper value of the confidence interval for beta.
    beta_confidence_interval_upper: Option<f64>,
    /// Mantissa of the p-value.
    p_value_mantissa: Option<f64>,
    /// Exponent of the p-value.
    p_value_exponent: Option<i32>,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
}

impl Table for EvidenceGeneBurden {
    type Col = EvidenceGeneBurdenCol;
    const NAME: &'static str = include_str!("../../sql/evidence/gene_burden.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceGeneBurdenCol {
    Id                               => "id"                               : Str,
    Score                            => "score"                            : F64,
    DiseaseName                      => "diseaseName"                      : Str,
    DiseaseFromSource                => "diseaseFromSource"                : Str,
    AllelicRequirements              => "allelicRequirements"              : StrArray,
    Ancestry                         => "ancestry"                         : Str,
    AncestryId                       => "ancestryId"                       : Str,
    CohortId                         => "cohortId"                         : Str,
    ProjectId                        => "projectId"                        : Str,
    StatisticalMethod                => "statisticalMethod"                : Str,
    StudyCases                       => "studyCases"                       : U64,
    StudyCasesWithQualifyingVariants => "studyCasesWithQualifyingVariants" : U64,
    StudySampleSize                  => "studySampleSize"                  : U64,
    OddsRatio                        => "oddsRatio"                        : F64,
    Beta                             => "beta"                             : F64,
    PValueMantissa                   => "pValueMantissa"                   : F64,
    PValueExponent                   => "pValueExponent"                   : I64,
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceGeneBurden>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceGeneBurdenCol::Id, DATASOURCE)?,
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
#[graphql(name = "EvidenceGeneBurdenSortField")]
pub enum SortField {
    /// Evidence score.
    Score,
    /// Number of cases carrying a qualifying variant.
    CasesWithQualifyingVariants,
    /// Number of cases.
    Cases,
    /// Sample size of the study.
    SampleSize,
    /// Odds ratio.
    OddsRatio,
    /// Effect size of numeric traits.
    Beta,
    /// P-value.
    PValue,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::Score,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceGeneBurdenCol>> {
        use EvidenceGeneBurdenCol as C;
        let cols: &[C] = match self {
            Self::Score => &[C::Score],
            Self::CasesWithQualifyingVariants => &[C::StudyCasesWithQualifyingVariants],
            Self::Cases => &[C::StudyCases],
            Self::SampleSize => &[C::StudySampleSize],
            Self::OddsRatio => &[C::OddsRatio],
            Self::Beta => &[C::Beta],
            Self::PValue => &[C::PValueExponent, C::PValueMantissa],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the disease/phenotype, cohort/project,
    /// ancestry, model and allelic requirement columns of the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceGeneBurdenSearchField")]
    pub enum SearchField => EvidenceGeneBurdenCol {
        DiseaseName,
        DiseaseFromSource,
        CohortId,
        ProjectId,
        Ancestry,
        AncestryId,
        StatisticalMethod,
        AllelicRequirements,
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
    evidences: Paged<EvidenceGeneBurden>,
}

#[derive(From)]
pub struct EvidenceGeneBurdenLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceGeneBurdenLoader {
    type Value = Paged<EvidenceGeneBurden>;
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
pub async fn load_evidence_gene_burden(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceGeneBurden>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceGeneBurdenLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
