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
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        sort::{Sort, SortKey},
    },
};

// --- models ---

/// Feature used in Locus2gene model predictions.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct L2GFeature {
    /// Name of the feature.
    name: String,
    /// Value of the feature.
    value: f64,
    /// SHAP (SHapley Additive exPlanations) value indicating the feature's contribution to the
    /// prediction.
    shap_value: f64,
}

///Predictions from Locus2gene gene assignment model. The dataset contains all predictions for
/// every combination of credible set and genes in the region as well as statistics to explain the
/// model interpretation of the predictions.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct L2GPrediction {
    /// Study-locus identifier for the credible set.
    study_locus_id: String,
    #[graphql(skip)]
    gene_id: String,
    /// Locus2gene prediction score for the gene assignment. Higher scores indicate a stronger
    /// association between the credible set and the gene. Scores range from 0 to 1.
    score: f64,
    /// Features used in the Locus2gene model prediction.
    features: Vec<L2GFeature>,
    /// SHAP base value for the prediction. This value is common to all predictions for a given
    /// credible set.
    shap_base_value: f64,
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Row, Default)]
pub struct L2GPredictionsRow {
    #[serde(rename = "studyLocusId")]
    study_locus_id: String,
    l2g_predictions: Vec<L2GPrediction>,
}

// ---- loaders ----

#[derive(From)]
pub struct L2GPredictionsLoader {
    ch: ClickHouse,
}

impl Loader<String> for L2GPredictionsLoader {
    type Value = Vec<L2GPrediction>;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<L2GPredictionsRow> = self
            .ch
            .query("SELECT ?fields FROM l2g_predictions WHERE studyLocusId IN ?")
            .bind(key)
            .fetch_all()
            .await?;
        let mut results: HashMap<String, Vec<L2GPrediction>> = HashMap::new();
        for row in rows {
            let id = row.study_locus_id.clone();
            results.entry(id).or_insert(row.l2g_predictions);
        }
        Ok(results)
    }
}

pub async fn load_l2g_predictions(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<Vec<L2GPrediction>>> {
    ctx.data_unchecked::<DataLoader<L2GPredictionsLoader>>()
        .load_one(id.clone())
        .await
}
