//! Golden regression tests for TrendLab robustness ladder.
//!
//! These tests lock in expected behavior for stable benchmarks.
//! If these tests fail, it means either:
//! 1. A bug was introduced, OR
//! 2. The implementation genuinely changed (update golden values)
//!
//! Run with: `cargo test --test regression_golden`

use trendlab_runner::robustness::levels::{compute_iqr, compute_stability_score};

/// Test helper to create a simple distribution for testing IQR and stability calculations.

#[test]
fn golden_iqr_calculation() {
    // Golden test: Known distribution -> known IQR
    // Values: [1.0, 2.0, 3.0, 4.0, 5.0]
    // Q1 = 2.0, Q3 = 4.0, IQR = 2.0

    let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let iqr = compute_iqr(&values);

    assert!((iqr - 2.0).abs() < 1e-6, "Golden IQR should be 2.0");
}

#[test]
fn golden_iqr_calculation_unsorted() {
    // Golden test: Unsorted input should give same result as sorted
    // Values: [5.0, 1.0, 3.0, 2.0, 4.0] -> sorted: [1.0, 2.0, 3.0, 4.0, 5.0]
    // IQR = 2.0

    let values = vec![5.0, 1.0, 3.0, 2.0, 4.0];
    let iqr = compute_iqr(&values);

    assert!((iqr - 2.0).abs() < 1e-6, "IQR should be order-independent");
}

#[test]
fn golden_stability_score_computation() {
    // Golden test: Known IQR should produce known stability score
    // IQR = 0.3 -> stability = 1 / (1 + 0.3) = 0.769...

    let iqr = 0.3;
    let stability = compute_stability_score(iqr);

    let expected = 1.0 / (1.0 + 0.3);
    assert!((stability - expected).abs() < 1e-6, "Stability score should match formula");
    assert!((stability - 0.7692307692).abs() < 1e-6, "Golden value check");
}

#[test]
fn golden_iqr_edge_case_empty() {
    // Golden test: Empty vector should return 0.0 IQR

    let values: Vec<f64> = vec![];
    let iqr = compute_iqr(&values);

    assert_eq!(iqr, 0.0, "Empty vector should have IQR=0");
}

#[test]
fn golden_iqr_edge_case_single() {
    // Golden test: Single value should return 0.0 IQR

    let values = vec![5.0];
    let iqr = compute_iqr(&values);

    assert_eq!(iqr, 0.0, "Single value should have IQR=0");
}

#[test]
fn golden_stability_score_perfect() {
    // Golden test: IQR=0 should give perfect stability score of 1.0

    let iqr = 0.0;
    let stability = compute_stability_score(iqr);

    assert_eq!(stability, 1.0, "IQR=0 should give perfect stability");
}

#[test]
fn golden_stability_score_high_variance() {
    // Golden test: High IQR should give low stability

    let iqr = 9.0; // Very high variance
    let stability = compute_stability_score(iqr);

    let expected = 1.0 / 10.0; // 1 / (1 + 9)
    assert!((stability - expected).abs() < 1e-6, "High IQR should give low stability");
    assert!(stability < 0.2, "High variance should have stability < 0.2");
}
