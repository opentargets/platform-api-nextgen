//! Europe PMC evidence: publications in which text mining found the target and the disease
//! together.
//!
//! The table has no sort or search, so the view counts and pages straight from
//! `evidence_by_disease_and_target` (always by score, highest first) and reads full rows from
//! `evidence` for the page only. Europe PMC has the biggest scopes of any datasource (177k rows
//! for diabetes + INS), which the generic shape can't count in time.

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
    entity::evidence::EvidenceTextMiningSentence,
    query::{
        paginate::Paged,
        sort::SortDirection,
        sql::{Column, OrderBy, PageSpec, Table, ValueKind, build, columns},
    },
};

const DATASOURCE: &str = "europepmc";

// ---- model ----

/// A Europe PMC evidence, as shown in the evidence page's Europe PMC table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/europe_pmc.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceEuropePmc {
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
    /// PubMed or preprint identifiers of the publication.
    literature: Vec<String>,
    /// PubMed Central identifiers of the publication.
    pmc_ids: Vec<String>,
    /// Year the publication was published.
    publication_year: Option<u16>,
    /// Sentences of the publication in which the target and the disease were found.
    text_mining_sentences: Vec<EvidenceTextMiningSentence>,
    /// Score from Europe PMC: the text-mining confidence of the target/disease co-occurrence.
    resource_score: Option<f64>,
}

impl Table for EvidenceEuropePmc {
    type Col = EvidenceEuropePmcCol;
    const NAME: &'static str = "evidence_by_disease_and_target";
    const TIE_BREAKER: &'static str = "id";
    const ROW_SOURCE: Option<&'static str> =
        Some(include_str!("../../sql/evidence/europe_pmc.sql"));
}

// Columns of `evidence_by_disease_and_target`; the sort one is in the row source too.
columns!(EvidenceEuropePmcCol {
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceEuropePmc>> {
        use EvidenceEuropePmcCol as C;
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
    evidences: Paged<EvidenceEuropePmc>,
}

#[derive(From)]
pub struct EvidenceEuropePmcLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceEuropePmcLoader {
    type Value = Paged<EvidenceEuropePmc>;
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
pub async fn load_evidence_europe_pmc(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceEuropePmc>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceEuropePmcLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
