use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        association::Datasource,
        disease::{Disease, load_disease},
        drug::{Drug, load_drug},
        gene_ontology::{GeneOntology, load_gene_ontology_one},
        sequence_ontology::{SequenceOntology, load_sequence_ontology_one},
        target::{Target, load_target},
    },
    query::Entity,
};

// ---- models ----

/// The key for filtering evidences.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvidenceKey {
    /// A list of disease ids to match on.
    pub efo_ids: Vec<String>,
    /// A list of target ids to match on.
    pub ensembl_ids: Vec<String>,
    /// A list of datasource ids to match on.
    pub datasource_ids: Vec<Datasource>,
}

/// Assays used in the study.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct Assay {
    /// Description of the assay.
    description: Option<String>,
    /// Indicates if the assay was positive or negative for the target.
    is_hit: Option<bool>,
    /// Short name of the assay.
    short_name: Option<String>,
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
    /// Raw gene expression annotation from the source.
    name: Option<String>,
    /// Gene Ontology (GO) identifiers of regulation or background expression processes
    /// [bioregistry:go].
    #[graphql(skip)]
    id: Option<String>,
}

#[ComplexObject]
impl BiomarkerGeneExpression {
    async fn id(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<GeneOntology>> {
        match &self.id {
            Some(id) => load_gene_ontology_one(ctx, id.clone()).await,
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
/// List of gene expression altering biomarkers.
struct GeneticVariation {
    /// Variant identifier.
    id: Option<String>,
    /// Name of the variant biomarker.
    name: Option<String>,
    /// Functional consequene identifier of the variant biomarker [bioregistry:so].
    #[graphql(skip)]
    functional_consequence_id: Option<String>,
}

#[ComplexObject]
impl GeneticVariation {
    // Functional consequence identifier of the pooled mutations [bioregistry:so].
    async fn functional_consequence(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SequenceOntology>> {
        match &self.functional_consequence_id {
            Some(id) => load_sequence_ontology_one(ctx, id.clone()).await,
            None => Ok(None),
        }
    }
}

/// Cancer cell lines used to generate evidence.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct DiseaseCellLine {
    /// Cell type identifier in cell ontology or in cell model database.
    id: Option<String>,
    /// Name of the cell model.
    name: Option<String>,
    /// Name of the tissue from which the cells were sampled.
    tissue: Option<String>,
    /// Anatomical identifier of the sampled organ/tissue [bioregistry:uberon].
    tissue_id: Option<String>,
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
    number_mutated_samples: Option<u32>,
    /// Number of cohort samples tested.
    number_samples_tested: Option<u32>,
    /// Number of cohort samples in which target is mutated with a specific mutation type.
    number_samples_with_mutation_type: Option<u32>,
}

#[ComplexObject]
impl EvidenceVariation {
    /// Functional consequene of the pooled mutations [bioregistry:so].
    async fn functional_consequence(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SequenceOntology>> {
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
    /// End position of the target label in the sentence.
    t_end: u32,
    /// Start position of the disease label in the sentence.
    d_start: u32,
    /// Start position of the target label in the sentence.
    t_start: u32,
    /// Section of the publication the sentence was extracted from.
    section: String,
    /// Text of the sentence with the disease/target relationship.
    text: String,
}

/// An element with a label.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct LabelledElement {
    /// The unique identifier of the element.
    id: String,
    /// The human-readable label of the element.
    label: String,
}

/// Reference to linked external resource (e.g. clinical trials, studies, package inserts, reports,
/// etc.).
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct LabelledUri {
    /// URL to remote resource.
    url: Option<String>,
    /// Human readable label of the remote reference.
    nice_name: Option<String>,
}

/// An element with name and description.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct NameDescription {
    /// Name of the element.
    name: String,
    /// Description of the element.
    description: String,
}

/// Pathway annotations for a target.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct Pathway {
    /// Name of the pathway.
    id: Option<String>,
    /// Unique identifier for the pathway.
    name: Option<String>,
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
    assays: Vec<Assay>,
    /// Short name of the assay.
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
    biomarker_list: Vec<NameDescription>,
    /// Altered characteristics that influences the disease process.
    biomarker_name: Option<String>,
    /// List of biomarkers.
    biomarkers: Biomarkers,
    /// Identifier of the referenced biological material.
    biosamples_from_source: Vec<String>,
    /// Background of the derived cell lines.
    cell_line_background: Option<String>,
    /// The studied cell type. Preferably the cell line ontology label.
    cell_type: Option<String>,
    /// Identifier of the clinical report.
    clinical_report_id: Option<String>,
    /// Standard terms to define clinical significance.
    clinical_significances: Vec<String>,
    /// Clinical stage of the drug-disease pair.
    clinical_stage: Option<String>,
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
    datasource_id: String,
    /// Datatype of the evidence.
    datatype_id: String,
    /// Gain or loss of function effect of the evidence on the target resulting from genetic
    /// variants, pharmacological modulation, or other perturbations.
    direction_on_target: Option<String>,
    /// Predicted direction of effect on the trait.
    direction_on_trait: Option<String>,
    /// Cancer cell lines used to generate evidence.
    disease_cell_lines: Vec<DiseaseCellLine>,
    /// Disease label from the original source.
    disease_from_source: Option<String>,
    /// Disease identifier from the original source.
    disease_from_source_id: Option<String>,
    /// Mapped Open Targets disease identifier.
    disease_from_source_mapped_id: Option<String>,
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
    gene_interaction_type: Option<String>,
    /// The strength of the genetic interaction. Directionality is captured as well: antagonistics
    /// < 0 < cooperative.
    genetic_interaction_score: Option<f64>,
    /// Identifer of the interacting target.
    interacting_target_from_source_id: Option<String>,
    /// Role of a target in the genetic interaction test.
    interacting_target_role: Option<String>,
    /// List of PubMed or preprint reference identifiers.
    literature: Vec<String>,
    /// Percentile of top differentially regulated genes (transcripts) within experiment.
    log_2_fold_change_percentile_rank: Option<i32>,
    /// Log 2 fold expression change in contrast experiment.
    log_2_fold_change_value: Option<f64>,
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
    phenotypic_consequence_log_fold_change: Option<f64>,
    /// P-value of the the cell survival test.
    phenotypic_consequence_p_value: Option<f64>,
    /// If a given target was found to be a hit in the primary project.
    primary_project_hit: Option<bool>,
    /// Open Targets project identifier of the primary project.
    primary_project_id: Option<String>,
    /// Description of the project that generated the data.
    project_description: Option<String>,
    /// The identifer of the project that generated the data.
    project_id: Option<String>,
    /// Date of the earliest publication supporting the evidence.
    publication_date: Option<String>,
    /// Last name and initials of the author of the publication that references the study.
    publication_first_author: Option<String>,
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
    target_from_source_id: Option<String>,
    #[graphql(skip)]
    target_id: String,
    /// Target name/synonym in animal model.
    target_in_model: Option<String>,
    /// Description of target modulation event.
    target_modulation: Option<String>,
    /// Role of a target in the genetic interaction test.
    target_role: Option<String>,
    /// Sentences of a publication supporting the disease/target relationship.
    text_mining_sentences: Vec<EvidenceTextMiningSentence>,
    /// Categorised reason(s) why the trial was stopped.
    trial_stop_reason_categories: Vec<String>,
    /// Reason why the trial was stopped, as reported.
    trial_why_stopped: Option<String>,
    /// Reference to linked external resource (e.g. clinical trials, studies, package inserts,
    /// reports, etc.).
    urls: Vec<LabelledUri>,
    /// Descriptions of variant consequences at protein level.
    variant_aminoacid_descriptions: Vec<String>,
    #[graphql(skip)]
    variant_functional_consequence_from_qtl_id: Option<String>,
    #[graphql(skip)]
    variant_functional_consequence_id: Option<String>,
    /// Variant reference SNP cluster ID (Rsid).
    variant_rs_id: Option<String>,
    // DEPRECATED - This is always empty in the data.
    #[graphql(deprecation = "empty")]
    warning_message: Option<String>,
}

// ---- query utilities ----

impl Entity for Evidence {
    fn id(&self) -> &str { &self.id }
}

// ---- loaders ----

pub struct EvidenceLoader {
    ch: ClickHouse,
}

impl EvidenceLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
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
                .query("SELECT ?fields FROM evidence WHERE id IN ?")
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

#[ComplexObject]
impl Evidence {
    /// Disease for which the target is associated in this evidence.
    async fn disease(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Disease>> {
        load_disease(ctx, &self.disease_id).await
    }

    /// Target for which the disease is associated in this evidence.
    async fn target(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Target>> {
        load_target(ctx, &self.target_id).await
    }

    /// Drug or clinical candidate targeting the target and studied/approved for the specific
    /// disease as potential indication [bioregistry:chembl].
    async fn drug(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Drug>> {
        match &self.drug_id {
            Some(drug_id) => return load_drug(ctx, drug_id).await,
            None => Ok(None),
        }
    }

    /// Observed patterns of drug response.
    async fn drug_response(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Drug>> {
        match &self.drug_response {
            Some(drug_response) => return load_drug(ctx, drug_response).await,
            None => Ok(None),
        }
    }

    /// Sequence ontology (SO) term of the functional consequence of the variant.
    async fn variant_functional_consequence(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SequenceOntology>> {
        match &self.variant_functional_consequence_id {
            Some(id) => load_sequence_ontology_one(ctx, id.clone()).await,
            None => Ok(None),
        }
    }

    /// Sequence ontology (SO) term of the functional consequence of the variant from QTL.
    async fn variant_functional_consequence_from_qtl(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SequenceOntology>> {
        match &self.variant_functional_consequence_from_qtl_id {
            Some(id) => load_sequence_ontology_one(ctx, id.clone()).await,
            None => Ok(None),
        }
    }
}
