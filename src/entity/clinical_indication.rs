pub struct ClinicalIndication {
    id: String,
    /// Hash of drugId
    drugId: Option<String>,
    /// Hash of diseaseId
    diseaseId: Option<String>,
    /// Maximum Clinical Development Status for the association.
    maxClinicalStage: String,
    clinicalReportIds: Vec<String>,
}
