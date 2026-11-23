use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, Enum, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::{Deserialize, Deserializer, de::Error as _};
use serde_repr::Deserialize_repr;
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        association::Datasource,
        clinical_report::ClinicalStage,
        credible_set::{CredibleSet, load_credible_set},
        disease::{Disease, load_disease},
        drug::{Drug, load_drug},
        gene_ontology::{GeneOntology, load_gene_ontology_one},
        sequence_ontology::{SequenceOntologyTerm, load_sequence_ontology_one},
        target::{Target, load_target},
        variant::{Variant, load_variant},
    },
    query::Entity,
};

// ---- models ----

/// Datatype of an evidence.
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
#[graphql(rename_items = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Datatype {
    /// Affected pathway.
    AffectedPathway = 1,
    /// Animal model.
    AnimalModel = 2,
    /// Clinical.
    Clinical = 3,
    /// Genetic association.
    GeneticAssociation = 4,
    /// Genetic literature.
    GeneticLiterature = 5,
    /// Known drug.
    KnownDrug = 6,
    /// Literature.
    Literature = 7,
    /// Open Targets partner.
    OtPartner = 8,
    /// Open Targets validation lab.
    OtValidationLab = 9,
    /// RNA expression.
    RnaExpression = 10,
    /// Somatic mutation.
    SomaticMutation = 11,
}

/// Gain or loss of function effect on the target.
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
pub enum DirectionOnTarget {
    /// Gain of function.
    #[graphql(name = "GoF")]
    GoF = 1,
    /// Loss of function.
    #[graphql(name = "LoF")]
    LoF = 2,
}

/// Direction of effect on the trait.
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
#[graphql(rename_items = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum DirectionOnTrait {
    /// Protective effect.
    Protect = 1,
    /// Risk effect.
    Risk = 2,
}

/// Section of a Europe PMC publication.
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
#[graphql(rename_items = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum EuropePmcSection {
    /// Abstract.
    Abstract = 1,
    /// Appendix.
    Appendix = 2,
    /// Case report.
    Case = 3,
    /// Conclusions.
    Concl = 4,
    /// Discussion.
    Discuss = 5,
    /// Figure caption.
    Figure = 6,
    /// Introduction.
    Intro = 7,
    /// Other section.
    Other = 8,
    /// Results.
    Results = 9,
    /// Table.
    Table = 10,
    /// Title.
    Title = 11,
}

/// Category of the reason a clinical trial stopped.
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
pub enum TrialStopReasonCategory {
    /// Another study.
    #[graphql(name = "Another_Study")]
    #[strum(serialize = "Another_Study")]
    AnotherStudy = 1,
    /// Business or administrative reasons.
    #[graphql(name = "Business_Administrative")]
    #[strum(serialize = "Business_Administrative")]
    BusinessAdministrative = 2,
    /// COVID-19.
    #[graphql(name = "Covid19")]
    #[strum(serialize = "Covid19")]
    Covid19 = 3,
    /// Endpoint met.
    #[graphql(name = "Endpoint_Met")]
    #[strum(serialize = "Endpoint_Met")]
    EndpointMet = 4,
    /// Ethical reason.
    #[graphql(name = "Ethical_Reason")]
    #[strum(serialize = "Ethical_Reason")]
    EthicalReason = 5,
    /// Insufficient data.
    #[graphql(name = "Insufficient_Data")]
    #[strum(serialize = "Insufficient_Data")]
    InsufficientData = 6,
    /// Insufficient enrollment.
    #[graphql(name = "Insufficient_Enrollment")]
    #[strum(serialize = "Insufficient_Enrollment")]
    InsufficientEnrollment = 7,
    /// Interim analysis.
    #[graphql(name = "Interim_Analysis")]
    #[strum(serialize = "Interim_Analysis")]
    InterimAnalysis = 8,
    /// Invalid reason.
    #[graphql(name = "Invalid_Reason")]
    #[strum(serialize = "Invalid_Reason")]
    InvalidReason = 9,
    /// Logistics or resources.
    #[graphql(name = "Logistics_Resources")]
    #[strum(serialize = "Logistics_Resources")]
    LogisticsResources = 10,
    /// Negative result.
    #[graphql(name = "Negative")]
    #[strum(serialize = "Negative")]
    Negative = 11,
    /// No context.
    #[graphql(name = "No_Context")]
    #[strum(serialize = "No_Context")]
    NoContext = 12,
    /// Regulatory.
    #[graphql(name = "Regulatory")]
    #[strum(serialize = "Regulatory")]
    Regulatory = 13,
    /// Safety or side effects.
    #[graphql(name = "Safety_Sideeffects")]
    #[strum(serialize = "Safety_Sideeffects")]
    SafetySideeffects = 14,
    /// Study design.
    #[graphql(name = "Study_Design")]
    #[strum(serialize = "Study_Design")]
    StudyDesign = 15,
    /// Study staff moved.
    #[graphql(name = "Study_Staff_Moved")]
    #[strum(serialize = "Study_Staff_Moved")]
    StudyStaffMoved = 16,
    /// Success.
    #[graphql(name = "Success")]
    #[strum(serialize = "Success")]
    Success = 17,
    /// Uncategorised.
    #[graphql(name = "Uncategorised")]
    #[strum(serialize = "Uncategorised")]
    Uncategorised = 18,
}

/// Decodes a `datasourceId` `Enum8` into its [`Datasource`].
fn datasource_from_enum8<'de, D: Deserializer<'de>>(d: D) -> Result<Datasource, D::Error> {
    let n = i8::deserialize(d)?;
    Datasource::iter()
        .find(|ds| *ds as i8 == n)
        .ok_or_else(|| D::Error::custom(format!("unknown datasourceId {n}")))
}

/// Assays used in the study.
#[cfg(feature = "product-ppp")]
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct Assay {
    /// Description of the assay.
    description: String,
    /// Indicates if the assay was positive or negative for the target.
    is_hit: bool,
    /// Short name of the assay.
    short_name: String,
}

/// List of biomarkers.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct Biomarkers {
    /// List of gene expression altering biomarkers.
    gene_expression: Vec<BiomarkerGeneExpression>,
    /// List of genetic variation biomarkers.
    genetic_variation: Vec<GeneticVariation>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
struct BiomarkerGeneExpression {
    /// Gene Ontology (GO) identifiers of regulation or background expression processes
    /// [bioregistry:go].
    #[graphql(skip)]
    id: String,
    /// Raw gene expression annotation from the source.
    name: String,
}

#[ComplexObject]
impl BiomarkerGeneExpression {
    async fn id(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<GeneOntology>> {
        load_gene_ontology_one(ctx, self.id.clone()).await
    }
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
/// List of gene expression altering biomarkers.
struct GeneticVariation {
    /// Functional consequene identifier of the variant biomarker [bioregistry:so].
    #[graphql(skip)]
    functional_consequence_id: Option<String>,
    /// Variant identifier.
    id: Option<String>,
    /// Name of the variant biomarker.
    name: String,
}

#[ComplexObject]
impl GeneticVariation {
    // Functional consequence identifier of the pooled mutations [bioregistry:so].
    async fn functional_consequence(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SequenceOntologyTerm>> {
        match &self.functional_consequence_id {
            Some(id) => load_sequence_ontology_one(ctx, id.clone()).await,
            None => Ok(None),
        }
    }
}

/// Cancer cell lines used to generate evidence.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DiseaseCellLine {
    /// Cell type identifier in cell ontology or in cell model database.
    id: String,
    /// Name of the cell model.
    name: String,
    /// Name of the tissue from which the cells were sampled.
    tissue: String,
    /// Anatomical identifier of the sampled organ/tissue [bioregistry:uberon].
    tissue_id: String,
}

/// Mutation counts for a single functional consequence, across a tested sample cohort.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
struct EvidenceVariation {
    /// Functional consequene identifier of the pooled mutations [bioregistry:so].
    #[graphql(skip)]
    functional_consequence_id: Option<String>,
    /// Number of cohort samples in which target is mutated with a mutation of any type.
    number_mutated_samples: u32,
    /// Number of cohort samples tested.
    number_samples_tested: u32,
    /// Number of cohort samples in which target is mutated with a specific mutation type.
    number_samples_with_mutation_type: Option<u32>,
}

#[ComplexObject]
impl EvidenceVariation {
    /// Functional consequene of the pooled mutations [bioregistry:so].
    async fn functional_consequence(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SequenceOntologyTerm>> {
        match &self.functional_consequence_id {
            Some(id) => load_sequence_ontology_one(ctx, id.clone()).await,
            None => Ok(None),
        }
    }
}

/// Sentences of a publication supporting the disease/target relationship.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct EvidenceTextMiningSentence {
    /// End position of the disease label in sentence.
    d_end: u32,
    /// Start position of the disease label in the sentence.
    d_start: u32,
    /// Section of the publication the sentence was extracted from.
    section: EuropePmcSection,
    /// End position of the target label in the sentence.
    t_end: u32,
    /// Start position of the target label in the sentence.
    t_start: u32,
    /// Text of the sentence with the disease/target relationship.
    text: String,
}

/// An element with a label.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct LabelledElement {
    /// The unique identifier of the element.
    id: String,
    /// The human-readable label of the element.
    label: String,
}

/// Reference to linked external resource (e.g. clinical trials, studies, package inserts, reports,
/// etc.).
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct LabelledUri {
    /// Human readable label of the remote reference.
    nice_name: Option<String>,
    /// URL to remote resource.
    url: String,
}

/// An element with name and description.
#[cfg(feature = "product-ppp")]
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct NameDescription {
    /// Description of the element.
    description: String,
    /// Name of the element.
    name: String,
}

/// Pathway annotations for a target.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Pathway {
    /// Unique identifier for the pathway.
    id: String,
    /// Name of the pathway.
    name: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Evidence {
    /// Identifer of the disease/target evidence.
    id: String,
    /// Origin of the variant allele.
    allele_origins: Vec<String>,
    /// Inheritance patterns.
    allelic_requirements: Vec<String>,
    /// Genetic origin of a population.
    ancestry: Option<String>,
    /// Identifier of the ancestry in the HANCESTRO ontology [bioregistry:hancestro].
    ancestry_id: Option<String>,
    /// Assays used in the study.
    #[cfg(feature = "product-ppp")]
    assays: Vec<Assay>,
    /// Short name of the assay.
    #[cfg(feature = "product-ppp")]
    assessments: Vec<String>,
    /// Effect size of numeric traits.
    beta: Option<f64>,
    /// Lower value of the confidence interval.
    beta_confidence_interval_lower: Option<f64>,
    /// Upper value of the confidence interval.
    beta_confidence_interval_upper: Option<f64>,
    /// Allelic composition of the model organism.
    biological_model_allelic_composition: Option<String>,
    /// Genetic background of the model organism.
    biological_model_genetic_background: Option<String>,
    /// Identifier of the biological model (eg. in MGI).
    biological_model_id: Option<String>,
    /// List of biomarkers associated with the biological model.
    #[cfg(feature = "product-ppp")]
    biomarker_list: Vec<NameDescription>,
    /// Altered characteristics that influences the disease process.
    biomarker_name: Option<String>,
    /// List of biomarkers.
    biomarkers: Biomarkers,
    /// Identifier of the referenced biological material.
    biosamples_from_source: Vec<String>,
    /// Background of the derived cell lines.
    #[cfg(feature = "product-ppp")]
    cell_line_background: Option<String>,
    /// The studied cell type. Preferably the cell line ontology label.
    cell_type: Option<String>,
    /// Identifier of the clinical report.
    clinical_report_id: Option<String>,
    /// Standard terms to define clinical significance.
    clinical_significances: Vec<String>,
    /// Clinical stage of the drug-disease pair.
    clinical_stage: Option<ClinicalStage>,
    /// Description of the studied cohort.
    cohort_description: Option<String>,
    /// Identifier of the studied cohort
    cohort_id: Option<String>,
    /// Clinical features/phenotypes observed in studied individuals.
    cohort_phenotypes: Vec<String>,
    /// Short name of the studied cohort.
    cohort_short_name: Option<String>,
    /// Confidence qualifier on the reported evidence.
    confidence: Option<String>,
    /// Experiment contrast.
    contrast: Option<String>,
    /// Applied screening library in the CRISPR/Cas9 project.
    crispr_screen_library: Option<String>,
    /// Identifer of the evidence source.
    #[serde(deserialize_with = "datasource_from_enum8")]
    datasource_id: Datasource,
    /// Datatype of the evidence.
    datatype_id: Datatype,
    /// Gain or loss of function effect of the evidence on the target resulting from genetic
    /// variants, pharmacological modulation, or other perturbations.
    direction_on_target: Option<DirectionOnTarget>,
    /// Predicted direction of effect on the trait.
    direction_on_trait: Option<DirectionOnTrait>,
    /// Cancer cell lines used to generate evidence.
    disease_cell_lines: Vec<DiseaseCellLine>,
    /// Disease label from the original source.
    disease_from_source: Option<String>,
    /// Disease identifier from the original source.
    disease_from_source_id: Option<String>,
    /// Mapped Open Targets disease identifier.
    disease_from_source_mapped_id: String,
    #[graphql(skip)]
    disease_id: String,
    /// Human phenotypes equivalent to those observed in animal models.
    disease_model_associated_human_phenotypes: Vec<LabelledElement>,
    /// Phenotypes observed in genetically modified animal models.
    disease_model_associated_model_phenotypes: Vec<LabelledElement>,
    /// Drug name/family in resource of origin.
    drug_from_source: Option<String>,
    #[graphql(skip)]
    drug_id: Option<String>,
    #[graphql(skip)]
    drug_response: Option<String>,
    /// Earliest data for the evidence.
    #[allow(clippy::struct_field_names)]
    evidence_date: Option<String>,
    /// Description of the interaction between the two genes.
    #[cfg(feature = "product-ppp")]
    genetic_interaction_type: Option<String>,
    /// The strength of the genetic interaction. Directionality is captured as well: antagonistics
    /// < 0 < cooperative.
    #[cfg(feature = "product-ppp")]
    genetic_interaction_score: Option<f64>,
    /// Identifer of the interacting target.
    #[cfg(feature = "product-ppp")]
    interacting_target_from_source_id: Option<String>,
    /// Role of a target in the genetic interaction test.
    #[cfg(feature = "product-ppp")]
    interacting_target_role: Option<String>,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
    /// Percentile of top differentially regulated genes (transcripts) within experiment.
    log_2_fold_change_percentile_rank: Option<i32>,
    /// Samples with a given mutation tested.
    mutated_samples: Vec<EvidenceVariation>,
    /// Size of effect captured as odds ratio.
    odds_ratio: Option<f64>,
    /// Lower value of the confidence interval for odds ratio.
    odds_ratio_confidence_interval_lower: Option<f64>,
    /// Upper value of the confidence interval for odds ratio.
    odds_ratio_confidence_interval_upper: Option<f64>,
    /// Exponent of the p-value.
    p_value_exponent: Option<i32>,
    /// Mantissa of the p-value.
    p_value_mantissa: Option<f64>,
    /// List of pooled pathways.
    pathways: Vec<Pathway>,
    /// Log 2 fold change of the cell survival.
    #[cfg(feature = "product-ppp")]
    phenotypic_consequence_log_fold_change: Option<f64>,
    /// P-value of the the cell survival test.
    #[cfg(feature = "product-ppp")]
    phenotypic_consequence_p_value: Option<f64>,
    /// Score of the phenotypic consequence of the target perturbation in the CRISPR screen.
    phenotypic_consequence_score: Option<f64>,
    /// If a given target was found to be a hit in the primary project.
    #[cfg(feature = "product-ppp")]
    primary_project_hit: Option<bool>,
    /// Open Targets project identifier of the primary project.
    #[cfg(feature = "product-ppp")]
    primary_project_id: Option<String>,
    /// Description of the project that generated the data.
    #[cfg(feature = "product-ppp")]
    project_description: Option<String>,
    /// The identifer of the project that generated the data.
    project_id: Option<String>,
    /// Date of the earliest publication supporting the evidence.
    publication_date: Option<String>,
    /// Year of publication.
    publication_year: Option<u16>,
    /// Evidence quality flags.
    quality_controls: Vec<String>,
    /// Pathway, gene set or reaction identifier in Reactome.
    reaction_id: Option<String>,
    /// Name of the reaction, patway or gene set in Reactome.
    reaction_name: Option<String>,
    /// Date of the release of the data in a 'YYYY-MM-DD' format.
    release_date: Option<String>,
    /// Open Targets data release version
    release_version: Option<String>,
    /// Score provided by datasource indicating strength of target-disease association.
    resource_score: Option<f64>,
    /// Score of the evidence reflecting the strength of the disease/target relationship.
    score: f64,
    /// Methods to detect cancer driver genes producing significant results.
    significant_driver_methods: Vec<String>,
    /// Statistical method used to calculate the association.
    statistical_method: Option<String>,
    /// Overview of the statistical method used to calculate the association.
    statistical_method_overview: Option<String>,
    /// End of the distribution the target was picked from.
    statistical_test_tail: Option<String>,
    /// Number of cases in case-control study.
    study_cases: Option<u32>,
    /// Number of cases in case-control study that carry at least one allele of the qualifying
    /// variant.
    study_cases_with_qualifying_variants: Option<u32>,
    /// Identifier of the study generating the data.
    study_id: Option<String>,
    /// Description of the study.
    study_overview: Option<String>,
    /// Sample size of study.
    study_sample_size: Option<u32>,
    /// Start date of study in a YYYY-MM-DD format.
    study_start_date: Option<String>,
    /// Target name/synonym or non HGNC symbol in resource of origin.
    target_from_source: Option<String>,
    /// Target ID in resource of origin (accepted sources include Ensembl gene ID, Uniprot ID, gene
    /// symbol), only capital letters are accepted.
    target_from_source_id: String,
    #[graphql(skip)]
    target_id: String,
    /// Target name/synonym in animal model.
    target_in_model: Option<String>,
    /// Description of target modulation event.
    target_modulation: Option<String>,
    /// Role of a target in the genetic interaction test.
    #[cfg(feature = "product-ppp")]
    target_role: Option<String>,
    /// Sentences of a publication supporting the disease/target relationship.
    text_mining_sentences: Vec<EvidenceTextMiningSentence>,
    /// Categorised reason(s) why the trial was stopped.
    trial_stop_reason_categories: Vec<TrialStopReasonCategory>,
    /// Reason why the trial was stopped, as reported.
    trial_why_stopped: Option<String>,
    /// Reference to linked external resource (e.g. clinical trials, studies, package inserts,
    /// reports, etc.).
    urls: Vec<LabelledUri>,
    /// Descriptions of variant consequences at protein level.
    variant_aminoacid_descriptions: Vec<String>,
    #[graphql(skip)]
    variant_functional_consequence_id: Option<String>,
    /// Variant reference SNP cluster ID (Rsid).
    variant_rs_id: Option<String>,
    #[graphql(skip)]
    variant_id: Option<String>,
    #[graphql(skip)]
    study_locus_id: Option<String>,
}

// ---- query utilities ----

impl Entity for Evidence {
    fn id(&self) -> &str { &self.id }
}

// ---- loaders ----

/// The key for filtering evidences.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvidenceKey {
    pub efo_ids: Vec<String>,
    pub ensembl_ids: Vec<String>,
    pub datasource_ids: Vec<Datasource>,
}

#[derive(From)]
pub struct EvidenceLoader {
    ch: ClickHouse,
}

impl Loader<EvidenceKey> for EvidenceLoader {
    type Value = Vec<Evidence>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[EvidenceKey],
    ) -> Result<HashMap<EvidenceKey, Vec<Evidence>>, async_graphql::Error> {
        let mut out = HashMap::with_capacity(keys.len());
        for key in keys {
            let ds_clause = if key.datasource_ids.is_empty() {
                ""
            } else {
                " AND datasourceId IN ?"
            };

            // step 1: propagate disease descendants and then
            // get their ids from the index table
            let ref_sql = format!(
                "SELECT id FROM evidence_by_disease_and_target \
                 WHERE diseaseId IN ( \
                     SELECT arrayJoin(arrayConcat(descendants, [id])) \
                     FROM disease WHERE id IN ? \
                 ) AND targetId IN ?{ds_clause}"
            );
            let mut rq = self
                .ch
                .query(&ref_sql)
                .bind(&key.efo_ids)
                .bind(&key.ensembl_ids);
            if !key.datasource_ids.is_empty() {
                rq = rq.bind(&key.datasource_ids);
            }
            tracing::trace!("query is {}", ref_sql);
            let ids: Vec<String> = rq.fetch_all::<String>().await?;
            tracing::trace!("ids: {:?}", ids);

            if ids.is_empty() {
                out.insert(key.clone(), Vec::new());
                continue;
            }

            // step 2: select those ids in the evidence table
            let rows = self
                .ch
                .query("SELECT ?fields FROM evidence WHERE id IN ? ORDER BY score DESC, id ASC")
                .bind(&ids)
                .fetch_all::<Evidence>()
                .await?;
            out.insert(key.clone(), rows);
        }
        Ok(out)
    }
}

/// Loads evidences by their IDs.
///
/// this function uses a [`DataLoader`] to fetch evidences from the cache or database.
///
/// # Returns
/// A `Vec` of [`Evidence`] objects.
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
pub async fn load_evidences(
    ctx: &Context<'_>,
    key: EvidenceKey,
) -> async_graphql::Result<Vec<Evidence>> {
    ctx.data_unchecked::<DataLoader<EvidenceLoader>>()
        .load_one(key)
        .await
        .map(Option::unwrap_or_default)
}

/// The key for loading the evidences of a single variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvidenceVariantKey {
    pub variant_id: String,
    pub datasource_ids: Vec<Datasource>,
}

#[derive(From)]
pub struct EvidenceVariantLoader {
    ch: ClickHouse,
}

impl Loader<EvidenceVariantKey> for EvidenceVariantLoader {
    type Value = Vec<Evidence>;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[EvidenceVariantKey],
    ) -> Result<HashMap<EvidenceVariantKey, Vec<Evidence>>, async_graphql::Error> {
        let mut out = HashMap::with_capacity(keys.len());
        for key in keys {
            let ds_clause = if key.datasource_ids.is_empty() {
                ""
            } else {
                " AND datasourceId IN ?"
            };

            // step 1: get the evidence ids for this variant from the join table
            let ref_sql = format!(
                "SELECT id FROM evidence_by_variant \
                WHERE variantId = ?{ds_clause}"
            );
            let mut rq = self.ch.query(&ref_sql).bind(&key.variant_id);
            if !key.datasource_ids.is_empty() {
                rq = rq.bind(&key.datasource_ids);
            }
            tracing::trace!("query is {}", ref_sql);
            let ids: Vec<String> = rq.fetch_all::<String>().await?;
            tracing::trace!("ids: {:?}", ids);

            if ids.is_empty() {
                out.insert(key.clone(), Vec::new());
                continue;
            }

            // step 2: select those ids in the evidence table
            let rows = self
                .ch
                .query("SELECT ?fields FROM evidence WHERE id IN ? ORDER BY score DESC, id ASC")
                .bind(&ids)
                .fetch_all::<Evidence>()
                .await?;
            out.insert(key.clone(), rows);
        }
        Ok(out)
    }
}

/// Loads evidences by a variant.
///
/// this function uses a [`DataLoader`] to fetch evidences from the cache or database.
///
/// # Returns
/// A `Vec` of [`Evidence`] objects.
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
pub async fn load_evidences_by_variant(
    ctx: &Context<'_>,
    key: EvidenceVariantKey,
) -> async_graphql::Result<Vec<Evidence>> {
    ctx.data_unchecked::<DataLoader<EvidenceVariantLoader>>()
        .load_one(key)
        .await
        .map(Option::unwrap_or_default)
}

#[ComplexObject]
impl Evidence {
    /// Disease for which the target is associated in this evidence.
    async fn disease(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Disease>> {
        load_disease(ctx, self.disease_id.clone()).await
    }

    /// Credible set supporting this evidence.
    async fn credible_set(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<CredibleSet>> {
        match &self.study_locus_id {
            Some(id) => load_credible_set(ctx, id.clone()).await,
            None => Ok(None),
        }
    }

    /// Variant supporting the relationship between the target and the disease.
    async fn variant(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Variant>> {
        match &self.variant_id {
            Some(id) => load_variant(ctx, id.clone()).await,
            None => Ok(None),
        }
    }

    /// Target for which the disease is associated in this evidence.
    async fn target(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Target>> {
        load_target(ctx, self.target_id.clone()).await
    }

    /// Drug or clinical candidate targeting the target and studied/approved for the specific
    /// disease as potential indication [bioregistry:chembl].
    async fn drug(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Drug>> {
        match &self.drug_id {
            Some(drug_id) => load_drug(ctx, drug_id.clone()).await,
            None => Ok(None),
        }
    }

    /// Observed patterns of drug response.
    async fn drug_response(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Drug>> {
        match &self.drug_response {
            Some(drug_response) => load_drug(ctx, drug_response.clone()).await,
            None => Ok(None),
        }
    }

    /// Sequence ontology (SO) term of the functional consequence of the variant.
    async fn variant_functional_consequence(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SequenceOntologyTerm>> {
        match &self.variant_functional_consequence_id {
            Some(id) => load_sequence_ontology_one(ctx, id.clone()).await,
            None => Ok(None),
        }
    }
}
