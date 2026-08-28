use std::cmp::Ordering;

use async_graphql::{Enum, InputObject, InputType};

use crate::{
    entity::{disease::DiseaseSortField, hpo::HpoSortField, study::StudySortField},
    query::Entity,
};

/// Sort direction: ascending or descending.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum, Default)]
pub enum SortDirection {
    /// Ascending order.
    #[default]
    Ascending,
    /// Descending order.
    Descending,
}

/// Sort types. Contain the sort field and direction.
#[derive(Debug, Clone, Copy, InputObject)]
#[graphql(concrete(name = "DiseaseSort", params(DiseaseSortField)))]
#[graphql(concrete(name = "StudySort", params(StudySortField)))]
#[graphql(concrete(name = "HpoSort", params(HpoSortField)))]
pub struct Sort<K: InputType> {
    /// The field to sort by.
    pub key: K,
    /// The direction to sort in.
    #[graphql(default)]
    pub direction: SortDirection,
}

/// Per-entity sort key: how do we order two rows?
pub trait SortKey<T> {
    fn compare(&self, a: &T, b: &T) -> Ordering;
}

/// Null-object sort key.
///
/// Used when we don't want to define sort keys.
#[derive(Clone, Copy)]
pub struct NoSort;
impl<T> SortKey<T> for NoSort {
    fn compare(&self, _: &T, _: &T) -> Ordering { Ordering::Equal }
}

pub fn sort_items<T, K>(items: &mut [T], key: Option<&K>, direction: SortDirection)
where
    T: Entity,
    K: SortKey<T>,
{
    items.sort_unstable_by(|a, b| {
        let primary = key.map_or(Ordering::Equal, |k| match direction {
            SortDirection::Ascending => k.compare(a, b),
            SortDirection::Descending => k.compare(a, b).reverse(),
        });
        primary.then_with(|| a.id().cmp(b.id()))
    });
}

pub fn nulls_last<T: Ord>(a: &Option<T>, b: &Option<T>) -> Ordering {
    a.is_none().cmp(&b.is_none()).then_with(|| a.cmp(b))
}
