use async_graphql::{InputObject, OutputType, SimpleObject};

use crate::entity::{disease::Disease, disease_hpo::DiseasePhenotype, hpo::Hpo, study::Study};

pub const MAX_PAGE_SIZE: usize = 1000;

/// Represents a paginated list of items.
#[derive(Debug, Clone, Copy, InputObject)]
pub struct Page {
    /// The index of the page to fetch, starting from 0.
    #[graphql(default = 0)]
    pub index: usize,
    /// The number of items per page.
    #[graphql(default = 10, validator(minimum = 1, maximum = 1000))]
    pub size: usize,
}

impl Default for Page {
    fn default() -> Self { Self { index: 0, size: 10 } }
}

/// The result of a paginated query, containing the total number of items and the items themselves.
#[derive(Debug, SimpleObject)]
#[graphql(concrete(name = "DiseasePage", params(Disease)))]
#[graphql(concrete(name = "HpoPage", params(Hpo)))]
#[graphql(concrete(name = "StudyPage", params(Study)))]
#[graphql(concrete(name = "DiseasePhenotypePage", params(DiseasePhenotype)))]
pub struct Paged<T: OutputType> {
    pub total: u64,
    pub items: Vec<T>,
}
