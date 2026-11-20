//! IMPC evidence: mouse models with a disrupted orthologue of the target whose phenotypes match
//! the disease.
//!
//! The table has no sort or search, so the view counts and pages straight from
//! `evidence_by_disease_and_target` (always by score, highest first) and reads full `evidence`
//! rows for the page only.

use std::collections::HashMap;

use async_graphql::{
    Context, Error, Result, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use super::EvidenceScope;
use crate::{
    datasource::clickhouse::ClickHouse,
    entity::evidence::LabelledElement,
    query::{
        paginate::Paged,
        sort::SortDirection,
        sql::{Column, OrderBy, PageSpec, Table, ValueKind, build, columns},
    },
};

const DATASOURCE: &str = "impc";

// ---- model ----

/// An IMPC evidence, as shown in the evidence page's IMPC table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/impc.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceImpc {
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
    /// Symbol of the target orthologue in the animal model.
    target_in_model: Option<String>,
    /// Gain or loss of function effect of the evidence on the target.
    direction_on_target: Option<String>,
    /// Predicted direction of effect on the trait.
    direction_on_trait: Option<String>,
    /// Identifier of the animal model (e.g. an MGI model ID).
    biological_model_id: Option<String>,
    /// Genetic background of the animal model (e.g. a mouse strain).
    biological_model_genetic_background: Option<String>,
    /// Allelic composition of the animal model.
    biological_model_allelic_composition: Option<String>,
    /// Phenotypes observed in the animal model.
    disease_model_associated_model_phenotypes: Vec<LabelledElement>,
    /// Human phenotypes of the disease that match the animal model's phenotypes.
    disease_model_associated_human_phenotypes: Vec<LabelledElement>,
}

impl Table for EvidenceImpc {
    type Col = EvidenceImpcCol;
    const NAME: &'static str = "evidence_by_disease_and_target";
    const TIE_BREAKER: &'static str = "id";
    const ROW_SOURCE: Option<&'static str> = Some(include_str!("../../sql/evidence/impc.sql"));
}

// Columns of `evidence_by_disease_and_target`; the sort one is in the row source too.
columns!(EvidenceImpcCol {
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
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(scope: EvidenceScope, index: u32, size: u32) -> Self { Self { scope, index, size } }

    fn to_spec(&self) -> Result<PageSpec<EvidenceImpc>> {
        use EvidenceImpcCol as C;
        Ok(PageSpec {
            filters: self.scope.index_filters(
                C::DatasourceId,
                C::DiseaseId,
                C::TargetId,
                DATASOURCE,
            )?,
            // Not exposed: always by score, highest first. `TIE_BREAKER` follows.
            order_by: OrderBy::each(&[C::Score], SortDirection::Descending),
            offset: u64::from(self.index) * u64::from(self.size),
            limit: u64::from(self.size),
        })
    }
}

// ---- loader ----

/// One row of the batched query: the page of results for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRow {
    key: usize,
    evidences: Paged<EvidenceImpc>,
}

#[derive(From)]
pub struct EvidenceImpcLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceImpcLoader {
    type Value = Paged<EvidenceImpc>;
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
pub async fn load_evidence_impc(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceImpc>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceImpcLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
