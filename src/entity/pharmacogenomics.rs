use std::collections::HashMap;

use async_graphql::{
    ComplexObject, Context, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        drug::{Drug, load_drug},
        sequence_ontology::{SequenceOntologyTerm, load_sequence_ontology_one},
        target::{Target, load_target},
    },
};

// ---- models ----

/// Drug with drug identifiers.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct DrugWithIdentifiers {
    /// Drug or clinical candidate identifier.
    drug_id: String,
    /// Drug identifier from the original data source.
    drug_from_source: String,
}

/// Genetic variants influencing individual drug responses. Pharmacogenetics data is integrated from
/// sources including Pharmacogenomics Knowledgebase (`PharmGKB`).
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct VariantAnnotation {
    /// Allele or genotype in the base case.
    base_allele_or_genotype: Option<String>,
    /// Allele or genotype in the comparison case.
    comparison_allele_or_genotype: Option<String>,
    /// Indicates in which direction the genetic variant increases or decreases drug response.
    directionality: Option<String>,
    /// Allele observed effect.
    effect: Option<String>,
    /// Summary of the impact of the allele on the drug response.
    effect_description: Option<String>,
    /// Type of effect.
    effect_type: Option<String>,
    /// Entity affected by the effect.
    entity: Option<String>,
    /// PubMed identifier (PMID) of the literature entry.
    literature: Option<String>,
}

/// Pharmacogenomics data linking genetic variants to drug responses. Data is integrated from
/// sources including ClinPGx.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Pharmacogenomics {
    /// Identifier for the data provider.
    datasource_id: String,
    /// Classification of the type of pharmacogenomic data (e.g., `clinical_annotation`).
    datatype_id: String,
    /// List of drugs or clinical candidates associated with the pharmacogenomic data.
    drugs: Vec<DrugWithIdentifiers>,
    /// Strength of the scientific support for the variant/drug response.
    evidence_level: String,
    /// Genetic variant configuration.
    genotype: Option<String>,
    /// Explanation of the genotype's clinical significance.
    genotype_annotation_text: Option<String>,
    /// Identifier for the specific genetic variant combination (e.g., `1_1500_A_A,T`).
    genotype_id: Option<String>,
    /// Haplotype ID in the ClinPGx dataset.
    haplotype_from_source_id: Option<String>,
    /// Combination of genetic variants that constitute a particular allele of a gene (e.g.,
    /// CYP2C9*3).
    haplotype_id: Option<String>,
    /// PubMed identifier (PMID) of the literature entry [bioregistry:pubmed].
    literature: Vec<String>,
    /// Classification of the drug response type (e.g., Toxicity).
    pgx_category: String,
    /// Phenotype identifier from the source.
    phenotype_from_source_id: Option<String>,
    /// Description of the phenotype associated with the variant.
    phenotype_text: Option<String>,
    /// Annotation details about the variant effect on drug response.
    variant_annotation: Vec<VariantAnnotation>,
    /// Identifier of the study providing the pharmacogenomic evidence.
    study_id: Option<String>,
    /// Target (gene/protein) identifier as reported by the data source.
    target_from_source_id: Option<String>,
    /// The sequence ontology identifier of the consequence of the variant based on Ensembl VEP in
    /// the context of the transcript [bioregistry:so].
    variant_functional_consequence_id: Option<String>,
    /// dbSNP rsID identifier for the variant.
    variant_rs_id: Option<String>,
    /// Variant identifier in CHROM_POS_REF_ALT notation.
    variant_id: Option<String>,
    /// Whether the target is directly affected by the variant.
    is_direct_target: bool,
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct PharmacogenomicsByVariantRow {
    variant_id: Option<String>,
    pharmacogenomics: Vec<Pharmacogenomics>,
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct PharmacogenomicsByTargetRow {
    target_from_source_id: Option<String>,
    pharmacogenomics: Vec<Pharmacogenomics>,
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct PharmacogenomicsByDrugRow {
    drug_id: String,
    pharmacogenomics: Vec<Pharmacogenomics>,
}
// ---- loaders ----

#[derive(From)]
pub struct PharmacogenomicsByTargetLoader {
    ch: ClickHouse,
}

#[derive(From)]
pub struct PharmacogenomicsByVariantLoader {
    ch: ClickHouse,
}

#[derive(From)]
pub struct PharmacogenomicsByDrugLoader {
    ch: ClickHouse,
}

impl Loader<String> for PharmacogenomicsByTargetLoader {
    type Value = Vec<Pharmacogenomics>;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<PharmacogenomicsByTargetRow> = self
            .ch
            .query("SELECT ?fields FROM pharmacogenomics_by_target WHERE targetFromSourceId IN ?")
            .bind(key)
            .fetch_all()
            .await?;
        let mut results: HashMap<String, Vec<Pharmacogenomics>> = HashMap::new();
        for row in rows {
            let id = row.target_from_source_id.unwrap_or_default().clone();
            results.entry(id).or_insert(row.pharmacogenomics);
        }
        Ok(results)
    }
}

impl Loader<String> for PharmacogenomicsByVariantLoader {
    type Value = Vec<Pharmacogenomics>;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<PharmacogenomicsByVariantRow> = self
            .ch
            .query("SELECT ?fields FROM pharmacogenomics_by_variant WHERE variantId IN ?")
            .bind(key)
            .fetch_all()
            .await?;
        let mut results: HashMap<String, Vec<Pharmacogenomics>> = HashMap::new();
        for row in rows {
            let id = row.variant_id.unwrap_or_default().clone();
            results.entry(id).or_insert(row.pharmacogenomics);
        }
        Ok(results)
    }
}

impl Loader<String> for PharmacogenomicsByDrugLoader {
    type Value = Vec<Pharmacogenomics>;
    type Error = async_graphql::Error;

    async fn load(&self, key: &[String]) -> Result<HashMap<String, Self::Value>, Self::Error> {
        let rows: Vec<PharmacogenomicsByDrugRow> = self
            .ch
            .query("SELECT ?fields FROM pharmacogenomics_by_drug WHERE drugId IN ?")
            .bind(key)
            .fetch_all()
            .await?;
        let mut results: HashMap<String, Vec<Pharmacogenomics>> = HashMap::new();
        for row in rows {
            let id = row.drug_id.clone();
            results.entry(id).or_insert(row.pharmacogenomics);
        }
        Ok(results)
    }
}

/// Loads Pharmacogenomics by the target id from the cache or database.
///
/// # Returns
/// A `Vec` of `Pharmacogenomics` objects corresponding to the given target IDs.
/// # Errors
/// Returns an error if the Pharmacogenomics could not be loaded.
pub async fn load_pharmacogenomics_by_target(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<Vec<Pharmacogenomics>>> {
    ctx.data_unchecked::<DataLoader<PharmacogenomicsByTargetLoader>>()
        .load_one(id.clone())
        .await
}

/// Loads Pharmacogenomics by the drug id from the cache or database.
///
/// # Returns
/// A `Vec` of `Pharmacogenomics` objects corresponding to the given drug IDs.
/// # Errors
/// Returns an error if the Pharmacogenomics could not be loaded.
pub async fn load_pharmacogenomics_by_drug(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<Vec<Pharmacogenomics>>> {
    ctx.data_unchecked::<DataLoader<PharmacogenomicsByDrugLoader>>()
        .load_one(id.clone())
        .await
}

/// Loads Pharmacogenomics by the variant id from the cache or database.
///
/// # Returns
/// A `Vec` of `Pharmacogenomics` objects corresponding to the given variant IDs.
/// # Errors
/// Returns an error if the Pharmacogenomics could not be loaded.
pub async fn load_pharmacogenomics_by_variant(
    ctx: &Context<'_>,
    id: String,
) -> async_graphql::Result<Option<Vec<Pharmacogenomics>>> {
    ctx.data_unchecked::<DataLoader<PharmacogenomicsByVariantLoader>>()
        .load_one(id.clone())
        .await
}

#[ComplexObject]
impl Pharmacogenomics {
    /// Target entity.
    async fn target(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Target>> {
        match &self.target_from_source_id {
            Some(target_id) => load_target(ctx, target_id.clone()).await,
            None => Ok(None),
        }
    }

    /// The sequence ontology identifier of the consequence of the variant based on Ensembl VEP in
    /// the context of the transcript [bioregistry:so]
    async fn variant_functional_consequence(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SequenceOntologyTerm>> {
        match &self.variant_functional_consequence_id {
            Some(id) => load_sequence_ontology_one(ctx, id.replace('_', ":")).await,
            None => Ok(None),
        }
    }
}

#[ComplexObject]
impl DrugWithIdentifiers {
    ///Drug or clinical candidate entity
    async fn drug(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Drug>> {
        load_drug(ctx, self.drug_id.clone()).await
    }
}
