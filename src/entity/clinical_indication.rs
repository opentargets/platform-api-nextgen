use std::sync::LazyLock;

use async_graphql::SimpleObject;
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::cache::{CachedLoader, entity_cache},
};

// ---- models ----

#[derive(Debug, Clone, Deserialize, Row, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ClinicalIndication {
    /// Hash of drugId and diseaseId.
    id: String,
    drug_id: Option<String>,
    disease_id: Option<String>,
    /// Maximum Clinical Development Status for the association.
    max_clinical_stage: String,
    clinical_report_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Row)]
#[serde(rename_all = "camelCase")]
pub struct ClinicalIndicationFromDrug {
    drug_id: String,
    clinical_indications: Vec<ClinicalIndication>,
}

// ---- loaders ----

pub type ClinicalIndicationFromDrugCache = Cache<String, Option<ClinicalIndicationFromDrug>>;
static CLINICAL_INDICATION_FROM_DRUG_CACHE: LazyLock<ClinicalIndicationFromDrugCache> =
    LazyLock::new(entity_cache);

pub struct ClinicalIndicationFromDrugLoader {
    ch: ClickHouse,
}

impl ClinicalIndicationFromDrugLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for ClinicalIndicationFromDrugLoader {
    type Key = String;
    type Value = ClinicalIndicationFromDrug;

    fn cache(&self) -> &ClinicalIndicationFromDrugCache { &CLINICAL_INDICATION_FROM_DRUG_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.drug_id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        // The table holds one row per drug-disease pair, so the rows are grouped into one
        // value per drug. The tuple must list the columns in `ClinicalIndication` field order.
        self.ch
            .query(
                "SELECT drugId, \
                 groupArray((id, drugId, diseaseId, maxClinicalStage, clinicalReportIds)) \
                 FROM clinical_indication_drug WHERE drugId IN ? GROUP BY drugId",
            )
            .bind(misses)
            .fetch_all::<ClinicalIndicationFromDrug>()
            .await
            .map_err(Into::into)
    }
}
