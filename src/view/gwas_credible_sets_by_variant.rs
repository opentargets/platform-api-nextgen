use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use moka::future::Cache;
use serde::Deserialize;
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::variant::Chromosome,
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        sort::{Nulls, Sort, SortDirection, SortKey, Term},
    },
};

// ---- models ----

#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GwasCredibleSetByVariant {
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

#[derive(Debug, Clone, Deserialize, SimpleObject, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct QueryParams {
    variant_id: String,
    index: u32,
    size: u32,
}

impl QueryParams {
    #[must_use]
    pub fn new(variant_id: String, index: u32, size: u32) -> Self {
        Self {
            variant_id,
            index,
            size,
        }
    }
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GwasCredibleSetByVariantRow {
    key: usize,
    gwas_credible_sets: Paged<GwasCredibleSetByVariant>,
}

// ---- loaders ----

#[derive(From)]
pub struct GwasCredibleSetByVariantLoader {
    ch: ClickHouse,
}

impl GwasCredibleSetByVariantLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<QueryParams> for GwasCredibleSetByVariantLoader {
    type Value = Paged<GwasCredibleSetByVariant>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[QueryParams],
    ) -> Result<HashMap<QueryParams, Self::Value>, Self::Error> {
        let base_query = "
            WITH
            ? AS q_variant_id,
            ? AS q_offset,
            ? AS q_limit,
            CAST(? AS UInt64) AS query_id,
            filtered AS
            (
                SELECT *
                FROM gwas_credible_sets_by_variant
                WHERE variantId = q_variant_id
            ),
            (
                SELECT count()
                FROM filtered
            ) as count,
            paged AS
            (
                SELECT *
                FROM filtered
                ORDER BY topL2GScore DESC
                LIMIT q_limit OFFSET q_offset
            )
            SELECT
                query_id,
                tuple(
                        CAST(count AS UInt64),
                        groupArray(tuple(*)) as rows
                    )
            FROM paged
            GROUP BY query_id
            ";
        let queries: Vec<String> = keys.iter().map(|_| base_query.to_string()).collect();
        let full_query = keys.iter().enumerate().fold(
            self.ch.query(&queries.join(" UNION ALL ")),
            |acc: clickhouse::query::Query, (i, key)| {
                acc.bind(key.variant_id.clone())
                    .bind(key.index * key.size)
                    .bind(key.size)
                    .bind(i as u64)
            },
        );
        let result = full_query
            .fetch_all::<GwasCredibleSetByVariantRow>()
            .await?;
        let results_with_keys = result
            .iter()
            .map(|res| (keys[res.key].clone(), res.gwas_credible_sets.clone()))
            .collect();

        Ok(results_with_keys)
    }
}

pub async fn load_gwas_credible_sets_by_variant(
    ctx: &async_graphql::Context<'_>,
    key: QueryParams,
) -> async_graphql::Result<Paged<GwasCredibleSetByVariant>> {
    Ok(ctx
        .data_unchecked::<DataLoader<GwasCredibleSetByVariantLoader>>()
        .load_one(key)
        .await?
        .unwrap_or_default())
}

// ---- resolvers ----
#[derive(Default)]
pub struct GwasCredibleSetByVariantQuery;

#[Object]
impl GwasCredibleSetByVariantQuery {
    async fn gwas_credible_sets_by_variant(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "variant ID to fetch credible sets for.")] variant_id: String,
        #[graphql(default, desc = "Pagination for the credible sets.")] page: Page,
    ) -> async_graphql::Result<Paged<GwasCredibleSetByVariant>> {
        let query_params = QueryParams::new(variant_id, page.index, page.size);
        load_gwas_credible_sets_by_variant(ctx, query_params).await
    }
}
