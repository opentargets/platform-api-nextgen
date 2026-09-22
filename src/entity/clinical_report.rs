use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use chrono::NaiveDate;
use clickhouse::Row;
use derive_more::From;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::{ClickHouse, from_nullable_string_opt, from_string},
    entity::{
        disease::{Disease, load_disease},
        drug::{Drug, load_drug},
    },
    query::{
        cache::{CachedLoader, entity_cache},
        load_ordered,
    },
};

// ---- models ----

/// Kind of clinical evidence the report describes.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
pub enum ClinicalReportType {
    /// Clinical evidence informing about a drug/disease claim.
    Indication,
    /// Clinical evidence informing about a drug's safety event.
    Safety,
}

/// Nature of the record the report originates from.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Enum)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
pub enum Origin {
    /// TODO DESCRIPTION: Clinical trial record.
    ClinicalTrial,
    /// TODO DESCRIPTION: Drug label record.
    DrugLabel,
    /// TODO DESCRIPTION: Regulatory agency record.
    RegulatoryAgency,
    /// TODO DESCRIPTION: Curated resource record.
    CuratedResource,
}

// TODO: This should become an enum in CH, then we don't need to rename it all.
// We also have to figure out some more descriptive descriptions. :)
/// Clinical stage of the report.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Enum)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
pub enum ClinicalStage {
    /// Unknown phase.
    Unknown,
    /// Preclinical stage.
    Preclinical,
    /// Ind.
    Ind,
    /// Early phase 1.
    #[serde(rename = "EARLY_PHASE_1")]
    #[graphql(name = "EARLY_PHASE_1")]
    EarlyPhase1,
    /// Phase 1.
    #[serde(rename = "PHASE_1")]
    #[graphql(name = "PHASE_1")]
    Phase1,
    /// Phase 1/2.
    #[serde(rename = "PHASE_1_2")]
    #[graphql(name = "PHASE_1_2")]
    Phase12,
    /// Phase 2.
    #[serde(rename = "PHASE_2")]
    #[graphql(name = "PHASE_2")]
    Phase2,
    /// Phase 2/3.
    #[serde(rename = "PHASE_2_3")]
    #[graphql(name = "PHASE_2_3")]
    Phase23,
    /// Phase 3.
    #[serde(rename = "PHASE_3")]
    #[graphql(name = "PHASE_3")]
    Phase3,
    /// Preapproval.
    Preapproval,
    /// The drog is approved for general use.
    Approval,
    /// Phase 4.
    #[serde(rename = "PHASE_4")]
    #[graphql(name = "PHASE_4")]
    Phase4,
    /// The drug has been Withdrawn.
    Withdrawal,
}

/// Type of clinical study.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Enum)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
pub enum TrialStudyType {
    /// Interventional.
    Interventional,
    /// Observational.
    Observational,
    /// Expanded access.
    ExpandedAccess,
}

/// Source that provided the clinical report.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Enum)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
pub enum Provider {
    /// ChEMBL.
    #[serde(rename = "ChEMBL")]
    Chembl,
    /// Therapeutic Target Database.
    #[serde(rename = "TTD")]
    Ttd,
    /// Aggregate Analysis of ClinicalTrials.gov.
    #[serde(rename = "AACT")]
    Aact,
    /// European Medicines Agency.
    #[serde(rename = "EMA")]
    Ema,
    /// Pharmaceuticals and Medical Devices Agency.
    #[serde(rename = "PMDA")]
    Pmda,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ClinicalReportDrugListItem {
    /// Drug name as reported in the source.
    drug_from_source: String,
    /// Open Targets molecule identifier.
    #[graphql(skip)]
    drug_id: String,
}

#[ComplexObject]
impl ClinicalReportDrugListItem {
    /// Drug in the clinical report.
    async fn drug(&self, ctx: &Context<'_>) -> async_graphql::Result<Drug> {
        load_drug(ctx, self.drug_id.clone())
            .await?
            .ok_or_else(|| async_graphql::Error::new("drug not found"))
    }
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

/// Literature references associated with the clinical trial.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TrialLiterature {
    /// PubMed identifier of the reference, when the source records one.
    id: String,
    /// How the reference relates to the trial: RESULT and DERIVED report its outcome, BACKGROUND
    /// is literature its authors cited.
    #[graphql(name = "referenceType")]
    r#type: String,
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
    disease_id: String,
}

#[ComplexObject]
impl ClinicalDiseaseListItem {
    /// Disease in the clinical report.
    async fn disease(&self, ctx: &Context<'_>) -> async_graphql::Result<Disease> {
        load_disease(ctx, self.disease_id.clone())
            .await?
            .ok_or_else(|| async_graphql::Error::new("disease not found"))
    }
}

/// Side effects or adverse events associated with the clinical report.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct ClinicalSideEffectListItem {
    /// Open Targets disease identifier for the side effect.
    #[graphql(skip)]
    disease_id: Option<String>,
    /// Side effect disease name as reported in the source.
    disease_from_source: Option<String>,
}

#[ComplexObject]
impl ClinicalSideEffectListItem {
    /// Disease for the reported side effect.
    async fn disease(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Disease>> {
        match &self.disease_id {
            Some(disease_id) => load_disease(ctx, disease_id.clone()).await,
            None => Ok(None),
        }
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
    #[serde(deserialize_with = "from_string")]
    clinical_stage: ClinicalStage,
    /// Clinical phase as reported in the source.
    phase_from_source: Option<String>,
    /// Kind of evidence the report describes: INDICATION for a drug/disease claim, SAFETY for a
    /// drug warning.
    #[graphql(name = "evidenceType")]
    #[serde(deserialize_with = "from_nullable_string_opt")]
    r#type: Option<String>,
    /// Title of the clinical report.
    title: Option<String>,
    /// Type of clinical study (e.g. Interventional, Observational).
    #[serde(deserialize_with = "from_nullable_string_opt")]
    trial_study_type: Option<TrialStudyType>,
    /// Brief description of the clinical trial.
    trial_description: Option<String>,
    /// Number of arms in the clinical trial.
    trial_number_of_arms: Option<i32>,
    /// Start date of the clinical trial.
    #[serde(with = "clickhouse::serde::chrono::date::option")]
    trial_start_date: Option<NaiveDate>,
    /// Literature references associated with the clinical trial.
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
    url: Option<String>,
    /// Nature of the record the report originates from.
    #[serde(deserialize_with = "from_string")]
    origin: Origin,
    /// Resource or organisation that distributes the data fetched from the primary source.
    #[serde(deserialize_with = "from_string")]
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
