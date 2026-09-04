use std::{char::CharTryFromError, cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;
use serde_repr::Deserialize_repr;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{protein_coding_coordinates::{ProteinCodingCoordinates, ProteinCodingCoordinateVariantLoader}, sequence_ontology::{SequenceOntologyTerm, SequenceOntologyTermLoader}},
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        // search::Searchable,
        // sort::{Sort, SortKey},
    },
};

// ---- models ----
/// Chromosome type.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Enum, Deserialize_repr)]
#[repr(i8)]
#[graphql(rename_items = "lowercase")]
pub enum Chromosome {
    Chr1 = 1,
    Chr2 = 2,
    Chr3 = 3,
    Chr4 = 4,
    Chr5 = 5,
    Chr6 = 6,
    Chr7 = 7,
    Chr8 = 8,
    Chr9 = 9,
    Chr10 = 10,
    Chr11 = 11,
    Chr12 = 12,
    Chr13 = 13,
    Chr14 = 14,
    Chr15 = 15,
    Chr16 = 16,
    Chr17 = 17,
    Chr18 = 18,
    Chr19 = 19,
    Chr20 = 20,
    Chr21 = 21,
    Chr22 = 22,
    ChrX = 23,
    ChrY = 24,
    ChrMT = 25,
}

/// Predicted or measured effect of the variant based on various methods.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct VariantEffect {
    /// Method name used to predict the effect of the variant.
    method: Option<String>,
    /// Textual assessment of the variant effect.
    assessment: Option<String>,
    /// Score of the variant effect.
    score: Option<f64>,
    /// Flagging if the variant effect is considered pathogenic.
    assessment_flag: Option<String>,
    /// Target identifier on which the variant effect is interpreted [bioregistry:ensembl].
    target_id: Option<String>,
    /// Variant effect normalised between -1 and 1.
    normalised_score: Option<f64>,
}

/// Predicted consequences on transcript context.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptConsequence {
    /// The sequence ontology identifier of the consequence of the variant based on Ensembl VEP in the context of the transcript [bioregistry:so].
    variant_functional_consequence_ids: Vec<String>,
    /// Amino acid change caused by this variant on this gene.
    amino_acid_change: Option<String>,
    /// Uniprot identifiers of the gene product [bioregistry:uniprot].
    uniprot_accessions: Vec<String>,
    /// Flagging if the transcript is the canonical transcript for the gene.
    is_ensembl_canonical: bool,
    /// Affected codon in the transcript.
    codons: Option<String>,
    /// Distance of the variant from the transcript.
    distance_from_footprint: i32,
    /// Distance of the variant from the transcription start site.
    distance_from_tss: i32,
    /// Open Target target identifier of the transcript [bioregistry:ensembl].
    target_id: Option<String>,
    /// Ensembl VEP predicted impact of the variant on the transcript.
    impact: Option<String>,
    /// Ensembl transcript identifier [bioregistry:ensembl].
    transcript_id: Option<String>,
    /// Loss-of-function prediction based on LOFTEE.
    loftee_prediction: Option<String>,
    /// SIFT prediction of the variant impact on the transcript.
    sift_prediction: Option<f64>,
    /// Polyphen prediction of the variant impact on the transcript.
    polyphen_prediction: Option<f64>,
    /// Index of the transcript in the list of transcripts around the gene.
    transcript_index: u32,
    /// Score assigned to transcript based on Ensembl VEP consequence.
    consequence_score: f64,
}

/// Cross-references for the variant in different databases.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DbXref {
    /// Identifier of the variant in the given database.
    id: Option<String>,
    /// Name of the database the variant is referenced in.
    source: Option<String>,
}

/// Allele frequencies of the variant in different populations.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct AlleleFrequency {
    /// Name of the population.
    population_name: Option<String>,
    /// Frequency of the alternate allele in the population.
    allele_frequency: Option<f64>,
}

/// Core variant information for all variants in the Platform.
/// Variants are included if any phenotypic information is available for the variant,
/// including GWAS or molQTL credible sets, ClinVar, Uniprot or ClinPGx.
/// The dataset includes variant metadata as well as variant effects derived from Ensembl VEP.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct Variant {
    /// Unique identifier for the variant following schema: {chromosome}-{position}-{referenceAllele}-{alternateAllele}.
    variant_id: String,
    /// Chromosome on which the variant is located.
    chromosome: Chromosome,
    /// Variant's position on the chromosome.
    position: u32,
    /// Reference allele for the variant.
    reference_allele: String,
    /// Alternate allele for the variant.
    alternate_allele: String,
    /// Predicted or measured effect of the variant based on various methods.
    variant_effect: Vec<VariantEffect>,
    /// Predicted consequences on transcript context.
    transcript_consequences: Vec<TranscriptConsequence>,
    /// RsIds for the variant.
    rs_ids: Vec<String>,
    /// Cross-references for the variant in different databases.
    db_xrefs: Vec<DbXref>,
    /// Allele frequencies of the variant in different populations.
    allele_frequencies: Vec<AlleleFrequency>,
    /// HGVS identifier of the variant.
    hgvs_id: Option<String>,
    /// Short summary of the variant effect.
    variant_description: String,

    // embedded fields
    /// Sequence ontology identifier of the most severe consequence of the variant based on Ensembl VEP [bioregistry:so].
    #[graphql(skip)]
    most_severe_consequence_id: String,
}

// ---- query utilities ----

// impl Entity for Variant {
//     fn id(&self) -> &str { &self.variant_id }
// }

// ---- loaders ----

pub type VariantCache = Cache<String, Option<Variant>>;
static VARIANT_CACHE: LazyLock<VariantCache> = LazyLock::new(entity_cache);

pub struct VariantLoader {
    ch: ClickHouse,
}

impl VariantLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for VariantLoader {
    type Key = String;
    type Value = Variant;

    fn cache(&self) -> &VariantCache { &VARIANT_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.variant_id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM variants WHERE variantId IN ?")
            .bind(misses)
            .fetch_all::<Variant>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for VariantLoader {
    type Value = Variant;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[String],
    ) -> Result<HashMap<String, Variant>, async_graphql::Error> {
        self.load_cached(keys).await
    }
}

async fn load_variants(
    ctx: &Context<'_>,
    ids: &[String],
) -> async_graphql::Result<Vec<Variant>> {
    load_ordered(ctx.data_unchecked::<DataLoader<VariantLoader>>(), ids).await
}



// ---- resolvers ----
#[derive(Default)]
pub struct VariantQuery;

#[Object]
impl VariantQuery {
    /// Fetch variants by ID.
    async fn variants(
        &self,
        ctx: &Context<'_>,
        variant_ids: Vec<String>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Variant>> {
        let items = load_variants(ctx, &variant_ids).await?;
        Ok(items
            .query()
            .paginate(page))
    }

    async fn variant(
        &self,
        ctx: &Context<'_>,
        variant_id: String,
    ) -> async_graphql::Result<Option<Variant>> {
        ctx.data_unchecked::<DataLoader<VariantLoader>>()
            .load_one(variant_id)
            .await
    }
}

#[ComplexObject]
impl Variant {
    /// The sequence ontology term of the most severe consequence of the variant based on Ensembl VEP.
    async fn most_severe_consequence(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<SequenceOntologyTerm>> {
        let item = ctx
            .data_unchecked::<DataLoader<SequenceOntologyTermLoader>>()
            .load_one(self.most_severe_consequence_id.clone().replace("_", ":"))
            .await?;
        Ok(item)
    }
    /// Protein coding coordinates linking this variant to its amino acid-level consequences in protein products. Describes variant consequences at the protein level including amino acid changes and their positions.
    async fn protein_coding_coordinates(
        &self,
        ctx: &Context<'_>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<ProteinCodingCoordinates>> {
        let items = ctx
            .data_unchecked::<DataLoader<ProteinCodingCoordinateVariantLoader>>()
            .load_one(self.variant_id.clone())
            .await?
            .unwrap_or_default();
        Ok(items.query().paginate(page))
    }

}
