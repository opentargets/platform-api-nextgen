use std::{char::CharTryFromError, cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;
use serde_repr::Deserialize_repr;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        // search::Searchable,
        // sort::{Sort, SortKey},
    },
};

// ---- models ----
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Enum, Deserialize_repr)]
#[repr(i8)]
#[graphql(rename_items = "lowercase")]
pub enum Chromosome {
    Chr1 = 1,
    Chr2 = 2,
    Chr3 = 3,
    Chr4 = 4,
    Chr5 = 5,
    Chr6 = 6,
    Chr7 = 7,
    Chr8 = 8,
    Chr9 = 9,
    Chr10 = 10,
    Chr11 = 11,
    Chr12 = 12,
    Chr13 = 13,
    Chr14 = 14,
    Chr15 = 15,
    Chr16 = 16,
    Chr17 = 17,
    Chr18 = 18,
    Chr19 = 19,
    Chr20 = 20,
    Chr21 = 21,
    Chr22 = 22,
    ChrX = 23,
    ChrY = 24,
    ChrMT = 25,
}


#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct VariantEffect {
    method: Option<String>,
    assessment: Option<String>,
    score: Option<f64>,
    assessment_flag: Option<String>,
    target_id: Option<String>,
    normalised_score: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptConsequence {
    variant_functional_consequence_ids: Vec<String>,
    amino_acid_change: Option<String>,
    uniprot_accessions: Vec<String>,
    is_ensembl_canonical: bool,
    codons: Option<String>,
    distance_from_footprint: i32,
    distance_from_tss: i32,
    target_id: Option<String>,
    impact: Option<String>,
    transcript_id: Option<String>,
    loftee_prediction: Option<String>,
    sift_prediction: Option<f64>,
    polyphen_prediction: Option<f64>,
    transcript_index: u32,
    consequence_score: f64,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DbXref {
    id: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct AlleleFrequency {
    population_name: Option<String>,
    allele_frequency: Option<f64>,
}

#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    variant_id: String,
    chromosome: Chromosome,
    position: u32,
    reference_allele: String,
    alternate_allele: String,
    variant_effect: Vec<VariantEffect>,
    most_severe_consequence_id: String,
    transcript_consequences: Vec<TranscriptConsequence>,
    rs_ids: Vec<String>,
    db_xrefs: Vec<DbXref>,
    allele_frequencies: Vec<AlleleFrequency>,
    hgvs_id: Option<String>,
    variant_description: String,
}

// ---- query utilities ----

// impl Entity for Variant {
//     fn id(&self) -> &str { &self.variant_id }
// }

// ---- loaders ----

pub type VariantCache = Cache<String, Option<Variant>>;
static VARIANT_CACHE: LazyLock<VariantCache> = LazyLock::new(entity_cache);

pub struct VariantLoader {
    ch: ClickHouse,
}

impl VariantLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for VariantLoader {
    type Key = String;
    type Value = Variant;

    fn cache(&self) -> &VariantCache { &VARIANT_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.variant_id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM variants WHERE variantId IN ?")
            .bind(misses)
            .fetch_all::<Variant>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for VariantLoader {
    type Value = Variant;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[String],
    ) -> Result<HashMap<String, Variant>, async_graphql::Error> {
        self.load_cached(keys).await
    }
}

async fn load_variants(
    ctx: &Context<'_>,
    ids: &[String],
) -> async_graphql::Result<Vec<Variant>> {
    load_ordered(ctx.data_unchecked::<DataLoader<VariantLoader>>(), ids).await
}

// ---- resolvers ----
#[derive(Default)]
pub struct VariantQuery;

#[Object]
impl VariantQuery {
    /// Fetch variants by ID.
    async fn variants(
        &self,
        ctx: &Context<'_>,
        variant_ids: Vec<String>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Variant>> {
        let items = load_variants(ctx, &variant_ids).await?;
        Ok(items
            .query()
            .paginate(page))
    }

    async fn variant(
        &self,
        ctx: &Context<'_>,
        variant_id: String,
    ) -> async_graphql::Result<Option<Variant>> {
        ctx.data_unchecked::<DataLoader<VariantLoader>>()
            .load_one(variant_id)
            .await
    }
}
