use std::collections::HashMap;

use async_graphql::{
    Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::variant::Chromosome,
    query::{
        paginate::{Page, Paged},
        sort::{Sort, SortDirection},
        sql::{Column, Filter, Op, OrderBy, PageSpec, Table, Value, ValueKind, build, columns},
    },
};

const MAX_PAGE_SIZE: u32 = 2000;

// ---- models ----

#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GwasCredibleSetsByVariant {
    variant_id: String,
    study_locus_id: String,
    posterior_probability: f64,
    locus_size: u64,
    p_value_mantissa: f64,
    p_value_exponent: i32,
    beta: Option<f64>,
    finemapping_method: String,
    confidence: String,
    lead_variant_id: String,
    lead_chromosome: Chromosome,
    lead_position: u32,
    lead_reference_allele: String,
    lead_alternate_allele: String,
    study_id: String,
    trait_from_source: String,
    disease_ids: Vec<String>,
    disease_names: Vec<String>,
    disease_therapeutic_area_ids: Vec<Vec<String>>,
    disease_therapeutic_area_names: Vec<Vec<String>>,
    top_l2_g_gene_id: String,
    top_l2_g_approved_symbol: String,
    top_l2_g_score: f64,
}
impl Table for GwasCredibleSetsByVariant {
    type Col = CredibleSetCol;
    const NAME: &'static str = "gwas_credible_sets_by_variant";
    const TIE_BREAKER: &'static str = "studyLocusId";
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct QueryParams {
    variant_id: String,
    sort: Vec<Sort<GwasCredibleSetsByVariantSortField>>,
    index: u32,
    size: u32,
}
impl QueryParams {
    #[must_use]
    pub fn new(
        variant_id: String,
        sort: Vec<Sort<GwasCredibleSetsByVariantSortField>>,
        index: u32,
        size: u32,
    ) -> Self {
        Self {
            variant_id,
            sort,
            index,
            size,
        }
    }
    #[allow(clippy::missing_errors_doc)]
    pub fn validate(&self) -> Result<(), async_graphql::Error> {
        if self.size == 0 || self.size > MAX_PAGE_SIZE {
            return Err(async_graphql::Error::new(format!(
                "size must be 1..={MAX_PAGE_SIZE}"
            )));
        }
        Ok(())
    }
    fn to_spec(&self) -> Result<PageSpec<GwasCredibleSetsByVariant>, async_graphql::Error> {
        self.validate()?;
        Ok(PageSpec {
            filters: vec![
                Filter::new(
                    CredibleSetCol::VariantId,
                    Op::Eq,
                    Value::Str(self.variant_id.clone()),
                )
                .map_err(async_graphql::Error::new)?,
            ],
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

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GwasCredibleSetsByVariantRow {
    key: usize,
    gwas_credible_sets: Paged<GwasCredibleSetsByVariant>,
}

// Columns for the query

columns!(CredibleSetCol {
    VariantId      => "variantId"       : Str,
    PValueMantissa => "pValueMantissa"  : F64,
    PValueExponent => "pValueExponent"  : I64,
    TopL2gScore    => "topL2GScore"     : F64,
});

// sort

/// Fields available for sorting.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Hash)]
pub enum GwasCredibleSetsByVariantSortField {
    TopL2gScore,
    PValue,
    // VariantId,
}
impl GwasCredibleSetsByVariantSortField {
    fn default_sort() -> Vec<Sort<Self>> { DEFAULT_SORT.to_vec() }
    fn as_order_by(self, direction: SortDirection) -> Vec<OrderBy<CredibleSetCol>> {
        match self {
            GwasCredibleSetsByVariantSortField::TopL2gScore => vec![OrderBy {
                col: CredibleSetCol::TopL2gScore,
                dir: direction,
            }],
            GwasCredibleSetsByVariantSortField::PValue => vec![
                OrderBy {
                    col: CredibleSetCol::PValueExponent,
                    dir: direction,
                },
                OrderBy {
                    col: CredibleSetCol::PValueMantissa,
                    dir: direction,
                },
            ],
        }
    }
}

const DEFAULT_SORT: [Sort<GwasCredibleSetsByVariantSortField>; 1] = [Sort {
    key: GwasCredibleSetsByVariantSortField::TopL2gScore,
    direction: SortDirection::Descending,
}];

// ---- loaders ----

#[derive(From)]
pub struct GwasCredibleSetsByVariantLoader {
    ch: ClickHouse,
}

impl GwasCredibleSetsByVariantLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for GwasCredibleSetsByVariantLoader {
    type Value = Paged<GwasCredibleSetsByVariant>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[QueryParams],
    ) -> Result<HashMap<QueryParams, Self::Value>, Self::Error> {
        let rendered = keys
            .iter()
            .map(|k| k.to_spec().map(|spec| spec.render()))
            .collect::<Result<Vec<_>, _>>()?;

        let rows = build(self.ch.client(), &rendered)
            .fetch_all::<GwasCredibleSetsByVariantRow>()
            .await?;

        Ok(rows
            .iter()
            .map(|res| (keys[res.key].clone(), res.gwas_credible_sets.clone()))
            .collect())
    }
}

pub async fn load_gwas_credible_sets_by_variant(
    ctx: &async_graphql::Context<'_>,
    key: QueryParams,
) -> async_graphql::Result<Paged<GwasCredibleSetsByVariant>> {
    Ok(ctx
        .data_unchecked::<DataLoader<GwasCredibleSetsByVariantLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}

// ---- resolvers ----
#[derive(Default)]
pub struct GwasCredibleSetsByVariantQuery;

#[Object]
impl GwasCredibleSetsByVariantQuery {
    async fn gwas_credible_sets_by_variant(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "variant ID to fetch credible sets for.")] variant_id: String,
        #[graphql(
            default_with = "GwasCredibleSetsByVariantSortField::default_sort()",
            desc = "List of sort statements."
        )]
        sort: Vec<Sort<GwasCredibleSetsByVariantSortField>>,
        #[graphql(default, desc = "Pagination for the credible sets.")] page: Page,
    ) -> async_graphql::Result<Paged<GwasCredibleSetsByVariant>> {
        let query_params = QueryParams::new(variant_id, sort, page.index, page.size);
        load_gwas_credible_sets_by_variant(ctx, query_params).await
    }
}
