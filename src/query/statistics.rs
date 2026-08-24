use std::{collections::HashMap, hash::Hash};

pub trait Statistics: Sized {
    type Stats: async_graphql::OutputType;
    fn compute(items: &[Self]) -> Self::Stats;
}

/// Stat: Counts
///
/// Counts the number of occurrences of each key in the given slice of items.
pub fn count_by<T, K, F>(items: &[T], key: F) -> Vec<(K, u64)>
where
    K: Eq + Hash + Ord,
    F: Fn(&T) -> K,
{
    let mut m: HashMap<K, u64> = HashMap::new();
    for item in items {
        *m.entry(key(item)).or_default() += 1;
    }
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}
