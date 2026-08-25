//! Query utilities: filtering, searching, sorting, pagination.

use async_graphql::{
    InputType, OutputType,
    dataloader::{DataLoader, Loader},
};

use crate::query::{
    filter::Filter,
    paginate::{MAX_PAGE_SIZE, Page, Paged, PagedWithStats},
    search::Searchable,
    sort::{Sort, SortDirection, SortKey, sort_items},
    statistics::Statistics,
};

pub mod cache;
pub mod filter;
pub mod paginate;
pub mod search;
pub mod sort;
pub mod statistics;
pub mod stats;

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

/// Carries a collection through an optional filter → search → sort → paginate pipeline. Call only
/// the stages a dataset supports; a stage's trait bound applies only when that stage is used.
pub struct Query<T>(Vec<T>);

impl<T: OutputType> Query<T> {
    #[must_use]
    pub fn filter(mut self, filter: Option<&impl Filter<T>>) -> Self {
        let _s = tracing::debug_span!("filter", n = self.0.len()).entered();
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
        let _s = tracing::debug_span!("search", n = self.0.len()).entered();
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
        let _s = tracing::debug_span!("sort", n = self.0.len()).entered();
        sort_items(
            &mut self.0,
            sort.map(|s| &s.key),
            sort.map_or(SortDirection::default(), |s| s.direction),
        );
        self
    }

    /// Returns a slice of the items for the given page.
    fn page_slice(&mut self, page: Page) -> Vec<T> {
        // Using drain here is much faster than into_iter().skip()/take() because it avoids copying
        // the items into a new vector, and instead moves them directly into the Paged struct.
        let total = self.0.len();
        let size = page.size.min(MAX_PAGE_SIZE);
        let start = (page.index * size).min(total);
        let end = (start + size).min(total);
        self.0.drain(start..end).collect()
    }

    #[must_use]
    pub fn paginate(mut self, page: Page) -> Paged<T> {
        let total = self.0.len() as u64;
        Paged {
            total,
            items: self.page_slice(page),
        }
    }

    #[must_use]
    pub fn paginate_with_stats(mut self, page: Page) -> PagedWithStats<T>
    where
        T: Statistics,
    {
        let stats = T::compute(&self.0);
        let total = self.0.len() as u64;
        PagedWithStats {
            total,
            items: self.page_slice(page),
            stats,
        }
    }
}

/// Load values by key and return them in the same order as `ids`, dropping misses.
///
/// # Returns
/// A vector of values in the same order as `ids`, with any missing values dropped.
/// # Errors
/// Returns an error if the loader fails to load any of the requested values.
pub async fn load_ordered<L>(
    loader: &DataLoader<L>,
    ids: &[String],
) -> Result<Vec<L::Value>, L::Error>
where
    L: Loader<String>,
{
    let mut found = loader.load_many(ids.iter().cloned()).await?;
    Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
}
