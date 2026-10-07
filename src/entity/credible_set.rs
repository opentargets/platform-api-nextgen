use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, InputObject, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{colocalisation, l2g_predictions, locus, study, variant},
    query::{
        QueryExt,
        cache::{CachedLoader, entity_cache},
        filter::Filter,
        load_ordered,
        paginate::{Page, Paged},
    },
};

// --- models ---

/// Variants in linkage disequilibrium (LD) with the credible set lead variant.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct LDSet {
    /// The variant ID for tag variants in LD with the credible set lead variant.
    tag_variant_id: String,
    /// The R-squared value for the tag variants with the credible set lead variant.
    r2_overall: f64,
}

/// 95% credible sets for GWAS and molQTL studies. Credible sets include all variants in the
/// credible set (locus) as well as the fine-mapping method and derived statistics.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct CredibleSet {
    /// Identifier of the credible set (`StudyLocus`).
    study_locus_id: String,
    #[graphql(skip)]
    variant_id: String,
    /// Chromosome which the credible set is located.
    chromosome: variant::Chromosome,
    /// Position of the lead variant for the credible set (`GRCh38`).
    position: u32,
    /// Start and end positions of the region used for fine-mapping.
    region: Option<String>,
    /// Identifier of the GWAS or molQTL study in which the credible set was identified.
    study_id: String,
    /// Beta coefficient of the lead variant.
    beta: Option<f64>,
    /// Z-score of the lead variant from the GWAS.
    z_score: Option<f64>,
    /// Mantissa value of the lead variant P-value.
    p_value_mantissa: f64,
    /// Exponent value of the lead variant P-value.
    p_value_exponent: i32,
    /// Allele frequency of the lead variant from the GWAS.
    effect_allele_frequency_from_source: Option<f64>,
    /// Standard error of the lead variant.
    standard_error: Option<f64>,
    /// [Deprecated]
    sub_study_description: Option<String>,
    /// Quality control flags for this credible set.
    quality_controls: Vec<String>,
    /// Method used for fine-mapping of credible set.
    finemapping_method: String,
    /// Integer label for the order of credible sets from study-region.
    credible_set_index: Option<u8>,
    /// Log10 Bayes factor for the entire credible set.
    credible_setlog10_b_f: Option<f64>,
    /// Mean R-squared linkage disequilibrium for variants in the credible set.
    purity_mean_r2: Option<f64>,
    /// Minimum R-squared linkage disequilibrium for variants in the credible set.
    purity_min_r2: Option<f64>,
    /// Start position of the region that was fine-mapped for this credible set.
    locus_start: Option<i32>,
    /// End position of the region that was fine-mapped for this credible set.
    locus_end: Option<i32>,
    /// Sample size of the study which this credible set is derived.
    sample_size: Option<u32>,
    /// Array of structs which denote the variants in LD with the credible set lead variant.
    ld_set: Vec<LDSet>,
    /// Descriptor for whether the credible set is derived from GWAS or molecular QTL.
    study_type: study::StudyType,
    /// Ensembl identifier of the gene representing a specific gene whose molecular is being
    /// analysed in molQTL study.
    qtl_gene_id: Option<String>,
    /// Description of how this credible set was derived in terms of data and fine-mapping method.
    confidence: String,
    /// Boolean for whether this credible set is a trans-pQTL or not.
    is_trans_qtl: Option<bool>,
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
    fn key_of(v: &Self::Value) -> Self::Key { v.study_locus_id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM credible_sets WHERE studyLocusId IN ?")
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
    async fn credible_sets(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "List of study locus IDs to fetch.")] study_locus_ids: Vec<String>,
        #[graphql(default, desc = "Pagination for the credible sets.")] page: Page,
    ) -> async_graphql::Result<Paged<CredibleSet>> {
        let results = load_credible_sets(ctx, &study_locus_ids).await?;
        Ok(results.query().paginate(page))
    }

    /// Retrieve a credible set by its identifier.
    async fn credible_set(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Study locus ID to fetch.")] study_locus_id: String,
    ) -> async_graphql::Result<Option<CredibleSet>> {
        load_credible_set(ctx, study_locus_id.clone()).await
    }
}

#[ComplexObject]
impl CredibleSet {
    /// The lead variant for the credible set, by posterior probability.
    async fn variant(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<variant::Variant>> {
        variant::load_variant(ctx, self.variant_id.clone()).await
    }
    /// GWAS or molQTL study in which the credible set was identified.
    async fn study(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<study::Study>> {
        study::load_study(ctx, self.study_id.clone()).await
    }
    /// Predictions from Locus2gene gene assignment model.
    async fn l2g_predictions(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the Locus2gene predictions.")] page: Page,
    ) -> async_graphql::Result<Paged<l2g_predictions::L2GPrediction>> {
        let l2g = l2g_predictions::load_l2g_predictions(ctx, self.study_locus_id.clone()).await?;
        Ok(l2g.unwrap_or_default().query().paginate(page))
    }
    /// Locus information for all variants in the credible set.
    async fn locus(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Filter criteria to apply.")] filter: Option<locus::LocusFilter>,
        #[graphql(default, desc = "Pagination for the locus information.")] page: Page,
    ) -> async_graphql::Result<Paged<locus::Locus>> {
        let locus = locus::load_locus(ctx, self.study_locus_id.clone()).await?;
        Ok(locus
            .unwrap_or_default()
            .query()
            .filter(filter.as_ref())
            .paginate(page))
    }
    /// GWAS-GWAS and GWAS-molQTL credible set colocalisation results. Dataset includes colocalising
    /// pairs as well as the method and statistics used to estimate the colocalisation.
    async fn colocalisation(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Filter criteria to apply.")]
        filter: colocalisation::ColocalisationFilter,
        #[graphql(default, desc = "Pagination for the colocalisation results.")] page: Page,
    ) -> async_graphql::Result<Paged<colocalisation::Colocalisation>> {
        let colocalisation =
            colocalisation::load_colocalisation(ctx, self.study_locus_id.clone()).await?;
        Ok(colocalisation
            .unwrap_or_default()
            .query()
            .filter(Some(&filter))
            .paginate(page))
    }
}
