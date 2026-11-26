use std::{collections::HashMap, hash::Hash};

use async_graphql::{OutputType, SimpleObject};

use crate::entity::study::StudyType;

// This type is needed because the macro in graphql concrete does not
// handle <> properly.
type OptionBool = Option<bool>;

/// A bucket for a distribution of values.
#[derive(Clone, SimpleObject)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_no_buckets() {
        let values: [StudyType; 0] = [];
        assert!(distribution(&values, |value| *value).is_empty());
    }

    #[test]
    fn counts_selected_keys_and_orders_buckets_independently_of_input_order() {
        use StudyType::{Eqtl, Gwas, Pqtl};

        let records = [
            ("study-1", Gwas),
            ("study-2", Eqtl),
            ("study-3", Gwas),
            ("study-4", Pqtl),
            ("study-5", Eqtl),
            ("study-6", Eqtl),
        ];
        let mut reversed = records;
        reversed.reverse();
        for items in [records, reversed] {
            let buckets = distribution(&items, |(_, study_type)| *study_type);
            let actual: Vec<_> = buckets.into_iter().map(|b| (b.value, b.count)).collect();
            assert_eq!(actual, vec![(Pqtl, 1), (Eqtl, 3), (Gwas, 2)]);
        }
    }

    #[test]
    fn missing_values_have_a_separate_bucket_from_false() {
        let values = [Some(true), None, Some(false), Some(true), None, Some(true)];
        let buckets = distribution(&values, |value| *value);
        let actual: Vec<_> = buckets.into_iter().map(|b| (b.value, b.count)).collect();
        assert_eq!(actual, vec![(None, 2), (Some(false), 1), (Some(true), 3)]);
    }
}
