use std::cmp::Ordering;

use async_graphql::Enum;

use crate::query::Entity;

/// Sort direction: ascending or descending.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Default)]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

/// Per-entity sort key: how do we order two rows?
pub trait SortKey<T> {
    fn compare(&self, a: &T, b: &T) -> Ordering;
}

pub fn sort_items<T, K>(items: &mut [T], key: &Option<K>, dir: SortDirection)
where
    T: Entity,
    K: SortKey<T>,
{
    items.sort_unstable_by(|a, b| {
        let primary = match &key {
            Some(k) => match dir {
                SortDirection::Asc => k.compare(a, b),
                SortDirection::Desc => k.compare(a, b).reverse(),
            },
            None => Ordering::Equal,
        };
        primary.then_with(|| a.id().cmp(b.id()))
    });
}

pub fn nulls_last<T: Ord>(a: &Option<T>, b: &Option<T>) -> Ordering {
    a.is_none().cmp(&b.is_none()).then_with(|| a.cmp(b))
}
