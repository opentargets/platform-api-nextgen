use async_graphql::{InputObject, OutputType, SimpleObject};

use crate::{
    entity::{
        association::{DiseaseAssociation, TargetAssociation},
        disease::Disease,
        disease_hpo::DiseasePhenotype,
        drug::Drug,
        hpo::Hpo,
        mouse_phenotype::MousePhenotype,
        study::Study,
        target::Target,
    },
    query::statistics::Statistics,
};

pub const MAX_PAGE_SIZE: usize = 100_000;

/// Represents a paginated list of items.
#[derive(Debug, Clone, Copy, InputObject)]
pub struct Page {
    /// The index of the page to fetch, starting from 0.
    #[graphql(default = 0)]
    pub index: usize,
    /// The number of items per page.
    #[graphql(default = 10, validator(minimum = 1, maximum = 100_000))]
    pub size: usize,
}

impl Default for Page {
    fn default() -> Self { Self { index: 0, size: 10 } }
}

/// The result of a paginated query, containing the total number of items and the items.
#[derive(Debug, SimpleObject)]
#[graphql(concrete(name = "DiseasePage", params(Disease)))]
#[graphql(concrete(name = "DiseasePhenotypePage", params(DiseasePhenotype)))]
#[graphql(concrete(name = "TargetPage", params(Target)))]
#[graphql(concrete(name = "DrugPage", params(Drug)))]
#[graphql(concrete(name = "HpoPage", params(Hpo)))]
#[graphql(concrete(name = "DiseaseAssociationPage", params(DiseaseAssociation)))]
#[graphql(concrete(name = "TargetAssociationPage", params(TargetAssociation)))]
#[graphql(concrete(name = "MousePhenotypePage", params(MousePhenotype)))]
pub struct Paged<T: OutputType> {
    pub total: u64,
    pub items: Vec<T>,
}

/// The result of a paginated query, containing the total number of items, the items, and statistics
/// about the query.
#[derive(Debug, SimpleObject)]
#[graphql(concrete(name = "StudyPage", params(Study)))]
pub struct PagedWithStats<T: OutputType + Statistics> {
    pub total: u64,
    pub items: Vec<T>,
    pub stats: T::Stats,
}
