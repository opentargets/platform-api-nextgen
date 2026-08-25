use async_graphql::SimpleObject;

/// Summary statistics for a set of values.
#[derive(SimpleObject)]
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
