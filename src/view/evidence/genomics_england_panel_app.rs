//! Genomics England `PanelApp` evidence: gene-disease relationships from expert-reviewed gene
//! panels.

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

const DATASOURCE: &str = "genomics_england";

// ---- model ----

/// A Genomics England `PanelApp` evidence, as shown in the evidence page's `PanelApp` table.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/evidence/genomics_england_panel_app.sql`. Update both together.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceGenomicsEnglandPanelApp {
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
    /// Approved symbol of the target, used to link to `PanelApp`. Empty if the target isn't found.
    target_approved_symbol: String,
    /// Allelic requirements of the gene-disease relationship.
    allelic_requirements: Vec<String>,
    /// Identifier of the Genomics England panel, used to link to `PanelApp`.
    panel_id: Option<String>,
    /// Name of the Genomics England panel.
    panel_name: Option<String>,
    /// `PanelApp` gene rating: green, amber or red.
    confidence: Option<String>,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    confidence_rank: u64,
}

impl Table for EvidenceGenomicsEnglandPanelApp {
    type Col = EvidenceGenomicsEnglandPanelAppCol;
    const NAME: &'static str = include_str!("../../sql/evidence/genomics_england_panel_app.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(EvidenceGenomicsEnglandPanelAppCol {
    Id                  => "id"                  : Str,
    Score               => "score"               : F64,
    DiseaseId           => "diseaseId"           : Str,
    DiseaseName         => "diseaseName"         : Str,
    TargetId            => "targetId"            : Str,
    TargetApprovedSymbol => "targetApprovedSymbol" : Str,
    AllelicRequirements => "allelicRequirements" : StrArray,
    PanelId             => "panelId"             : Str,
    PanelName           => "panelName"           : Str,
    Confidence          => "confidence"          : Str,
    // Position of `confidence` in the rating order, green highest.
    ConfidenceRank      => "confidenceRank"      : U64,
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

    fn to_spec(&self) -> Result<PageSpec<EvidenceGenomicsEnglandPanelApp>> {
        let mut filters = vec![
            self.scope
                .candidate_filter(EvidenceGenomicsEnglandPanelAppCol::Id, DATASOURCE)?,
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
#[graphql(name = "EvidenceGenomicsEnglandPanelAppSortField")]
pub enum SortField {
    /// Evidence score.
    Score,
    /// Gene rating, from red up to green.
    Confidence,
}

const DEFAULT_SORT: [Sort<SortField>; 1] = [Sort {
    key: SortField::Score,
    direction: SortDirection::Descending,
}];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<EvidenceGenomicsEnglandPanelAppCol>> {
        use EvidenceGenomicsEnglandPanelAppCol as C;
        let cols: &[C] = match self {
            Self::Confidence => &[C::ConfidenceRank],
            Self::Score => &[C::Score],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against: the text shown in the table.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "EvidenceGenomicsEnglandPanelAppSearchField")]
    pub enum SearchField => EvidenceGenomicsEnglandPanelAppCol {
        DiseaseName,
        TargetApprovedSymbol,
        AllelicRequirements,
        PanelName,
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
    evidences: Paged<EvidenceGenomicsEnglandPanelApp>,
}

#[derive(From)]
pub struct EvidenceGenomicsEnglandPanelAppLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for EvidenceGenomicsEnglandPanelAppLoader {
    type Value = Paged<EvidenceGenomicsEnglandPanelApp>;
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
pub async fn load_evidence_genomics_england_panel_app(
    ctx: &Context<'_>,
    key: QueryParams,
) -> Result<Paged<EvidenceGenomicsEnglandPanelApp>> {
    if key.scope.is_empty() {
        return Ok(Paged::default());
    }
    Ok(ctx
        .data_unchecked::<DataLoader<EvidenceGenomicsEnglandPanelAppLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}
