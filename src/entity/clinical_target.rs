use async_graphql::SimpleObject;
use serde::Deserialize;

use crate::entity::clinical_report::ClinicalDiseaseListItem;

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ClinicalTarget {
    id: String,
    #[graphql(skip)]
    drug_id: Option<String>,
    #[graphql(skip)]
    target_id: Option<String>,
    diseases: Vec<ClinicalDiseaseListItem>,
    max_clinical_stage: String,
    #[graphql(skip)]
    clinical_report_ids: Vec<String>,
}
