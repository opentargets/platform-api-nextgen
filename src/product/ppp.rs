//! The definition of the `PPP` product.

use async_graphql::{MergedObject, Request};

use crate::{
    datasource::clickhouse::ClickHouse,
    entity::{
        association_timeseries_ppp::AssociationTimeseriesLoader,
        baseline_expression::BaselineExpressionLoader,
        biosample::BiosampleLoader,
        clinical_indication::{
            ClinicalIndicationFromDiseaseLoader, ClinicalIndicationFromDrugLoader,
        },
        clinical_report::ClinicalReportLoader,
        disease::{DiseaseLoader, DiseaseQuery},
        disease_hpo::DiseasePhenotypeLoader,
        drug::{DrugLoader, DrugQuery},
        drug_warning::DrugWarningLoader,
        enhancer_to_gene::EnhancerToGeneLoader,
        evidence::EvidenceLoader,
        gene_ontology::GeneOntologyLoader,
        hpo::HpoLoader,
        interaction::InteractionLoader,
        meta::MetaQuery,
        mouse_phenotype::MousePhenotypeLoader,
        pharmacogenomics::{
            PharmacogenomicsByDrugLoader, PharmacogenomicsByTargetLoader,
            PharmacogenomicsByVariantLoader,
        },
        protein_coding_coordinates::ProteinCodingCoordinateVariantLoader,
        publications::PublicationLoader,
        search::SearchQuery,
        search_facet::FacetQuery,
        sequence_ontology::SequenceOntologyTermLoader,
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
        req.data(loader::<AssociationTimeseriesLoader>(ch))
            .data(loader::<BaselineExpressionLoader>(ch))
            .data(loader::<BiosampleLoader>(ch))
            .data(loader::<ClinicalIndicationFromDiseaseLoader>(ch))
            .data(loader::<ClinicalIndicationFromDrugLoader>(ch))
            .data(loader::<ClinicalReportLoader>(ch))
            .data(loader::<DiseaseLoader>(ch))
            .data(loader::<DiseasePhenotypeLoader>(ch))
            .data(loader::<DrugLoader>(ch))
            .data(loader::<DrugWarningLoader>(ch))
            .data(loader::<EnhancerToGeneLoader>(ch))
            .data(loader::<EvidenceLoader>(ch))
            .data(loader::<GeneOntologyLoader>(ch))
            .data(loader::<HpoLoader>(ch))
            .data(loader::<InteractionLoader>(ch))
            .data(loader::<MousePhenotypeLoader>(ch))
            .data(loader::<PharmacogenomicsByDrugLoader>(ch))
            .data(loader::<PharmacogenomicsByTargetLoader>(ch))
            .data(loader::<PharmacogenomicsByVariantLoader>(ch))
            .data(loader::<ProteinCodingCoordinateVariantLoader>(ch))
            .data(loader::<PublicationLoader>(ch))
            .data(loader::<SequenceOntologyTermLoader>(ch))
            .data(loader::<StudyLoader>(ch))
            .data(loader::<TargetLoader>(ch))
            .data(loader::<TargetEssentialityLoader>(ch))
            .data(loader::<TargetPrioritisationsLoader>(ch))
            .data(loader::<VariantLoader>(ch))
    }
}
