//! Clinical precedence evidence: drugs that target the target and are in clinical development
//! or approved for the disease.

use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, Enum, Error, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use strum::EnumIter;

use super::EvidenceScope;
use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        disease::{Disease, load_disease},
        drug::{Drug, load_drug},
        target::{Target, load_target},
    },
    query::{
        paginate::Paged,
        sort::{Sort, SortDirection},
        sql::{Column, OrderBy, PageSpec, Table, ValueKind, build, column_subset, columns},
    },
};

const DATASOURCE: &str = "clinical_precedence";

// ---- model ----

/// A clinical precedence evidence, as shown in the evidence page's clinical precedence table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/clinical_precedence.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct EvidenceClinicalPrecedence {
    /// Identifier of the disease/target evidence.
    id: String,
    /// Score of the evidence reflecting the strength of the disease/target relationship.
    score: f64,
    #[graphql(skip)]
    disease_id: String,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    disease_name: String,
    #[graphql(skip)]
    target_id: String,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    target_approved_symbol: String,
    #[graphql(skip)]
    drug_id: Option<String>,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    drug_name: Option<String>,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    drug_target_ids: Vec<String>,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    drug_target_symbols: Vec<String>,
    /// Gain or loss of function effect of the evidence on the target resulting from genetic
    /// variants, pharmacological modulation, or other perturbations.
    direction_on_target: Option<String>,
    /// Predicted direction of effect on the trait.
    direction_on_trait: Option<String>,
    /// Clinical stage of the drug-disease pair.
    clinical_stage: Option<String>,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    clinical_stage_rank: u64,
    /// Identifier of the clinical report.
    clinical_report_id: Option<String>,
    /// Start date of study in a YYYY-MM-DD format.
    study_start_date: Option<String>,
}

#[ComplexObject]
impl EvidenceClinicalPrecedence {
    /// Disease for which the target is associated in this evidence.
    async fn disease(&self, ctx: &Context<'_>) -> Result<Option<Disease>> {
        load_disease(ctx, self.disease_id.clone()).await
    }

    /// Target for which the disease is associated in this evidence.
    async fn target(&self, ctx: &Context<'_>) -> Result<Option<Target>> {
        load_target(ctx, self.target_id.clone()).await
    }

    /// Drug or clinical candidate targeting the target and studied/approved for the disease.
    async fn drug(&self, ctx: &Context<'_>) -> Result<Option<Drug>> {
        match &self.drug_id {
            Some(id) => load_drug(ctx, id.clone()).await,
            None => Ok(None),
        }
    }
}

impl Table for EvidenceClinicalPrecedence {
    type Col = EvidenceClinicalPrecedenceCol;
    const NAME: &'static str = include_str!("../../sql/evidence/clinical_precedence.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceClinicalPrecedenceCol {
    Id                   => "id"                   : Str,
    Score                => "score"                : F64,
    DiseaseId            => "diseaseId"            : Str,
    DiseaseName          => "diseaseName"          : Str,
    TargetId             => "targetId"             : Str,
    TargetApprovedSymbol => "targetApprovedSymbol" : Str,
    DrugId               => "drugId"               : Str,
    DrugName             => "drugName"             : Str,
    // Approved symbols of all the drug's mechanism-of-action targets.
    DrugTargetSymbols    => "drugTargetSymbols"    : StrArray,
    DirectionOnTarget    => "directionOnTarget"    : Str,
    DirectionOnTrait     => "directionOnTrait"     : Str,
    ClinicalStage        => "clinicalStage"        : Str,
    // Position of `clinicalStage` in the stage order, approval highest.
    ClinicalStageRank    => "clinicalStageRank"    : U64,
    ClinicalReportId     => "clinicalReportId"     : Str,
    StudyStartDate       => "studyStartDate"       : Str,
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceClinicalPrecedence>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceClinicalPrecedenceCol::Id, DATASOURCE)?,
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
#[graphql(name = "EvidenceClinicalPrecedenceSortField")]
pub enum SortField {
    /// Evidence score.
    Score,
    /// Clinical stage, from unknown up to approval.
    ClinicalStage,
    /// Study start date.
    StudyStartDate,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::Score,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceClinicalPrecedenceCol>> {
        use EvidenceClinicalPrecedenceCol as C;
        let cols: &[C] = match self {
            Self::Score => &[C::Score],
            Self::ClinicalStage => &[C::ClinicalStageRank],
            Self::StudyStartDate => &[C::StudyStartDate],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the text shown in the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceClinicalPrecedenceSearchField")]
    pub enum SearchField => EvidenceClinicalPrecedenceCol {
        DiseaseName,
        TargetApprovedSymbol,
        DrugTargetSymbols,
        DrugName,
        DirectionOnTarget,
        DirectionOnTrait,
        ClinicalStage,
        ClinicalReportId,
        StudyStartDate,
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
    evidences: Paged<EvidenceClinicalPrecedence>,
}

#[derive(From)]
pub struct EvidenceClinicalPrecedenceLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceClinicalPrecedenceLoader {
    type Value = Paged<EvidenceClinicalPrecedence>;
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
pub async fn load_evidence_clinical_precedence(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceClinicalPrecedence>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceClinicalPrecedenceLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
