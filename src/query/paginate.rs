use async_graphql::{InputObject, OutputType, SimpleObject};
use serde::{Deserialize, Serialize};

use crate::{
    entity::{
        adverse_event::AdverseEvent,
        association::{DiseaseAssociation, TargetAssociation},
        association_timeseries_ppp::AssociationTimeseries,
        baseline_expression::BaselineExpression,
        biosample::Biosample,
        clinical_indication::ClinicalIndication,
        clinical_target::ClinicalTarget,
        colocalisation::Colocalisation,
        credible_set::CredibleSet,
        disease_hpo::DiseasePhenotype,
        drug::Drug,
        drug_warning::DrugWarning,
        enhancer_to_gene::EnhancerToGene,
        evidence::Evidence,
        hpo::Hpo,
        interaction::Interaction,
        l2g_predictions::L2GPrediction,
        locus::Locus,
        mouse_phenotype::MousePhenotype,
        otar_project_ppp::OtarProject,
        pharmacogenomics::Pharmacogenomics,
        protein_coding_coordinates::ProteinCodingCoordinates,
        publication::Publication,
        similar_entity::Similarity,
        study::Study,
        target::Target,
        target_essentiality::DepMapEssentiality,
        variant::Variant,
    },
    query::stats::HasStats,
    view::{
        credible_set_colocalisations::CredibleSetColocalisation,
        credible_set_variants::CredibleSetVariant,
        disease_studies::DiseaseStudy,
        evidence::{
            cancer_biomarkers::EvidenceCancerBiomarkers,
            cancer_gene_census::EvidenceCancerGeneCensus, clingen::EvidenceClinGen,
            clinical_precedence::EvidenceClinicalPrecedence, crispr::EvidenceCrispr,
            crispr_screen::EvidenceCrisprScreen, europe_pmc::EvidenceEuropePmc, eva::EvidenceEva,
            eva_somatic::EvidenceEvaSomatic, expression_atlas::EvidenceExpressionAtlas,
            gene_burden::EvidenceGeneBurden, gene2phenotype::EvidenceGene2Phenotype,
            genomics_england_panel_app::EvidenceGenomicsEnglandPanelApp,
            gwas_credible_sets::EvidenceGwasCredibleSets, impc::EvidenceImpc,
            intogen::EvidenceIntogen, orphanet::EvidenceOrphanet, reactome::EvidenceReactome,
            uniprot_literature::EvidenceUniprotLiterature,
            uniprot_variants::EvidenceUniprotVariants,
        },
        study_credible_sets::StudyCredibleSet,
        target_credible_sets::TargetCredibleSet,
        variant_credible_sets::VariantCredibleSet,
        variant_transcript_consequences::VariantTranscriptConsequence,
    },
};

pub const MAX_PAGE_SIZE: u32 = 100_000;
pub const DEFAULT_PAGE_INDEX: u32 = 0;
pub const DEFAULT_PAGE_SIZE: u32 = 10;

/// Represents a paginated list of items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject, Serialize)]
pub struct Page {
    /// The index of the page to fetch, starting from 0.
    #[graphql(default_with = "DEFAULT_PAGE_INDEX")]
    pub index: u32,
    /// The number of items per page.
    #[graphql(default_with = "DEFAULT_PAGE_SIZE", validator(minimum = 1, maximum = 100_000))]
    pub size: u32,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            index: DEFAULT_PAGE_INDEX,
            size: DEFAULT_PAGE_SIZE,
        }
    }
}
/// GraphQL complexity of a paged field: each row costs `child_complexity`.
#[must_use]
pub fn page_complexity(page: Page, child_complexity: usize) -> usize {
    usize::try_from(page.size)
        .unwrap_or(usize::MAX)
        .saturating_mul(child_complexity)
}

/// The result of a paginated query, containing the total number of items and the items.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[graphql(concrete(name = "AssociationTimeseriesPage", params(AssociationTimeseries)))]
#[graphql(concrete(name = "BaselineExpressionPage", params(BaselineExpression)))]
#[graphql(concrete(name = "BiosamplePage", params(Biosample)))]
#[graphql(concrete(name = "ClinicalIndicationPage", params(ClinicalIndication)))]
#[graphql(concrete(name = "ClinicalTargetPage", params(ClinicalTarget)))]
#[graphql(concrete(name = "ColocalisationPage", params(Colocalisation)))]
#[graphql(concrete(name = "CredibleSetPage", params(CredibleSet)))]
#[graphql(concrete(name = "DepMapEssentialityPage", params(DepMapEssentiality)))]
#[graphql(concrete(name = "DiseaseAssociationPage", params(DiseaseAssociation)))]
#[graphql(concrete(name = "DiseasePhenotypePage", params(DiseasePhenotype)))]
#[graphql(concrete(name = "DrugPage", params(Drug)))]
#[graphql(concrete(name = "DrugWarningPage", params(DrugWarning)))]
#[graphql(concrete(name = "EnhancerToGenePage", params(EnhancerToGene)))]
#[graphql(concrete(name = "EvidencePage", params(Evidence)))]
#[graphql(concrete(name = "HpoPage", params(Hpo)))]
#[graphql(concrete(name = "InteractionPage", params(Interaction)))]
#[graphql(concrete(name = "L2GPredictionPage", params(L2GPrediction)))]
#[graphql(concrete(name = "LocusPage", params(Locus)))]
#[graphql(concrete(name = "MousePhenotypePage", params(MousePhenotype)))]
#[graphql(concrete(name = "PharmacogenomicsPage", params(Pharmacogenomics)))]
#[graphql(concrete(name = "ProjectPage", params(OtarProject)))]
#[graphql(concrete(name = "ProteinCodingCoordinatesPage", params(ProteinCodingCoordinates)))]
#[graphql(concrete(name = "PublicationPage", params(Publication)))]
#[graphql(concrete(name = "SimilarEntityPage", params(Similarity)))]
#[graphql(concrete(name = "TargetAssociationPage", params(TargetAssociation)))]
#[graphql(concrete(name = "TargetPage", params(Target)))]
#[graphql(concrete(name = "VariantPage", params(Variant)))]
#[graphql(concrete(name = "VariantCredibleSetPage", params(VariantCredibleSet)))]
#[graphql(concrete(name = "DiseaseStudyPage", params(DiseaseStudy)))]
#[graphql(concrete(name = "StudyCredibleSetPage", params(StudyCredibleSet)))]
#[graphql(concrete(name = "CredibleSetVariantPage", params(CredibleSetVariant)))]
#[graphql(concrete(name = "CredibleSetColocalisationPage", params(CredibleSetColocalisation)))]
#[graphql(concrete(name = "TargetCredibleSetPage", params(TargetCredibleSet)))]
#[graphql(concrete(name = "EvidenceClinicalPrecedencePage", params(EvidenceClinicalPrecedence)))]
#[graphql(concrete(name = "EvidenceGwasCredibleSetsPage", params(EvidenceGwasCredibleSets)))]
#[graphql(concrete(name = "EvidenceEvaPage", params(EvidenceEva)))]
#[graphql(concrete(name = "EvidenceExpressionAtlasPage", params(EvidenceExpressionAtlas)))]
#[graphql(concrete(name = "EvidenceImpcPage", params(EvidenceImpc)))]
#[graphql(concrete(name = "EvidenceEvaSomaticPage", params(EvidenceEvaSomatic)))]
#[graphql(concrete(name = "EvidenceEuropePmcPage", params(EvidenceEuropePmc)))]
#[graphql(concrete(name = "EvidenceGene2PhenotypePage", params(EvidenceGene2Phenotype)))]
#[graphql(concrete(name = "EvidenceUniprotLiteraturePage", params(EvidenceUniprotLiterature)))]
#[graphql(concrete(name = "EvidenceUniprotVariantsPage", params(EvidenceUniprotVariants)))]
#[graphql(concrete(name = "EvidenceGeneBurdenPage", params(EvidenceGeneBurden)))]
#[graphql(concrete(name = "EvidenceOrphanetPage", params(EvidenceOrphanet)))]
#[graphql(concrete(name = "EvidenceReactomePage", params(EvidenceReactome)))]
#[graphql(concrete(name = "EvidenceCrisprPage", params(EvidenceCrispr)))]
#[graphql(concrete(name = "EvidenceCrisprScreenPage", params(EvidenceCrisprScreen)))]
#[graphql(concrete(name = "EvidenceIntogenPage", params(EvidenceIntogen)))]
#[graphql(concrete(name = "EvidenceCancerBiomarkersPage", params(EvidenceCancerBiomarkers)))]
#[graphql(concrete(name = "EvidenceClinGenPage", params(EvidenceClinGen)))]
#[graphql(concrete(name = "EvidenceCancerGeneCensusPage", params(EvidenceCancerGeneCensus)))]
#[graphql(concrete(
    name = "EvidenceGenomicsEnglandPanelAppPage",
    params(EvidenceGenomicsEnglandPanelApp)
))]
#[graphql(concrete(
    name = "VariantTranscriptConsequencePage",
    params(VariantTranscriptConsequence)
))]
pub struct Paged<T: OutputType> {
    pub count: u64,
    pub rows: Vec<T>,
}

impl<T: OutputType> Default for Paged<T> {
    fn default() -> Self {
        Self {
            count: 0,
            rows: Vec::new(),
        }
    }
}

/// The result of a paginated query, containing the total number of items, the items, and statistics
/// about the query.
#[derive(Debug, Clone, SimpleObject)]
#[graphql(concrete(name = "AdverseEventPage", params(AdverseEvent)))]
#[graphql(concrete(name = "PublicationPage", params(Publication)))]
#[graphql(concrete(name = "StudyPage", params(Study)))]
pub struct PagedWithStats<T: OutputType + HasStats> {
    pub count: u64,
    pub rows: Vec<T>,
    pub stats: T::Stats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_complexity_scales_with_size_and_child_cost_not_index() {
        for index in [0, 20] {
            let page = Page { index, size: 7 };
            assert_eq!(page_complexity(page, 3), 21);
            assert_eq!(page_complexity(page, 0), 0);
        }
        assert_eq!(page_complexity(Page { index: 0, size: 14 }, 3), 42);
    }

    #[test]
    fn page_complexity_saturates_instead_of_overflowing() {
        assert_eq!(
            page_complexity(
                Page {
                    index: 0,
                    size: MAX_PAGE_SIZE
                },
                usize::MAX
            ),
            usize::MAX
        );
    }
}
