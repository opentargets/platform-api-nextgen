//! Cancer biomarkers evidence: genomic biomarkers of drug response in cancer, from the Cancer
//! Genome Interpreter.
//!
//! The table has no sort, so rows always come by score, highest first. It has a search on
//! joined text, so it isn't index-paged.

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
    entity::evidence::LabelledUri,
    query::{
        paginate::Paged,
        sort::SortDirection,
        sql::{Column, OrderBy, PageSpec, Table, ValueKind, build, column_subset, columns},
    },
};

const DATASOURCE: &str = "cancer_biomarkers";

// ---- model ----

/// A genetic variation biomarker.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(name = "EvidenceCancerBiomarkersGeneticVariation")]
pub struct GeneticVariation {
    /// Variant identifier.
    id: Option<String>,
    /// Name of the variant biomarker, e.g. `NF1:D1644A`.
    name: String,
    /// Sequence Ontology identifier of the functional consequence.
    functional_consequence_id: Option<String>,
    /// Sequence Ontology label of the functional consequence, e.g. `somatic_variant`.
    functional_consequence_label: Option<String>,
}

/// A gene expression biomarker.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(name = "EvidenceCancerBiomarkersGeneExpression")]
pub struct GeneExpression {
    /// Raw gene expression annotation from the source, e.g. `ESR1:over`.
    name: String,
    /// Gene Ontology identifier of the expression process.
    id: String,
    /// Gene Ontology label of the expression process.
    label: Option<String>,
}

/// A cancer biomarkers evidence, as shown in the evidence page's cancer biomarkers table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/cancer_biomarkers.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCancerBiomarkers {
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
    /// Name of the biomarker, e.g. `BRCA1 oncogenic mutation`.
    biomarker_name: Option<String>,
    /// Genetic variation biomarkers.
    biomarker_genetic_variations: Vec<GeneticVariation>,
    /// Gene expression biomarkers.
    biomarker_gene_expressions: Vec<GeneExpression>,
    /// Identifier of the drug. Null if the source's drug isn't mapped.
    drug_id: Option<String>,
    /// Name of the drug. Null if the drug isn't found.
    drug_name: Option<String>,
    /// Reported drug: the drug label from the original source.
    drug_from_source: Option<String>,
    /// Identifier of the drug response term (a disease or Gene Ontology term).
    drug_response_id: Option<String>,
    /// Name of the drug response, e.g. `drug resistance`. Null if the term isn't found.
    drug_response_name: Option<String>,
    /// Source: the level of evidence, e.g. `FDA guidelines` or `Early trials`.
    confidence: Option<String>,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
    /// Links to external resources, e.g. conference abstracts.
    urls: Vec<LabelledUri>,
}

impl Table for EvidenceCancerBiomarkers {
    type Col = EvidenceCancerBiomarkersCol;
    const NAME: &'static str = include_str!("../../sql/evidence/cancer_biomarkers.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceCancerBiomarkersCol {
    Id               => "id"               : Str,
    Score            => "score"            : F64,
    DiseaseName      => "diseaseName"      : Str,
    BiomarkerName    => "biomarkerName"    : Str,
    DrugName         => "drugName"         : Str,
    DrugFromSource   => "drugFromSource"   : Str,
    DrugResponseName => "drugResponseName" : Str,
    Confidence       => "confidence"       : Str,
});

// ---- query params ----

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    scope: EvidenceScope,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(scope: EvidenceScope, search: Option<Search>, index: u32, size: u32) -> Self {
        Self {
            scope,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> Result<PageSpec<EvidenceCancerBiomarkers>> {
        use EvidenceCancerBiomarkersCol as C;
        let mut filters = vec![self.scope.candidate_filter(C::Id, DATASOURCE)?];
        filters.extend(self.search.as_ref().map(Search::to_filter).transpose()?);
        Ok(PageSpec {
            filters,
            // Not exposed: always by score, highest first. `TIE_BREAKER` follows.
            order_by: OrderBy::each(&[C::Score], SortDirection::Descending),
            offset: u64::from(self.index) * u64::from(self.size),
            limit: u64::from(self.size),
        })
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the text shown in the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceCancerBiomarkersSearchField")]
    pub enum SearchField => EvidenceCancerBiomarkersCol {
        DiseaseName,
        BiomarkerName,
        DrugName,
        DrugFromSource,
        DrugResponseName,
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
    evidences: Paged<EvidenceCancerBiomarkers>,
}

#[derive(From)]
pub struct EvidenceCancerBiomarkersLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceCancerBiomarkersLoader {
    type Value = Paged<EvidenceCancerBiomarkers>;
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
pub async fn load_evidence_cancer_biomarkers(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceCancerBiomarkers>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceCancerBiomarkersLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
