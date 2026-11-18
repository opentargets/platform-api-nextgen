//! `ClinVar` (EVA) evidence: germline variants in the target with a clinical significance for the
//! disease.
//!
//! The table only sorts by score and has no search, so the view counts and pages straight from
//! `evidence_by_disease_and_target` and reads full rows (`evidence` + `variants`) for the page
//! only. Big scopes (TTN + nervous system disorder, 72k rows) stay well under a second; the
//! generic shape read every candidate row twice and took 10s.

use std::collections::HashMap;

use async_graphql::{
    Context, Enum, Error, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use super::EvidenceScope;
use crate::{
    datasource::clickhouse::ClickHouse,
    entity::variant::Chromosome,
    query::{
        paginate::Paged,
        sort::{Sort, SortDirection},
        sql::{Column, OrderBy, PageSpec, Table, ValueKind, build, columns},
    },
};

const DATASOURCE: &str = "eva";

// ---- model ----

/// A `ClinVar` evidence, as shown in the evidence page's `ClinVar` table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/eva.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceEva {
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
    /// Identifier of the variant, as `chromosome_position_ref_alt`.
    variant_id: Option<String>,
    /// Chromosome of the variant. Null if the variant isn't found.
    variant_chromosome: Option<Chromosome>,
    /// Position of the variant (`GRCh38`). Null if the variant isn't found.
    variant_position: Option<u32>,
    /// Reference allele of the variant. Null if the variant isn't found.
    variant_reference_allele: Option<String>,
    /// Alternate allele of the variant. Null if the variant isn't found.
    variant_alternate_allele: Option<String>,
    /// HGVS identifier of the variant.
    variant_hgvs_id: Option<String>,
    /// dbSNP rsID of the variant.
    variant_rs_id: Option<String>,
    /// `ClinVar` record (RCV) identifier.
    study_id: Option<String>,
    /// Sequence Ontology identifier of the variant's functional consequence.
    variant_functional_consequence_id: Option<String>,
    /// Sequence Ontology label of the variant's functional consequence.
    variant_functional_consequence_label: Option<String>,
    /// Gain or loss of function effect of the evidence on the target.
    direction_on_target: Option<String>,
    /// Predicted direction of effect on the trait.
    direction_on_trait: Option<String>,
    /// Clinical significances of the variant.
    clinical_significances: Vec<String>,
    /// Inheritance patterns.
    allelic_requirements: Vec<String>,
    /// Origins of the variant allele.
    allele_origins: Vec<String>,
    /// `ClinVar` review status.
    confidence: Option<String>,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
    /// Phenotypes of the cohort, from the source.
    cohort_phenotypes: Vec<String>,
}

impl Table for EvidenceEva {
    type Col = EvidenceEvaCol;
    const NAME: &'static str = "evidence_by_disease_and_target";
    const TIE_BREAKER: &'static str = "id";
    const ROW_SOURCE: Option<&'static str> = Some(include_str!("../../sql/evidence/eva.sql"));
}

// Columns of `evidence_by_disease_and_target`; the sort ones are in the row source too.
columns!(EvidenceEvaCol {
    Id           => "id"           : Str,
    Score        => "score"        : F64,
    DatasourceId => "datasourceId" : Str,
    DiseaseId    => "diseaseId"    : Str,
    TargetId     => "targetId"     : Str,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    scope: EvidenceScope,
    sort: Vec<Sort<SortField>>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(scope: EvidenceScope, sort: Vec<Sort<SortField>>, index: u32, size: u32) -> Self {
        Self {
            scope,
            sort,
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<EvidenceEva>> {
        use EvidenceEvaCol as C;
        Ok(PageSpec {
            filters: self.scope.index_filters(
                C::DatasourceId,
                C::DiseaseId,
                C::TargetId,
                DATASOURCE,
            )?,
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
#[graphql(name = "EvidenceEvaSortField")]
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

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceEvaCol>> {
        let cols: &[EvidenceEvaCol] = match self {
            Self::Score => &[EvidenceEvaCol::Score],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- loader ----

/// One row of the batched query: the page of results for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRow {
    key: usize,
    evidences: Paged<EvidenceEva>,
}

#[derive(From)]
pub struct EvidenceEvaLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceEvaLoader {
    type Value = Paged<EvidenceEva>;
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
pub async fn load_evidence_eva(ctx: &Context<'_>, key: QueryParams) -> Result<Paged<EvidenceEva>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceEvaLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
