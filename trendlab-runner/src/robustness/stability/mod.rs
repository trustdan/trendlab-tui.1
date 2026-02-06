//! Stability scoring and distribution storage.

mod scoring;
mod distributions;

pub use scoring::StabilityScore;
pub use distributions::MetricDistribution;

/// Calculate percentile from sorted values.
pub(crate) fn percentile(values: &[f64], p: f64) -> f64 {
    assert!((0.0..=1.0).contains(&p), "percentile must be in [0, 1]");

    if values.is_empty() {
        return 0.0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let idx = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile(&values, 0.0), 1.0);
        assert_eq!(percentile(&values, 0.5), 3.0);
        assert_eq!(percentile(&values, 1.0), 5.0);
    }
}
