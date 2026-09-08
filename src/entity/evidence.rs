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
        sequence_ontology::{SequenceOntology, load_sequence_ontology_one},
    },
    query::Entity,
};

// ---- models ----
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvidenceKey {
    pub efo_ids: Vec<String>,
    pub ensembl_ids: Vec<String>,
    pub datasource_ids: Vec<Datasource>,
}

/// A labelled element, with an ID and a human-readable label.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct LabelledElement {
    /// The unique identifier of the element.
    id: String,
    /// The human-readable label of the element.
    label: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct LabelledUri {
    url: Option<String>,
    nice_name: Option<String>,
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

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct NameDescription {
    name: String,
    description: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct DiseaseCellLine {
    id: Option<String>,
    name: Option<String>,
    tissue: Option<String>,
    tissue_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct Pathway {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
struct EvidenceVariation {
    #[graphql(skip)]
    functional_consequence_id: Option<String>,
    number_mutated_samples: Option<u32>,
    number_samples_tested: Option<u32>,
    number_samples_with_mutation_type: Option<u32>,
}

#[ComplexObject]
impl EvidenceVariation {
    /// SO.
    async fn functional_consequence(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SequenceOntology>> {
        tracing::trace!(
            "functional_consequence_id: {:?}",
            &self.functional_consequence_id
        );
        match &self.functional_consequence_id {
            Some(id) => load_sequence_ontology_one(ctx, id.clone()).await,
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct EvidenceTextMiningSentence {
    d_end: u32,
    t_end: u32,
    d_start: u32,
    t_start: u32,
    section: String,
    text: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct Biomarkers {
    gene_expression: Vec<BiomarkerGeneExpression>,
    genetic_variation: Vec<GeneticVariation>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct GeneticVariation {
    id: Option<String>,
    name: Option<String>,
    functional_consequence_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
struct BiomarkerGeneExpression {
    name: Option<String>,
    id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Evidence {
    // Common fields
    id: String,
    score: f64,
    target_id: String,
    datasource_id: String,
    datatype_id: String,
    target_from_source_id: Option<String>,
    disease_from_source_mapped_id: Option<String>,
    release_date: Option<String>,
    #[allow(clippy::struct_field_names)]
    evidence_date: Option<String>,
    publication_year: Option<u16>,
    quality_controls: Vec<String>,
    warning_message: Option<String>,
    study_start_date: Option<String>,
    biological_model_id: Option<String>,
    beta: Option<f64>,
    direction_on_trait: Option<String>,
    odds_ratio_confidence_interval_upper: Option<f64>,
    beta_confidence_interval_lower: Option<f64>,
    beta_confidence_interval_upper: Option<f64>,
    biosamples_from_source: Vec<String>,
    statistical_test_tail: Option<String>,
    target_in_model: Option<String>,
    target_role: Option<String>,
    log_2_fold_change_percentile_rank: Option<i32>,
    ancestry_id: Option<String>,
    project_id: Option<String>,
    publication_first_author: Option<String>,
    crispr_screen_library: Option<String>,
    assays: Vec<Assay>,
    variant_rs_id: Option<String>,
    biological_model_genetic_background: Option<String>,
    trial_why_stopped: Option<String>,
    assessments: Vec<String>,
    disease_model_associated_model_phenotypes: Vec<LabelledElement>,
    target_modulation: Option<String>,
    cell_type: Option<String>,
    phenotypic_consequence_log_fold_change: Option<f64>,
    pathways: Vec<Pathway>,
    project_description: Option<String>,
    cohort_short_name: Option<String>,
    trial_stop_reason_categories: Vec<String>,
    clinical_report_id: Option<String>,
    resource_score: Option<f64>,
    drug_from_source: Option<String>,
    literature: Vec<String>,
    study_sample_size: Option<u32>,
    study_id: Option<String>,
    cohort_phenotypes: Vec<String>,
    biological_model_allelic_composition: Option<String>,
    cohort_id: Option<String>,
    biomarker_list: Vec<NameDescription>,
    odds_ratio: Option<f64>,
    variant_aminoacid_descriptions: Vec<String>,
    mutated_samples: Vec<EvidenceVariation>,
    text_mining_sentences: Vec<EvidenceTextMiningSentence>,
    interacting_target_from_source_id: Option<String>,
    p_value_exponent: Option<i32>,
    odds_ratio_confidence_interval_lower: Option<f64>,
    gene_interaction_type: Option<String>,
    cohort_description: Option<String>,
    interacting_target_role: Option<String>,
    cell_line_background: Option<String>,
    disease_from_source_id: Option<String>,
    reaction_id: Option<String>,
    direction_on_target: Option<String>,
    reaction_name: Option<String>,
    significant_driver_methods: Vec<String>,
    biomarkers: Biomarkers,
    study_cases: Option<u32>,
    urls: Vec<LabelledUri>,
    study_cases_with_qualifying_variants: Option<u32>,
    primary_project_id: Option<String>,
    statistical_method_overview: Option<String>,
    statistical_method: Option<String>,
    release_version: Option<String>,
    primary_project_hit: Option<bool>,
    p_value_mantissa: Option<f64>,
    biomarker_name: Option<String>,
    allele_origins: Vec<String>,
    publication_date: Option<String>,
    log_2_fold_change_value: Option<f64>,
    genetic_interaction_score: Option<f64>,
    target_from_source: Option<String>,
    clinical_significances: Vec<String>,
    disease_from_source: Option<String>,
    allelic_requirements: Vec<String>,
    study_overview: Option<String>,
    contrast: Option<String>,
    ancestry: Option<String>,
    clinical_stage: Option<String>,
    disease_cell_lines: Vec<DiseaseCellLine>,
    confidence: Option<String>,
    phenotypic_consequence_p_value: Option<f64>,

    // embedded fields
    #[graphql(skip)]
    disease_id: String,
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

            // step 1: get ids from the index table
            let ref_sql = format!(
                "SELECT id FROM evidence_by_disease_and_target \
                 WHERE diseaseId IN ? AND targetId IN ?{ds_clause}"
            );
            let mut rq = self
                .ch
                .query(&ref_sql)
                .bind(&key.efo_ids)
                .bind(&key.ensembl_ids);
            if !key.datasource_ids.is_empty() {
                rq = rq.bind(&key.datasource_ids);
            }
            let ids: Vec<String> = rq.fetch_all::<String>().await?;

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
    /// Disease.
    async fn disease(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Disease>> {
        load_disease(ctx, &self.disease_id).await
    }
}
