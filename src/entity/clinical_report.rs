use async_graphql::{ComplexObject, Enum, SimpleObject};
use chrono::NaiveDate;
use clickhouse::Row;
use serde::Deserialize;

// ---- models ----

/// Kind of clinical evidence the report describes.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
pub enum ClinicalReportType {
    /// Clinical evidence informing about a drug/disease claim.
    Indication,
    /// Clinical evidence informing about a drug's safety event.
    Safety,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ClinRepDrugListItem {
    /// Drug name as reported in the source.
    drug_from_source: Option<String>,
    /// Open Targets molecule identifier.
    drug_id: Option<String>,
}

/// Lead sponsor associated with the clinical trial.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TrialSponsor {
    /// Classification of the lead trial sponsor (e.g. INDUSTRY, NIH, OTHER).
    agency_class: Option<String>,
    /// Name of the lead sponsor associated with the clinical trial.
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialLiterature {
    /// PubMed identifier of the reference, when the source records one.
    id: String,
    /// How the reference relates to the trial: RESULT and DERIVED report its outcome, BACKGROUND
    /// is literature its authors cited.
    r#type: String,
}

/// A clinical record (e.g. trial, drug label) reporting on drugs and diseases.
#[derive(Debug, Clone, Deserialize, Row, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct ClinicalReport {
    /// Unique identifier for the clinical report.
    id: String,
    /// Source database or registry from which the clinical report was obtained.
    source: String,
    /// Clinical stage of the report.
    clinical_stage: String,
    /// Clinical phase as reported in the source.
    phase_from_source: Option<String>,
    /// Kind of evidence the report describes: INDICATION for a drug/disease claim, SAFETY for a
    /// drug warning.
    #[graphql(skip)]
    r#type: Option<String>,
    /// Title of the clinical report.
    title: Option<String>,
    /// Type of clinical study (e.g. Interventional, Observational).
    trial_study_type: Option<String>,
    /// Brief description of the clinical trial.
    trial_description: Option<String>,
    /// Number of arms in the clinical trial.
    trial_number_of_arms: Option<i32>,
    /// Start date of the clinical trial.
    #[graphql(skip)]
    #[serde(with = "clickhouse::serde::chrono::date::option")]
    trial_start_date: Option<NaiveDate>,
    /// Literature references associated with the clinical trial.
    #[graphql(skip)]
    trial_literature: Vec<TrialLiterature>,
    /// Overall status of the clinical trial (e.g. Completed, Terminated).
    trial_overall_status: Option<String>,
    /// Reason provided for stopping the clinical trial.
    trial_why_stopped: Option<String>,
    /// Primary purpose of the clinical trial (e.g. Treatment, Prevention).
    trial_primary_purpose: Option<String>,
    /// Phase of the clinical trial.
    trial_phase: Option<String>,
    /// Categories describing reasons why the trial was stopped.
    trial_stop_reason_categories: Vec<String>,
    /// Lead sponsor associated with the clinical trial.
    #[graphql(skip)]
    trial_sponsor: TrialSponsor,
    /// Quality control flags or notes for the clinical report.
    quality_controls: Vec<String>,
    /// Diseases associated with the clinical report.
    diseases: Vec<ClinicalDiseaseListItem>,
    /// Drugs associated with the clinical report.
    drugs: Vec<ClinRepDrugListItem>,
    /// Countries associated with the clinical report.
    countries: Vec<String>,
    /// Year the report was registered or published.
    year: Option<i32>,
    /// Side effects or adverse events associated with the clinical report.
    side_effects: Vec<ClinicalDiseaseListItem>,
    /// Official title of the clinical trial as registered.
    trial_official_title: Option<String>,
    /// URL linking to the source record.
    url: Option<String>,
    /// Nature of the record the report originates from (e.g. CLINICAL_TRIAL, DRUG_LABEL,
    /// REGULATORY_AGENCY, CURATED_RESOURCE).
    origin: String,
    /// Resource or organisation that distributes the data fetched from the primary source (e.g.
    /// AACT, ChEMBL, EMA, PMDA, TTD).
    provider: String,
}

// ---- resolvers ----

#[ComplexObject]
impl ClinicalReport {
    /// Kind of evidence the report describes: INDICATION for a drug/disease claim, SAFETY for a
    /// drug warning.
    async fn r#type(&self) -> Option<ClinicalReportType> {
        match self.r#type.as_deref()? {
            "INDICATION" => Some(ClinicalReportType::Indication),
            "SAFETY" => Some(ClinicalReportType::Safety),
            _ => None,
        }
    }

    /// Start date of the clinical trial.
    async fn trial_start_date(&self) -> Option<String> {
        self.trial_start_date.map(|date| date.to_string())
    }

    /// Lead sponsor associated with the clinical trial.
    async fn trial_sponsor(&self) -> Option<&TrialSponsor> { Some(&self.trial_sponsor) }

    pub fn trial_literature(&self) -> Vec<String> { &self.trialLiterature.id }
}
