use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, Enum, Error, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;
use strum::EnumIter;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        clinical_report::{ClinicalDiseaseListItem, ClinicalReport, load_clinical_reports},
        drug::{Drug, load_drug},
    },
    query::{
        paginate::Paged,
        sort::{Sort, SortDirection},
        sql::{
            Column, Filter, Op, OrderBy, PageSpec, Table, Value, ValueKind, build, column_subset,
            columns,
        },
    },
};

// ---- models ----

/// Target-drug associations derived from clinical reports, capturing the maximum clinical stage and
/// associated diseases for each target-drug pair.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/clinical_target.sql`. Update both together.
#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
#[graphql(complex, name = "ClinicalTargetFromTarget")]
pub struct ClinicalTarget {
    /// Hash between drugId and targetId.
    id: String,
    #[graphql(skip)]
    drug_id: String,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    target_id: String,
    /// Diseases associated with the clinical reports linked to the drug in question.
    diseases: Vec<ClinicalDiseaseListItem>,
    max_clinical_stage: String,
    #[graphql(skip)]
    clinical_report_ids: Vec<String>,
    /// Name of the drug. Null if the drug isn't found.
    drug_name: Option<String>,
    /// Number of clinical reports for the target-drug pair.
    clinical_report_count: u32,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    max_clinical_stage_rank: u64,
}

impl Table for ClinicalTarget {
    type Col = ClinicalTargetCol;
    const NAME: &'static str = include_str!("../sql/clinical_target.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(ClinicalTargetCol {
    TargetId             => "targetId"             : Str,
    DrugName             => "drugName"             : Str,
    ClinicalReportCount  => "clinicalReportCount"  : I64,
    MaxClinicalStageRank => "maxClinicalStageRank" : U64,
});

// ---- query params ----

/// Key of a page of a target's drug and clinical candidates.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    target_id: String,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        target_id: String,
        sort: Vec<Sort<SortField>>,
        search: Option<Search>,
        index: u32,
        size: u32,
    ) -> Self {
        Self {
            target_id,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> async_graphql::Result<PageSpec<ClinicalTarget>> {
        // On the sort-key prefix of `clinical_target`.
        let mut filters = vec![
            Filter::new(
                ClinicalTargetCol::TargetId,
                Op::Eq,
                Value::Str(self.target_id.clone()),
            )
            .map_err(Error::new)?,
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
#[graphql(name = "ClinicalTargetSortField")]
pub enum SortField {
    /// By stage, from unknown and withdrawn up to approved and phase IV.
    MaxClinicalStage,
    /// Number of clinical reports.
    ClinicalReportCount,
    /// Alphabetically by drug name.
    DrugName,
}

const DEFAULT_SORT: [Sort<SortField>; 2] = [
    Sort {
        key: SortField::MaxClinicalStage,
        direction: SortDirection::Descending,
    },
    Sort {
        key: SortField::ClinicalReportCount,
        direction: SortDirection::Descending,
    },
];

impl SortField {
    pub(crate) fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<ClinicalTargetCol>> {
        use ClinicalTargetCol as C;
        let cols: &[C] = match self {
            Self::MaxClinicalStage => &[C::MaxClinicalStageRank],
            Self::ClinicalReportCount => &[C::ClinicalReportCount],
            Self::DrugName => &[C::DrugName],
        };
        OrderBy::each(cols, dir)
    }
}

// ---- search ----

column_subset! {
    /// Text fields that `search` can match against.
    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Enum, EnumIter)]
    #[graphql(name = "ClinicalTargetSearchField")]
    pub enum SearchField => ClinicalTargetCol {
        DrugName,
    }
}

/// Case-insensitive text search over the `SearchField`s.
pub type Search = crate::query::search::Search<SearchField>;

// ---- loaders ----

/// One row of the batched query: the page of results for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRow {
    key: usize,
    candidates: Paged<ClinicalTarget>,
}

/// Pages of a target's drug and clinical candidates, sorted and searched in `ClickHouse`.
#[derive(From)]
pub struct ClinicalTargetLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for ClinicalTargetLoader {
    type Value = Paged<ClinicalTarget>;
    type Error = async_graphql::Error;

    #[tracing::instrument(skip_all, level = "debug", fields(n = keys.len()))]
    async fn load(
        &self,
        keys: &[QueryParams],
    ) -> Result<HashMap<QueryParams, Self::Value>, Self::Error> {
        let rendered = keys
            .iter()
            .map(|k| k.to_spec().map(|spec| spec.render()))
            .collect::<async_graphql::Result<Vec<_>>>()?;

        let rows = build(self.ch.client(), &rendered)
            .fetch_all::<BatchRow>()
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (keys[row.key].clone(), row.candidates))
            .collect())
    }
}

/// Loads one sorted, searched page of a target's drug and clinical candidates.
///
/// # Errors
/// Returns an error if the clinical targets could not be loaded.
pub async fn load_clinical_targets_from_target(
    ctx: &Context<'_>,
    key: QueryParams,
) -> async_graphql::Result<Paged<ClinicalTarget>> {
    Ok(ctx
        .data_unchecked::<DataLoader<ClinicalTargetLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}

#[ComplexObject]
impl ClinicalTarget {
    /// Drug or clinical candidate entity.
    async fn drug(&self, ctx: &Context<'_>) -> async_graphql::Result<Drug> {
        load_drug(ctx, self.drug_id.clone())
            .await?
            .ok_or_else(|| async_graphql::Error::new("drug not found"))
    }

    /// Reports related with the drug in question.
    async fn clinical_reports(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<ClinicalReport>> {
        load_clinical_reports(ctx, &self.clinical_report_ids).await
    }
}
