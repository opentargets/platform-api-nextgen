use std::{collections::HashMap, hash::Hash};

use async_graphql::{OutputType, SimpleObject};

use crate::entity::study::StudyType;

// This type is needed because the macro in graphql concrete does not
// handle <> properly.
type OptionBool = Option<bool>;

/// A bucket for a distribution of values.
#[derive(SimpleObject)]
#[graphql(concrete(name = "StudyTypeBucket", params(StudyType)))]
#[graphql(concrete(name = "OptBoolBucket", params(OptionBool)))]
pub struct StatsBucket<K: OutputType> {
    /// The value of the bucket.
    value: K,
    /// The number of occurrences of the value.
    count: u64,
}

/// Stat: Distribution
///
/// Counts the number of occurrences of each key in the given slice of items.
pub fn distribution<T, K, F>(items: &[T], key: F) -> Vec<StatsBucket<K>>
where
    K: Eq + Hash + Ord + OutputType,
    F: Fn(&T) -> K,
{
    let mut m: HashMap<K, u64> = HashMap::new();
    for item in items {
        *m.entry(key(item)).or_default() += 1;
    }
    let mut v: Vec<StatsBucket<K>> = m
        .into_iter()
        .map(|(value, count)| StatsBucket { value, count })
        .collect();
    v.sort_unstable_by(|a, b| a.value.cmp(&b.value));
    v
}
