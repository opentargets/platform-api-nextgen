//! The definition of the `PPP` product.

use async_graphql::{MergedObject, Request};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        baseline_expression::BaselineExpressionLoader,
        biosample::BiosampleLoader,
        clinical_indication::{
            ClinicalIndicationFromDiseaseLoader, ClinicalIndicationFromDrugLoader,
        },
        disease::{DiseaseLoader, DiseaseQuery},
        disease_hpo::DiseasePhenotypeLoader,
        drug::{DrugLoader, DrugQuery},
        drug_warning::DrugWarningLoader,
        evidence::EvidenceLoader,
        gene_ontology::GeneOntologyLoader,
        hpo::HpoLoader,
        interaction::InteractionLoader,
        meta::MetaQuery,
        mouse_phenotype::MousePhenotypeLoader,
        protein_coding_coordinates::ProteinCodingCoordinateVariantLoader,
        search::SearchQuery,
        search_facet::FacetQuery,
        sequence_ontology::SequenceOntologyLoader,
        study::{StudyLoader, StudyQuery},
        target::{TargetLoader, TargetQuery},
        target_essentiality::TargetEssentialityLoader,
        target_prioritisation::TargetPrioritisationsLoader,
        variant::{VariantLoader, VariantQuery},
    },
    product::{Flavor, Product, loader},
};

/// The query root for the `PPP` product.
#[derive(MergedObject, Default)]
pub struct Query(
    MetaQuery,    // API data (version, data release, product, etc.)
    SearchQuery,  // Search bar functionality
    FacetQuery,   // Facet search for AOTF
    DiseaseQuery, // Diseases
    StudyQuery,   // Studies
    DrugQuery,    // Drugs
    VariantQuery, // Variants
    TargetQuery,  // Targets
);

pub struct Ppp;

impl Product for Ppp {
    const NAME: &'static Flavor = &Flavor::Ppp;
    type Query = Query;

    fn prepare_request(req: Request, ch: &ClickHouse) -> Request {
        req.data(loader::<BaselineExpressionLoader>(ch))
            .data(loader::<BiosampleLoader>(ch))
            .data(loader::<ClinicalIndicationFromDrugLoader>(ch))
            .data(loader::<ClinicalIndicationFromDiseaseLoader>(ch))
            .data(loader::<DiseaseLoader>(ch))
            .data(loader::<DrugLoader>(ch))
            .data(loader::<DiseasePhenotypeLoader>(ch))
            .data(loader::<DrugWarningLoader>(ch))
            .data(loader::<EvidenceLoader>(ch))
            .data(loader::<GeneOntologyLoader>(ch))
            .data(loader::<HpoLoader>(ch))
            .data(loader::<InteractionLoader>(ch))
            .data(loader::<MousePhenotypeLoader>(ch))
            .data(loader::<ProteinCodingCoordinateVariantLoader>(ch))
            .data(loader::<SequenceOntologyLoader>(ch))
            .data(loader::<StudyLoader>(ch))
            .data(loader::<TargetLoader>(ch))
            .data(loader::<TargetEssentialityLoader>(ch))
            .data(loader::<TargetPrioritisationsLoader>(ch))
            .data(loader::<VariantLoader>(ch))
    }
}
