use std::collections::HashMap;

use async_graphql::{
    Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, QueryExt,
        paginate::{Page, PagedWithStats},
        stats::HasStats,
    },
};

// ---- models ----

#[derive(Debug, Clone, Row, Deserialize)]
pub struct AdverseEventRow {
    pub chembl_id: String,
    pub adverse_events: Vec<AdverseEvent>,
    #[serde(rename = "criticalValue")]
    pub critical_value: f64,
}

impl AdverseEventRow {
    #[must_use]
    pub fn paged_with_stats(self, page: Page) -> PagedWithStats<AdverseEvent> {
        let paged = self.adverse_events.query().paginate(page);
        PagedWithStats {
            count: paged.count,
            rows: paged.rows,
            stats: AdverseEventStats {
                critical_value: self.critical_value,
            },
        }
    }
}

/// Significant adverse events associated with drugs sharing the same pharmacological target. This
/// dataset is based on the FDA's Adverse Event Reporting System (FAERS) reporting post-marketing
/// surveillance data and it's filtered to include only reports submitted by health professionals.
/// The significance of a given target-ADR is estimated using a Likelihood Ratio Test (LRT) using
/// all reports associated with the drugs with the same target.
#[derive(Debug, Clone, Deserialize, SimpleObject, Default)]
#[serde(rename_all = "camelCase")]
pub struct AdverseEvent {
    /// Number of reports mentioning drug and adverse event.
    count: u32,
    /// Critical value used to determine statistical significance of the association.
    #[graphql(skip)]
    #[allow(unused)]
    critval: f64,
    /// Meddra term on adverse event.
    name: String,
    /// Log-likelihood ratio.
    log_l_r: f64,
    /// 8 digit unique meddra identification number.
    meddra_code: String,
}

/// Statistics for a set of adverse events.
#[derive(Clone, SimpleObject)]
pub struct AdverseEventStats {
    /// LLR critical value to define significance.
    pub critical_value: f64,
}

impl HasStats for AdverseEvent {
    type Stats = AdverseEventStats;
}

// ---- query utilities ----

impl Entity for AdverseEvent {
    fn id(&self) -> &str { &self.meddra_code }
}

// ---- loaders ----

#[derive(From)]
pub struct AdverseEventLoader {
    ch: ClickHouse,
}

impl Loader<String> for AdverseEventLoader {
    type Value = AdverseEventRow;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows = self
            .ch
            .query("SELECT ?fields FROM openfda_faers WHERE chembl_id IN ?")
            .bind(keys)
            .fetch_all::<AdverseEventRow>()
            .await?;
        Ok(rows.into_iter().map(|r| (r.chembl_id.clone(), r)).collect())
    }
}

/// Loads the adverse events row for a drug (ChEMBL id).
///
/// # Errors
/// Returns an error if the row could not be loaded.
pub async fn load_adverse_events(
    ctx: &Context<'_>,
    chembl_id: String,
) -> async_graphql::Result<Option<AdverseEventRow>> {
    ctx.data_unchecked::<DataLoader<AdverseEventLoader>>()
        .load_one(chembl_id)
        .await
}
