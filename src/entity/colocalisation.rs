use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, InputObject, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        credible_set::{CredibleSet, load_credible_set},
        study, variant,
    },
    query::filter::Filter,
};

// --- models ---

/// GWAS-GWAS and GWAS-molQTL credible set colocalisation results. Dataset includes colocalising
/// pairs as well as the method and statistics used to estimate the colocalisation.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Colocalisation {
    /// Credible set (study-locus) on the left side of the colocalisation pair.
    study_locus_id: String,
    /// The other credible set (study-locus) in the colocalisation pair.
    other_study_locus_id: String,
    /// Type of the right-side study (e.g., gwas, eqtl, pqtl).
    right_study_type: study::StudyType,
    /// Chromosome where the colocalisation occurs.
    chromosome: variant::Chromosome,
    /// Method used to estimate colocalisation (e.g., coloc, eCAVIAR).
    colocalisation_method: String,
    /// Number of variants intersecting between two overlapping study-loci.
    number_colocalising_variants: u32,
    /// Posterior probability that both traits are associated, but with different causal variants
    /// (H3). Used in coloc method.
    h3: f64,
    /// Posterior probability that both traits are associated and share a causal variant (H4). Used
    /// in coloc method.
    h4: f64,
    /// Colocalisation posterior probability (CLPP) score estimating the probability of shared
    /// causal variants. Used in eCAVIAR method.
    clpp: f64,
    /// Average sign of the beta ratio between colocalised variants.
    beta_ratio_sign_average: f64,
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Row, Default)]
#[serde(rename_all = "camelCase")]
pub struct ColocalisationRow {
    study_locus_id: String,
    colocalisation: Vec<Colocalisation>,
}

// ---- filters ---

/// Filter for colocalisations.
#[derive(Debug, InputObject)]
pub struct ColocalisationFilter {
    /// Keep colocalisations whose study type is one of these.
    #[graphql(default_with = "ColocalisationFilter::default_study_types()")]
    pub study_types: Vec<study::StudyType>,
}

impl Filter<Colocalisation> for ColocalisationFilter {
    fn matches(&self, item: &Colocalisation) -> bool {
        self.study_types.is_empty() || self.study_types.iter().any(|t| t == &item.right_study_type)
    }
}

impl ColocalisationFilter {
    fn default_study_types() -> Vec<study::StudyType> { vec![study::StudyType::Gwas] }
}

impl Default for ColocalisationFilter {
    fn default() -> Self {
        Self {
            study_types: Self::default_study_types(),
        }
    }
}

// ---- loaders ----

#[derive(From)]
pub struct ColocalisationLoader {
    ch: ClickHouse,
}

impl Loader<String> for ColocalisationLoader {
    type Value = Vec<Colocalisation>;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<ColocalisationRow> = self
            .ch
            .query("SELECT ?fields FROM colocalisation WHERE studyLocusId IN ?")
            .bind(key)
            .fetch_all()
            .await?;
        let mut results: HashMap<String, Vec<Colocalisation>> = HashMap::new();
        for row in rows {
            let id = row.study_locus_id.clone();
            results.entry(id).or_insert(row.colocalisation);
        }
        Ok(results)
    }
}

/// Loads a colocalisation by its study locus ID.
///
/// # Returns
/// Returns `None` if no colocalisation is found for the given ID.
/// # Errors
/// Returns an error if the colocalisation cannot be loaded.
pub async fn load_colocalisation(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<Vec<Colocalisation>>> {
    ctx.data_unchecked::<DataLoader<ColocalisationLoader>>()
        .load_one(id.clone())
        .await
}

#[ComplexObject]
impl Colocalisation {
    /// The other credible set (study-locus) in the colocalisation pair.
    async fn other_study_locus(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<CredibleSet>> {
        load_credible_set(ctx, self.other_study_locus_id.clone()).await
    }
}
