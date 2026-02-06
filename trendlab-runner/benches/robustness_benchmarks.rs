//! Criterion benchmarks for TrendLab robustness ladder hot loops.
//!
//! Run with: `cargo bench -p trendlab-runner`
//!
//! These benchmarks measure the performance-critical paths:
//! - Utility functions (IQR, stability score)
//! - Promotion criteria evaluation
//! - Batch processing
//!
//! Note: Full level benchmarks (CheapPass, WalkForward, etc.) are not included
//! because they require actual backtest execution, which is too slow for micro-benchmarks.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use trendlab_runner::robustness::levels::{compute_iqr, compute_stability_score};
use trendlab_runner::robustness::promotion::{PromotionCriteria, PromotionFilter};
use trendlab_runner::robustness::stability::{MetricDistribution, StabilityScore};

/// Generate synthetic metric values for benchmarking.
fn generate_metric_values(count: usize) -> Vec<f64> {
    (0..count)
        .map(|i| 0.5 + (i % 100) as f64 * 0.01)
        .collect()
}

/// Benchmark IQR computation (core hot loop)
fn bench_iqr_computation(c: &mut Criterion) {
    let mut group = c.benchmark_group("iqr_computation");

    for size in [10, 100, 1000, 10000].iter() {
        let values = generate_metric_values(*size);

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, _| {
                b.iter(|| {
                    let _ = compute_iqr(black_box(&values));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark stability score computation
fn bench_stability_score(c: &mut Criterion) {
    let mut group = c.benchmark_group("stability_score");

    let iqr_values = vec![0.1, 0.3, 0.5, 1.0, 2.0];

    for iqr in iqr_values {
        group.bench_with_input(
            BenchmarkId::from_parameter(iqr),
            &iqr,
            |b, &iqr_val| {
                b.iter(|| {
                    let _ = compute_stability_score(black_box(iqr_val));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark promotion filter evaluation
fn bench_promotion_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("promotion_filter");

    let criteria = PromotionCriteria {
        min_stability_score: 1.5,
        max_iqr: 0.5,
        min_trades: Some(10),
        min_raw_metric: Some(1.0),
    };

    let filter = PromotionFilter::new(criteria);

    let stability_score = StabilityScore {
        metric: "sharpe".to_string(),
        median: 1.2,
        iqr: 0.3,
        score: 1.05, // median - 0.5 * iqr
        penalty_factor: 0.5,
    };

    group.bench_function("should_promote", |b| {
        b.iter(|| {
            let _ = filter.should_promote(
                black_box(&stability_score),
                black_box(50),
                black_box(1.2), // sharpe_ratio
            );
        });
    });

    group.finish();
}

/// Benchmark MetricDistribution creation
fn bench_metric_distribution(c: &mut Criterion) {
    let mut group = c.benchmark_group("metric_distribution");

    for size in [10, 100, 1000].iter() {
        let values = generate_metric_values(*size);

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, _| {
                b.iter(|| {
                    let _ = MetricDistribution::from_values(
                        black_box("sharpe"),
                        black_box(&values),
                    );
                });
            },
        );
    }

    group.finish();
}

/// Benchmark StabilityScore computation
fn bench_stability_score_compute(c: &mut Criterion) {
    let mut group = c.benchmark_group("stability_score_compute");

    for size in [10, 100, 1000].iter() {
        let values = generate_metric_values(*size);

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, _| {
                b.iter(|| {
                    let _ = StabilityScore::compute(
                        black_box("sharpe"),
                        black_box(&values),
                        black_box(1.0),
                    );
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_iqr_computation,
    bench_stability_score,
    bench_promotion_filter,
    bench_metric_distribution,
    bench_stability_score_compute
);

criterion_main!(benches);
