//! Query utilities: filtering, searching, sorting, pagination.

use async_graphql::{InputType, OutputType};

use crate::query::{
    filter::Filter,
    paginate::{MAX_PAGE_SIZE, Page, Paged},
    search::Searchable,
    sort::{Sort, SortDirection, SortKey, sort_items},
};

pub mod filter;
pub mod paginate;
pub mod search;
pub mod sort;
pub mod statistics;

/// A row with a primary key, used as a stable sort tiebreaker.
pub trait Entity {
    fn id(&self) -> &str;
}

/// Start a staged in-memory query over a collection.
pub trait QueryExt<T>: Sized {
    fn query(self) -> Query<T>;
}

impl<T> QueryExt<T> for Vec<T> {
    fn query(self) -> Query<T> { Query(self) }
}

/// Carries a collection through an optional filter → search → sort → paginate
/// pipeline. Call only the stages a dataset supports; a stage's trait bound
/// applies only when that stage is used.
pub struct Query<T>(Vec<T>);

impl<T: OutputType> Query<T> {
    #[must_use]
    pub fn filter(mut self, filter: Option<&impl Filter<T>>) -> Self {
        if let Some(f) = filter {
            self.0.retain(|item| f.matches(item));
        }
        self
    }

    #[must_use]
    pub fn search(mut self, needle: Option<&str>) -> Self
    where
        T: Searchable,
    {
        if let Some(n) = needle {
            let n = n.to_lowercase();
            self.0.retain(|item| item.matches_search(&n));
        }
        self
    }

    #[must_use]
    pub fn sort<K>(mut self, sort: Option<&Sort<K>>) -> Self
    where
        T: Entity,
        K: SortKey<T> + InputType,
    {
        sort_items(
            &mut self.0,
            sort.map(|s| &s.key),
            sort.map_or(SortDirection::default(), |s| s.direction),
        );
        self
    }

    #[must_use]
    #[tracing::instrument(skip_all, fields(total = self.0.len(), size = page.size))]
    pub fn paginate(self, page: Page) -> Paged<T>
    where
        T: OutputType,
    {
        let total = self.0.len() as u64;
        let size = page.size.min(MAX_PAGE_SIZE);
        let items = self
            .0
            .into_iter()
            .skip(page.index * size)
            .take(size)
            .collect();
        Paged { total, items }
    }
}
