use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
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
    entity::variant,
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        sort::{Sort, SortKey},
    },
};

// --- models ---

/// List of variants within the credible set.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Locus {
    /// Boolean for if the variant is part of the 95% credible set.
    is_95_credible_set: bool,
    /// Boolean for if the variant is part of the 99% credible set.
    is_99_credible_set: bool,
    /// Log (natural) Bayes factor for the variant from fine-mapping.
    log_b_f: Option<f64>,
    /// Posterior inclusion probability for the variant within this credible set.
    posterior_probability: f64,
    #[graphql(skip)]
    variant_id: String,
    /// Mantissa of the P-value for this variant in the credible set.
    p_value_mantissa: Option<f64>,
    /// Exponent of the P-value for this variant in the credible set.
    p_value_exponent: Option<i32>,
    /// Beta coefficient of this variant in the credible set.
    beta: Option<f64>,
    /// Standard error of this variant in the credible set.
    standard_error: Option<f64>,
    /// R-squared (LD) between this credible set variant and the lead variant.
    r2_overall: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Row, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocusRow {
    study_locus_id: String,
    locus: Vec<Locus>,
}

// ---- loaders ----

#[derive(From)]
pub struct LocusLoader {
    ch: ClickHouse,
}

impl Loader<String> for LocusLoader {
    type Value = Vec<Locus>;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<LocusRow> = self
            .ch
            .query("SELECT ?fields FROM credible_sets_locus WHERE studyLocusId IN ?")
            .bind(key)
            .fetch_all()
            .await?;
        let mut results: HashMap<String, Vec<Locus>> = HashMap::new();
        for row in rows {
            let id = row.study_locus_id.clone();
            results.entry(id).or_insert(row.locus);
        }
        Ok(results)
    }
}

pub async fn load_locus(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<Vec<Locus>>> {
    ctx.data_unchecked::<DataLoader<LocusLoader>>()
        .load_one(id.clone())
        .await
}

#[ComplexObject]
impl Locus {
    /// Variant in the credible set/
    pub async fn variant(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<variant::Variant>> {
        variant::load_variant(ctx, self.variant_id.clone()).await
    }
}
