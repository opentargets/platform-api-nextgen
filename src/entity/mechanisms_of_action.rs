use std::{collections::HashMap, sync::LazyLock};

use async_graphql::{
    ComplexObject, Context, Enum, SimpleObject,
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
    entity::target::{Target, load_targets},
    query::cache::{CachedLoader, entity_cache},
};

// ---- models ----

/// How a drug interacts with its target, as classified by ChEMBL.
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
pub enum MechanismActionType {
    /// Activator.
    #[graphql(name = "ACTIVATOR")]
    #[strum(serialize = "ACTIVATOR")]
    Activator = 1,
    /// Agonist.
    #[graphql(name = "AGONIST")]
    #[strum(serialize = "AGONIST")]
    Agonist = 2,
    /// Allosteric antagonist.
    #[graphql(name = "ALLOSTERIC_ANTAGONIST")]
    #[strum(serialize = "ALLOSTERIC ANTAGONIST")]
    AllostericAntagonist = 3,
    /// Antagonist.
    #[graphql(name = "ANTAGONIST")]
    #[strum(serialize = "ANTAGONIST")]
    Antagonist = 4,
    /// Antisense inhibitor.
    #[graphql(name = "ANTISENSE_INHIBITOR")]
    #[strum(serialize = "ANTISENSE INHIBITOR")]
    AntisenseInhibitor = 5,
    /// Binding agent.
    #[graphql(name = "BINDING_AGENT")]
    #[strum(serialize = "BINDING AGENT")]
    BindingAgent = 6,
    /// Blocker.
    #[graphql(name = "BLOCKER")]
    #[strum(serialize = "BLOCKER")]
    Blocker = 7,
    /// Chelating agent.
    #[graphql(name = "CHELATING_AGENT")]
    #[strum(serialize = "CHELATING AGENT")]
    ChelatingAgent = 8,
    /// Cross-linking agent.
    #[graphql(name = "CROSS_LINKING_AGENT")]
    #[strum(serialize = "CROSS-LINKING AGENT")]
    CrossLinkingAgent = 9,
    /// Degrader.
    #[graphql(name = "DEGRADER")]
    #[strum(serialize = "DEGRADER")]
    Degrader = 10,
    /// Disrupting agent.
    #[graphql(name = "DISRUPTING_AGENT")]
    #[strum(serialize = "DISRUPTING AGENT")]
    DisruptingAgent = 11,
    /// Exogenous gene.
    #[graphql(name = "EXOGENOUS_GENE")]
    #[strum(serialize = "EXOGENOUS GENE")]
    ExogenousGene = 12,
    /// Exogenous protein.
    #[graphql(name = "EXOGENOUS_PROTEIN")]
    #[strum(serialize = "EXOGENOUS PROTEIN")]
    ExogenousProtein = 13,
    /// Gene editing negative modulator.
    #[graphql(name = "GENE_EDITING_NEGATIVE_MODULATOR")]
    #[strum(serialize = "GENE EDITING NEGATIVE MODULATOR")]
    GeneEditingNegativeModulator = 14,
    /// Hydrolytic enzyme.
    #[graphql(name = "HYDROLYTIC_ENZYME")]
    #[strum(serialize = "HYDROLYTIC ENZYME")]
    HydrolyticEnzyme = 15,
    /// Inhibitor.
    #[graphql(name = "INHIBITOR")]
    #[strum(serialize = "INHIBITOR")]
    Inhibitor = 16,
    /// Inverse agonist.
    #[graphql(name = "INVERSE_AGONIST")]
    #[strum(serialize = "INVERSE AGONIST")]
    InverseAgonist = 17,
    /// Methylating agent.
    #[graphql(name = "METHYLATING_AGENT")]
    #[strum(serialize = "METHYLATING AGENT")]
    MethylatingAgent = 18,
    /// Modulator.
    #[graphql(name = "MODULATOR")]
    #[strum(serialize = "MODULATOR")]
    Modulator = 19,
    /// Negative allosteric modulator.
    #[graphql(name = "NEGATIVE_ALLOSTERIC_MODULATOR")]
    #[strum(serialize = "NEGATIVE ALLOSTERIC MODULATOR")]
    NegativeAllostericModulator = 20,
    /// Negative modulator.
    #[graphql(name = "NEGATIVE_MODULATOR")]
    #[strum(serialize = "NEGATIVE MODULATOR")]
    NegativeModulator = 21,
    /// Opener.
    #[graphql(name = "OPENER")]
    #[strum(serialize = "OPENER")]
    Opener = 22,
    /// Oxidative enzyme.
    #[graphql(name = "OXIDATIVE_ENZYME")]
    #[strum(serialize = "OXIDATIVE ENZYME")]
    OxidativeEnzyme = 23,
    /// Partial agonist.
    #[graphql(name = "PARTIAL_AGONIST")]
    #[strum(serialize = "PARTIAL AGONIST")]
    PartialAgonist = 24,
    /// Positive allosteric modulator.
    #[graphql(name = "POSITIVE_ALLOSTERIC_MODULATOR")]
    #[strum(serialize = "POSITIVE ALLOSTERIC MODULATOR")]
    PositiveAllostericModulator = 25,
    /// Positive modulator.
    #[graphql(name = "POSITIVE_MODULATOR")]
    #[strum(serialize = "POSITIVE MODULATOR")]
    PositiveModulator = 26,
    /// Proteolytic enzyme.
    #[graphql(name = "PROTEOLYTIC_ENZYME")]
    #[strum(serialize = "PROTEOLYTIC ENZYME")]
    ProteolyticEnzyme = 27,
    /// Reducing agent.
    #[graphql(name = "REDUCING_AGENT")]
    #[strum(serialize = "REDUCING AGENT")]
    ReducingAgent = 28,
    /// Releasing agent.
    #[graphql(name = "RELEASING_AGENT")]
    #[strum(serialize = "RELEASING AGENT")]
    ReleasingAgent = 29,
    /// RNA interference inhibitor.
    #[graphql(name = "RNAI_INHIBITOR")]
    #[strum(serialize = "RNAI INHIBITOR")]
    RnaiInhibitor = 30,
    /// Sequestering agent.
    #[graphql(name = "SEQUESTERING_AGENT")]
    #[strum(serialize = "SEQUESTERING AGENT")]
    SequesteringAgent = 31,
    /// Stabiliser.
    #[graphql(name = "STABILISER")]
    #[strum(serialize = "STABILISER")]
    Stabiliser = 32,
    /// Substrate.
    #[graphql(name = "SUBSTRATE")]
    #[strum(serialize = "SUBSTRATE")]
    Substrate = 33,
    /// Vaccine antigen.
    #[graphql(name = "VACCINE_ANTIGEN")]
    #[strum(serialize = "VACCINE ANTIGEN")]
    VaccineAntigen = 34,
    /// Other.
    #[graphql(name = "OTHER")]
    #[strum(serialize = "OTHER")]
    Other = 35,
}

/// Kind of target a drug acts on, as classified by ChEMBL.
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
pub enum MechanismTargetType {
    /// 3D cell culture.
    #[graphql(name = "_3d_cell_culture")]
    #[strum(serialize = "3d cell culture")]
    CellCulture3d = 1,
    /// ADMET.
    #[graphql(name = "admet")]
    #[strum(serialize = "admet")]
    Admet = 2,
    /// Cell-line.
    #[graphql(name = "cell_line")]
    #[strum(serialize = "cell-line")]
    CellLine = 3,
    /// Chimeric protein.
    #[graphql(name = "chimeric_protein")]
    #[strum(serialize = "chimeric protein")]
    ChimericProtein = 4,
    /// Lipid.
    #[graphql(name = "lipid")]
    #[strum(serialize = "lipid")]
    Lipid = 5,
    /// Macromolecule.
    #[graphql(name = "macromolecule")]
    #[strum(serialize = "macromolecule")]
    Macromolecule = 6,
    /// Metal.
    #[graphql(name = "metal")]
    #[strum(serialize = "metal")]
    Metal = 7,
    /// No target.
    #[graphql(name = "no_target")]
    #[strum(serialize = "no target")]
    NoTarget = 8,
    /// Non-molecular.
    #[graphql(name = "non_molecular")]
    #[strum(serialize = "non-molecular")]
    NonMolecular = 9,
    /// Nucleic-acid.
    #[graphql(name = "nucleic_acid")]
    #[strum(serialize = "nucleic-acid")]
    NucleicAcid = 10,
    /// Oligosaccharide.
    #[graphql(name = "oligosaccharide")]
    #[strum(serialize = "oligosaccharide")]
    Oligosaccharide = 11,
    /// Organism.
    #[graphql(name = "organism")]
    #[strum(serialize = "organism")]
    Organism = 12,
    /// Phenotype.
    #[graphql(name = "phenotype")]
    #[strum(serialize = "phenotype")]
    Phenotype = 13,
    /// Protein complex.
    #[graphql(name = "protein_complex")]
    #[strum(serialize = "protein complex")]
    ProteinComplex = 14,
    /// Protein complex group.
    #[graphql(name = "protein_complex_group")]
    #[strum(serialize = "protein complex group")]
    ProteinComplexGroup = 15,
    /// Protein family.
    #[graphql(name = "protein_family")]
    #[strum(serialize = "protein family")]
    ProteinFamily = 16,
    /// Protein nucleic-acid complex.
    #[graphql(name = "protein_nucleic_acid_complex")]
    #[strum(serialize = "protein nucleic-acid complex")]
    ProteinNucleicAcidComplex = 17,
    /// Protein-protein interaction.
    #[graphql(name = "protein_protein_interaction")]
    #[strum(serialize = "protein-protein interaction")]
    ProteinProteinInteraction = 18,
    /// Selectivity group.
    #[graphql(name = "selectivity_group")]
    #[strum(serialize = "selectivity group")]
    SelectivityGroup = 19,
    /// Single protein.
    #[graphql(name = "single_protein")]
    #[strum(serialize = "single protein")]
    SingleProtein = 20,
    /// Small molecule.
    #[graphql(name = "small_molecule")]
    #[strum(serialize = "small molecule")]
    SmallMolecule = 21,
    /// Subcellular.
    #[graphql(name = "subcellular")]
    #[strum(serialize = "subcellular")]
    Subcellular = 22,
    /// Tissue.
    #[graphql(name = "tissue")]
    #[strum(serialize = "tissue")]
    Tissue = 23,
    /// Unchecked.
    #[graphql(name = "unchecked")]
    #[strum(serialize = "unchecked")]
    Unchecked = 24,
    /// Unknown.
    #[graphql(name = "unknown")]
    #[strum(serialize = "unknown")]
    Unknown = 25,
    /// Other.
    #[graphql(name = "other")]
    #[strum(serialize = "other")]
    Other = 26,
}

/// Reference information supporting the drug mechanisms of action.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    /// List of reference identifiers.
    ids: Vec<String>,
    /// Source of the reference (e.g., PubMed, FDA, package inserts).
    source: String,
    /// List of URLs linking to the reference.
    urls: Vec<String>,
}

/// Collection of mechanisms of action for a drug molecule.
#[derive(Debug, Clone, Deserialize, SimpleObject, Row)]
#[serde(rename_all = "camelCase")]
pub struct MechanismsOfAction {
    #[graphql(skip)]
    chembl_id: String,
    /// List of mechanism of action entries.
    rows: Vec<MechanismOfActionRow>,
    ///Unique list of action types across all mechanisms.
    unique_action_types: Vec<MechanismActionType>,
    ///Unique list of target types across all mechanisms.
    unique_target_types: Vec<MechanismTargetType>,
}

/// Mechanism of action information for a drug.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
#[graphql(complex)]
pub struct MechanismOfActionRow {
    /// Description of the mechanism of action.
    mechanism_of_action: String,
    /// Classification of how the drug interacts with its target (e.g., ACTIVATOR, INHIBITOR).
    action_type: Option<String>,
    /// Name of the target molecule.
    target_name: Option<String>,
    /// List of on-target (genes or proteins) involved in the drug or clinical candidate mechanism
    /// of action.
    #[graphql(skip)]
    targets: Vec<String>,
    /// How the mechanism was curated, e.g. whether it was inferred from trial text or the
    /// literature.
    curation: String,
    /// Reference information supporting the mechanism of action.
    references: Vec<Reference>,
}

#[ComplexObject]
impl MechanismOfActionRow {
    /// List of on-target (genes or proteins) involved in the drug or clinical candidate
    /// mechanism of action.
    async fn targets(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Target>> {
        load_targets(ctx, &self.targets).await
    }
}

// ---- loaders ----

pub type MechanismsOfActionCache = Cache<String, Option<MechanismsOfAction>>;
static MECHANISMS_OF_ACTION_CACHE: LazyLock<MechanismsOfActionCache> = LazyLock::new(entity_cache);

#[derive(From)]
pub struct MechanismsOfActionLoader {
    ch: ClickHouse,
}

impl CachedLoader for MechanismsOfActionLoader {
    type Key = String;
    type Value = MechanismsOfAction;

    fn cache(&self) -> &MechanismsOfActionCache { &MECHANISMS_OF_ACTION_CACHE }
    fn key_of(v: &Self::Value) -> Self::Key { v.chembl_id.clone() }

    #[tracing::instrument(skip_all, level = "debug", fields(n = misses.len()))]
    async fn fetch(&self, misses: &[Self::Key]) -> Result<Vec<Self::Value>, async_graphql::Error> {
        self.ch
            .query("SELECT ?fields FROM mechanism_of_action WHERE chemblId IN ?")
            .bind(misses)
            .fetch_all::<MechanismsOfAction>()
            .await
            .map_err(Into::into)
    }
}

impl Loader<String> for MechanismsOfActionLoader {
    type Value = MechanismsOfAction;
    type Error = async_graphql::Error;

    async fn load(
        &self,
        keys: &[String],
    ) -> Result<HashMap<String, MechanismsOfAction>, Self::Error> {
        self.load_cached(keys).await
    }
}

/// Loads the mechanisms of action recorded for a single drug by its ChEMBL id.
///
/// # Returns
/// The mechanisms of action, or None if the drug has none.
/// # Errors
/// Returns an error if the mechanisms of action could not be loaded.
pub async fn load_mechanisms_of_action(
    ctx: &Context<'_>,
    chembl_id: String,
) -> async_graphql::Result<Option<MechanismsOfAction>> {
    ctx.data_unchecked::<DataLoader<MechanismsOfActionLoader>>()
        .load_one(chembl_id)
        .await
}
