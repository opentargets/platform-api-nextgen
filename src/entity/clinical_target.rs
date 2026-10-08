use async_graphql::SimpleObject;
use serde::Deserialize;

use crate::entity::clinical_report::ClinicalDiseaseListItem;

// ---- models ----

/// Target-drug associations derived from clinical reports, capturing the maximum clinical stage and
/// associated diseases for each target-drug pair.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ClinicalTarget {
    id: String,
    #[graphql(skip)]
    drug_id: String,
    #[graphql(skip)]
    target_id: String,
    diseases: Vec<ClinicalDiseaseListItem>,
    max_clinical_stage: String,
    #[graphql(skip)]
    clinical_report_ids: Vec<String>,
}
