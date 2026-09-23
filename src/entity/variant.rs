use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use derive_more::From;
use moka::future::Cache;
use serde::Deserialize;
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        enhancer_to_gene,
        pharmacogenomics::{Pharmacogenomics, load_pharmacogenomics_by_variant},
        protein_coding_coordinates::{
            ProteinCodingCoordinateVariantLoader, ProteinCodingCoordinates,
        },
        sequence_ontology::{
            SequenceOntologyTerm, load_sequence_ontology_many, load_sequence_ontology_one,
        },
        target::{Target, TargetLoader, load_target},
    },
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        sort::{Sort, SortKey, nulls_last},
    },
};

// ---- models ----
/// Chromosome type.
#[derive(
    Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Enum, Deserialize_repr, Serialize_repr,
)]
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
#[graphql(complex)]
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
    #[graphql(skip)]
    target_id: Option<String>,
    /// Variant effect normalised between -1 and 1.
    normalised_score: Option<f64>,
}

#[ComplexObject]
impl VariantEffect {
    /// The target (gene/protein) on which the variant effect is interpreted.
    async fn target(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Target>> {
        match &self.target_id {
            Some(target_id) => load_target(ctx, target_id.clone()).await,
            None => Ok(None),
        }
    }
}

/// Predicted consequences on transcript context.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct TranscriptConsequence {
    /// The sequence ontology identifier of the consequence of the variant based on Ensembl VEP in
    /// the context of the transcript [bioregistry:so].
    #[graphql(skip)]
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
    #[graphql(skip)]
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

    // sort fields
    #[graphql(skip)]
    #[serde(skip)]
    target: Option<Target>,
}

#[ComplexObject]
impl TranscriptConsequence {
    /// The target (gene/protein) associated with the transcript.
    async fn target(&self, ctx: &Context<'_>) -> async_graphql::Result<Option<Target>> {
        match &self.target_id {
            Some(target_id) => load_target(ctx, target_id.clone()).await,
            None => Ok(None),
        }
    }

    /// The sequence ontology term of the consequence of the variant based on Ensembl VEP in the
    /// context of the transcript.
    async fn variant_consequences(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<SequenceOntologyTerm>> {
        load_sequence_ontology_many(ctx, &self.variant_functional_consequence_ids).await
    }
}

impl TranscriptConsequence {
    fn approved_symbol(&self) -> Option<&str> {
        self.target.as_ref().map(|t| t.approved_symbol.as_str())
    }

    /// Sets `target` on each `TranscriptConsequence`, for sorting by target fields.
    async fn fetch_targets(
        ctx: &Context<'_>,
        rows: &mut [TranscriptConsequence],
    ) -> async_graphql::Result<()> {
        let loader = ctx.data_unchecked::<DataLoader<TargetLoader>>();
        let ids = rows.iter().filter_map(|tc| tc.target_id.clone());
        let targets = loader.load_many(ids).await?;

        for tc in rows {
            if let Some(id) = &tc.target_id {
                tc.target = targets.get(id).cloned();
            }
        }
        Ok(())
    }
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
    /// Name of the population where the allele frequency was measured.
    population_name: Option<String>,
    /// Frequency of the alternate allele in the population (ranging from 0 to 1).
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
    /// Unique identifier for the variant following schema:
    /// {chromosome}-{position}-{referenceAllele}-{alternateAllele}.
    variant_id: String,
    /// Chromosome on which the variant is located.
    chromosome: Chromosome,
    /// Variant's position on the chromosome.
    position: u32,
    /// Reference allele for the variant.
    reference_allele: String,
    /// Alternate allele for the variant.
    alternate_allele: String,
    /// List of predicted or measured effects of the variant based on various methods.
    variant_effect: Vec<VariantEffect>,
    /// Predicted consequences on transcript context.
    #[graphql(skip)]
    transcript_consequences: Vec<TranscriptConsequence>,
    /// `RsIds` for the variant.
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
    /// Sequence ontology identifier of the most severe consequence of the variant based on Ensembl
    /// VEP [bioregistry:so].
    #[graphql(skip)]
    most_severe_consequence_id: String,
}

// ---- query utilities ----

impl Entity for TranscriptConsequence {
    fn id(&self) -> &str { self.transcript_id.as_deref().unwrap_or_default() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Enum)]
pub enum TranscriptConsequenceSortField {
    /// Approved gene symbol of the target (gene/protein) associated with the transcript.
    TargetApprovedSymbol,
    #[default]
    /// Distance of the variant from the transcript.
    DistanceFromFootprint,
    /// Distance of the variant from the transcription start site.
    DistanceFromTss,
}

impl TranscriptConsequenceSortField {
    fn needs_target(self) -> bool { matches!(self, Self::TargetApprovedSymbol) }
}

impl SortKey<TranscriptConsequence> for TranscriptConsequenceSortField {
    fn compare(&self, a: &TranscriptConsequence, b: &TranscriptConsequence) -> Ordering {
        match self {
            Self::TargetApprovedSymbol => nulls_last(&a.approved_symbol(), &b.approved_symbol()),
            Self::DistanceFromFootprint => {
                a.distance_from_footprint.cmp(&b.distance_from_footprint)
            }
            Self::DistanceFromTss => a.distance_from_tss.cmp(&b.distance_from_tss),
        }
    }
}

// ---- loaders ----

pub type VariantCache = Cache<String, Option<Variant>>;
static VARIANT_CACHE: LazyLock<VariantCache> = LazyLock::new(entity_cache);

#[derive(From)]
pub struct VariantLoader {
    ch: ClickHouse,
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

/// Loads variants by their IDs.
///
/// # Returns
/// Returns a vector of [`Variant`] objects.
/// # Errors
/// Returns an error if the variants could not be loaded.
pub async fn load_variants(
    ctx: &Context<'_>,
    ids: &[String],
) -> async_graphql::Result<Vec<Variant>> {
    load_ordered(ctx.data_unchecked::<DataLoader<VariantLoader>>(), ids).await
}

/// Loads a single variant by its ID.
///
/// # Returns
/// Returns an [`Option<Variant>`] object.
/// # Errors
/// Returns an error if the variant could not be loaded.
pub async fn load_variant(ctx: &Context<'_>, id: String) -> async_graphql::Result<Option<Variant>> {
    ctx.data_unchecked::<DataLoader<VariantLoader>>()
        .load_one(id)
        .await
}

// ---- resolvers ----
#[derive(Default)]
pub struct VariantQuery;

#[Object]
impl VariantQuery {
    /// Retrieve a list of variants by their identifier in the format of CHROM_POS_REF_ALT for SNPs
    /// and short indels (e.g. 19_44908684_T_C).
    async fn variants(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "List of variant IDs to fetch.")] variant_ids: Vec<String>,
        #[graphql(default, desc = "Pagination for the variants.")] page: Page,
    ) -> async_graphql::Result<Paged<Variant>> {
        let items = load_variants(ctx, &variant_ids).await?;
        Ok(items.query().paginate(page))
    }

    /// Retrieve a variant by identifier in the format of CHROM_POS_REF_ALT for SNPs and short
    /// indels (e.g. 19_44908684_T_C).
    async fn variant(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Variant ID to fetch.")] variant_id: String,
    ) -> async_graphql::Result<Option<Variant>> {
        load_variant(ctx, variant_id.clone()).await
    }
}

#[ComplexObject]
impl Variant {
    /// Predicted consequences on transcript context.
    async fn transcript_consequences(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Sort field and direction.")] sort: Sort<
            TranscriptConsequenceSortField,
        >,
    ) -> async_graphql::Result<Vec<TranscriptConsequence>> {
        // If the sort is by `target.approved_symbol`, we need to fetch the targets.
        let mut rows = self.transcript_consequences.clone();
        if sort.key.needs_target() {
            TranscriptConsequence::fetch_targets(ctx, &mut rows).await?;
        }
        Ok(rows.query().sort(Some(&sort)).into_vec())
    }

    /// The sequence ontology term of the most severe consequence of the variant based on Ensembl
    /// VEP.
    async fn most_severe_consequence(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Option<SequenceOntologyTerm>> {
        load_sequence_ontology_one(
            ctx,
            self.most_severe_consequence_id.clone().replace('_', ":"),
        )
        .await
    }
    /// Protein coding coordinates linking this variant to its amino acid-level consequences in
    /// protein products. Describes variant consequences at the protein level including amino acid
    /// changes and their positions.
    async fn protein_coding_coordinates(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the protein coding coordinates.")] page: Page,
    ) -> async_graphql::Result<Paged<ProteinCodingCoordinates>> {
        let items = ctx
            .data_unchecked::<DataLoader<ProteinCodingCoordinateVariantLoader>>()
            .load_one(self.variant_id.clone())
            .await?
            .unwrap_or_default();
        Ok(items.query().paginate(page))
    }
    async fn enhancer_to_gene(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the enhancer-to-gene relationships.")] page: Page,
    ) -> async_graphql::Result<Paged<enhancer_to_gene::EnhancerToGene>> {
        let e2g_key = enhancer_to_gene::Key::new(
            self.chromosome,
            self.position,
            self.position,
            page.index,
            page.size,
        );
        enhancer_to_gene::load_enhancer_to_genes(ctx, e2g_key).await
    }

    ///Pharmacogenomics data linking this genetic variant to drug responses. Data is integrated
    /// from sources including ClinPGx and describes how genetic variants influence individual drug
    /// responses.
    async fn pharmacogenomics(
        &self,
        ctx: &Context<'_>,
        #[graphql(default, desc = "Pagination for the Associations time series.")] page: Page,
    ) -> async_graphql::Result<Paged<Pharmacogenomics>> {
        let pharmacogenomics =
            load_pharmacogenomics_by_variant(ctx, self.variant_id.clone()).await?;
        Ok(pharmacogenomics.unwrap_or_default().query().paginate(page))
    }
}
