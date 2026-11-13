use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, InputObject, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use moka::future::Cache;
use serde::Deserialize;
use serde_repr::Deserialize_repr;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{colocalisation, l2g_predictions, locus, study, variant},
    query::{
        QueryExt,
        cache::{CachedLoader, entity_cache},
        filter::Filter,
        load_ordered,
        paginate::{Page, Paged, page_complexity},
    },
};

// --- models ---

/// Fine-mapping method used to derive a credible set.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Enum, Deserialize_repr)]
#[repr(i8)]
pub enum FineMappingMethod {
    /// Probabilistic Identification of Causal SNPs, from summary statistics or a reported top hit.
    Pics = 1,
    /// Sum of Single Effects regression (`SuSiE`).
    Susie = 2,
    /// `SuSiE` with an infinitesimal effects component.
    SusieInf = 3,
}

/// 95% credible sets for GWAS and molQTL studies. Credible sets include all variants in the
/// credible set (locus) as well as the fine-mapping method and derived statistics.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct CredibleSet {
    /// Identifier of the credible set.
    credible_set_id: String,
    /// Identifier of the GWAS or molQTL study in which the credible set was identified.
    study_id: String,
    /// Descriptor for whether the credible set is derived from GWAS or molecular QTL.
    study_type: study::StudyType,
    /// Chromosome which the credible set is located.
    chromosome: variant::Chromosome,
    /// Identifier of the lead variant of the credible set.
    lead_variant_id: String,
    /// Position of the lead variant for the credible set (`GRCh38`).
    lead_position: u32,
    /// Mantissa value of the lead variant P-value.
    lead_p_value_mantissa: Option<f32>,
    /// Exponent value of the lead variant P-value.
    lead_p_value_exponent: Option<i32>,
    /// Beta coefficient of the lead variant, as reported by the source.
    lead_beta: Option<f64>,
    /// Standard error of the lead variant, as reported by the source.
    lead_standard_error: Option<f64>,
    /// Effect allele frequency of the lead variant.
    lead_effect_allele_frequency: Option<f32>,
    /// Sample size for the lead variant.
    lead_sample_size: Option<u32>,
    /// Start position of the region that was fine-mapped for this credible set.
    locus_start: u32,
    /// End position of the region that was fine-mapped for this credible set.
    locus_end: u32,
    /// Number of variants in the credible set.
    locus_size: u32,
    /// Method used for fine-mapping of credible set.
    fine_mapping_method: FineMappingMethod,
    /// Natural log Bayes factor for the entire credible set.
    log_bayes_factor: Option<f64>,
    /// Confidence level of the credible set, from 1 (PICS on a reported top hit) to 4 (`SuSiE`
    /// with in-sample LD); 0 means unknown.
    confidence: u8,
    /// Mean R-squared linkage disequilibrium for variants in the credible set.
    purity_mean_r2: Option<f64>,
    /// Minimum R-squared linkage disequilibrium for variants in the credible set.
    purity_min_r2: Option<f64>,
    /// Quality control flags for this credible set.
    quality_controls: Vec<String>,
    /// Whether this credible set is a trans-QTL.
    is_trans_qtl: bool,
}

// ---- filters ---

/// Filter for credible sets.
#[derive(Debug, InputObject)]
pub struct CredibleSetFilter {
    /// Keep credible sets whose study type is one of these.
    pub study_types: Option<Vec<study::StudyType>>,
}

impl Filter<CredibleSet> for CredibleSetFilter {
    fn matches(&self, item: &CredibleSet) -> bool {
        self.study_types
            .as_ref()
            .is_none_or(|t| t.contains(&item.study_type))
    }
}

// --- loaders ---

pub type CredibleSetCache = Cache<String, Option<CredibleSet>>;
static CREDIBLE_SET_CACHE: LazyLock<CredibleSetCache> = LazyLock::new(entity_cache);

#[derive(From)]
pub struct CredibleSetLoader {
    ch: ClickHouse,
}

impl CachedLoader for CredibleSetLoader {
    type Key = String;
    type Value = CredibleSet;

    fn cache(&self) -> &CredibleSetCache { &CREDIBLE_SET_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.credible_set_id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM credible_sets WHERE credibleSetId IN ?")
            .bind(misses)
            .fetch_all::<CredibleSet>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for CredibleSetLoader {
    type Value = CredibleSet;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[String],
    ) -> Result<HashMap<String, CredibleSet>, async_graphql::Error> {
        self.load_cached(keys).await
    }
}

/// Loads credible sets by their IDs.
///
/// # Returns
/// Returns a vector of [`CredibleSet`] objects.
/// # Errors
/// Returns an error if the credible sets could not be loaded.
pub async fn load_credible_sets(
    ctx: &Context<'_>,
    ids: &[String],
) -> async_graphql::Result<Vec<CredibleSet>> {
    load_ordered(ctx.data_unchecked::<DataLoader<CredibleSetLoader>>(), ids).await
}

/// Loads a single credible set by its ID.
///
/// # Returns
/// Returns an [`Option<CredibleSet>`] object.
/// # Errors
/// Returns an error if the credible set could not be loaded.
pub async fn load_credible_set(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<CredibleSet>> {
    ctx.data_unchecked::<DataLoader<CredibleSetLoader>>()
        .load_one(id)
        .await
}

// ---- resolvers ----
#[derive(Default)]
pub struct CredibleSetQuery;

#[Object]
impl CredibleSetQuery {
    /// Retrieve a list of credible sets by their identifier.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn credible_sets(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "List of credible set IDs to fetch.")] credible_set_ids: Vec<String>,
        #[graphql(default, desc = "Pagination for the credible sets.")] page: Page,
    ) -> async_graphql::Result<Paged<CredibleSet>> {
        let results = load_credible_sets(ctx, &credible_set_ids).await?;
        Ok(results.query().paginate(page))
    }

    /// Retrieve a credible set by its identifier.
    async fn credible_set(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Credible set ID to fetch.")] credible_set_id: String,
    ) -> async_graphql::Result<Option<CredibleSet>> {
        load_credible_set(ctx, credible_set_id).await
    }
}

#[ComplexObject]
impl CredibleSet {
    /// The lead variant for the credible set.
    async fn lead_variant(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<variant::Variant>> {
        variant::load_variant(ctx, self.lead_variant_id.clone()).await
    }
    /// GWAS or molQTL study in which the credible set was identified.
    async fn study(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<study::Study>> {
        study::load_study(ctx, self.study_id.clone()).await
    }
    /// Predictions from Locus2gene gene assignment model.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn l2g_predictions(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the Locus2gene predictions.")] page: Page,
    ) -> async_graphql::Result<Paged<l2g_predictions::L2GPrediction>> {
        let l2g = l2g_predictions::load_l2g_predictions(ctx, self.credible_set_id.clone()).await?;
        Ok(l2g.unwrap_or_default().query().paginate(page))
    }
    /// Variants in the credible set, with their fine-mapping statistics.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn locus(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter criteria to apply.")] filter: Option<locus::LocusFilter>,
        #[graphql(default, desc = "Pagination for the locus information.")] page: Page,
    ) -> async_graphql::Result<Paged<locus::Locus>> {
        let locus = locus::load_locus(ctx, self.credible_set_id.clone()).await?;
        Ok(locus
            .unwrap_or_default()
            .query()
            .filter(filter.as_ref())
            .paginate(page))
    }
    /// GWAS-GWAS and GWAS-molQTL credible set colocalisation results. Dataset includes colocalising
    /// pairs as well as the method and statistics used to estimate the colocalisation.
    #[graphql(complexity = "page_complexity(page, child_complexity)")]
    async fn colocalisation(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Filter criteria to apply.")]
        filter: colocalisation::ColocalisationFilter,
        #[graphql(default, desc = "Pagination for the colocalisation results.")] page: Page,
    ) -> async_graphql::Result<Paged<colocalisation::Colocalisation>> {
        let colocalisation =
            colocalisation::load_colocalisation(ctx, self.credible_set_id.clone()).await?;
        Ok(colocalisation
            .unwrap_or_default()
            .query()
            .filter(Some(&filter))
            .paginate(page))
    }
}
