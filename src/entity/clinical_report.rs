#[serde(rename_all = "camelCase")]
pub struct ClinRepDrugListItem {
    drug_from_source: Option<String>,
    drug_id: Option<String>,
}

#[serde(rename_all = "camelCase")]
pub struct TrialSponsor {
    agency_class: Option<String>,
    name: Option<String>,
}

#[serde(rename_all = "camelCase")]
pub struct TrialLiterature {
    id: String,
    _type: String,
}

#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct ClinicalReport {
    id: String,
    source: String,
    clinical_stage: String,
    phase_from_source: Option<String>,
    clinical_report_type: Vec<ClinicalReportType>,
    title: Option<String>,
    trial_study_type: Option<String>,
    trial_description: Option<String>,
    trial_number_of_arms: Option<i32>,
    trial_start_date: Option<String>,
    #[graphql(skip)]
    trial_literature_struct: Vec<TrialLiterature>,
    trial_overall_status: Option<String>,
    trial_why_stopped: Option<String>,
    trial_primary_purpose: Option<String>,
    trial_phase: Option<String>,
    trial_stop_reason_categories: Vec<String>,
    trial_sponsor: Option<TrialSponsor>,
    quality_controls: Vec<String>,
    diseases: Vec<ClinicalDiseaseListItem>,
    drugs: Vec<ClinRepDrugListItem>,
    countries: Vec<String>,
    year: Option<i32>,
    side_effects: Vec<ClinicalDiseaseListItem>,
    trial_official_title: Option<String>,
    url: Option<String>,
    origin: String,
    provider: String,
}

#[graphql(complex)]
impl ClinicalReport {
    pub fn trial_literature(&self) -> Vec<String> { &self.trialLiteratureStruct.id }
}
