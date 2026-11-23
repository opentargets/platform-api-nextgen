//! Query utilities: filtering, searching, sorting, pagination.

use async_graphql::{
    InputType, OutputType,
    dataloader::{DataLoader, Loader},
};
use strum::IntoEnumIterator;

use crate::query::{
    filter::Filter,
    paginate::{MAX_PAGE_SIZE, Page, Paged, PagedWithStats},
    search::{Search, Searchable},
    sort::{Sort, SortKey, sort_items},
    stats::ComputeStats,
};

pub mod cache;
pub mod filter;
pub mod paginate;
pub mod search;
pub mod sort;
pub mod sql;
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
    pub fn into_vec(self) -> Vec<T> { self.0 }

    #[must_use]
    pub fn filter(mut self, filter: Option<&impl Filter<T>>) -> Self {
        let _s = tracing::debug_span!("filter", n = self.0.len()).entered();
        if let Some(f) = filter {
            self.0.retain(|item| f.matches(item));
        }
        self
    }

    #[must_use]
    pub fn search(mut self, search: Option<Search<T::Field>>) -> Self
    where
        T: Searchable,
        T::Field: InputType + IntoEnumIterator + Ord,
    {
        let _s = tracing::debug_span!("search", n = self.0.len()).entered();
        if let Some(s) = search.and_then(Search::normalise) {
            let needle = s.text.to_lowercase();
            self.0.retain(|item| s.matches(item, &needle));
        }
        self
    }

    #[must_use]
    pub fn sort<K>(mut self, sorts: &[Sort<K>]) -> Self
    where
        T: Entity,
        K: SortKey<T> + InputType,
    {
        let _s = tracing::debug_span!("sort", n = self.0.len()).entered();
        sort_items(&mut self.0, sorts);
        self
    }

    /// Returns a slice of the items for the given page.
    fn page_slice(&mut self, page: Page) -> Vec<T> {
        // Using drain here is much faster than into_iter().skip()/take() because it avoids copying
        // the items into a new vector, and instead moves them directly into the Paged struct.
        let total = self.0.len();
        let size = usize::try_from(page.size.min(MAX_PAGE_SIZE)).unwrap_or(usize::MAX);
        let index = usize::try_from(page.index).unwrap_or(usize::MAX);
        let start = index.saturating_mul(size).min(total);
        let end = start.saturating_add(size).min(total);
        self.0.drain(start..end).collect()
    }

    #[must_use]
    pub fn paginate(mut self, page: Page) -> Paged<T> {
        let count = self.0.len() as u64;
        Paged {
            count,
            rows: self.page_slice(page),
        }
    }

    #[must_use]
    pub fn paginate_with_stats(mut self, page: Page) -> PagedWithStats<T>
    where
        T: ComputeStats,
    {
        let stats = T::compute(&self.0);
        let count = self.0.len() as u64;
        PagedWithStats {
            count,
            rows: self.page_slice(page),
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

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use async_graphql::{Enum, SimpleObject};
    use strum::EnumIter;

    use super::*;
    use crate::query::{
        search::contains,
        sort::{Nulls, SortDirection, Term},
        stats::HasStats,
    };

    #[derive(Clone, SimpleObject)]
    struct Item {
        name: String,
        value: i32,
    }

    impl Entity for Item {
        fn id(&self) -> &str { &self.name }
    }

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Enum, EnumIter)]
    enum Field {
        Name,
    }

    impl Searchable for Item {
        type Field = Field;

        fn field_matches(&self, _: Field, needle: &str) -> bool { contains(&self.name, needle) }
    }

    impl SortKey<Item> for Field {
        fn compare(&self, a: &Item, b: &Item, direction: SortDirection) -> Ordering {
            Term::new(direction, Nulls::Last).cmp(&a.name, &b.name)
        }
    }

    struct MinimumValue(i32);

    impl Filter<Item> for MinimumValue {
        fn matches(&self, item: &Item) -> bool { item.value >= self.0 }
    }

    impl HasStats for Item {
        type Stats = i32;
    }

    impl ComputeStats for Item {
        fn compute(items: &[Self]) -> i32 { items.iter().map(|item| item.value).sum() }
    }

    fn items() -> Vec<Item> {
        [
            ("Gamma match", 30),
            ("Zulu match", 1),
            ("Alpha match", 10),
            ("Unrelated", 100),
            ("Beta match", 20),
            ("Delta match", 40),
        ]
        .into_iter()
        .map(|(name, value)| Item {
            name: name.into(),
            value,
        })
        .collect()
    }

    fn matching_items() -> Query<Item> {
        items()
            .query()
            .filter(Some(&MinimumValue(10)))
            .search(Some(Search {
                text: " MATCH ".into(),
                fields: vec![Field::Name],
            }))
            .sort(&[Sort {
                key: Field::Name,
                direction: SortDirection::Ascending,
            }])
    }

    #[test]
    fn pagination_preserves_order_and_total_across_page_boundaries() {
        for (index, expected) in [
            (0, vec![50, 10]),
            (1, vec![30, 20]),
            (2, vec![40]),
            (3, vec![]),
            (10, vec![]),
        ] {
            let result = vec![50, 10, 30, 20, 40]
                .query()
                .paginate(Page { index, size: 2 });
            assert_eq!(result.count, 5, "page {index}");
            assert_eq!(result.rows, expected, "page {index}");
        }
    }

    #[test]
    fn default_pagination_returns_the_first_ten_rows() {
        let result = (0..12)
            .collect::<Vec<_>>()
            .query()
            .paginate(Page::default());
        assert_eq!(result.count, 12);
        assert_eq!(result.rows, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn page_size_cap_applies_to_both_length_and_offset() {
        for size in [MAX_PAGE_SIZE, MAX_PAGE_SIZE + 1, u32::MAX] {
            let values: Vec<_> = (0..MAX_PAGE_SIZE + 3).collect();
            let first = values.clone().query().paginate(Page { index: 0, size });
            let second = values.query().paginate(Page { index: 1, size });
            assert_eq!(first.count, u64::from(MAX_PAGE_SIZE + 3), "size {size}");
            assert_eq!(
                first.rows,
                (0..MAX_PAGE_SIZE).collect::<Vec<_>>(),
                "size {size}"
            );
            assert_eq!(second.count, first.count, "size {size}");
            assert_eq!(
                second.rows,
                vec![MAX_PAGE_SIZE, MAX_PAGE_SIZE + 1, MAX_PAGE_SIZE + 2],
                "size {size}"
            );
        }
    }

    #[test]
    fn large_page_indices_return_empty_rows_without_overflowing() {
        for page in [
            Page {
                index: 65_536,
                size: 65_536,
            },
            Page {
                index: u32::MAX,
                size: MAX_PAGE_SIZE,
            },
            Page {
                index: u32::MAX,
                size: 1,
            },
        ] {
            let result = items().query().paginate(page);
            assert_eq!(result.count, 6, "{page:?}");
            assert!(result.rows.is_empty(), "{page:?}");

            let with_stats = items().query().paginate_with_stats(page);
            assert_eq!(with_stats.count, 6, "{page:?}");
            assert!(with_stats.rows.is_empty(), "{page:?}");
            assert_eq!(with_stats.stats, 201, "{page:?}");
        }
    }

    #[test]
    fn empty_input_has_no_rows_and_empty_statistics() {
        for index in [0, 1] {
            let page = Page { index, size: 2 };
            let result = Vec::<Item>::new().query().paginate(page);
            assert_eq!(result.count, 0);
            assert!(result.rows.is_empty());

            let with_stats = Vec::<Item>::new().query().paginate_with_stats(page);
            assert_eq!(with_stats.count, 0);
            assert!(with_stats.rows.is_empty());
            assert_eq!(with_stats.stats, 0);
        }
    }

    #[test]
    fn filtered_and_searched_pages_keep_total_and_stats_for_all_matches() {
        for (index, expected) in [
            (0, vec!["Alpha match", "Beta match"]),
            (1, vec!["Delta match", "Gamma match"]),
            (2, vec![]),
        ] {
            let page = Page { index, size: 2 };
            let result = matching_items().paginate(page);
            assert_eq!(result.count, 4, "page {index}");
            assert_eq!(
                result
                    .rows
                    .iter()
                    .map(|item| item.name.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "page {index}"
            );

            let with_stats = matching_items().paginate_with_stats(page);
            assert_eq!(with_stats.count, 4, "page {index}");
            assert_eq!(with_stats.stats, 100, "page {index}");
            assert_eq!(
                with_stats
                    .rows
                    .iter()
                    .map(|item| item.name.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "page {index}"
            );
        }
    }
}
