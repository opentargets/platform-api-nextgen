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
    clinicalStage: String,
    phaseFromSource: Option<String>,
    clinicalReportType: Vec<ClinicalReportType>,
    title: Option<String>,
    trialStudyType: Option<String>,
    trialDescription: Option<String>,
    trialNumberOfArms: Option<i32>,
    trialStartDate: Option<String>,
    #[graphql(skip)]
    trialLiteratureStruct: Vec<TrialLiterature>,
    trialOverallStatus: Option<String>,
    trialWhyStopped: Option<String>,
    trialPrimaryPurpose: Option<String>,
    trialPhase: Option<String>,
    trialStopReasonCategories: Vec<String>,
    trialSponsor: Option<TrialSponsor>,
    qualityControls: Vec<String>,
    diseases: Vec<ClinicalDiseaseListItem>,
    drugs: Vec<ClinRepDrugListItem>,
    countries: Vec<String>,
    year: Option<i32>,
    sideEffects: Vec<ClinicalDiseaseListItem>,
    trialOfficialTitle: Option<String>,
    url: Option<String>,
    origin: String,
    provider: String,
}

#[graphql(complex)]
impl ClinicalReport {
    pub fn trial_literature(&self) -> Vec<String> { &self.trialLiteratureStruct.id }
}
