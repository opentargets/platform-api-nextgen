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
        clinical_report::{ClinicalReport, load_clinical_reports},
        disease::{Disease, load_disease},
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

/// A drug-disease pair as reported by clinical trial records.
///
/// Rows are decoded by position, so field order must match the column order of
/// `src/sql/clinical_indication.sql`. Update both together.
#[derive(Debug, Clone, Deserialize, Row, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct ClinicalIndication {
    /// Hash of drugId and diseaseId.
    id: String,
    #[graphql(skip)]
    drug_id: String,
    #[graphql(skip)]
    disease_id: String,
    /// Maximum Clinical Development Status for the association.
    max_clinical_stage: String,
    #[graphql(skip)]
    clinical_report_ids: Vec<String>,
    /// Name of the drug. Null if the drug isn't found.
    drug_name: Option<String>,
    /// Number of clinical reports for the drug-disease pair.
    clinical_report_count: u32,
    #[allow(dead_code, reason = "read only to keep the column order")]
    #[graphql(skip)]
    max_clinical_stage_rank: u64,
}

impl Table for ClinicalIndication {
    type Col = ClinicalIndicationCol;
    const NAME: &'static str = include_str!("../sql/clinical_indication.sql");
    const TIE_BREAKER: &'static str = "id";
}

columns!(ClinicalIndicationCol {
    DiseaseId            => "diseaseId"            : Str,
    DrugName             => "drugName"             : Str,
    ClinicalReportCount  => "clinicalReportCount"  : I64,
    MaxClinicalStageRank => "maxClinicalStageRank" : U64,
});

// ---- query params ----

/// Key of a page of a disease's clinical indications.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    disease_id: String,
    sort: Vec<Sort<SortField>>,
    search: Option<Search>,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(
        disease_id: String,
        sort: Vec<Sort<SortField>>,
        search: Option<Search>,
        index: u32,
        size: u32,
    ) -> Self {
        Self {
            disease_id,
            sort,
            search: search.and_then(Search::normalise),
            index,
            size,
        }
    }

    fn to_spec(&self) -> async_graphql::Result<PageSpec<ClinicalIndication>> {
        // On the sort-key prefix of `clinical_indication_disease`.
        let mut filters = vec![
            Filter::new(
                ClinicalIndicationCol::DiseaseId,
                Op::Eq,
                Value::Str(self.disease_id.clone()),
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
#[graphql(name = "ClinicalIndicationSortField")]
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

    fn as_order_by(self, dir: SortDirection) -> Vec<OrderBy<ClinicalIndicationCol>> {
        use ClinicalIndicationCol as C;
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
    #[graphql(name = "ClinicalIndicationSearchField")]
    pub enum SearchField => ClinicalIndicationCol {
        DrugName,
    }
}

/// Case-insensitive text search over the `SearchField`s.
pub type Search = crate::query::search::Search<SearchField>;

// ---- loaders ----

#[derive(From)]
pub struct ClinicalIndicationFromDrugLoader {
    ch: ClickHouse,
}

impl Loader<String> for ClinicalIndicationFromDrugLoader {
    type Value = Vec<ClinicalIndication>;
    type Error = async_graphql::Error;

    #[tracing::instrument(skip_all, level = "debug", fields(n = ids.len()))]
    async fn load(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Vec<ClinicalIndication>>, Self::Error> {
        // The view reads `clinical_indication_disease`, and the whole table is ~90k rows, so a
        // `drugId` filter on it is as fast as reading `clinical_indication_drug`.
        let sql = format!(
            "SELECT ?fields FROM {} WHERE drugId IN ?",
            <ClinicalIndication as Table>::NAME
        );
        let rows = self.ch.query(&sql).bind(ids).fetch_all().await?;
        Ok(group_by(rows, |row| &row.drug_id))
    }
}

/// Groups the rows by the given key. The table holds one row per drug-disease pair, while a
/// loader returns one entry per key, so the rows are collected into a list per key.
fn group_by(
    rows: Vec<ClinicalIndication>,
    key: fn(&ClinicalIndication) -> &String,
) -> HashMap<String, Vec<ClinicalIndication>> {
    rows.into_iter().fold(HashMap::new(), |mut acc, row| {
        acc.entry(key(&row).clone()).or_default().push(row);
        acc
    })
}

/// One row of the batched query: the page of results for `keys[key]`.
#[derive(Debug, Clone, Row, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchRow {
    key: usize,
    indications: Paged<ClinicalIndication>,
}

/// Pages of a disease's clinical indications, sorted and searched in `ClickHouse`.
#[derive(From)]
pub struct ClinicalIndicationFromDiseaseLoader {
    ch: ClickHouse,
}

impl Loader<QueryParams> for ClinicalIndicationFromDiseaseLoader {
    type Value = Paged<ClinicalIndication>;
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
            .map(|row| (keys[row.key].clone(), row.indications))
            .collect())
    }
}

/// Loads the clinical indications recorded for a single drug by its ChEMBL id.
///
/// # Returns
/// The drug's clinical indications, or an empty `Vec` if it has none.
/// # Errors
/// Returns an error if the clinical indications could not be loaded.
pub async fn load_clinical_indications_from_drug(
    ctx: &Context<'_>,
    chembl_id: String,
) -> async_graphql::Result<Vec<ClinicalIndication>> {
    Ok(ctx
        .data_unchecked::<DataLoader<ClinicalIndicationFromDrugLoader>>()
        .load_one(chembl_id)
        .await?
        .unwrap_or_default())
}

/// Loads one sorted, searched page of a disease's clinical indications.
///
/// # Errors
/// Returns an error if the clinical indications could not be loaded.
pub async fn load_clinical_indications_from_disease(
    ctx: &Context<'_>,
    key: QueryParams,
) -> async_graphql::Result<Paged<ClinicalIndication>> {
    Ok(ctx
        .data_unchecked::<DataLoader<ClinicalIndicationFromDiseaseLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}

#[ComplexObject]
impl ClinicalIndication {
    /// The molecule entity related to this clinical indication.
    async fn drug(&self, ctx: &Context<'_>) -> async_graphql::Result<Drug> {
        load_drug(ctx, self.drug_id.clone())
            .await?
            .ok_or_else(|| async_graphql::Error::new("drug not found"))
    }

    /// The disease entity related to this clinical indication.
    async fn disease(&self, ctx: &Context<'_>) -> async_graphql::Result<Disease> {
        load_disease(ctx, self.disease_id.clone())
            .await?
            .ok_or_else(|| async_graphql::Error::new("disease not found"))
    }

    async fn clinical_reports(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<ClinicalReport>> {
        load_clinical_reports(ctx, &self.clinical_report_ids).await
    }
}
