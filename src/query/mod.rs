//! Query utilities like filtering, sorting, pagination, searching and statistics.

use async_graphql::OutputType;

use crate::{
    query::{
        filter::Filter,
        paginate::{MAX_PAGE_SIZE, Page},
        search::Searchable,
        sort::{SortDirection, SortKey, sort_items},
    },
    schema::Paged,
};

pub mod filter;
pub mod paginate;
pub mod search;
pub mod sort;
pub mod statistics;

/// A row that has a stable identity. Used as tiebreaker for sort.
pub trait Entity {
    fn id(&self) -> &str;
}

/// Execute a query with optional filtering, searching, sorting and pagination.
#[tracing::instrument(
    skip_all,
    fields(input = items.len(), total = tracing::field::Empty, size = page.size)
)]
pub fn execute<T, F, K>(
    mut items: Vec<T>,
    filter: Option<&F>,
    search: Option<&str>,
    sort: &Option<K>,
    dir: SortDirection,
    page: Page,
) -> Paged<T>
where
    T: Entity + Searchable + OutputType,
    F: Filter<T>,
    K: SortKey<T>,
{
    let size = page.size.min(MAX_PAGE_SIZE);
    let needle = search.map(str::to_lowercase);
    items.retain(|item| {
        filter.is_none_or(|f| f.matches(item))
            && needle.as_deref().is_none_or(|n| item.matches_search(n))
    });
    let total = items.len() as u64;
    sort_items(&mut items, sort, dir);
    let items: Vec<T> = items
        .into_iter()
        .skip(page.index * size)
        .take(size)
        .collect();
    Paged { total, items }
}
