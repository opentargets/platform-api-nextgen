use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, InputObject, Object, SimpleObject, context,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use moka::{future::Cache, ops::compute::Op};
use serde::Deserialize;
use serde_repr::{Deserialize_repr, Serialize_repr};
use tokio::io::Chain;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{credible_set, study, variant},
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        filter::{Filter, IntFilter, StringFilter},
        load_ordered,
        paginate::{Page, Paged},
        sort::{Sort, SortKey},
    },
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
    /// The other credible set (study-locus) in the colocalisation pair
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
    pub study_types: Option<Vec<study::StudyType>>,
}

impl Filter<Colocalisation> for ColocalisationFilter {
    fn matches(&self, item: &Colocalisation) -> bool {
        self.study_types
            .as_ref()
            .is_none_or(|t| t.contains(&item.right_study_type))
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
    pub async fn other_study_locus(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<credible_set::CredibleSet>> {
        credible_set::load_credible_set(ctx, self.other_study_locus_id.clone()).await
    }
}
