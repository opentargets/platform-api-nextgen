use std::collections::HashMap;

use async_graphql::{
    Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use serde::Deserialize;

use crate::datasource::clickhouse::ClickHouse;

// ---- models ----

/// A drug-disease pair as reported by clinical trial records.
#[derive(Debug, Clone, Deserialize, Row, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ClinicalIndication {
    /// Hash of drugId and diseaseId.
    id: String,
    drug_id: String,
    disease_id: String,
    /// Maximum Clinical Development Status for the association.
    max_clinical_stage: String,
    clinical_report_ids: Vec<String>,
}

// ---- loaders ----

pub struct ClinicalIndicationFromDrugLoader {
    ch: ClickHouse,
}

impl ClinicalIndicationFromDrugLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl Loader<String> for ClinicalIndicationFromDrugLoader {
    type Value = Vec<ClinicalIndication>;
    type Error = async_graphql::Error;

    #[tracing::instrument(skip_all, level = "debug", fields(n = ids.len()))]
    async fn load(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Vec<ClinicalIndication>>, Self::Error> {
        let rows: Vec<ClinicalIndication> = self
            .ch
            .query("SELECT ?fields FROM clinical_indication_drug WHERE drugId IN ?")
            .bind(ids)
            .fetch_all()
            .await?;
        // The table holds one row per drug-disease pair, so the rows are grouped by drug.
        Ok(rows.into_iter().fold(HashMap::new(), |mut acc, row| {
            acc.entry(row.drug_id.clone()).or_default().push(row);
            acc
        }))
    }
}

/// Loads the clinical indications recorded for a single drug by its ChEMBL id.
///
/// # Returns
/// The drug's clinical indications, or an empty `Vec` if it has none.
/// # Errors
/// Returns an error if the clinical indications could not be loaded.
pub async fn load_clinical_indications_from_drug(
    ctx: &Context<'_>,
    chembl_id: &str,
) -> async_graphql::Result<Vec<ClinicalIndication>> {
    Ok(ctx
        .data_unchecked::<DataLoader<ClinicalIndicationFromDrugLoader>>()
        .load_one(chembl_id.to_owned())
        .await?
        .unwrap_or_default())
}
