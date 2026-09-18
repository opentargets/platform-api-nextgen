use async_graphql::{InputObject, OutputType, SimpleObject};
use serde::{Deserialize, Serialize};

use crate::{
    entity::{
        association::{DiseaseAssociation, TargetAssociation},
        baseline_expression::BaselineExpression,
        biosample::Biosample,
        clinical_indication::ClinicalIndication,
        disease::Disease,
        disease_hpo::DiseasePhenotype,
        drug::Drug,
        drug_warning::DrugWarning,
        enhancer_to_gene::EnhancerToGene,
        evidence::Evidence,
        hpo::Hpo,
        mouse_phenotype::MousePhenotype,
        protein_coding_coordinates::ProteinCodingCoordinates,
        study::Study,
        target::Target,
        target_essentiality::DepMapEssentiality,
        variant::Variant,
    },
    query::statistics::Statistics,
};

pub const MAX_PAGE_SIZE: u32 = 100_000;

/// Represents a paginated list of items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, InputObject, Serialize)]
pub struct Page {
    /// The index of the page to fetch, starting from 0.
    #[graphql(default = 0)]
    pub index: u32,
    /// The number of items per page.
    #[graphql(default = 10, validator(minimum = 1, maximum = 100_000))]
    pub size: u32,
}

impl Default for Page {
    fn default() -> Self { Self { index: 0, size: 10 } }
}

/// The result of a paginated query, containing the total number of items and the items.
#[derive(Debug, Clone, Deserialize, SimpleObject)]
#[graphql(concrete(name = "DiseaseAssociationPage", params(DiseaseAssociation)))]
#[graphql(concrete(name = "ClinicalIndicationPage", params(ClinicalIndication)))]
#[graphql(concrete(name = "DiseasePage", params(Disease)))]
#[graphql(concrete(name = "DiseasePhenotypePage", params(DiseasePhenotype)))]
#[graphql(concrete(name = "DrugPage", params(Drug)))]
#[graphql(concrete(name = "DrugWarningPage", params(DrugWarning)))]
#[graphql(concrete(name = "EnhancerToGenePage", params(EnhancerToGene)))]
#[graphql(concrete(name = "EvidencePage", params(Evidence)))]
#[graphql(concrete(name = "HpoPage", params(Hpo)))]
#[graphql(concrete(name = "MousePhenotypePage", params(MousePhenotype)))]
#[graphql(concrete(
    name = "ProteinCodingCoordinatesPage",
    params(ProteinCodingCoordinates)
))]
#[graphql(concrete(name = "BaselineExpressionPage", params(BaselineExpression)))]
#[graphql(concrete(name = "BiosamplePage", params(Biosample)))]
#[graphql(concrete(name = "DepMapEssentialityPage", params(DepMapEssentiality)))]
#[graphql(concrete(name = "TargetAssociationPage", params(TargetAssociation)))]
#[graphql(concrete(name = "TargetPage", params(Target)))]
#[graphql(concrete(name = "VariantPage", params(Variant)))]
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
#[derive(Debug, SimpleObject)]
#[graphql(concrete(name = "StudyPage", params(Study)))]
pub struct PagedWithStats<T: OutputType + Statistics> {
    pub count: u64,
    pub rows: Vec<T>,
    pub stats: T::Stats,
}
