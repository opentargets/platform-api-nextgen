use std::collections::HashMap;

use async_graphql::{
    Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{datasource::clickhouse::ClickHouse, query::stats::HasStats};

// ---- models ----

#[derive(Debug, Clone, Row, Deserialize)]
pub struct AdverseEventRow {
    pub chembl_id: String,
    pub adverse_events: Vec<AdverseEvent>,
    pub critical_value: f64,
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Default)]
#[serde(rename_all = "camelCase")]
pub struct AdverseEvent {
    count: u32,
    critval: f64,
    name: String,
    log_l_r: f64,
    meddra_code: String,
}

/// Statistics for a set of adverse events.
#[derive(Clone, SimpleObject)]
pub struct AdverseEventStats {
    pub critical_value: f64,
    pub id: String,
}

impl HasStats for AdverseEvent {
    type Stats = AdverseEventStats;
}

// ---- loaders ----

#[derive(From)]
pub struct AdverseEventLoader {
    ch: ClickHouse,
}

impl Loader<String> for AdverseEventLoader {
    type Value = AdverseEventRow;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<AdverseEventRow> = self
            .ch
            .query("SELECT chembl_id, adverse_events, criticalValue FROM openfda_faers WHERE chembl_id IN ?")
            .bind(key)
            .fetch_all()
            .await?;
        let mut results: HashMap<String, AdverseEventRow> = HashMap::new();
        for row in rows {
            results.entry(row.chembl_id.clone()).or_insert(row.clone());
        }
        Ok(results)
    }
}

/// Loads AdverseEvents by the disease id from the cache or database.
///
/// # Returns
/// A `Vec` of `AdverseEvents` objects corresponding to the given disease IDs.
/// # Errors
/// Returns an error if the AdverseEvents could not be loaded.
pub async fn load_adverse_events(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<AdverseEventRow>> {
    ctx.data_unchecked::<DataLoader<AdverseEventLoader>>()
        .load_one(id.clone())
        .await
}
