use std::cmp::Ordering;

use async_graphql::{Enum, InputObject, InputType};

use crate::{
    entity::{
        disease::DiseaseSortField, hpo::HpoSortField, study::StudySortField,
        variant::TranscriptConsequenceSortField,
    },
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
#[derive(Debug, Clone, Copy, InputObject, Default)]
#[graphql(concrete(name = "DiseaseSort", params(DiseaseSortField)))]
#[graphql(concrete(name = "HpoSort", params(HpoSortField)))]
#[graphql(concrete(name = "StudySort", params(StudySortField)))]
#[graphql(concrete(name = "TranscriptConsequenceSort", params(TranscriptConsequenceSortField)))]
pub struct Sort<K: InputType> {
    /// The field to sort by.
    pub key: K,
    /// The direction to sort in.
    #[graphql(default)]
    pub direction: SortDirection,
}

/// Contains the compare function for sorting values on each of the sort fields.
pub trait SortKey<T> {
    /// Compares two values of type `T` using the given sort direction.
    fn compare(&self, a: &T, b: &T, direction: SortDirection) -> Ordering;
}

/// Null-object sort key.
///
/// Used when we don't want to define sort keys.
impl<T> SortKey<T> for NoSort {
    fn compare(&self, _: &T, _: &T, _: SortDirection) -> Ordering { Ordering::Equal }
}

/// Null-object sort key.
///
/// Used when we don't want to define sort keys.
#[derive(Clone, Copy)]
pub struct NoSort;

/// Where nulls go, independent of direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Nulls {
    /// Nulls come first, before non-nulls.
    First,
    /// Nulls come last, after non-nulls.
    #[default]
    Last,
}

/// One term of an ordering, like `x DESC NULLS LAST` in SQL.
#[derive(Debug, Clone, Copy)]
pub struct Term {
    /// The sort direction.
    pub direction: SortDirection,
    /// Where nulls go, independent of direction.
    pub nulls: Nulls,
}

impl Term {
    #[must_use]
    pub const fn new(direction: SortDirection, nulls: Nulls) -> Self { Self { direction, nulls } }

    /// Compares two values using the sort direction.
    pub fn cmp<V: Ord + ?Sized>(self, a: &V, b: &V) -> Ordering {
        match self.direction {
            SortDirection::Ascending => a.cmp(b),
            SortDirection::Descending => b.cmp(a),
        }
    }

    /// Compares two optional values using the sort direction and nulls ordering.
    pub fn cmp_opt<V: Ord>(self, a: &Option<V>, b: &Option<V>) -> Ordering {
        let null_vs_value = match self.nulls {
            Nulls::First => Ordering::Less,
            Nulls::Last => Ordering::Greater,
        };
        match (a, b) {
            (Some(x), Some(y)) => self.cmp(x, y),
            (None, Some(_)) => null_vs_value,
            (Some(_), None) => null_vs_value.reverse(),
            (None, None) => Ordering::Equal,
        }
    }
}

/// Sorts a slice of items using the given sort terms.
pub fn sort_items<T, K>(items: &mut [T], sorts: &[Sort<K>])
where
    T: Entity,
    K: InputType + SortKey<T>,
{
    items.sort_unstable_by(|a, b| {
        sorts
            .iter()
            .fold(Ordering::Equal, |acc, s| {
                acc.then_with(|| s.key.compare(a, b, s.direction))
            })
            .then_with(|| a.id().cmp(b.id()))
    });
}
