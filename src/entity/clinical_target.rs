use async_graphql::SimpleObject;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ClinicalTarget {
    id: String,
    drug_id: Option<String>,
    target_id: Option<String>,
    diseases: Vec<ClinicalDiseaseListItem>,
    max_clinical_stage: String,
    clinical_report_ids: Vec<String>,
}
