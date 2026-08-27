use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        association::{DiseaseAssociation, load_associations},
        disease::Disease,
    },
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
    },
};

// ---- models ----

/// The Ensembl canonical transcript of the target gene.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalTranscript {
    /// The Ensembl transcript identifier for the canonical transcript.
    id: String,
    /// Chromosome location of the canonical transcript.
    chromosome: String,
    /// Genomic start position of the canonical transcript.
    start: i32,
    /// Genomic end position of the canonical transcript.
    end: u32,
    /// Strand orientation of the canonical transcript.
    strand: String,
}

/// External resource link with an optional display name.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct URL {
    /// Optional human-readable label for the URL.
    nice_name: String,
    /// URL to the external resource.
    url: Option<String>,
}

/// Chemical probes related to the target. High-quality chemical probes are small molecules that can
/// be used to modulate and study the function of proteins.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ChemicalProbes {
    /// Unique identifier for the chemical probe.
    id: String,
    /// Whether the chemical probe serves as a control.
    control: Option<String>,
    /// Drug ID associated with the chemical probe.
    drug_id: Option<String>,
    drug_from_source_id: Option<String>,
    /// Mechanism of action of the chemical probe.
    mechanism_of_action: Vec<String>,
    /// Indicates if the chemical probe is high quality.
    is_high_quality: bool,
    /// Origin of the chemical probe.
    origin: Vec<String>,
    /// Score from ProbeMiner for chemical probe quality.
    probe_miner_score: Option<f64>,
    /// Score for chemical probes related to druggability.
    probes_drugs_score: Option<f64>,
    /// Score indicating chemical probe activity in cells.
    score_in_cells: Option<f64>,
    /// Score indicating chemical probe activity in organisms.
    score_in_organisms: Option<f64>,
    /// Ensembl gene ID of the target for the chemical probe.
    target_from_source_id: String,
    /// URLs linking to more information about the chemical probe.
    urls: Vec<URL>,
}

/// Identifier with source information.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DBXrefs {
    /// Identifier value.
    id: String,
    /// Source database or organization providing the identifier.
    source: String,
}

/// Constraint scores for the target gene from GnomAD. Indicates gene intolerance to
/// loss-of-function mutations.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Constraint {
    /// Type of constraint applied to the target.
    constraint_type: String,
    /// Expected constraint score.
    exp: Option<f64>,
    /// Observed constraint score.
    obs: Option<u32>,
    /// Observed/Expected (OE) constraint score.
    oe: Option<f64>,
    /// Lower bound of the OE constraint score.
    oe_lower: Option<f64>,
    /// Upper bound of the OE constraint score.
    oe_upper: Option<f64>,
    /// Constraint score indicating gene intolerance.
    score: Option<f64>,
    /// Upper bin classification going from more constrained to less constrained.
    upper_bin: Option<u32>,
    /// Interpretable classification of constraint based on 6 bins. [GnomAD labels: 0: `very high`,
    /// 1: `high`, 2: `medium`, 3: `low`, 4: `very low`, 5: `very low`].
    upper_bin6: Option<u32>,
    /// Upper rank classification for every coding gene assessed by GnomAD going from more
    /// constrained to less constrained.
    upper_rank: Option<u32>,
}

/// Genomic location information of the target gene.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GenomicLocation {
    /// Chromosome on which the target is located.
    chromosome: String,
    /// Genomic start position of the target gene.
    start: u32,
    /// Genomic end position of the target gene.
    end: u32,
    /// Strand orientation of the target gene.
    strand: i8,
}

/// Gene Ontology (GO) annotations related to the target.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(name = "GeneOntology")]
pub struct GO {
    #[graphql(visible = false)]
    /// Gene ontology term identifier [bioregistry:go].
    id: String, //TODO: replace with loader from GeneOntologyTerm
    /// Type of the GO annotation: molecular function (F), biological process (P) and cellular
    /// localisation (C).
    aspect: String,
    /// Evidence supporting the GO annotation.
    evidence: String,
    /// Gene product associated with the GO annotation [bioregistry:uniprot].
    gene_product: String,
    /// Source database and identifier where the ontology term was sourced from.
    source: String,
}

/// Cancer hallmarks associated with the target gene.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct CancerHallmarks {
    /// Description of the cancer hallmark.
    description: String,
    /// Impact of the cancer hallmark on the target.
    impact: Option<String>,
    /// Label associated with the cancer hallmark.
    label: String,
    /// PubMed ID of the supporting literature for the cancer hallmark [bioregistry:pubmed].
    pmid: u32,
}

/// Attributes of the hallmark annotation.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Attributes {
    /// Name of the hallmark attribute.
    name: String,
    /// Description of the hallmark attribute.
    description: String,
    /// PubMed ID of the supporting literature for the hallmark attribute [bioregistry:pubmed].
    pmid: Option<u32>,
}

/// Hallmarks related to the target gene sourced from COSMIC.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Hallmarks {
    /// Cancer hallmarks associated with the target gene.
    cancer_hallmarks: Vec<CancerHallmarks>,
    /// Attributes of the hallmark annotation.
    attributes: Vec<Attributes>,
}

/// Homologues of the target gene in other species according to Ensembl Compara.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Homologue {
    /// Type of homology relationship.
    homology_type: String,
    /// Percentage identity of the query gene in the homologue.
    query_percentage_identity: f64,
    /// Species ID for the homologue.
    species_id: String,
    /// Species name for the homologue.
    species_name: String,
    /// Gene ID of the homologue.
    target_gene_id: String,
    /// Gene symbol of the homologous target.
    target_gene_symbol: String,
    /// Percentage identity of the homologue in the query gene.
    target_percentage_identity: f64,
    /// Indicates if the homology is high confidence according to Ensembl Compara.
    is_high_confidence: Option<String>,
}

/// Pathway metadata from Reactome pathway database.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(name = "ReactomePathway")]
pub struct Pathways {
    /// Reactome pathway name.
    pathway: String,
    /// Reactome pathway identifier [bioregistry:reactome].
    pathway_id: String,
    top_level_term: String,
}

/// Identifier with source information.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct IdAndSource {
    /// Identifier value.
    id: String,
    /// Source database or organization providing the identifier.
    source: String,
}

/// Biosamples used in safety assessments.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Biosamples {
    /// Label of the biosample tissue.
    tissue_label: Option<String>,
    /// Tissue ID for the biosample.
    tissue_id: Option<String>,
    /// Label of the biosample cell.
    cell_label: Option<String>,
    /// Format of the biosample cells.
    cell_format: Option<String>,
    /// Cell identifier for the biosample.
    cell_id: Option<String>,
}

/// Effects reported for safety events.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Effects {
    /// Direction of the reported effect (e.g., increase or decrease).
    direction: String,
    /// Dosing conditions related to the reported effect.
    dosing: Option<String>,
}

/// Studies related to safety assessments.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Studies {
    /// Name of the safety study.
    name: Option<String>,
    /// Description of the safety study.
    description: Option<String>,
    /// Type of safety study.
    #[serde(rename = "type")]
    r#type: Option<String>,
}

/// Safety liabilities associated with the target.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct SafetyLiabilities {
    /// Biosamples used in safety assessments.
    biosamples: Vec<Biosamples>,
    /// Data source reporting the safety liability.
    datasource: String,
    /// Effects reported for the safety event.
    effects: Vec<Effects>,
    /// Safety event associated with the target.
    event: Option<String>,
    /// Unique identifier for the safety event.
    event_id: Option<String>,
    /// Literature references for the safety liability.
    literature: Option<String>,
    /// URL linking to more details on safety liabilities.
    url: Option<String>,
    /// Studies related to safety assessments.
    studies: Vec<Studies>,
}

/// Subcellular location information with source.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct SubcellularLocations {
    /// Name of the subcellular compartment where the protein was found.
    location: String,
    /// Source database for the subcellular location.
    source: String,
    /// Subcellular location term identifier from SwissProt [bioregistry:sl].
    term_s_l: Option<String>,
    /// Subcellular location category from SwissProt.
    label_s_l: Option<String>,
    /// Protein isoform or modification that specific for the given subcellular location.
    target_modifier: Option<String>,
}

/// Label with source information.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct LabelSource {
    /// Label value (e.g., synonym, symbol).
    label: String,
    /// Source database of the label.
    source: String,
}

/// Target classification categories from ChEMBL.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TargetClass {
    /// Unique identifier for the target class.
    id: u32,
    /// Label for the target class.
    label: String,
    /// Hierarchical level of the target class.
    level: String,
}

/// Target Enabling Package (TEP) information.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TEP {
    #[graphql(name = "uri")]
    /// URL linking to more information on the TEP target.
    url: String,
    #[graphql(name = "name")]
    /// Ensembl gene ID for the TEP target.
    target_from_source_id: String,
    /// Therapeutic area associated with the TEP target.
    therapeutic_area: String,
    /// Description of the TEP target.
    description: String,
}

/// Tractability information for the target. Indicates the feasibility of targeting the gene/protein
/// with different therapeutic modalities.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Tractability {
    #[graphql(name = "label")]
    /// Tractability category label.
    id: String,
    /// Modality of the tractability assessment.
    modality: String,
    /// Tractability value assigned to the target (true indicates tractable).
    value: bool,
}

/// Transcript annotation for a target gene.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Transcripts {
    /// Ensembl transcript identifier.
    transcript_id: String,
    /// Biotype classification of the transcript.
    biotype: String,
    /// Whether this is the Ensembl canonical transcript.
    is_ensembl_canonical: Option<bool>,
    /// UniProt accession mapped to the transcript.
    uniprot_id: Option<String>,
    /// Whether the UniProt entry is reviewed (Swiss-Prot).
    is_uniprot_reviewed: Option<bool>,
    /// Ensembl translation identifier.
    translation_id: Option<String>,
    /// AlphaFold structure prediction identifier.
    alphafold_id: Option<String>,
    /// UniProt isoform identifier.
    uniprot_isoform_id: Option<String>,
}

/// Core annotation for drug targets (gene/proteins). Targets are defined based on EMBL-EBI Ensembl
/// database and uses the Ensembl gene ID as the primary identifier. An Ensembl gene ID is
/// considered potential drug target if included in the canonical assembly or if present alternative
/// assemblies but encoding for a reviewed protein product according to the UniProt database.
#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// Unique identifier for the target [bioregistry:ensembl].
    id: String,
    /// List of alternative Ensembl gene identifiers mapped to non-canonical chromosomes.
    alternative_genes: Vec<String>,
    /// Approved gene symbol of the target.
    approved_symbol: String,
    /// Approved full name of the target gene.
    approved_name: String,
    /// Biotype classification of the target gene, indicating if the gene is protein coding.
    biotype: String,
    /// The Ensembl canonical transcript of the target gene.
    canonical_transcript: CanonicalTranscript,
    /// Chemical probes with high selectivity and specificity for the target.
    chemical_probes: Vec<ChemicalProbes>,
    /// Database cross-references for the target.
    db_xrefs: Vec<DBXrefs>,
    /// Functional descriptions of the target gene sourced from UniProt.
    function_descriptions: Vec<String>,
    #[graphql(name = "geneticConstraint")]
    /// Constraint scores for the target gene from GnomAD based on loss-of-function intolerance.
    constraint: Vec<Constraint>,
    /// Genomic location information of the target gene.
    genomic_location: GenomicLocation,
    #[graphql(name = "geneOntology")]
    /// List of Gene Ontology (GO) annotations related to the target.
    go: Vec<GO>,
    /// Hallmarks related to the target gene sourced from COSMIC.
    hallmarks: Hallmarks,
    /// Homologues of the target gene in other species.
    homologues: Vec<Homologue>,
    /// Pathway annotations for the target.
    pathways: Vec<Pathways>,
    /// Protein identifiers associated with the target.
    protein_ids: Vec<IdAndSource>,
    /// Known target safety effects and target safety risk information.
    safety_liabilities: Vec<SafetyLiabilities>,
    /// List of subcellular locations where the target protein is found.
    subcellular_locations: Vec<SubcellularLocations>,
    /// List of synonyms for the target gene.
    synonyms: Vec<LabelSource>,
    /// List of symbol-based synonyms for the target gene.
    symbol_synonyms: Vec<LabelSource>,
    /// List of name-based synonyms for the target gene.
    name_synonyms: Vec<LabelSource>,
    /// List of obsolete symbols previously used for the target gene.
    obsolete_symbols: Vec<LabelSource>,
    /// List of obsolete names previously used for the target gene.
    obsolete_names: Vec<LabelSource>,
    /// Target classification categories from ChEMBL.
    target_class: Vec<TargetClass>,
    /// Target Enabling Package (TEP) information.
    tep: TEP,
    /// Tractability information for the target.
    tractability: Vec<Tractability>,
    /// List of Ensembl transcript identifiers associated with the target.
    transcript_ids: Vec<String>,
    /// List of transcripts associated with the target including protein and structure annotations.
    transcripts: Vec<Transcripts>,
}

// ---- query utilities ----

impl Entity for Target {
    fn id(&self) -> &str { &self.id }
}

// ---- loaders ----

pub type TargetCache = Cache<String, Option<Target>>;
static TARGET_CACHE: LazyLock<TargetCache> = LazyLock::new(entity_cache);

pub struct TargetLoader {
    ch: ClickHouse,
}

impl TargetLoader {
    #[must_use]
    pub fn new(ch: ClickHouse) -> Self { Self { ch } }
}

impl CachedLoader for TargetLoader {
    type Key = String;
    type Value = Target;

    fn cache(&self) -> &TargetCache { &TARGET_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM targets WHERE id IN ?")
            .bind(misses)
            .fetch_all::<Target>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for TargetLoader {
    type Value = Target;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[String]) -> Result<HashMap<String, Target>, async_graphql::Error> {
        self.load_cached(keys).await
    }
}

/// Load targets by their EFO IDs.
///
/// This function uses a [`DataLoader`] to fetch targets from the cache or database.
///
/// # Returns
/// A [`Vec`] of [`Target`] entities.
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
pub async fn load_targets(ctx: &Context<'_>, ids: &[String]) -> async_graphql::Result<Vec<Target>> {
    load_ordered(ctx.data_unchecked::<DataLoader<TargetLoader>>(), ids).await
}

/// Load a target by its ID.
///
/// This function uses a [`DataLoader`] to fetch a target from the cache or database.
///
/// # Returns
/// An [`Option`] of [`Target`] entity.
/// # Errors
/// Returns an [`async_graphql::Error`] if the database query fails.
pub async fn load_target(ctx: &Context<'_>, id: &str) -> async_graphql::Result<Option<Target>> {
    ctx.data_unchecked::<DataLoader<TargetLoader>>()
        .load_one(id.to_string())
        .await
}
// ---- resolvers ----

#[derive(Default)]
pub struct TargetQuery;

#[Object]
impl TargetQuery {
    /// Retrieve multiple targets by target identifiers.
    async fn targets(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "List of Ensembl IDs.")] ensembl_ids: Vec<String>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<Target>> {
        let targets = load_targets(ctx, &ensembl_ids).await?;
        Ok(targets.query().paginate(page))
    }

    /// Retrieve a target (gene/protein) by target identifier (e.g. ENSG00000139618).
    async fn target(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Ensembl ID")] ensembl_id: String,
        ensembl_id: String,
    ) -> async_graphql::Result<Option<Target>> {
        ctx.data_unchecked::<DataLoader<TargetLoader>>()
            .load_one(ensembl_id)
            .await
    }
}

#[ComplexObject]
impl Target {
    /// Target-disease associations calculated on-the-fly using configurable data source weights and
    /// evidence filters. Returns associations with aggregated scores and evidence counts supporting
    /// the target-disease relationship.
    #[allow(clippy::unused_async)]
    async fn associated_diseases(
        &self,
        ctx: &Context<'_>,
        #[graphql(default)] page: Page,
    ) -> async_graphql::Result<Paged<DiseaseAssociation>> {
        load_associations::<Disease>(ctx, &self.id, page)
    }
}
