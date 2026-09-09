use async_graphql::SimpleObject;
use clickhouse::Row;
use serde::Deserialize;

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
