
#[serde(rename_all = "camelCase")]
pub struct ClinRepDrugListItem{
    drug_from_source: Option<String>,
    drug_id: Option<String>,
}

#[serde(rename_all = "camelCase")]
pub struct TrialSponsor{
    agency_class: Option<String>,
    name: Option<String>,
}

#[serde(rename_all = "camelCase")]
pub struct  TrialLiterature{
    id: String,
    `type`: String,
}

#[serde(rename_all = "camelCase")]
pub struct ClinicalReport{
    id: String,
    source: String,
    clinicalStage: String,
    phaseFromSource: Option<String>,
    `type`: Vec<ClinicalReportType>,
    title: Option<String>,
    trialStudyType: Option<String>,
    trialDescription: Option<String>,
    trialNumberOfArms: Option<i32>,
    trialStartDate: Option<String>,
    trialLiterature: Vec<TrialLiterature>,
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
    provider: String
}
