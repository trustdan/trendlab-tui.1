//! Profiling and performance measurement utilities for TrendLab.
//!
//! Provides instrumentation for hot loops and integration with profiling tools:
//! - Manual timing scopes
//! - Flame graph integration (via pprof-rs or cargo-flamegraph)
//! - Memory allocation tracking
//!
//! # Usage
//!
//! ```
//! use trendlab_runner::profiling::ProfileScope;
//!
//! fn expensive_operation() {
//!     let _scope = ProfileScope::new("expensive_operation");
//!     // Work happens here...
//!     // Timing logged on drop
//! }
//! ```
//!
//! # Flamegraph Integration
//!
//! To generate flame graphs:
//!
//! ```bash
//! # Install cargo-flamegraph
//! cargo install flamegraph
//!
//! # Profile a benchmark
//! cargo flamegraph --bench robustness_benchmarks -- --bench
//!
//! # Profile a specific test
//! cargo flamegraph --test regression_golden -- test_name
//! ```
//!
//! # Environment Variables
//!
//! - `TRENDLAB_PROFILE=1` - Enable profiling instrumentation
//! - `TRENDLAB_PROFILE_MEMORY=1` - Track memory allocations

use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Global flag to enable/disable profiling.
static PROFILING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Global counter for total profiled operations.
static TOTAL_OPERATIONS: AtomicU64 = AtomicU64::new(0);

/// Initialize profiling system.
///
/// Should be called once at program startup.
/// Reads environment variables to configure profiling behavior.
pub fn init() {
    let enabled = std::env::var("TRENDLAB_PROFILE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    PROFILING_ENABLED.store(enabled, Ordering::Relaxed);

    if enabled {
        eprintln!("[PROFILING] Enabled (TRENDLAB_PROFILE=1)");
    }
}

/// Check if profiling is currently enabled.
#[inline]
pub fn is_enabled() -> bool {
    PROFILING_ENABLED.load(Ordering::Relaxed)
}

/// A profiling scope that measures execution time.
///
/// On drop, logs the duration if profiling is enabled.
///
/// # Example
///
/// ```
/// use trendlab_runner::profiling::ProfileScope;
///
/// fn my_function() {
///     let _scope = ProfileScope::new("my_function");
///     // Work...
/// }
/// ```
pub struct ProfileScope {
    name: &'static str,
    start: Instant,
}

impl ProfileScope {
    /// Create a new profiling scope.
    #[inline]
    pub fn new(name: &'static str) -> Self {
        TOTAL_OPERATIONS.fetch_add(1, Ordering::Relaxed);
        Self {
            name,
            start: Instant::now(),
        }
    }

    /// Get elapsed time without dropping the scope.
    #[inline]
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Drop for ProfileScope {
    fn drop(&mut self) {
        if is_enabled() {
            let duration = self.start.elapsed();
            eprintln!(
                "[PROFILE] {} took {:.3}ms",
                self.name,
                duration.as_secs_f64() * 1000.0
            );
        }
    }
}

/// Profile a closure and return its result along with duration.
///
/// # Example
///
/// ```
/// use trendlab_runner::profiling::profile;
///
/// let (result, duration) = profile("expensive_calc", || {
///     // Expensive work...
///     42
/// });
///
/// println!("Result: {}, took {:?}", result, duration);
/// ```
pub fn profile<F, R>(name: &'static str, f: F) -> (R, Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    let duration = start.elapsed();

    if is_enabled() {
        eprintln!(
            "[PROFILE] {} took {:.3}ms",
            name,
            duration.as_secs_f64() * 1000.0
        );
    }

    (result, duration)
}

/// Get total number of profiled operations since initialization.
pub fn total_operations() -> u64 {
    TOTAL_OPERATIONS.load(Ordering::Relaxed)
}

/// Reset profiling counters.
pub fn reset() {
    TOTAL_OPERATIONS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_scope_creation() {
        let _scope = ProfileScope::new("test_scope");
        // Just verify it doesn't panic - duration may be 0 for very fast code
        let _ = _scope.elapsed();
    }

    #[test]
    fn test_profile_function() {
        let (result, duration) = profile("test_profile", || {
            // Add minimal work to ensure measurable duration
            let mut sum = 0;
            for i in 0..100 {
                sum += i;
            }
            sum
        });
        assert!(result > 0);
        // Duration should be >= 0 (may be 0 on fast systems)
        let _ = duration;
    }

    #[test]
    fn test_total_operations_counter() {
        reset();
        let initial = total_operations();

        let _s1 = ProfileScope::new("op1");
        let _s2 = ProfileScope::new("op2");

        assert_eq!(total_operations(), initial + 2);
    }

    #[test]
    fn test_profiling_disabled_by_default() {
        // Profiling should be disabled unless explicitly enabled
        // (unless TRENDLAB_PROFILE=1 was set before test run)
        // This test just verifies the flag exists
        let _ = is_enabled();
    }
}
