use std::{cmp::Ordering, collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, Object, SimpleObject,
    dataloader::{DataLoader, Loader},
};
use clickhouse::Row;
use moka::future::Cache;
use serde::Deserialize;

use crate::{
    datasource::clickhouse::ClickHouse,
    query::{
        Entity, QueryExt,
        cache::{CachedLoader, entity_cache},
        load_ordered,
        paginate::{Page, Paged},
        search::Searchable,
        sort::{Sort, SortKey},
    },
};

// ---- models ----

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalTranscript {
    id: String,
    chromosome: String,
    start: i32,
    end: u32,
    strand: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct URL {
    nice_name: String,
    url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ChemicalProbes {
    id: String,
    control: Option<String>,
    drug_id: Option<String>,
    drug_from_source_id: Option<String>,
    mechanism_of_action: Vec<String>,
    is_high_quality: bool,
    origin: Vec<String>,
    probe_miner_score: Option<f64>,
    probes_drugs_score: Option<f64>,
    score_in_cells: Option<f64>,
    score_in_organisms: Option<f64>,
    target_from_source_id: String,
    urls: Vec<URL>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct DBXrefs {
    id: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Constraint {
    constraint_type: String,
    exp: Option<f64>,
    obs: Option<u32>,
    oe: Option<f64>,
    oe_lower: Option<f64>,
    oe_upper: Option<f64>,
    score: Option<f64>,
    upper_bin: Option<u32>,
    upper_bin6: Option<u32>,
    upper_rank: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GenomicLocation {
    chromosome: String,
    start: u32,
    end: u32,
    strand: i8,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct GO {
    id: String,
    aspect: String,
    evidence: String,
    gene_product: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct CancerHallmarks {
    description: String,
    impact: Option<String>,
    label: String,
    pmid: u32,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Attributes {
    name: String,
    description: String,
    pmid: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Hallmarks {
    cancer_hallmarks: Vec<CancerHallmarks>,
    attributes: Vec<Attributes>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Homologues {
    homology_type: String,
    query_percentage_identity: f64,
    species_id: String,
    species_name: String,
    target_gene_id: String,
    target_gene_symbol: String,
    target_percentage_identity: f64,
    is_high_confidence: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Pathways {
    pathway: String,
    pathway_id: String,
    top_level_term: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct ProteinIds {
    id: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Biosamples {
    tissue_label: Option<String>,
    tissue_id: Option<String>,
    cell_label: Option<String>,
    cell_format: Option<String>,
    cell_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Effects {
    direction: String,
    dosing: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Studies {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "type")]
    r#type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct SafetyLiabilities {
    biosamples: Vec<Biosamples>,
    datasource: String,
    effects: Vec<Effects>,
    event: Option<String>,
    event_id: Option<String>,
    literature: Option<String>,
    url: Option<String>,
    studies: Vec<Studies>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct SubcellularLocations {
    location: String,
    source: String,
    term_sl: Option<String>,
    label_sl: Option<String>,
    target_modifier: Option<String>,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct LabelSource {
    label: String,
    source: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TargetClass {
    id: u32,
    label: String,
    level: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct TEP {
    url: String,
    target_from_source_id: String,
    therapeutic_area: String,
    description: String,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Tractability {
    id: String,
    modality: String,
    value: bool,
}

#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Transcripts {
    transcript_id: String,
    biotype: String,
    is_ensembl_canonical: Option<bool>,
    uniprot_id: Option<String>,
    is_uniprot_reviewed: Option<bool>,
    translation_id: Option<String>,
    alphafold_id: Option<String>,
    uniprot_isoform_id: Option<String>,
}

#[derive(Debug, Clone, Row, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    id: String,
    alternative_genes: Vec<String>,
    approved_symbol: String,
    approved_name: String,
    biotype: String,
    canonical_transcript: CanonicalTranscript,
    chemical_probes: Vec<ChemicalProbes>,
    db_xrefs: Vec<DBXrefs>,
    function_descriptions: Vec<String>,
    constraint: Vec<Constraint>,
    genomic_location: GenomicLocation,
    go: Vec<GO>,
    hallmarks: Hallmarks,
    homologues: Vec<Homologues>,
    pathways: Vec<Pathways>,
    protein_ids: Vec<ProteinIds>,
    safety_liabilities: Vec<SafetyLiabilities>,
    subcellular_locations: Vec<SubcellularLocations>,
    synonyms: Vec<LabelSource>,
    symbol_synonyms: Vec<LabelSource>,
    name_synonyms: Vec<LabelSource>,
    obsolete_symbols: Vec<LabelSource>,
    obsolete_names: Vec<LabelSource>,
    target_class: Vec<TargetClass>,
    tep: TEP,
    tractability: Vec<Tractability>,
    transcript_ids: Vec<String>,
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

pub async fn load_diseases(
    ctx: &Context<'_>,
    ids: &&[String],
) -> async_graphql::Result<Vec<Target>> {
    load_ordered(ctx.data_unchecked::<DataLoader<TargetLoader>>(), ids).await
}

// ---- resolvers ----

#[derive(Default)]
pub struct TargetQuery;

#[Object]
impl TargetQuery {
    async fn target(
        &self,
        ctx: &Context<'_>,
        target_id: String,
    ) -> async_graphql::Result<Option<Target>> {
        ctx.data_unchecked::<DataLoader<TargetLoader>>()
            .load_one(target_id)
            .await
    }
}
