// use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};
//
// use async_graphql::{
//     ComplexObject, Context, Enum, Object, SimpleObject,
//     dataloader::{DataLoader, Loader},
// };
// use clickhouse::Row;
// use derive_more::From;
// use moka::future::Cache;
// use serde::Deserialize;
// use serde_repr::{Deserialize_repr, Serialize_repr};
//
// use crate::{
//     datasource::clickhouse::ClickHouse,
//     entity::variant::Chromosome,
//     query::{
//         Entity, QueryExt,
//         cache::{CachedLoader, entity_cache},
//         load_ordered,
//         paginate::{Page, Paged},
//         sort::{Nulls, Sort, SortDirection, SortKey, Term},
//     },
// };
//
// // ---- models ----
//
// #[allow(clippy::struct_field_names)]
// #[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
// #[serde(rename_all = "camelCase")]
// #[graphql(complex)]
// pub struct GwasCredibleSetByVariant {
//     variant_id: String,
//     study_locus_id: String,
//     posterior_probability: f64,
//     locus_size: u64,
//     p_value_mantissa: f64,
//     p_value_exponent: i32,
//     beta: f64,
//     finemapping_method: String,
//     confidence: String,
//     lead_variant_id: String,
//     lead_chromosome: Chromosome,
//     lead_position: u32,
//     lead_reference_allele: String,
//     lead_alternate_allele: String,
//     study_id: String,
//     trait_from_source: String,
//     disease_ids: Vec<String>,
//     disease_names: Vec<String>,
//     disease_therapeutic_area_ids: Vec<String>,
//     disease_therapeutic_area_names: Vec<String>,
//     top_l2_g_gene_id: String,
//     top_l2_g_approved_symbol: String,
//     top_l2_g_score: f64,
// }
//
// #[derive(Debug, Clone, Deserialize, SimpleObject, Eq, PartialEq, Hash)]
// #[serde(rename_all = "camelCase")]
// pub struct QueryParams {
//     variant_id: String,
// }
//
// // ---- loaders ----
//
// #[derive(From)]
// pub struct GwasCredibleSetByVariantLoader {
//     ch: ClickHouse,
// }
//
// impl GwasCredibleSetByVariantLoader {
//     #[must_use]
//     pub fn new(ch: ClickHouse) -> Self { Self { ch } }
// }
//
// impl Loader<QueryParams> for GwasCredibleSetByVariantLoader {
//     type Value = Paged<GwasCredibleSetByVariant>;
//     type Error = async_graphql::Error;
//
//     async fn load(&self, keys: &[Key]) -> Result<HashMap<Key, Self::Value>, Self::Error> {
//         let base_query = "
//             SELECT *
//             FROM gwas_credible_sets_by_variant
//             WHERE variant_id = ?
//             ";
//
//
// }
