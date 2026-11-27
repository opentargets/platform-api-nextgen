use async_graphql::SimpleObject;

/// Summary statistics for a set of values.
#[derive(Clone, SimpleObject)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct Sumstats {
    /// The number of values.
    count: u64,
    /// The mean of the values.
    mean: f64,
    /// The standard deviation of the values.
    std: f64,
    /// The minimum value.
    min: f64,
    /// The 25th percentile value.
    #[graphql(name = "_25")]
    _25: f64,
    /// The 50th percentile value.
    #[graphql(name = "_50")]
    _50: f64,
    /// The 75th percentile value.
    #[graphql(name = "_75")]
    _75: f64,
    /// The maximum value.
    max: f64,
}

/// Computes summary statistics for the given slice of values.
pub fn sumstats<T, K, F>(items: &[T], key: F) -> Sumstats
where
    F: Fn(&T) -> K,
    K: Into<f64>,
{
    let mut values: Vec<f64> = items.iter().map(|item| key(item).into()).collect();
    if values.is_empty() {
        return Sumstats {
            count: 0,
            mean: 0.0,
            std: 0.0,
            min: 0.0,
            _25: 0.0,
            _50: 0.0,
            _75: 0.0,
            max: 0.0,
        };
    }

    values.sort_unstable_by(f64::total_cmp);

    let n = values.len();
    let count = n as u64;
    let mean = values.iter().sum::<f64>() / n as f64;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
    let std = var.sqrt();

    let pct = |p: f64| -> f64 {
        if n == 1 {
            return values[0];
        }
        let rank = p * (n - 1) as f64;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let lo = rank.floor() as usize;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let hi = rank.ceil() as usize;
        let frac = rank - lo as f64;
        values[lo] + (values[hi] - values[lo]) * frac
    };

    let mean = values.iter().sum::<f64>() / n as f64;
    Sumstats {
        count,
        mean,
        std,
        min: values[0],
        _25: pct(0.25),
        _50: pct(0.50),
        _75: pct(0.75),
        max: values[n - 1],
    }
}

#[cfg(test)]
#[allow(
    clippy::used_underscore_binding,
    reason = "The percentile fields being tested are named _25, _50 and _75."
)]
mod tests {
    use super::*;

    fn assert_close(field: &str, actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-12,
            "{field}: expected {expected}, got {actual}"
        );
    }

    #[test]
    fn empty_input_returns_zero_count_and_zero_statistics() {
        let values: [f64; 0] = [];
        let stats = sumstats(&values, |value| *value);
        assert_eq!(stats.count, 0);
        for (field, value) in [
            ("mean", stats.mean),
            ("std", stats.std),
            ("min", stats.min),
            ("25th percentile", stats._25),
            ("median", stats._50),
            ("75th percentile", stats._75),
            ("max", stats.max),
        ] {
            assert_close(field, value, 0.0);
        }
    }

    #[test]
    fn single_and_identical_values_have_zero_spread() {
        for (values, count) in [(vec![-3.5], 1), (vec![-3.5; 4], 4)] {
            let stats = sumstats(&values, |value| *value);
            assert_eq!(stats.count, count);
            assert_close("std", stats.std, 0.0);
            for (field, value) in [
                ("mean", stats.mean),
                ("min", stats.min),
                ("25th percentile", stats._25),
                ("median", stats._50),
                ("75th percentile", stats._75),
                ("max", stats.max),
            ] {
                assert_close(field, value, -3.5);
            }
        }
    }

    #[test]
    fn unsorted_values_preserve_duplicates_and_use_population_standard_deviation() {
        let stats = sumstats(&[9, 4, 2, 5, 4, 7, 5, 4], |value| *value);

        // Sorted: 2, 4, 4, 4, 5, 5, 7, 9. Sum = 40; squared deviations sum to 32.
        // Population variance is 32 / 8 = 4, so the standard deviation is 2.
        assert_eq!(stats.count, 8);
        assert_close("mean", stats.mean, 5.0);
        assert_close("std", stats.std, 2.0);
        assert_close("min", stats.min, 2.0);
        assert_close("25th percentile", stats._25, 4.0);
        assert_close("median", stats._50, 4.5);
        assert_close("75th percentile", stats._75, 5.5);
        assert_close("max", stats.max, 9.0);
    }

    #[test]
    fn two_fractional_values_interpolate_all_percentiles() {
        let stats = sumstats(&[5.5, -2.5], |value| *value);

        // The endpoints are eight units apart; quartiles advance two units at a time.
        assert_eq!(stats.count, 2);
        assert_close("mean", stats.mean, 1.5);
        assert_close("std", stats.std, 4.0);
        assert_close("min", stats.min, -2.5);
        assert_close("25th percentile", stats._25, -0.5);
        assert_close("median", stats._50, 1.5);
        assert_close("75th percentile", stats._75, 3.5);
        assert_close("max", stats.max, 5.5);
    }
}
