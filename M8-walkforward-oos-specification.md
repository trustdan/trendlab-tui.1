# M8: Walk-Forward + OOS Validation — Specification

**Status:** Complete (Specification) ✅
**Milestone:** M8 of 12
**Focus:** Preventing curve-fitting via train/test splits and rolling walk-forward validation
**Estimated LOC:** ~600 lines
**Complexity:** Medium (accounting + validation logic)

---

## Progress Tracker

```
M0   [..............ᗧ] 100% (complete)
M0.5 [..............ᗧ] 100% (complete)
M1   [..............ᗧ] 100% (complete)
M2   [..............ᗧ] 100% (complete)
M3   [..............ᗧ] 100% (complete)
M4   [..............ᗧ] 100% (complete)
M5   [..............ᗧ] 100% (complete)
M6   [..............ᗧ] 100% (complete)
M7   [..............ᗧ] 100% (complete)
M8   [..............ᗧ] 100% (complete) ← NOW COMPLETE
M9   [ᗧ··············] 0%   (not started)
M10  [ᗧ··············] 0%   (not started)
M11  [ᗧ··············] 0%   (not started)
M12  [ᗧ··············] 0%   (not started)
```

**Meta-plan status:** 10/12 milestones enhanced (83%)

---

## Overview

### The Overfitting Problem

Traditional backtesting has a fatal flaw: **train on all data → test on all data = curve-fitting**.

**Example (dangerous!):**
```rust
// Optimize MA cross parameters on 2000-2020 data
let best = optimize_ma_cross(data_2000_2020); // (50, 200) wins with Sharpe 2.1

// Test on same data (2000-2020)
let sharpe = backtest(best, data_2000_2020); // Sharpe 2.1 ✓

// Deploy in 2021...
let live = backtest(best, data_2021); // Sharpe 0.3 ❌ (curve-fitted!)
```

**Problem:** Parameters were optimized to fit historical noise, not true signal.

### M8 Solution: Walk-Forward Validation

**M8 introduces two core concepts:**

1. **Train/Test Splits (IS/OOS):**
   - **In-Sample (IS):** Training data (optimize parameters here)
   - **Out-of-Sample (OOS):** Test data (validation, never seen during optimization)

2. **Rolling Walk-Forward Windows:**
   - Train on period 1 → test on period 2
   - Train on period 2 → test on period 3
   - Repeat, accumulating OOS results

**Key insight:** If IS Sharpe >> OOS Sharpe → **curve-fitted!** (reject strategy)

---

## Architecture

### File Structure

```
trendlab-core/src/
├── validation/
│   ├── mod.rs                        # Module root + exports
│   ├── split.rs                      # Split trait + Period struct
│   ├── splits/
│   │   ├── mod.rs                    # Splits submodule
│   │   ├── fixed_split.rs            # Fixed train/test split (e.g., 70/30)
│   │   ├── date_split.rs             # Date-based split (e.g., train before 2020, test after)
│   │   ├── walkforward_split.rs      # Rolling walk-forward windows
│   │   └── rolling_split.rs          # Expanding window (train grows, test slides)
│   ├── validator.rs                  # Validator trait (IS vs OOS metrics)
│   ├── validators/
│   │   ├── mod.rs                    # Validators submodule
│   │   ├── is_oos_validator.rs       # Compare IS vs OOS Sharpe/DD/trades
│   │   └── degradation_validator.rs  # Alert if OOS << IS (overfitting detector)
│   ├── window.rs                     # Window struct (train/test periods)
│   └── promotion.rs                  # L2 promotion filter (pass L1 → earn L2)

trendlab-runner/src/
├── sweep/
│   ├── walkforward_sweep.rs          # Multi-window sweep runner
│   └── oos_report.rs                 # OOS vs IS comparison report
```

**Total:** ~9 files, ~600 lines

---

## Core Components

### 1. Period (Time Range)

```rust
// validation/split.rs

use chrono::NaiveDate;

/// A contiguous time period (inclusive start, exclusive end)
#[derive(Debug, Clone, PartialEq)]
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,   // Exclusive
    pub label: String,    // e.g., "train_1", "test_1"
}

impl Period {
    pub fn new(start: NaiveDate, end: NaiveDate, label: impl Into<String>) -> Self {
        assert!(start < end, "Period start must be before end");
        Self {
            start,
            end,
            label: label.into(),
        }
    }

    pub fn contains(&self, date: &NaiveDate) -> bool {
        date >= &self.start && date < &self.end
    }

    pub fn duration_days(&self) -> i64 {
        (self.end - self.start).num_days()
    }
}
```

**Usage:**
```rust
let train = Period::new(
    NaiveDate::from_ymd(2010, 1, 1),
    NaiveDate::from_ymd(2015, 1, 1),
    "train_1"
);

let test = Period::new(
    NaiveDate::from_ymd(2015, 1, 1),
    NaiveDate::from_ymd(2017, 1, 1),
    "test_1"
);

assert!(train.contains(&NaiveDate::from_ymd(2012, 6, 15))); // ✓
assert!(!test.contains(&NaiveDate::from_ymd(2012, 6, 15))); // ✓
```

---

### 2. Window (Train + Test Pair)

```rust
// validation/window.rs

use crate::validation::split::Period;

/// A train/test window pair
#[derive(Debug, Clone)]
pub struct Window {
    pub train: Period,
    pub test: Period,
    pub window_id: usize,
}

impl Window {
    pub fn new(train: Period, test: Period, window_id: usize) -> Self {
        // Ensure train ends before test starts (no overlap)
        assert!(
            train.end <= test.start,
            "Train period must end before test period starts"
        );
        Self {
            train,
            test,
            window_id,
        }
    }

    pub fn total_duration_days(&self) -> i64 {
        self.train.duration_days() + self.test.duration_days()
    }
}
```

**Usage:**
```rust
let window = Window::new(train, test, 0);

println!("Window 0:");
println!("  Train: {} - {} ({} days)",
    window.train.start, window.train.end, window.train.duration_days());
println!("  Test:  {} - {} ({} days)",
    window.test.start, window.test.end, window.test.duration_days());
```

**Output:**
```
Window 0:
  Train: 2010-01-01 - 2015-01-01 (1826 days)
  Test:  2015-01-01 - 2017-01-01 (731 days)
```

---

### 3. Split Trait (Generate Windows)

```rust
// validation/split.rs

use crate::validation::window::Window;
use chrono::NaiveDate;

/// Strategy for generating train/test windows
pub trait Split: Send + Sync {
    /// Generate all windows for the given date range
    fn generate_windows(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Vec<Window>;

    /// Name of this split strategy (for logging/reports)
    fn name(&self) -> &str;
}
```

---

### 4. Split Implementations

#### A) FixedSplit (Single 70/30 Split)

```rust
// validation/splits/fixed_split.rs

use crate::validation::split::{Period, Split};
use crate::validation::window::Window;
use chrono::NaiveDate;

/// Fixed percentage split (e.g., 70% train, 30% test)
pub struct FixedSplit {
    pub train_pct: f64, // 0.0 - 1.0
}

impl FixedSplit {
    pub fn new(train_pct: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&train_pct),
            "train_pct must be in [0.0, 1.0]"
        );
        Self { train_pct }
    }

    pub fn default_70_30() -> Self {
        Self::new(0.7)
    }
}

impl Split for FixedSplit {
    fn generate_windows(&self, start: NaiveDate, end: NaiveDate) -> Vec<Window> {
        let total_days = (end - start).num_days();
        let train_days = (total_days as f64 * self.train_pct).round() as i64;

        let split_date = start + chrono::Duration::days(train_days);

        let train = Period::new(start, split_date, "train");
        let test = Period::new(split_date, end, "test");

        vec![Window::new(train, test, 0)]
    }

    fn name(&self) -> &str {
        "FixedSplit"
    }
}
```

**Usage:**
```rust
let split = FixedSplit::default_70_30();
let windows = split.generate_windows(
    NaiveDate::from_ymd(2010, 1, 1),
    NaiveDate::from_ymd(2020, 1, 1),
);

assert_eq!(windows.len(), 1);
// Train: 2010-2017 (70%)
// Test:  2017-2020 (30%)
```

---

#### B) DateSplit (Explicit Date Boundary)

```rust
// validation/splits/date_split.rs

use crate::validation::split::{Period, Split};
use crate::validation::window::Window;
use chrono::NaiveDate;

/// Split at a specific date (e.g., train before 2020, test after)
pub struct DateSplit {
    pub split_date: NaiveDate,
}

impl DateSplit {
    pub fn new(split_date: NaiveDate) -> Self {
        Self { split_date }
    }
}

impl Split for DateSplit {
    fn generate_windows(&self, start: NaiveDate, end: NaiveDate) -> Vec<Window> {
        assert!(
            self.split_date > start && self.split_date < end,
            "Split date must be within [start, end)"
        );

        let train = Period::new(start, self.split_date, "train");
        let test = Period::new(self.split_date, end, "test");

        vec![Window::new(train, test, 0)]
    }

    fn name(&self) -> &str {
        "DateSplit"
    }
}
```

**Usage:**
```rust
// Train on pre-2020 data, test on 2020+
let split = DateSplit::new(NaiveDate::from_ymd(2020, 1, 1));
let windows = split.generate_windows(
    NaiveDate::from_ymd(2010, 1, 1),
    NaiveDate::from_ymd(2022, 1, 1),
);

// Train: 2010-2020
// Test:  2020-2022
```

---

#### C) WalkForwardSplit (Rolling Windows)

```rust
// validation/splits/walkforward_split.rs

use crate::validation::split::{Period, Split};
use crate::validation::window::Window;
use chrono::{Duration, NaiveDate};

/// Rolling walk-forward windows
///
/// Example (train=365d, test=90d, step=90d):
///   Window 0: train [0-365],   test [365-455]
///   Window 1: train [90-455],  test [455-545]
///   Window 2: train [180-545], test [545-635]
///   ...
pub struct WalkForwardSplit {
    pub train_days: i64,
    pub test_days: i64,
    pub step_days: i64, // How far to slide forward (typically = test_days)
}

impl WalkForwardSplit {
    pub fn new(train_days: i64, test_days: i64, step_days: i64) -> Self {
        assert!(train_days > 0, "train_days must be > 0");
        assert!(test_days > 0, "test_days must be > 0");
        assert!(step_days > 0, "step_days must be > 0");
        Self {
            train_days,
            test_days,
            step_days,
        }
    }

    /// Common preset: 1 year train, 3 months test, slide every 3 months
    pub fn year_quarter() -> Self {
        Self::new(365, 90, 90)
    }
}

impl Split for WalkForwardSplit {
    fn generate_windows(&self, start: NaiveDate, end: NaiveDate) -> Vec<Window> {
        let mut windows = Vec::new();
        let mut window_id = 0;

        let mut train_start = start;

        loop {
            let train_end = train_start + Duration::days(self.train_days);
            let test_start = train_end;
            let test_end = test_start + Duration::days(self.test_days);

            // Stop if we'd exceed the available data
            if test_end > end {
                break;
            }

            let train = Period::new(
                train_start,
                train_end,
                format!("train_{}", window_id),
            );
            let test = Period::new(
                test_start,
                test_end,
                format!("test_{}", window_id),
            );

            windows.push(Window::new(train, test, window_id));

            // Slide forward
            train_start = train_start + Duration::days(self.step_days);
            window_id += 1;
        }

        windows
    }

    fn name(&self) -> &str {
        "WalkForward"
    }
}
```

**Usage:**
```rust
// 1 year train, 3 months test, slide every 3 months
let split = WalkForwardSplit::year_quarter();
let windows = split.generate_windows(
    NaiveDate::from_ymd(2010, 1, 1),
    NaiveDate::from_ymd(2020, 1, 1),
);

println!("Generated {} windows", windows.len()); // ~40 windows

for (i, window) in windows.iter().take(3).enumerate() {
    println!("Window {}:", i);
    println!("  Train: {} - {}", window.train.start, window.train.end);
    println!("  Test:  {} - {}", window.test.start, window.test.end);
}
```

**Output:**
```
Generated 40 windows
Window 0:
  Train: 2010-01-01 - 2011-01-01
  Test:  2011-01-01 - 2011-04-01
Window 1:
  Train: 2010-04-01 - 2011-04-01
  Test:  2011-04-01 - 2011-07-01
Window 2:
  Train: 2010-07-01 - 2011-07-01
  Test:  2011-07-01 - 2011-10-01
```

**Key benefit:** OOS results are never contaminated by training data!

---

#### D) RollingSplit (Expanding Window)

```rust
// validation/splits/rolling_split.rs

use crate::validation::split::{Period, Split};
use crate::validation::window::Window;
use chrono::{Duration, NaiveDate};

/// Expanding train window (train grows, test slides)
///
/// Example (initial_train=365d, test=90d, step=90d):
///   Window 0: train [0-365],   test [365-455]
///   Window 1: train [0-455],   test [455-545]  (train expanded!)
///   Window 2: train [0-545],   test [545-635]
///   ...
pub struct RollingSplit {
    pub initial_train_days: i64,
    pub test_days: i64,
    pub step_days: i64,
}

impl RollingSplit {
    pub fn new(initial_train_days: i64, test_days: i64, step_days: i64) -> Self {
        assert!(initial_train_days > 0);
        assert!(test_days > 0);
        assert!(step_days > 0);
        Self {
            initial_train_days,
            test_days,
            step_days,
        }
    }

    pub fn expanding_year_quarter() -> Self {
        Self::new(365, 90, 90)
    }
}

impl Split for RollingSplit {
    fn generate_windows(&self, start: NaiveDate, end: NaiveDate) -> Vec<Window> {
        let mut windows = Vec::new();
        let mut window_id = 0;

        let train_start = start; // Fixed (train always starts here)
        let mut train_end = start + Duration::days(self.initial_train_days);

        loop {
            let test_start = train_end;
            let test_end = test_start + Duration::days(self.test_days);

            if test_end > end {
                break;
            }

            let train = Period::new(
                train_start,
                train_end,
                format!("train_{}", window_id),
            );
            let test = Period::new(
                test_start,
                test_end,
                format!("test_{}", window_id),
            );

            windows.push(Window::new(train, test, window_id));

            // Expand train window (include previous test period)
            train_end = test_end;
            window_id += 1;
        }

        windows
    }

    fn name(&self) -> &str {
        "RollingExpanding"
    }
}
```

**Usage:**
```rust
let split = RollingSplit::expanding_year_quarter();
let windows = split.generate_windows(
    NaiveDate::from_ymd(2010, 1, 1),
    NaiveDate::from_ymd(2020, 1, 1),
);

for (i, window) in windows.iter().take(3).enumerate() {
    println!("Window {}: train {} days, test {} days",
        i,
        window.train.duration_days(),
        window.test.duration_days(),
    );
}
```

**Output:**
```
Window 0: train 365 days, test 90 days
Window 1: train 455 days, test 90 days  (train grew by 90!)
Window 2: train 545 days, test 90 days  (train grew again!)
```

**Key benefit:** More training data over time (but requires stationarity assumption).

---

### 5. Validator (IS vs OOS Comparison)

```rust
// validation/validator.rs

use crate::portfolio::performance::PerformanceStats;

/// Validates strategy performance across IS/OOS periods
pub trait Validator: Send + Sync {
    /// Check if OOS performance is acceptable vs IS
    /// Returns (passed, reason)
    fn validate(
        &self,
        is_stats: &PerformanceStats,
        oos_stats: &PerformanceStats,
    ) -> (bool, String);

    fn name(&self) -> &str;
}
```

---

### 6. Validator Implementations

#### A) IsOosValidator (Basic Comparison)

```rust
// validation/validators/is_oos_validator.rs

use crate::portfolio::performance::PerformanceStats;
use crate::validation::validator::Validator;

/// Compare IS vs OOS metrics (Sharpe, DD, trades)
pub struct IsOosValidator {
    pub min_oos_sharpe: f64,      // Absolute minimum OOS Sharpe
    pub max_degradation_pct: f64, // Max % drop (IS → OOS)
}

impl IsOosValidator {
    pub fn new(min_oos_sharpe: f64, max_degradation_pct: f64) -> Self {
        Self {
            min_oos_sharpe,
            max_degradation_pct,
        }
    }

    /// Reasonable defaults: OOS Sharpe >= 0.5, degradation <= 40%
    pub fn default() -> Self {
        Self::new(0.5, 0.4)
    }
}

impl Validator for IsOosValidator {
    fn validate(
        &self,
        is_stats: &PerformanceStats,
        oos_stats: &PerformanceStats,
    ) -> (bool, String) {
        // Check 1: OOS Sharpe must exceed minimum
        if oos_stats.sharpe_ratio < self.min_oos_sharpe {
            return (
                false,
                format!(
                    "OOS Sharpe ({:.2}) < minimum ({:.2})",
                    oos_stats.sharpe_ratio, self.min_oos_sharpe
                ),
            );
        }

        // Check 2: OOS Sharpe must not degrade too much vs IS
        if is_stats.sharpe_ratio > 0.0 {
            let degradation = (is_stats.sharpe_ratio - oos_stats.sharpe_ratio)
                / is_stats.sharpe_ratio;

            if degradation > self.max_degradation_pct {
                return (
                    false,
                    format!(
                        "OOS degradation ({:.1}%) > max ({:.1}%)",
                        degradation * 100.0,
                        self.max_degradation_pct * 100.0
                    ),
                );
            }
        }

        // Passed
        (
            true,
            format!(
                "IS Sharpe {:.2} → OOS Sharpe {:.2} (degradation {:.1}%)",
                is_stats.sharpe_ratio,
                oos_stats.sharpe_ratio,
                ((is_stats.sharpe_ratio - oos_stats.sharpe_ratio) / is_stats.sharpe_ratio * 100.0).max(0.0)
            ),
        )
    }

    fn name(&self) -> &str {
        "IsOosValidator"
    }
}
```

**Usage:**
```rust
let validator = IsOosValidator::default();

let is_stats = PerformanceStats {
    sharpe_ratio: 1.5,
    max_drawdown: -0.12,
    total_trades: 20,
    // ...
};

let oos_stats = PerformanceStats {
    sharpe_ratio: 1.2,  // 20% degradation
    max_drawdown: -0.15,
    total_trades: 8,
    // ...
};

let (passed, reason) = validator.validate(&is_stats, &oos_stats);
assert!(passed); // ✓ (degradation 20% < max 40%)
println!("{}", reason);
// "IS Sharpe 1.50 → OOS Sharpe 1.20 (degradation 20.0%)"
```

**Example (curve-fitted!):**
```rust
let is_stats = PerformanceStats { sharpe_ratio: 2.0, .. };
let oos_stats = PerformanceStats { sharpe_ratio: 0.3, .. }; // 85% drop!

let (passed, reason) = validator.validate(&is_stats, &oos_stats);
assert!(!passed); // ❌
println!("{}", reason);
// "OOS degradation (85.0%) > max (40.0%)"
```

---

#### B) DegradationValidator (Alert Only)

```rust
// validation/validators/degradation_validator.rs

use crate::portfolio::performance::PerformanceStats;
use crate::validation::validator::Validator;

/// Alert on degradation, but don't reject (for logging/monitoring)
pub struct DegradationValidator {
    pub warn_threshold_pct: f64, // Warn if degradation > this
}

impl DegradationValidator {
    pub fn new(warn_threshold_pct: f64) -> Self {
        Self { warn_threshold_pct }
    }

    pub fn default() -> Self {
        Self::new(0.3) // Warn at 30% degradation
    }
}

impl Validator for DegradationValidator {
    fn validate(
        &self,
        is_stats: &PerformanceStats,
        oos_stats: &PerformanceStats,
    ) -> (bool, String) {
        if is_stats.sharpe_ratio <= 0.0 {
            return (true, "IS Sharpe <= 0 (skipping degradation check)".into());
        }

        let degradation = (is_stats.sharpe_ratio - oos_stats.sharpe_ratio)
            / is_stats.sharpe_ratio;

        if degradation > self.warn_threshold_pct {
            (
                true, // Don't reject, just warn
                format!(
                    "⚠️  High degradation: IS {:.2} → OOS {:.2} ({:.1}% drop)",
                    is_stats.sharpe_ratio,
                    oos_stats.sharpe_ratio,
                    degradation * 100.0
                ),
            )
        } else {
            (
                true,
                format!(
                    "✓ Low degradation: IS {:.2} → OOS {:.2} ({:.1}% drop)",
                    is_stats.sharpe_ratio,
                    oos_stats.sharpe_ratio,
                    degradation * 100.0
                ),
            )
        }
    }

    fn name(&self) -> &str {
        "DegradationValidator"
    }
}
```

---

### 7. Promotion Filter (L1 → L2)

```rust
// validation/promotion.rs

use crate::portfolio::performance::PerformanceStats;
use crate::validation::validator::Validator;
use std::sync::Arc;

/// L2 promotion filter (cheap candidates earn expensive simulation)
///
/// Workflow:
///   1. Run all candidates at L1 (cheap, deterministic)
///   2. Filter: keep candidates passing L1 IS/OOS validation
///   3. Promote: re-run survivors at L2 (expensive, walk-forward)
pub struct PromotionFilter {
    pub l1_validator: Arc<dyn Validator>,
}

impl PromotionFilter {
    pub fn new(l1_validator: Arc<dyn Validator>) -> Self {
        Self { l1_validator }
    }

    pub fn default() -> Self {
        Self::new(Arc::new(IsOosValidator::default()))
    }

    /// Check if candidate should be promoted to L2
    pub fn should_promote(
        &self,
        is_stats: &PerformanceStats,
        oos_stats: &PerformanceStats,
    ) -> (bool, String) {
        self.l1_validator.validate(is_stats, oos_stats)
    }
}
```

**Usage (runner):**
```rust
// Step 1: Run 1000 candidates at L1 (70/30 split, cheap)
let split = FixedSplit::default_70_30();
let windows = split.generate_windows(start, end);

let mut l1_results = Vec::new();
for candidate in &candidates {
    let is_stats = backtest(candidate, &windows[0].train, ExecutionLevel::L1);
    let oos_stats = backtest(candidate, &windows[0].test, ExecutionLevel::L1);
    l1_results.push((candidate, is_stats, oos_stats));
}

// Step 2: Filter (keep survivors)
let filter = PromotionFilter::default();
let mut promoted = Vec::new();

for (candidate, is_stats, oos_stats) in l1_results {
    let (should_promote, reason) = filter.should_promote(&is_stats, &oos_stats);
    if should_promote {
        println!("✓ Promoting {}: {}", candidate.name(), reason);
        promoted.push(candidate);
    } else {
        println!("✗ Rejecting {}: {}", candidate.name(), reason);
    }
}

println!("L1 → L2: {} / {} promoted", promoted.len(), candidates.len());

// Step 3: Re-run survivors at L2 (walk-forward, expensive)
let wf_split = WalkForwardSplit::year_quarter();
let wf_windows = wf_split.generate_windows(start, end);

for candidate in promoted {
    for window in &wf_windows {
        // Train on window.train, test on window.test
        let is_stats = backtest(candidate, &window.train, ExecutionLevel::L2);
        let oos_stats = backtest(candidate, &window.test, ExecutionLevel::L2);
        // Accumulate stats...
    }
}
```

**Example output:**
```
✓ Promoting DonchianBreakout(20): IS Sharpe 1.50 → OOS Sharpe 1.20 (degradation 20.0%)
✗ Rejecting MA_Cross(5,20): OOS degradation (78.0%) > max (40.0%)
✓ Promoting AtrChannel(14): IS Sharpe 1.30 → OOS Sharpe 1.10 (degradation 15.4%)
✗ Rejecting RSI(7): OOS Sharpe (0.2) < minimum (0.5)

L1 → L2: 2 / 1000 promoted (99.8% filtered!)
```

**Key benefit:** Reduce L2 simulation cost by 99%+ (only test survivors).

---

## BDD Scenarios

### Feature 1: Time Period Management
```gherkin
Feature: Time Period Management
  As a backtester
  I need to define non-overlapping train/test periods
  So that I can validate strategies on unseen data

  Scenario: Create a valid period
    Given a start date of 2010-01-01
    And an end date of 2015-01-01
    When I create a Period with label "train_1"
    Then the period should span 1826 days
    And the period should contain 2012-06-15
    And the period should not contain 2015-01-01

  Scenario: Period boundaries (exclusive end)
    Given a period from 2010-01-01 to 2015-01-01
    When I check if 2014-12-31 is contained
    Then it should return true
    When I check if 2015-01-01 is contained
    Then it should return false

  Scenario: Create train/test window
    Given a train period from 2010-01-01 to 2015-01-01
    And a test period from 2015-01-01 to 2017-01-01
    When I create a Window with id 0
    Then the window should have train ending 2015-01-01
    And the window should have test starting 2015-01-01
    And the window total duration should be 2557 days

  Scenario: Reject overlapping train/test periods
    Given a train period from 2010-01-01 to 2015-06-01
    And a test period from 2015-01-01 to 2017-01-01
    When I try to create a Window
    Then it should panic with "Train period must end before test period starts"
```

---

### Feature 2: Fixed Split (Single Train/Test)
```gherkin
Feature: Fixed Split
  As a backtester
  I need a simple 70/30 train/test split
  So that I can quickly validate a strategy

  Scenario: 70/30 split on 10-year dataset
    Given a FixedSplit with 70% train
    And a date range from 2010-01-01 to 2020-01-01
    When I generate windows
    Then I should get 1 window
    And window 0 train should span ~7 years (2555 days)
    And window 0 test should span ~3 years (1095 days)
    And train should end before test starts

  Scenario: Custom 80/20 split
    Given a FixedSplit with 80% train
    And a date range from 2010-01-01 to 2020-01-01
    When I generate windows
    Then window 0 train should span ~8 years (2920 days)
    And window 0 test should span ~2 years (730 days)

  Scenario: Edge case (90/10 split)
    Given a FixedSplit with 90% train
    And a date range from 2010-01-01 to 2020-01-01
    When I generate windows
    Then window 0 train should span ~9 years
    And window 0 test should span ~1 year
```

---

### Feature 3: Date Split (Explicit Boundary)
```gherkin
Feature: Date Split
  As a backtester
  I need to split at a specific date (e.g., before/after 2020)
  So that I can test on a known regime change

  Scenario: Split before/after 2020
    Given a DateSplit at 2020-01-01
    And a date range from 2010-01-01 to 2022-01-01
    When I generate windows
    Then I should get 1 window
    And train should be 2010-01-01 to 2020-01-01
    And test should be 2020-01-01 to 2022-01-01

  Scenario: Split at arbitrary date
    Given a DateSplit at 2015-06-15
    And a date range from 2010-01-01 to 2020-01-01
    When I generate windows
    Then train should end 2015-06-15
    And test should start 2015-06-15

  Scenario: Invalid split date (outside range)
    Given a DateSplit at 2025-01-01
    And a date range from 2010-01-01 to 2020-01-01
    When I try to generate windows
    Then it should panic with "Split date must be within [start, end)"
```

---

### Feature 4: Walk-Forward Split (Rolling Windows)
```gherkin
Feature: Walk-Forward Split
  As a backtester
  I need rolling train/test windows
  So that I can validate strategies across multiple time periods

  Scenario: Year/quarter walk-forward (1y train, 3m test, slide 3m)
    Given a WalkForwardSplit with train=365d, test=90d, step=90d
    And a date range from 2010-01-01 to 2020-01-01
    When I generate windows
    Then I should get ~40 windows
    And window 0 train should be 2010-01-01 to 2011-01-01
    And window 0 test should be 2011-01-01 to 2011-04-01
    And window 1 train should be 2010-04-01 to 2011-04-01
    And window 1 test should be 2011-04-01 to 2011-07-01

  Scenario: Each test period is independent
    Given a WalkForwardSplit with train=365d, test=90d, step=90d
    And 40 generated windows
    When I check test periods
    Then no test period should overlap with any train period
    And no test period should overlap with any other test period

  Scenario: Custom window sizes (2y train, 6m test, slide 6m)
    Given a WalkForwardSplit with train=730d, test=180d, step=180d
    And a date range from 2010-01-01 to 2020-01-01
    When I generate windows
    Then I should get ~16 windows
    And each train period should span 730 days
    And each test period should span 180 days
```

---

### Feature 5: Rolling Split (Expanding Window)
```gherkin
Feature: Rolling Split (Expanding Window)
  As a backtester
  I need an expanding train window (train grows, test slides)
  So that I can use all available historical data

  Scenario: Expanding year/quarter (initial 1y train, 3m test, grow by 3m)
    Given a RollingSplit with initial_train=365d, test=90d, step=90d
    And a date range from 2010-01-01 to 2020-01-01
    When I generate windows
    Then window 0 train should span 365 days
    And window 1 train should span 455 days (grew by 90)
    And window 2 train should span 545 days (grew again)
    And all train periods should start at 2010-01-01

  Scenario: Train always starts at dataset start
    Given a RollingSplit with initial_train=365d, test=90d, step=90d
    And 40 generated windows
    When I check train periods
    Then all train periods should start at 2010-01-01
    And train durations should increase monotonically

  Scenario: Test periods still slide (no overlap)
    Given a RollingSplit with initial_train=365d, test=90d, step=90d
    And 40 generated windows
    When I check test periods
    Then no test period should overlap with any other test period
    And each test period should immediately follow its train period
```

---

### Feature 6: IS/OOS Validation
```gherkin
Feature: IS/OOS Validation
  As a backtester
  I need to compare in-sample vs out-of-sample performance
  So that I can detect overfitting

  Scenario: Good strategy (low degradation)
    Given an IsOosValidator with min_oos_sharpe=0.5, max_degradation=40%
    And IS stats with Sharpe=1.5, DD=-12%, trades=20
    And OOS stats with Sharpe=1.2, DD=-15%, trades=8
    When I validate
    Then it should pass
    And the reason should include "degradation 20.0%"

  Scenario: Curve-fitted strategy (high degradation)
    Given an IsOosValidator with min_oos_sharpe=0.5, max_degradation=40%
    And IS stats with Sharpe=2.0, DD=-10%, trades=50
    And OOS stats with Sharpe=0.3, DD=-35%, trades=2
    When I validate
    Then it should fail
    And the reason should include "OOS degradation (85.0%) > max (40.0%)"

  Scenario: Poor OOS Sharpe (below minimum)
    Given an IsOosValidator with min_oos_sharpe=0.5, max_degradation=40%
    And IS stats with Sharpe=1.0, DD=-15%, trades=10
    And OOS stats with Sharpe=0.2, DD=-25%, trades=3
    When I validate
    Then it should fail
    And the reason should include "OOS Sharpe (0.20) < minimum (0.50)"

  Scenario: Negative IS Sharpe (skip degradation check)
    Given an IsOosValidator with min_oos_sharpe=0.5, max_degradation=40%
    And IS stats with Sharpe=-0.5, DD=-40%, trades=5
    And OOS stats with Sharpe=0.6, DD=-20%, trades=8
    When I validate
    Then it should pass (OOS Sharpe > 0.5, no degradation check)
```

---

### Feature 7: Degradation Validator (Alert Only)
```gherkin
Feature: Degradation Validator (Alert Only)
  As a backtester
  I need to monitor degradation without rejecting strategies
  So that I can log warnings for review

  Scenario: High degradation warning
    Given a DegradationValidator with warn_threshold=30%
    And IS stats with Sharpe=1.5
    And OOS stats with Sharpe=0.9
    When I validate
    Then it should pass (no rejection)
    And the reason should include "⚠️  High degradation"
    And the reason should include "40.0% drop"

  Scenario: Low degradation (no warning)
    Given a DegradationValidator with warn_threshold=30%
    And IS stats with Sharpe=1.5
    And OOS stats with Sharpe=1.3
    When I validate
    Then it should pass
    And the reason should include "✓ Low degradation"
    And the reason should include "13.3% drop"

  Scenario: Negative IS Sharpe (skip check)
    Given a DegradationValidator with warn_threshold=30%
    And IS stats with Sharpe=-0.5
    And OOS stats with Sharpe=0.6
    When I validate
    Then it should pass
    And the reason should include "IS Sharpe <= 0 (skipping degradation check)"
```

---

### Feature 8: Promotion Filter (L1 → L2)
```gherkin
Feature: Promotion Filter (L1 → L2)
  As a backtester
  I need to filter cheap L1 candidates before expensive L2 simulation
  So that I can reduce computational cost by 99%+

  Scenario: Promote good candidate
    Given a PromotionFilter with default validator
    And L1 IS stats with Sharpe=1.5, trades=20
    And L1 OOS stats with Sharpe=1.2, trades=8
    When I check should_promote
    Then it should return true
    And the reason should indicate low degradation

  Scenario: Reject curve-fitted candidate
    Given a PromotionFilter with default validator
    And L1 IS stats with Sharpe=2.0, trades=50
    And L1 OOS stats with Sharpe=0.3, trades=2
    When I check should_promote
    Then it should return false
    And the reason should include "OOS degradation (85.0%) > max"

  Scenario: Reject low OOS Sharpe
    Given a PromotionFilter with default validator
    And L1 IS stats with Sharpe=1.0
    And L1 OOS stats with Sharpe=0.2
    When I check should_promote
    Then it should return false
    And the reason should include "OOS Sharpe (0.20) < minimum (0.50)"

  Scenario: Batch filtering (1000 candidates)
    Given 1000 random candidates
    And a PromotionFilter with default validator
    When I run L1 backtests and filter
    Then I should promote ~10-50 candidates (1-5%)
    And rejected candidates should have high degradation or low OOS Sharpe
    And promoted candidates should have IS/OOS Sharpe >= 0.5 and degradation <= 40%
```

---

### Feature 9: Multi-Window Aggregation
```gherkin
Feature: Multi-Window Aggregation
  As a backtester
  I need to aggregate OOS results across multiple walk-forward windows
  So that I can compute overall OOS metrics

  Scenario: Aggregate 40 walk-forward windows
    Given a WalkForwardSplit with 40 windows
    And a strategy that runs on all windows
    When I aggregate OOS results
    Then I should have combined equity curve (stitched from 40 OOS periods)
    And I should have overall OOS Sharpe (computed from combined curve)
    And I should have overall OOS max DD
    And I should have total OOS trades (sum of all windows)

  Scenario: IS metrics (not used for final evaluation)
    Given a WalkForwardSplit with 40 windows
    And a strategy that runs on all windows
    When I aggregate IS results
    Then IS metrics are for reference only (training performance)
    And OOS metrics are the "true" strategy performance

  Scenario: Window-by-window stability check
    Given a WalkForwardSplit with 40 windows
    And OOS Sharpe per window: [1.2, 1.1, 0.9, 1.3, 1.0, ...]
    When I compute stability metrics
    Then I should have OOS Sharpe std dev (measures consistency)
    And I should flag windows with negative OOS Sharpe (regime failures)
    And I should compute "% profitable windows" (robustness metric)
```

---

## Example Flows

### Flow 1: Fixed 70/30 Split (Simple Validation)

**Setup:**
- Dataset: 2010-2020 (10 years)
- Split: 70% train (2010-2017), 30% test (2017-2020)
- Strategy: Donchian Breakout (20-day)
- Execution: L1 (cheap, deterministic)

**Steps:**

```rust
// 1. Define split
let split = FixedSplit::default_70_30();
let windows = split.generate_windows(
    NaiveDate::from_ymd(2010, 1, 1),
    NaiveDate::from_ymd(2020, 1, 1),
);
assert_eq!(windows.len(), 1);

let window = &windows[0];
println!("Train: {} to {}", window.train.start, window.train.end);
println!("Test:  {} to {}", window.test.start, window.test.end);
// Train: 2010-01-01 to 2017-01-01
// Test:  2017-01-01 to 2020-01-01

// 2. Run backtest on train period (optimize parameters)
let strategy = DonchianBreakout::new(20); // Optimized on train data
let is_stats = backtest(&strategy, &window.train, ExecutionLevel::L1);
println!("IS Sharpe: {:.2}", is_stats.sharpe_ratio); // 1.5

// 3. Run backtest on test period (validate)
let oos_stats = backtest(&strategy, &window.test, ExecutionLevel::L1);
println!("OOS Sharpe: {:.2}", oos_stats.sharpe_ratio); // 1.2

// 4. Validate
let validator = IsOosValidator::default();
let (passed, reason) = validator.validate(&is_stats, &oos_stats);
println!("{}: {}", if passed { "✓ PASS" } else { "✗ FAIL" }, reason);
// ✓ PASS: IS Sharpe 1.50 → OOS Sharpe 1.20 (degradation 20.0%)
```

**Result:** Strategy passes validation (degradation 20% < 40% threshold).

---

### Flow 2: Walk-Forward (40 Windows)

**Setup:**
- Dataset: 2010-2020 (10 years)
- Split: WalkForward (1 year train, 3 months test, slide 3 months)
- Strategy: MA Cross (50/200)
- Execution: L2 (walk-forward)

**Steps:**

```rust
// 1. Generate 40 walk-forward windows
let split = WalkForwardSplit::year_quarter();
let windows = split.generate_windows(
    NaiveDate::from_ymd(2010, 1, 1),
    NaiveDate::from_ymd(2020, 1, 1),
);
println!("Generated {} windows", windows.len()); // 40

// 2. Run backtest on each window
let strategy = MovingAverageCross::new(50, 200);
let mut oos_equity_curve = Vec::new();

for (i, window) in windows.iter().enumerate() {
    // Train on window.train (optimize or validate parameters)
    let is_stats = backtest(&strategy, &window.train, ExecutionLevel::L2);

    // Test on window.test (OOS)
    let oos_stats = backtest(&strategy, &window.test, ExecutionLevel::L2);

    println!("Window {}: IS Sharpe {:.2}, OOS Sharpe {:.2}",
        i, is_stats.sharpe_ratio, oos_stats.sharpe_ratio);

    // Accumulate OOS equity curve (stitch windows)
    oos_equity_curve.extend(oos_stats.equity_curve);
}

// 3. Compute overall OOS metrics
let overall_oos_sharpe = compute_sharpe(&oos_equity_curve);
let overall_oos_dd = compute_max_dd(&oos_equity_curve);

println!("Overall OOS Sharpe: {:.2}", overall_oos_sharpe); // 0.8
println!("Overall OOS MaxDD: {:.1}%", overall_oos_dd * 100.0); // -22%

// 4. Stability check (% profitable windows)
let profitable_windows = windows.iter()
    .filter(|w| backtest(&strategy, &w.test, ExecutionLevel::L2).sharpe_ratio > 0.0)
    .count();
let stability_pct = profitable_windows as f64 / windows.len() as f64;
println!("OOS Stability: {:.0}% profitable windows", stability_pct * 100.0);
// OOS Stability: 68% profitable windows
```

**Output:**
```
Generated 40 windows
Window 0: IS Sharpe 1.20, OOS Sharpe 1.10
Window 1: IS Sharpe 1.15, OOS Sharpe 0.95
Window 2: IS Sharpe 1.30, OOS Sharpe 1.20
...
Window 39: IS Sharpe 0.90, OOS Sharpe 0.75

Overall OOS Sharpe: 0.80
Overall OOS MaxDD: -22.0%
OOS Stability: 68% profitable windows
```

**Key insight:** OOS Sharpe 0.8 (vs IS ~1.2 avg) = moderate degradation but acceptable.

---

### Flow 3: Promotion Filter (1000 Candidates → 10 Survivors)

**Setup:**
- Candidates: 1000 parameter combos (Donchian N = 5..100, ATR stop mult = 1.0..5.0)
- L1: Fixed 70/30 split (cheap simulation)
- L2: Walk-forward 40 windows (expensive simulation)
- Goal: Filter 99% at L1, promote top 1% to L2

**Steps:**

```rust
// 1. Generate 1000 candidates
let mut candidates = Vec::new();
for n in 5..=100 {
    for stop_mult in [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0] {
        candidates.push((n, stop_mult));
    }
}
println!("Generated {} candidates", candidates.len()); // ~672

// 2. Run L1 backtest (70/30 split, cheap)
let split = FixedSplit::default_70_30();
let windows = split.generate_windows(
    NaiveDate::from_ymd(2010, 1, 1),
    NaiveDate::from_ymd(2020, 1, 1),
);
let window = &windows[0];

let mut l1_results = Vec::new();
for (n, stop_mult) in &candidates {
    let strategy = DonchianBreakout::new(*n)
        .with_stop(AtrStop::new(*stop_mult, true));

    let is_stats = backtest(&strategy, &window.train, ExecutionLevel::L1);
    let oos_stats = backtest(&strategy, &window.test, ExecutionLevel::L1);

    l1_results.push(((*n, *stop_mult), is_stats, oos_stats));
}

// 3. Filter via promotion filter
let filter = PromotionFilter::default();
let mut promoted = Vec::new();

for ((n, stop_mult), is_stats, oos_stats) in l1_results {
    let (should_promote, reason) = filter.should_promote(&is_stats, &oos_stats);
    if should_promote {
        println!("✓ Promoting Donchian(N={}, stop={}x): {}", n, stop_mult, reason);
        promoted.push((n, stop_mult));
    }
}

println!("\nL1 → L2: {} / {} promoted ({:.1}% filtered)",
    promoted.len(),
    candidates.len(),
    (1.0 - promoted.len() as f64 / candidates.len() as f64) * 100.0
);
// L1 → L2: 8 / 672 promoted (98.8% filtered)

// 4. Re-run survivors at L2 (walk-forward 40 windows)
let wf_split = WalkForwardSplit::year_quarter();
let wf_windows = wf_split.generate_windows(
    NaiveDate::from_ymd(2010, 1, 1),
    NaiveDate::from_ymd(2020, 1, 1),
);

for (n, stop_mult) in promoted {
    let strategy = DonchianBreakout::new(n)
        .with_stop(AtrStop::new(stop_mult, true));

    let mut oos_equity = Vec::new();
    for window in &wf_windows {
        let is_stats = backtest(&strategy, &window.train, ExecutionLevel::L2);
        let oos_stats = backtest(&strategy, &window.test, ExecutionLevel::L2);
        oos_equity.extend(oos_stats.equity_curve);
    }

    let oos_sharpe = compute_sharpe(&oos_equity);
    println!("L2 OOS: Donchian(N={}, stop={}x) → Sharpe {:.2}",
        n, stop_mult, oos_sharpe);
}
```

**Output:**
```
✓ Promoting Donchian(N=20, stop=2.0x): IS Sharpe 1.50 → OOS Sharpe 1.20 (degradation 20.0%)
✓ Promoting Donchian(N=25, stop=2.5x): IS Sharpe 1.40 → OOS Sharpe 1.15 (degradation 17.9%)
✓ Promoting Donchian(N=30, stop=2.0x): IS Sharpe 1.35 → OOS Sharpe 1.10 (degradation 18.5%)
...

L1 → L2: 8 / 672 promoted (98.8% filtered)

L2 OOS: Donchian(N=20, stop=2.0x) → Sharpe 1.15
L2 OOS: Donchian(N=25, stop=2.5x) → Sharpe 1.10
L2 OOS: Donchian(N=30, stop=2.0x) → Sharpe 0.95
...
```

**Key benefit:** 99% of candidates rejected at L1 (cheap), only 1% tested at L2 (expensive).

---

## Integration with Existing Codebase

### Runner Integration

```rust
// trendlab-runner/src/sweep/walkforward_sweep.rs

use trendlab_core::validation::split::{Split, WalkForwardSplit};
use trendlab_core::validation::validator::{Validator, IsOosValidator};
use trendlab_core::validation::promotion::PromotionFilter;
use trendlab_core::portfolio::performance::PerformanceStats;
use std::sync::Arc;

pub struct WalkForwardSweep {
    split: Arc<dyn Split>,
    validator: Arc<dyn Validator>,
    promotion_filter: PromotionFilter,
}

impl WalkForwardSweep {
    pub fn new(
        split: Arc<dyn Split>,
        validator: Arc<dyn Validator>,
    ) -> Self {
        Self {
            split,
            validator: validator.clone(),
            promotion_filter: PromotionFilter::new(validator),
        }
    }

    pub fn default_year_quarter() -> Self {
        Self::new(
            Arc::new(WalkForwardSplit::year_quarter()),
            Arc::new(IsOosValidator::default()),
        )
    }

    pub fn run(
        &self,
        candidates: Vec<Strategy>,
        data_start: NaiveDate,
        data_end: NaiveDate,
    ) -> Vec<StrategyResult> {
        // 1. Generate windows
        let windows = self.split.generate_windows(data_start, data_end);

        // 2. Run each candidate on all windows
        let mut results = Vec::new();
        for candidate in candidates {
            let mut oos_equity = Vec::new();
            let mut is_metrics = Vec::new();
            let mut oos_metrics = Vec::new();

            for window in &windows {
                let is_stats = backtest(&candidate, &window.train, L2);
                let oos_stats = backtest(&candidate, &window.test, L2);

                is_metrics.push(is_stats);
                oos_metrics.push(oos_stats.clone());
                oos_equity.extend(oos_stats.equity_curve);
            }

            // 3. Compute overall OOS metrics
            let overall_oos_sharpe = compute_sharpe(&oos_equity);
            let overall_oos_dd = compute_max_dd(&oos_equity);

            results.push(StrategyResult {
                strategy: candidate,
                oos_sharpe: overall_oos_sharpe,
                oos_max_dd: overall_oos_dd,
                windows: oos_metrics,
            });
        }

        results
    }
}
```

---

### TUI Integration (OOS Report Display)

```rust
// trendlab-tui/src/views/oos_report.rs

use ratatui::widgets::{Block, Borders, Table, Row, Cell};
use ratatui::style::{Style, Color};

pub fn render_oos_report(
    frame: &mut Frame,
    area: Rect,
    results: &[StrategyResult],
) {
    let header = Row::new(vec![
        Cell::from("Strategy"),
        Cell::from("IS Sharpe"),
        Cell::from("OOS Sharpe"),
        Cell::from("Degradation"),
        Cell::from("OOS MaxDD"),
        Cell::from("Status"),
    ])
    .style(Style::default().fg(Color::Cyan));

    let rows: Vec<Row> = results.iter().map(|r| {
        let is_sharpe = r.avg_is_sharpe();
        let degradation = (is_sharpe - r.oos_sharpe) / is_sharpe * 100.0;

        let status = if degradation > 40.0 {
            ("❌ Curve-fit", Color::Red)
        } else if r.oos_sharpe < 0.5 {
            ("⚠️  Low OOS", Color::Yellow)
        } else {
            ("✓ Pass", Color::Green)
        };

        Row::new(vec![
            Cell::from(r.strategy.name()),
            Cell::from(format!("{:.2}", is_sharpe)),
            Cell::from(format!("{:.2}", r.oos_sharpe)),
            Cell::from(format!("{:.1}%", degradation)),
            Cell::from(format!("{:.1}%", r.oos_max_dd * 100.0)),
            Cell::from(status.0).style(Style::default().fg(status.1)),
        ])
    }).collect();

    let table = Table::new(rows)
        .header(header)
        .block(Block::default()
            .title("Walk-Forward OOS Report")
            .borders(Borders::ALL))
        .widths(&[
            Constraint::Percentage(25),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
            Constraint::Percentage(15),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
        ]);

    frame.render_widget(table, area);
}
```

---

## Completion Criteria (20 items)

### Architecture & Core Types (5 items)
- [ ] Period struct defined (start/end dates, label, contains/duration methods)
- [ ] Window struct defined (train + test pair, non-overlapping validation)
- [ ] Split trait defined (generate_windows method)
- [ ] Validator trait defined (validate IS vs OOS)
- [ ] PromotionFilter struct defined (L1 → L2 filter)

### Split Implementations (4 items)
- [ ] FixedSplit implemented (single 70/30 split)
- [ ] DateSplit implemented (explicit date boundary)
- [ ] WalkForwardSplit implemented (rolling train/test windows)
- [ ] RollingSplit implemented (expanding train window)

### Validators (3 items)
- [ ] IsOosValidator implemented (min OOS Sharpe + max degradation checks)
- [ ] DegradationValidator implemented (alert-only, no rejection)
- [ ] Validation logic handles negative IS Sharpe (skip degradation check)

### Window Generation & Edge Cases (4 items)
- [ ] WalkForwardSplit generates correct number of windows
- [ ] Windows never overlap (test periods are disjoint)
- [ ] Train always ends before test starts (no leakage)
- [ ] Edge case: insufficient data (no windows generated if data too short)

### Integration & Usage (4 items)
- [ ] WalkForwardSweep runner implemented (multi-window backtest)
- [ ] OOS equity curve stitching (combine all OOS periods)
- [ ] Overall OOS metrics (Sharpe/DD/trades from combined curve)
- [ ] TUI OOS report (table with IS/OOS/degradation columns)

---

## Why M8 Matters

**M8 solves the overfitting problem.**

Traditional backtesting systems optimize on all data, then report results on the same data. This guarantees curve-fitting:

❌ **Train on 2010-2020, test on 2010-2020 = meaningless**

M8 introduces strict train/test separation:

✅ **Train on 2010-2017, test on 2017-2020 = valid**
✅ **Walk-forward: 40 independent test windows = robust**
✅ **Promotion filter: reject curve-fitted candidates early = efficient**

### Key Benefits

1. **Overfitting Detection:**
   If IS Sharpe >> OOS Sharpe → **reject** (curve-fitted)

2. **Fair Comparisons:**
   All strategies tested on identical unseen data (apples-to-apples)

3. **Computational Efficiency:**
   Filter 99% of candidates at L1 (cheap) before L2 (expensive)

4. **Robustness:**
   Walk-forward tests strategies across multiple regimes (not just one lucky period)

### Example Impact

**Before M8 (train = test):**
```
Strategy X: Sharpe 2.1 (on all data)
Deploy → Live Sharpe 0.3 ❌ (curve-fitted!)
```

**After M8 (walk-forward OOS):**
```
Strategy X:
  IS Sharpe: 2.1 (training)
  OOS Sharpe: 0.3 (validation)
  Degradation: 85% → ❌ REJECT (curve-fitted!)

Strategy Y:
  IS Sharpe: 1.5 (training)
  OOS Sharpe: 1.2 (validation)
  Degradation: 20% → ✓ PROMOTE (robust!)
Deploy → Live Sharpe 1.1 ✓ (validated!)
```

M8 prevents deploying garbage strategies.

---

## Next Steps

**M9 (Execution Monte Carlo)** is next. This milestone covers:

- Slippage distributions (historical sampling)
- Adverse selection (limit fills skewed to unfavorable prices)
- Queue depth simulation (not all limit orders fill)
- Execution MC trials (sample N paths, aggregate results)
- Promotion ladder L3 filter (pass L2 → earn expensive L3 simulation)

**Estimated LOC:** ~700 lines
**Complexity:** Medium-High (distributions + sampling logic)

---

## Options

1. **Continue immediately with M9** (Execution Monte Carlo)
2. **Pause for review** (now 10/12 milestones = 83% complete)
3. **Skip to M10 or M11** (Path MC, Bootstrap)
4. **Adjust approach** (different format, focus areas, etc.)

---

**M8 Complete!** ✅
