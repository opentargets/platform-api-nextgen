use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use moka::future::Cache;
use serde::Deserialize;
use serde_repr::Deserialize_repr;
use strum::{EnumIter, IntoStaticStr};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        date::Date,
        disease::{Disease, load_disease},
        drug::{Drug, load_drug},
        evidence::TrialStopReasonCategory,
    },
    query::{
        cache::{CachedLoader, entity_cache},
        load_ordered,
    },
};

// ---- models ----

/// Kind of clinical evidence the report describes.
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Enum,
    Deserialize_repr,
    IntoStaticStr,
    EnumIter,
)]
#[repr(i8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ClinicalReportType {
    /// Clinical evidence informing about a drug/disease claim.
    Indication = 1,
    /// Clinical evidence informing about a drug's safety event.
    Safety = 2,
}

/// Nature of the record the report originates from.
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Enum,
    Deserialize_repr,
    IntoStaticStr,
    EnumIter,
)]
#[repr(i8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Origin {
    /// Clinical trial record.
    ClinicalTrial = 1,
    /// Drug label record.
    DrugLabel = 2,
    /// Regulatory agency record.
    RegulatoryAgency = 3,
    /// Curated resource record.
    CuratedResource = 4,
}

/// Clinical stage of the report.
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Enum,
    Deserialize_repr,
    IntoStaticStr,
    EnumIter,
)]
#[repr(i8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ClinicalStage {
    /// Unknown phase.
    Unknown = 1,
    /// Preclinical stage.
    Preclinical = 2,
    /// Ind.
    Ind = 3,
    /// Early phase 1.
    #[graphql(name = "EARLY_PHASE_1")]
    #[strum(serialize = "EARLY_PHASE_1")]
    EarlyPhase1 = 4,
    /// Phase 1.
    #[graphql(name = "PHASE_1")]
    #[strum(serialize = "PHASE_1")]
    Phase1 = 5,
    /// Phase 1/2.
    #[graphql(name = "PHASE_1_2")]
    #[strum(serialize = "PHASE_1_2")]
    Phase12 = 6,
    /// Phase 2.
    #[graphql(name = "PHASE_2")]
    #[strum(serialize = "PHASE_2")]
    Phase2 = 7,
    /// Phase 2/3.
    #[graphql(name = "PHASE_2_3")]
    #[strum(serialize = "PHASE_2_3")]
    Phase23 = 8,
    /// Phase 3.
    #[graphql(name = "PHASE_3")]
    #[strum(serialize = "PHASE_3")]
    Phase3 = 9,
    /// Preapproval.
    Preapproval = 10,
    /// The drog is approved for general use.
    Approval = 11,
    /// Phase 4.
    #[graphql(name = "PHASE_4")]
    #[strum(serialize = "PHASE_4")]
    Phase4 = 12,
    /// The drug has been Withdrawn.
    Withdrawal = 13,
}

/// Type of clinical study.
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Enum,
    Deserialize_repr,
    IntoStaticStr,
    EnumIter,
)]
#[repr(i8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrialStudyType {
    /// Interventional.
    Interventional = 1,
    /// Observational.
    Observational = 2,
    /// Expanded access.
    ExpandedAccess = 3,
}

/// Overall recruitment status of the clinical trial.
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Enum,
    Deserialize_repr,
    IntoStaticStr,
    EnumIter,
)]
#[repr(i8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrialOverallStatus {
    /// Active, not recruiting.
    ActiveNotRecruiting = 1,
    /// Completed.
    Completed = 2,
    /// Enrolling by invitation.
    EnrollingByInvitation = 3,
    /// Not yet recruiting.
    NotYetRecruiting = 4,
    /// Recruiting.
    Recruiting = 5,
    /// Suspended.
    Suspended = 6,
    /// Terminated.
    Terminated = 7,
    /// Withdrawn.
    Withdrawn = 8,
    /// Expanded access available.
    Available = 9,
    /// Expanded access no longer available.
    NoLongerAvailable = 10,
    /// Expanded access temporarily not available.
    TemporarilyNotAvailable = 11,
    /// Approved for marketing.
    ApprovedForMarketing = 12,
    /// Withheld.
    Withheld = 13,
    /// Unknown status.
    Unknown = 14,
}

/// Phase of the clinical trial as registered.
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Enum,
    Deserialize_repr,
    IntoStaticStr,
    EnumIter,
)]
#[repr(i8)]
pub enum TrialPhase {
    /// Early phase 1.
    #[graphql(name = "EARLY_PHASE1")]
    #[strum(serialize = "EARLY_PHASE1")]
    EarlyPhase1 = 1,
    /// Phase 1.
    #[graphql(name = "PHASE1")]
    #[strum(serialize = "PHASE1")]
    Phase1 = 2,
    /// Phase 1/phase 2.
    #[graphql(name = "PHASE1_PHASE2")]
    #[strum(serialize = "PHASE1/PHASE2")]
    Phase1Phase2 = 3,
    /// Phase 2.
    #[graphql(name = "PHASE2")]
    #[strum(serialize = "PHASE2")]
    Phase2 = 4,
    /// Phase 2/phase 3.
    #[graphql(name = "PHASE2_PHASE3")]
    #[strum(serialize = "PHASE2/PHASE3")]
    Phase2Phase3 = 5,
    /// Phase 3.
    #[graphql(name = "PHASE3")]
    #[strum(serialize = "PHASE3")]
    Phase3 = 6,
    /// Phase 4.
    #[graphql(name = "PHASE4")]
    #[strum(serialize = "PHASE4")]
    Phase4 = 7,
}

/// Primary purpose of the clinical trial.
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Enum,
    Deserialize_repr,
    IntoStaticStr,
    EnumIter,
)]
#[repr(i8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrialPrimaryPurpose {
    /// Treatment.
    Treatment = 1,
    /// Prevention.
    Prevention = 2,
    /// Diagnostic.
    Diagnostic = 3,
    /// Educational, counseling or training.
    Ect = 4,
    /// Supportive care.
    SupportiveCare = 5,
    /// Screening.
    Screening = 6,
    /// Health services research.
    HealthServicesResearch = 7,
    /// Basic science.
    BasicScience = 8,
    /// Device feasibility.
    DeviceFeasibility = 9,
    /// Other purpose.
    Other = 10,
}

/// Classification of the lead trial sponsor.
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Enum,
    Deserialize_repr,
    IntoStaticStr,
    EnumIter,
)]
#[repr(i8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AgencyClass {
    /// National Institutes of Health.
    Nih = 1,
    /// Other US federal agency.
    Fed = 2,
    /// Other government agency.
    OtherGov = 3,
    /// Individual.
    Indiv = 4,
    /// Industry.
    Industry = 5,
    /// Network.
    Network = 6,
    /// Ambiguous.
    Ambig = 7,
    /// Other sponsor.
    Other = 8,
    /// Unknown sponsor.
    Unknown = 9,
}

/// How a literature reference relates to the clinical trial.
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Enum,
    Deserialize_repr,
    IntoStaticStr,
    EnumIter,
)]
#[repr(i8)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrialLiteratureType {
    /// Literature the trial's authors cited.
    Background = 1,
    /// Reports the trial's outcome.
    Result = 2,
    /// Derived from the trial's outcome.
    Derived = 3,
}

/// Source that provided the clinical report.
#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Enum,
    Deserialize_repr,
    IntoStaticStr,
    EnumIter,
)]
#[repr(i8)]
pub enum Provider {
    /// ChEMBL.
    #[graphql(name = "ChEMBL")]
    #[strum(serialize = "ChEMBL")]
    Chembl = 1,
    /// Therapeutic Target Database.
    #[graphql(name = "TTD")]
    #[strum(serialize = "TTD")]
    Ttd = 2,
    /// Aggregate Analysis of ClinicalTrials.gov.
    #[graphql(name = "AACT")]
    #[strum(serialize = "AACT")]
    Aact = 3,
    /// European Medicines Agency.
    #[graphql(name = "EMA")]
    #[strum(serialize = "EMA")]
    Ema = 4,
    /// Pharmaceuticals and Medical Devices Agency.
    #[graphql(name = "PMDA")]
    #[strum(serialize = "PMDA")]
    Pmda = 5,
    /// US Food and Drug Administration.
    #[graphql(name = "FDA")]
    #[strum(serialize = "FDA")]
    Fda = 6,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct ClinicalReportDrugListItem {
    /// Drug name as reported in the source.
    drug_from_source: String,
    /// Open Targets molecule identifier.
    #[graphql(skip)]
    drug_id: Option<String>,
}

#[ComplexObject]
impl ClinicalReportDrugListItem {
    /// Drug in the clinical report. Null if the source drug isn't mapped or isn't found.
    async fn drug(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Drug>> {
        match &self.drug_id {
            Some(drug_id) => load_drug(ctx, drug_id.clone()).await,
            None => Ok(None),
        }
    }
}

/// Lead sponsor associated with the clinical trial.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TrialSponsor {
    /// Classification of the lead trial sponsor (e.g. INDUSTRY, NIH, OTHER).
    agency_class: Option<AgencyClass>,
    /// Name of the lead sponsor associated with the clinical trial.
    name: Option<String>,
}

/// Literature references associated with the clinical trial.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TrialLiterature {
    /// PubMed identifier of the reference, when the source records one.
    id: Option<String>,
    /// How the reference relates to the trial: RESULT and DERIVED report its outcome, BACKGROUND
    /// is literature its authors cited.
    #[graphql(name = "referenceType")]
    r#type: TrialLiteratureType,
}

/// Diseases associated with the clinical report.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct ClinicalDiseaseListItem {
    /// Disease name as reported in the source.
    disease_from_source: String,
    /// Open Targets disease identifier.
    #[graphql(skip)]
    disease_id: Option<String>,
}

#[ComplexObject]
impl ClinicalDiseaseListItem {
    /// Disease in the clinical report. Null if the source disease isn't mapped or isn't found.
    async fn disease(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Disease>> {
        match &self.disease_id {
            Some(disease_id) => load_disease(ctx, disease_id.clone()).await,
            None => Ok(None),
        }
    }
}

/// Side effects or adverse events associated with the clinical report.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct ClinicalSideEffectListItem {
    /// Open Targets disease identifier for the side effect.
    #[graphql(skip)]
    disease_id: String,
    /// Side effect disease name as reported in the source.
    disease_from_source: String,
}

#[ComplexObject]
impl ClinicalSideEffectListItem {
    /// Disease for the reported side effect.
    async fn disease(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Disease>> {
        load_disease(ctx, self.disease_id.clone()).await
    }
}

/// A clinical record (e.g. trial, drug label) reporting on drugs and diseases.
#[derive(Debug, Clone, Deserialize, Row, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ClinicalReport {
    /// Unique identifier for the clinical report.
    id: String,
    /// Source database or registry from which the clinical report was obtained.
    source: String,
    /// Clinical stage of the report.
    clinical_stage: ClinicalStage,
    /// Clinical phase as reported in the source.
    phase_from_source: Option<String>,
    /// Kind of evidence the report describes: INDICATION for a drug/disease claim, SAFETY for a
    /// drug warning.
    #[graphql(name = "evidenceType")]
    r#type: ClinicalReportType,
    /// Title of the clinical report.
    title: String,
    /// Type of clinical study (e.g. Interventional, Observational).
    trial_study_type: Option<TrialStudyType>,
    /// Brief description of the clinical trial.
    trial_description: Option<String>,
    /// Number of arms in the clinical trial.
    trial_number_of_arms: Option<i32>,
    /// Start date of the clinical trial.
    trial_start_date: Option<Date>,
    /// Literature references associated with the clinical trial.
    trial_literature: Vec<TrialLiterature>,
    /// Overall status of the clinical trial (e.g. Completed, Terminated).
    trial_overall_status: Option<TrialOverallStatus>,
    /// Reason provided for stopping the clinical trial.
    trial_why_stopped: Option<String>,
    /// Primary purpose of the clinical trial (e.g. Treatment, Prevention).
    trial_primary_purpose: Option<TrialPrimaryPurpose>,
    /// Phase of the clinical trial.
    trial_phase: Option<TrialPhase>,
    /// Categories describing reasons why the trial was stopped.
    trial_stop_reason_categories: Vec<TrialStopReasonCategory>,
    /// Lead sponsor associated with the clinical trial.
    trial_sponsor: TrialSponsor,
    /// Quality control flags or notes for the clinical report.
    quality_controls: Vec<String>,
    /// Diseases associated with the clinical report.
    diseases: Vec<ClinicalDiseaseListItem>,
    /// Drugs associated with the clinical report.
    drugs: Vec<ClinicalReportDrugListItem>,
    /// Countries associated with the clinical report.
    countries: Vec<String>,
    /// Year the report was registered or published.
    year: Option<i32>,
    /// Side effects or adverse events associated with the clinical report.
    side_effects: Vec<ClinicalSideEffectListItem>,
    /// Official title of the clinical trial as registered.
    trial_official_title: Option<String>,
    /// URL linking to the source record.
    url: String,
    /// Nature of the record the report originates from.
    origin: Origin,
    /// Resource or organisation that distributes the data fetched from the primary source.
    provider: Provider,
}

// ---- loaders ----

pub type ClinicalReportCache = Cache<String, Option<ClinicalReport>>;
static CLINICAL_REPORT_CACHE: LazyLock<ClinicalReportCache> = LazyLock::new(entity_cache);

#[derive(From)]
pub struct ClinicalReportLoader {
    ch: ClickHouse,
}

impl CachedLoader for ClinicalReportLoader {
    type Key = String;
    type Value = ClinicalReport;

    fn cache(&self) -> &ClinicalReportCache { &CLINICAL_REPORT_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM clinical_report WHERE id IN ?")
            .bind(misses)
            .fetch_all::<ClinicalReport>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for ClinicalReportLoader {
    type Value = ClinicalReport;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[String],
    ) -> Result<HashMap<String, ClinicalReport>, async_graphql::Error> {
        self.load_cached(keys).await
    }
}

#[allow(clippy::missing_errors_doc)]
pub async fn load_clinical_reports(
    ctx: &Context<'_>,
    ids: &[String],
) -> async_graphql::Result<Vec<ClinicalReport>> {
    load_ordered(
        ctx.data_unchecked::<DataLoader<ClinicalReportLoader>>(),
        ids,
    )
    .await
}

#[allow(clippy::missing_errors_doc)]
pub async fn load_clinical_report(
    ctx: &Context<'_>,
    id: &str,
) -> async_graphql::Result<Option<ClinicalReport>> {
    ctx.data_unchecked::<DataLoader<ClinicalReportLoader>>()
        .load_one(id.to_string())
        .await
}

// ---- resolvers ----

#[derive(Default)]
pub struct ClinicalReportQuery;

#[Object]
impl ClinicalReportQuery {
    /// Retrieve a clinical report by an identifier.
    async fn clinical_report(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Clinical Report ID.")] clinical_report_id: String,
    ) -> async_graphql::Result<Option<ClinicalReport>> {
        load_clinical_report(ctx, &clinical_report_id).await
    }

    /// Retrieve a list of clinical reports by identifiers.
    async fn clinical_reports(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "List of Clinical Report IDs.")] clinical_reports_ids: Vec<String>,
    ) -> async_graphql::Result<Vec<ClinicalReport>> {
        load_clinical_reports(ctx, &clinical_reports_ids).await
    }
}
