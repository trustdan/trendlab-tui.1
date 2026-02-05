# M9: Execution Monte Carlo — Technical Specification

**Milestone:** M9 (Level 3 Promotion — Execution Realism)
**Status:** Specification Complete ✅
**Estimated LOC:** ~700 lines
**Complexity:** Medium-High

---

## Overview

**M9 implements Level 3 of the promotion ladder: Execution Monte Carlo (MC).**

### The Problem

Traditional backtesting assumes **fixed slippage** (e.g., 5 bps per trade). This is unrealistic:

- **Real slippage varies** based on volatility, liquidity, market conditions
- **Adverse selection bias**: Limit orders fill when price moves *against* you
- **Queue depth**: Not all limit orders execute (partial fills, no fills)
- **Fixed slippage = optimistic bias**: Real trading is messier!

### M9's Solution

Instead of fixed slippage, **sample from realistic distributions**:

1. **Slippage distributions** (Historical, Gaussian, Regime-conditional)
2. **Adverse selection** (limits skewed to unfavorable prices)
3. **Queue depth model** (partial fills based on volume/spread)
4. **Monte Carlo trials**: Run N paths (e.g., 100), get distribution of outcomes

**Result:** More realistic performance estimates + uncertainty bands (P10/P50/P90).

---

## Goals

1. **Replace fixed slippage with distributions** (Historical, Gaussian, Uniform)
2. **Adverse selection for limit orders** (fill bias toward unfavorable prices)
3. **Queue depth model** (partial fills, no fills based on volume)
4. **MC trial engine** (run N trials, aggregate stats, compute percentiles)
5. **L3 promotion filter** (L2 survivors → earn expensive L3 simulation)
6. **Performance stats with uncertainty** (P10/P50/P90 Sharpe/DD)

---

## Architecture

### File Structure (~700 lines, 8 modules)

```
trendlab-core/src/execution/
├── mod.rs                          # Re-exports
├── slippage.rs                     # SlippageDistribution trait
├── slippage/
│   ├── fixed.rs                    # Fixed (legacy, L1)
│   ├── historical.rs               # Historical sampling (L3)
│   ├── gaussian.rs                 # Gaussian(μ, σ) (L3)
│   ├── uniform.rs                  # Uniform(min, max) (L3)
│   └── regime_conditional.rs       # Regime-dependent (L3+)
├── adverse_selection.rs            # Adverse selection model
├── queue_depth.rs                  # Partial fill model
├── mc_trial.rs                     # Single MC trial
├── mc_engine.rs                    # Run N trials + aggregate
└── promotion_l3.rs                 # L2 → L3 filter
```

### Dependencies

```toml
[dependencies]
rand = "0.8"                        # Random sampling
statrs = "0.17"                     # Distributions (Normal, etc.)
```

---

## Core Concepts

### 1. Slippage Distribution

**Trait:**

```rust
pub trait SlippageDistribution: Send + Sync {
    /// Sample slippage in basis points (positive = unfavorable)
    fn sample(&self, rng: &mut impl Rng) -> f64;

    /// Distribution name
    fn name(&self) -> &str;

    /// Expected (mean) slippage
    fn expected_bps(&self) -> f64;
}
```

**Implementations:**

#### A) FixedSlippage (L1, deterministic)

```rust
pub struct FixedSlippage {
    pub bps: f64,  // e.g., 5.0 bps
}

impl SlippageDistribution for FixedSlippage {
    fn sample(&self, _rng: &mut impl Rng) -> f64 {
        self.bps  // Always returns same value
    }

    fn expected_bps(&self) -> f64 { self.bps }
}
```

**Use case:** L1 (cheap, deterministic). Not realistic!

#### B) HistoricalSlippage (L3, realistic)

```rust
pub struct HistoricalSlippage {
    pub samples: Vec<f64>,  // Historical slippage observations (bps)
}

impl SlippageDistribution for HistoricalSlippage {
    fn sample(&self, rng: &mut impl Rng) -> f64 {
        // Bootstrap: sample with replacement
        self.samples.choose(rng).copied().unwrap_or(5.0)
    }

    fn expected_bps(&self) -> f64 {
        self.samples.iter().sum::<f64>() / self.samples.len() as f64
    }
}
```

**Use case:** Most realistic (uses actual historical slippage data).

**Example historical data:**

```rust
let historical = HistoricalSlippage {
    samples: vec![
        3.2, 4.1, 5.8, 7.2, 3.9,  // Normal days
        12.4, 15.3, 18.1,          // High volatility days
        2.1, 2.8, 3.5,             // Low volatility days
    ],
};
```

#### C) GaussianSlippage (L3, parametric)

```rust
pub struct GaussianSlippage {
    pub mean_bps: f64,
    pub std_bps: f64,
}

impl SlippageDistribution for GaussianSlippage {
    fn sample(&self, rng: &mut impl Rng) -> f64 {
        let normal = Normal::new(self.mean_bps, self.std_bps).unwrap();
        normal.sample(rng).max(0.0)  // Clamp to non-negative
    }

    fn expected_bps(&self) -> f64 { self.mean_bps }
}
```

**Use case:** When historical data unavailable (assume normal distribution).

**Example:**

```rust
let gaussian = GaussianSlippage {
    mean_bps: 5.0,
    std_bps: 2.0,   // σ = 2 bps
};
```

#### D) UniformSlippage (L3, conservative)

```rust
pub struct UniformSlippage {
    pub min_bps: f64,
    pub max_bps: f64,
}

impl SlippageDistribution for UniformSlippage {
    fn sample(&self, rng: &mut impl Rng) -> f64 {
        rng.gen_range(self.min_bps..=self.max_bps)
    }

    fn expected_bps(&self) -> f64 {
        (self.min_bps + self.max_bps) / 2.0
    }
}
```

**Use case:** Conservative stress testing (equal weight to all outcomes).

**Example:**

```rust
let uniform = UniformSlippage {
    min_bps: 2.0,
    max_bps: 10.0,  // Uniform [2, 10] bps
};
```

#### E) RegimeConditionalSlippage (L3+, advanced)

```rust
pub struct RegimeConditionalSlippage {
    pub low_vol: Box<dyn SlippageDistribution>,    // VIX < 15
    pub normal_vol: Box<dyn SlippageDistribution>,  // VIX 15-25
    pub high_vol: Box<dyn SlippageDistribution>,    // VIX > 25
}

impl SlippageDistribution for RegimeConditionalSlippage {
    fn sample(&self, rng: &mut impl Rng, context: &BarContext) -> f64 {
        let vix = context.vix;
        match vix {
            v if v < 15.0 => self.low_vol.sample(rng),
            v if v > 25.0 => self.high_vol.sample(rng),
            _ => self.normal_vol.sample(rng),
        }
    }
}
```

**Use case:** Condition slippage on market regime (volatility, liquidity).

**Example:**

```rust
let regime = RegimeConditionalSlippage {
    low_vol: Box::new(GaussianSlippage { mean_bps: 2.0, std_bps: 0.5 }),
    normal_vol: Box::new(GaussianSlippage { mean_bps: 5.0, std_bps: 1.5 }),
    high_vol: Box::new(GaussianSlippage { mean_bps: 12.0, std_bps: 4.0 }),
};
```

---

### 2. Adverse Selection (Limit Order Bias)

**Problem:** Limit orders are **not neutral** — they fill when price moves *against* you!

**Example:**
- Place limit buy at $100
- If price drops to $99.50, limit fills at $100 (you overpaid!)
- If price rises to $101, limit doesn't fill (you missed the move)

**Result:** Limit fills are biased toward unfavorable prices.

**Model:**

```rust
pub struct AdverseSelectionModel {
    pub skew_factor: f64,  // 0.0 = neutral, 1.0 = full adverse
}

impl AdverseSelectionModel {
    /// Adjust fill price for adverse selection
    pub fn adjust_limit_fill(
        &self,
        order: &Order,
        bar: &Bar,
        rng: &mut impl Rng,
    ) -> Option<f64> {
        match order.order_type {
            OrderType::Limit { limit_price } => {
                if order.side == Side::Buy {
                    // Buy limit: fill closer to high (worse price)
                    let neutral_fill = limit_price;
                    let adverse_fill = bar.high.min(limit_price);
                    let fill = self.interpolate(neutral_fill, adverse_fill, rng);
                    Some(fill)
                } else {
                    // Sell limit: fill closer to low (worse price)
                    let neutral_fill = limit_price;
                    let adverse_fill = bar.low.max(limit_price);
                    let fill = self.interpolate(neutral_fill, adverse_fill, rng);
                    Some(fill)
                }
            }
            _ => None,
        }
    }

    fn interpolate(&self, neutral: f64, adverse: f64, rng: &mut impl Rng) -> f64 {
        // skew_factor = 0.0 → neutral
        // skew_factor = 1.0 → fully adverse
        // skew_factor = 0.5 → sample uniformly in [neutral, adverse]
        let t = rng.gen_range(0.0..=self.skew_factor);
        neutral + t * (adverse - neutral)
    }
}
```

**Example:**

```rust
let adverse = AdverseSelectionModel { skew_factor: 0.7 };

// Bar: low=$99, high=$102
// Limit buy at $100
// Neutral fill: $100
// Adverse fill: $102 (high)
// Actual fill: $100 + rand(0.0-0.7) * ($102 - $100)
//            = $100 to $101.40 (biased toward $102)
```

**Presets:**

```rust
impl AdverseSelectionModel {
    pub fn neutral() -> Self {
        Self { skew_factor: 0.0 }  // No bias (legacy)
    }

    pub fn moderate() -> Self {
        Self { skew_factor: 0.5 }  // 50% adverse bias
    }

    pub fn aggressive() -> Self {
        Self { skew_factor: 1.0 }  // Full adverse bias
    }
}
```

---

### 3. Queue Depth (Partial Fills)

**Problem:** Not all limit orders execute (you're in a queue behind others).

**Model:**

```rust
pub struct QueueDepthModel {
    pub fill_probability: f64,  // 0.0 = never fills, 1.0 = always fills
}

impl QueueDepthModel {
    /// Determine if limit order fills (and what fraction)
    pub fn sample_fill_fraction(&self, rng: &mut impl Rng) -> f64 {
        if rng.gen_bool(self.fill_probability) {
            1.0  // Full fill
        } else {
            0.0  // No fill (missed the queue)
        }
    }
}
```

**Example:**

```rust
let queue = QueueDepthModel { fill_probability: 0.8 };

// 80% of limit orders fill
// 20% of limit orders don't execute (missed the queue)
```

**Advanced (volume-based):**

```rust
pub struct VolumeQueueDepthModel {
    pub order_size: u64,
    pub volume_fraction_threshold: f64,  // 0.05 = 5% of bar volume
}

impl VolumeQueueDepthModel {
    pub fn sample_fill_fraction(&self, bar: &Bar, rng: &mut impl Rng) -> f64 {
        let our_fraction = self.order_size as f64 / bar.volume as f64;

        if our_fraction <= self.volume_fraction_threshold {
            1.0  // Small order → full fill
        } else {
            // Large order → partial fill
            rng.gen_range(0.3..=0.8)  // Fill 30-80%
        }
    }
}
```

**Use case:** Large orders (> 5% of volume) get partial fills.

---

### 4. MC Trial (Single Execution Path)

**Struct:**

```rust
pub struct ExecutionMcTrial {
    pub trial_id: usize,
    pub slippage_dist: Arc<dyn SlippageDistribution>,
    pub adverse_selection: AdverseSelectionModel,
    pub queue_depth: QueueDepthModel,
    pub seed: u64,  // For reproducibility
}

impl ExecutionMcTrial {
    /// Run a single backtest with sampled execution
    pub fn run(
        &self,
        candidate: &Candidate,
        data: &TimeSeriesData,
        orders: &[Order],
    ) -> TrialResult {
        let mut rng = StdRng::seed_from_u64(self.seed);
        let mut portfolio = Portfolio::new(100_000.0);

        for bar in data.bars() {
            // Process orders for this bar
            for order in orders.iter().filter(|o| o.bar_index == bar.index) {
                // Sample slippage
                let slippage_bps = self.slippage_dist.sample(&mut rng);

                // Apply adverse selection (limits)
                let fill_price = match order.order_type {
                    OrderType::Limit { .. } => {
                        self.adverse_selection.adjust_limit_fill(order, bar, &mut rng)
                    }
                    OrderType::Market => {
                        Some(self.sample_market_fill(order, bar, slippage_bps))
                    }
                    _ => None,
                };

                // Check queue depth (limits)
                if matches!(order.order_type, OrderType::Limit { .. }) {
                    let fill_fraction = self.queue_depth.sample_fill_fraction(&mut rng);
                    if fill_fraction == 0.0 {
                        continue;  // No fill (missed queue)
                    }
                }

                // Execute fill
                if let Some(price) = fill_price {
                    portfolio.execute_fill(order, price);
                }
            }

            // Update portfolio equity
            portfolio.update(bar);
        }

        TrialResult {
            trial_id: self.trial_id,
            final_equity: portfolio.equity(),
            sharpe: portfolio.sharpe_ratio(),
            max_drawdown: portfolio.max_drawdown(),
            num_trades: portfolio.trades.len(),
        }
    }

    fn sample_market_fill(
        &self,
        order: &Order,
        bar: &Bar,
        slippage_bps: f64,
    ) -> f64 {
        let base_price = match order.side {
            Side::Buy => bar.open,
            Side::Sell => bar.open,
        };

        let slippage = base_price * (slippage_bps / 10_000.0);

        match order.side {
            Side::Buy => base_price + slippage,   // Pay more
            Side::Sell => base_price - slippage,  // Receive less
        }
    }
}
```

---

### 5. MC Engine (Aggregate N Trials)

**Struct:**

```rust
pub struct ExecutionMcEngine {
    pub num_trials: usize,         // e.g., 100
    pub slippage_dist: Arc<dyn SlippageDistribution>,
    pub adverse_selection: AdverseSelectionModel,
    pub queue_depth: QueueDepthModel,
}

impl ExecutionMcEngine {
    /// Run N Monte Carlo trials, aggregate results
    pub fn run(
        &self,
        candidate: &Candidate,
        data: &TimeSeriesData,
        orders: &[Order],
    ) -> McResult {
        let mut results = Vec::with_capacity(self.num_trials);

        // Run N trials in parallel
        let trials: Vec<_> = (0..self.num_trials)
            .map(|i| ExecutionMcTrial {
                trial_id: i,
                slippage_dist: Arc::clone(&self.slippage_dist),
                adverse_selection: self.adverse_selection.clone(),
                queue_depth: self.queue_depth.clone(),
                seed: i as u64,  // Different seed per trial
            })
            .collect();

        for trial in trials {
            let result = trial.run(candidate, data, orders);
            results.push(result);
        }

        // Aggregate: compute percentiles
        McResult::from_trials(results)
    }
}
```

**Result:**

```rust
pub struct McResult {
    pub num_trials: usize,
    pub sharpe_p10: f64,   // 10th percentile (pessimistic)
    pub sharpe_p50: f64,   // Median
    pub sharpe_p90: f64,   // 90th percentile (optimistic)
    pub dd_p10: f64,       // 10th percentile (best DD)
    pub dd_p50: f64,       // Median DD
    pub dd_p90: f64,       // 90th percentile (worst DD)
    pub equity_curves: Vec<Vec<f64>>,  // All trial equity curves
}

impl McResult {
    fn from_trials(mut results: Vec<TrialResult>) -> Self {
        // Sort by Sharpe
        results.sort_by(|a, b| a.sharpe.partial_cmp(&b.sharpe).unwrap());

        let p10_idx = (results.len() as f64 * 0.10) as usize;
        let p50_idx = (results.len() as f64 * 0.50) as usize;
        let p90_idx = (results.len() as f64 * 0.90) as usize;

        Self {
            num_trials: results.len(),
            sharpe_p10: results[p10_idx].sharpe,
            sharpe_p50: results[p50_idx].sharpe,
            sharpe_p90: results[p90_idx].sharpe,
            // ... (same for DD)
            equity_curves: results.iter().map(|r| r.equity_curve.clone()).collect(),
        }
    }
}
```

**Example output:**

```
MC Results (100 trials):
  Sharpe Ratio:
    P10 (pessimistic): 0.85
    P50 (median):      1.20
    P90 (optimistic):  1.55

  Max Drawdown:
    P10 (best):   -12%
    P50 (median): -18%
    P90 (worst):  -25%
```

---

### 6. L3 Promotion Filter (L2 → L3)

**Problem:** Running MC (100 trials each) is 100× more expensive than L2!

**Solution:** Promotion filter (only run L3 on best L2 survivors).

**Struct:**

```rust
pub struct L3PromotionFilter {
    pub min_oos_sharpe: f64,       // e.g., 0.8 (higher than L2)
    pub max_oos_drawdown: f64,     // e.g., -20%
    pub min_stability: f64,        // e.g., 0.70 (70% profitable windows)
}

impl L3PromotionFilter {
    pub fn should_promote(&self, l2_result: &WalkForwardResult) -> (bool, String) {
        // Check 1: OOS Sharpe
        if l2_result.oos_sharpe < self.min_oos_sharpe {
            return (
                false,
                format!("OOS Sharpe ({:.2}) < min ({:.2})",
                    l2_result.oos_sharpe, self.min_oos_sharpe)
            );
        }

        // Check 2: OOS Drawdown
        if l2_result.oos_max_drawdown < self.max_oos_drawdown {
            return (
                false,
                format!("OOS DD ({:.1}%) > max ({:.1}%)",
                    l2_result.oos_max_drawdown * 100.0,
                    self.max_oos_drawdown * 100.0)
            );
        }

        // Check 3: Stability
        if l2_result.stability < self.min_stability {
            return (
                false,
                format!("Stability ({:.0}%) < min ({:.0}%)",
                    l2_result.stability * 100.0,
                    self.min_stability * 100.0)
            );
        }

        // PROMOTE!
        (
            true,
            format!("L2 → L3: Sharpe {:.2}, DD {:.1}%, Stability {:.0}%",
                l2_result.oos_sharpe,
                l2_result.oos_max_drawdown * 100.0,
                l2_result.stability * 100.0)
        )
    }
}
```

**Preset:**

```rust
impl L3PromotionFilter {
    pub fn default() -> Self {
        Self {
            min_oos_sharpe: 0.8,
            max_oos_drawdown: -0.20,  // -20%
            min_stability: 0.70,       // 70%
        }
    }
}
```

**Example flow:**

```rust
let l3_filter = L3PromotionFilter::default();

// L2 results (10 survivors from L1)
for candidate in l2_survivors {
    let l2_result = run_walk_forward(candidate);  // 40 windows

    let (promote, reason) = l3_filter.should_promote(&l2_result);
    if promote {
        println!("✓ L2 → L3: {}: {}", candidate.name(), reason);
        l3_candidates.push(candidate);
    } else {
        println!("✗ Rejected at L2: {}: {}", candidate.name(), reason);
    }
}

// L3: Run MC (100 trials) on ~3 survivors
for candidate in l3_candidates {
    let mc_result = mc_engine.run(candidate);  // 100 trials
    println!("MC Result: P10={:.2}, P50={:.2}, P90={:.2}",
        mc_result.sharpe_p10,
        mc_result.sharpe_p50,
        mc_result.sharpe_p90);
}
```

**Example output:**

```
L2 Results (10 candidates):
  ✓ L2 → L3: Donchian(20, 2.0x): Sharpe 1.15, DD -16%, Stability 75%
  ✓ L2 → L3: Donchian(25, 2.5x): Sharpe 1.10, DD -18%, Stability 72%
  ✗ Rejected at L2: MA_Cross(50,200): Sharpe 0.75 < min 0.80
  ✗ Rejected at L2: RSI_Mean(14): DD -24% > max -20%
  ...

L2 → L3: 3 / 10 promoted (70% filtered)

L3 Results (3 × 100 = 300 trials):
  Donchian(20, 2.0x):
    P10: 0.92  P50: 1.15  P90: 1.38
  Donchian(25, 2.5x):
    P10: 0.85  P50: 1.10  P90: 1.35
  ...
```

---

## BDD Scenarios

### Feature 1: Slippage Distributions (5 scenarios)

#### Scenario 1.1: Fixed slippage (deterministic)

```gherkin
Feature: Fixed slippage distribution

Scenario: Sample fixed slippage multiple times
  Given a FixedSlippage distribution with 5.0 bps
  When I sample 100 times
  Then all samples should equal 5.0 bps
  And expected_bps should equal 5.0
```

#### Scenario 1.2: Historical slippage (bootstrap sampling)

```gherkin
Scenario: Sample historical slippage
  Given a HistoricalSlippage distribution with samples [3.0, 5.0, 7.0, 12.0]
  When I sample 1000 times
  Then all samples should be in [3.0, 5.0, 7.0, 12.0]
  And expected_bps should equal 6.75  # (3+5+7+12)/4
  And the empirical distribution should match the input distribution
```

#### Scenario 1.3: Gaussian slippage (parametric)

```gherkin
Scenario: Sample Gaussian slippage
  Given a GaussianSlippage distribution with mean=5.0 bps, std=2.0 bps
  When I sample 10000 times
  Then the mean should be approximately 5.0 (±0.1)
  And the std should be approximately 2.0 (±0.1)
  And all samples should be non-negative
```

#### Scenario 1.4: Uniform slippage (conservative)

```gherkin
Scenario: Sample uniform slippage
  Given a UniformSlippage distribution with min=2.0 bps, max=10.0 bps
  When I sample 1000 times
  Then all samples should be in range [2.0, 10.0]
  And expected_bps should equal 6.0  # (2+10)/2
  And the distribution should be approximately uniform
```

#### Scenario 1.5: Regime-conditional slippage

```gherkin
Scenario: Sample regime-conditional slippage
  Given a RegimeConditionalSlippage with:
    | Regime     | Mean | Std |
    | Low Vol    | 2.0  | 0.5 |
    | Normal Vol | 5.0  | 1.5 |
    | High Vol   | 12.0 | 4.0 |
  When I sample with VIX=10 (low vol)
  Then samples should have mean ≈ 2.0
  When I sample with VIX=20 (normal vol)
  Then samples should have mean ≈ 5.0
  When I sample with VIX=35 (high vol)
  Then samples should have mean ≈ 12.0
```

---

### Feature 2: Adverse Selection (4 scenarios)

#### Scenario 2.1: Neutral (no bias)

```gherkin
Feature: Adverse selection for limit orders

Scenario: Neutral adverse selection (skew=0.0)
  Given an AdverseSelectionModel with skew_factor=0.0
  And a limit buy order at $100
  And a bar with low=$99, high=$102
  When I sample fill price 100 times
  Then all fills should equal $100 (no bias)
```

#### Scenario 2.2: Moderate adverse bias

```gherkin
Scenario: Moderate adverse selection (skew=0.5)
  Given an AdverseSelectionModel with skew_factor=0.5
  And a limit buy order at $100
  And a bar with low=$99, high=$102
  When I sample fill price 1000 times
  Then mean fill should be approximately $101 (between $100 and $102)
  And all fills should be in range [$100, $102]
```

#### Scenario 2.3: Full adverse bias

```gherkin
Scenario: Full adverse selection (skew=1.0)
  Given an AdverseSelectionModel with skew_factor=1.0
  And a limit buy order at $100
  And a bar with low=$99, high=$102
  When I sample fill price 100 times
  Then all fills should be in range [$100, $102]
  And mean fill should be approximately $102 (worst case)
```

#### Scenario 2.4: Sell limit adverse selection

```gherkin
Scenario: Sell limit adverse selection
  Given an AdverseSelectionModel with skew_factor=0.7
  And a limit sell order at $100
  And a bar with low=$98, high=$102
  When I sample fill price 1000 times
  Then mean fill should be < $100 (biased toward low=$98)
  And all fills should be in range [$98, $100]
```

---

### Feature 3: Queue Depth (3 scenarios)

#### Scenario 3.1: Full fill probability

```gherkin
Feature: Queue depth and partial fills

Scenario: High fill probability (80%)
  Given a QueueDepthModel with fill_probability=0.8
  When I sample 1000 limit orders
  Then approximately 800 should fill (1.0 fraction)
  And approximately 200 should not fill (0.0 fraction)
```

#### Scenario 3.2: Low fill probability

```gherkin
Scenario: Low fill probability (30%)
  Given a QueueDepthModel with fill_probability=0.3
  When I sample 1000 limit orders
  Then approximately 300 should fill
  And approximately 700 should not fill
```

#### Scenario 3.3: Volume-based partial fills

```gherkin
Scenario: Large order gets partial fill
  Given a VolumeQueueDepthModel with order_size=10000, threshold=0.05
  And a bar with volume=50000 (order is 20% of volume)
  When I sample fill fraction 100 times
  Then mean fill fraction should be in range [0.3, 0.8]  # Partial fills

  Given a bar with volume=500000 (order is 2% of volume)
  When I sample fill fraction 100 times
  Then all fill fractions should equal 1.0  # Full fills (small order)
```

---

### Feature 4: MC Trial Execution (4 scenarios)

#### Scenario 4.1: Single trial deterministic (seed)

```gherkin
Feature: Monte Carlo trial execution

Scenario: Single trial is deterministic (given seed)
  Given an ExecutionMcTrial with seed=42
  And a Donchian(20) candidate
  And historical data (2010-2020)
  When I run the trial twice with seed=42
  Then both results should be identical (same slippage samples)
```

#### Scenario 4.2: Different seeds produce different results

```gherkin
Scenario: Different seeds produce different outcomes
  Given an ExecutionMcTrial with GaussianSlippage(5.0, 2.0)
  When I run with seed=1
  And I run with seed=2
  Then the results should be different (different slippage samples)
  But both results should have similar expected Sharpe (within 0.2)
```

#### Scenario 4.3: Adverse selection reduces Sharpe

```gherkin
Scenario: Adverse selection reduces performance
  Given a strategy with 50% limit orders, 50% market orders
  When I run with AdverseSelectionModel(skew=0.0)  # Neutral
  Then the Sharpe is 1.20

  When I run with AdverseSelectionModel(skew=1.0)  # Full adverse
  Then the Sharpe should be < 1.20 (degraded due to adverse fills)
```

#### Scenario 4.4: Queue depth reduces number of fills

```gherkin
Scenario: Queue depth reduces fill rate
  Given a strategy that emits 100 limit orders
  When I run with QueueDepthModel(fill_probability=1.0)
  Then 100 orders should fill

  When I run with QueueDepthModel(fill_probability=0.7)
  Then approximately 70 orders should fill
  And final_equity should be lower (fewer trades)
```

---

### Feature 5: MC Engine Aggregation (4 scenarios)

#### Scenario 5.1: Aggregate 100 trials

```gherkin
Feature: MC Engine aggregation

Scenario: Run 100 trials and compute percentiles
  Given an ExecutionMcEngine with 100 trials
  And GaussianSlippage(5.0, 2.0)
  When I run MC on Donchian(20)
  Then I should get 100 TrialResults
  And sharpe_p10 < sharpe_p50 < sharpe_p90
  And dd_p10 > dd_p50 > dd_p90  # P10 is best (least negative)
```

#### Scenario 5.2: P50 (median) matches expected

```gherkin
Scenario: Median Sharpe matches deterministic expected
  Given an ExecutionMcEngine with 1000 trials
  And GaussianSlippage(5.0, 1.0)
  When I run MC on a strategy with expected Sharpe ≈ 1.20
  Then sharpe_p50 should be approximately 1.20 (±0.1)
```

#### Scenario 5.3: Uncertainty bands (P10-P90 spread)

```gherkin
Scenario: High-volatility slippage increases uncertainty
  Given an ExecutionMcEngine with 100 trials

  When I run with GaussianSlippage(5.0, 0.5)  # Low variance
  Then (sharpe_p90 - sharpe_p10) should be < 0.5  # Narrow band

  When I run with GaussianSlippage(5.0, 5.0)  # High variance
  Then (sharpe_p90 - sharpe_p10) should be > 1.0  # Wide band
```

#### Scenario 5.4: Parallel execution (performance)

```gherkin
Scenario: Run 100 trials in parallel
  Given an ExecutionMcEngine with 100 trials
  When I run MC on a strategy (single-threaded)
  Then execution time is T

  When I run MC with Rayon parallelization
  Then execution time should be < T/4 (on 8-core machine)
```

---

### Feature 6: L3 Promotion Filter (4 scenarios)

#### Scenario 6.1: Promote strong L2 candidate

```gherkin
Feature: L3 promotion filter (L2 → L3)

Scenario: Promote candidate with strong L2 metrics
  Given an L3PromotionFilter with:
    | min_oos_sharpe | max_oos_drawdown | min_stability |
    | 0.8            | -0.20            | 0.70          |
  And an L2 result with:
    | oos_sharpe | oos_max_drawdown | stability |
    | 1.15       | -0.16            | 0.75      |
  When I check should_promote
  Then the result should be (true, "L2 → L3: Sharpe 1.15, DD -16.0%, Stability 75%")
```

#### Scenario 6.2: Reject low OOS Sharpe

```gherkin
Scenario: Reject candidate with low OOS Sharpe
  Given an L3PromotionFilter with min_oos_sharpe=0.8
  And an L2 result with oos_sharpe=0.65
  When I check should_promote
  Then the result should be (false, "OOS Sharpe (0.65) < min (0.80)")
```

#### Scenario 6.3: Reject excessive drawdown

```gherkin
Scenario: Reject candidate with excessive drawdown
  Given an L3PromotionFilter with max_oos_drawdown=-0.20
  And an L2 result with oos_max_drawdown=-0.28
  When I check should_promote
  Then the result should be (false, "OOS DD (-28.0%) > max (-20.0%)")
```

#### Scenario 6.4: Batch promotion (10 → 3)

```gherkin
Scenario: Promote top 3 from 10 L2 survivors
  Given an L3PromotionFilter (default thresholds)
  And 10 L2 results with varying metrics
  When I filter with should_promote
  Then exactly 3 candidates should be promoted
  And 7 candidates should be rejected
  And promoted candidates should have highest OOS Sharpe
```

---

### Feature 7: MC Report (TUI Integration) (3 scenarios)

#### Scenario 7.1: Display MC percentiles table

```gherkin
Feature: MC results display (TUI)

Scenario: Display MC percentiles in table
  Given an McResult with:
    | Metric       | P10  | P50  | P90  |
    | Sharpe       | 0.85 | 1.20 | 1.55 |
    | Max Drawdown | -12% | -18% | -25% |
  When I render the MC report table
  Then it should display:
    """
    ╭─────────────┬──────┬──────┬──────╮
    │ Metric      │ P10  │ P50  │ P90  │
    ├─────────────┼──────┼──────┼──────┤
    │ Sharpe      │ 0.85 │ 1.20 │ 1.55 │
    │ Max DD      │ -12% │ -18% │ -25% │
    ╰─────────────┴──────┴──────┴──────╯
    """
```

#### Scenario 7.2: Equity curve fan chart

```gherkin
Scenario: Display equity curve fan chart (P10/P50/P90)
  Given 100 equity curves from MC trials
  When I render the fan chart
  Then it should display:
    - P90 curve (top, neon green)
    - P50 curve (middle, electric cyan)
    - P10 curve (bottom, hot pink)
    - Shaded region between P10 and P90 (uncertainty band)
```

#### Scenario 7.3: Compare L2 vs L3 metrics

```gherkin
Scenario: Compare deterministic L2 vs MC L3
  Given L2 result: Sharpe=1.20, DD=-18%
  And L3 result: P50 Sharpe=1.15, P50 DD=-19%
  When I render the comparison table
  Then it should display:
    """
    ╭──────────┬──────┬──────────╮
    │ Level    │ Sharpe│ Max DD  │
    ├──────────┼───────┼─────────┤
    │ L2 (Det) │ 1.20  │ -18%    │
    │ L3 (P50) │ 1.15  │ -19%    │
    │ L3 (P10) │ 0.85  │ -25%    │  ← Pessimistic
    │ L3 (P90) │ 1.55  │ -12%    │  ← Optimistic
    ╰──────────┴───────┴──────────╯
    """
```

---

### Feature 8: Cost Analysis (2 scenarios)

#### Scenario 8.1: L2 → L3 computational cost

```gherkin
Feature: L3 computational cost

Scenario: Measure cost of L3 vs L2
  Given 10 L2 survivors
  And each L2 backtest = 40 windows = 1 unit
  And each L3 backtest = 100 trials × 40 windows = 100 units

  When I run all 10 at L3 (no filter)
  Then total cost = 10 × 100 = 1000 units

  When I use L3PromotionFilter (promotes 3 / 10)
  Then total cost = 10 (L2 screening) + 3 × 100 (L3) = 310 units
  And cost savings = 69% (vs no filter)
```

#### Scenario 8.2: Full ladder cost (L1 → L2 → L3)

```gherkin
Scenario: Full promotion ladder cost
  Given 1000 initial candidates

  L1 (fixed 70/30, deterministic):
    Cost: 1000 × 1 = 1000 units
    Survivors: 8 (99.2% filtered)

  L2 (walk-forward 40 windows):
    Cost: 8 × 40 = 320 units
    Survivors: 3 (62.5% filtered)

  L3 (MC 100 trials):
    Cost: 3 × 100 = 300 units
    Survivors: 1 (best candidate)

  Total cost: 1000 + 320 + 300 = 1620 units

  Without ladder: 1000 × 100 = 100,000 units
  Savings: 98.4%
```

---

## Example Flows

### Flow 1: Single MC Trial (Deterministic Seed)

```rust
use rand::rngs::StdRng;
use rand::SeedableRng;

// Setup
let slippage = Arc::new(GaussianSlippage {
    mean_bps: 5.0,
    std_bps: 2.0,
});

let adverse = AdverseSelectionModel { skew_factor: 0.7 };
let queue = QueueDepthModel { fill_probability: 0.8 };

let trial = ExecutionMcTrial {
    trial_id: 0,
    slippage_dist: slippage,
    adverse_selection: adverse,
    queue_depth: queue,
    seed: 42,
};

// Run
let result = trial.run(&candidate, &data, &orders);

// Output
println!("Trial 0 (seed=42):");
println!("  Final Equity: ${:.2}", result.final_equity);
println!("  Sharpe Ratio: {:.2}", result.sharpe);
println!("  Max Drawdown: {:.1}%", result.max_drawdown * 100.0);
println!("  Num Trades: {}", result.num_trades);
```

**Output:**

```
Trial 0 (seed=42):
  Final Equity: $182,450
  Sharpe Ratio: 1.18
  Max Drawdown: -19.3%
  Num Trades: 87
```

---

### Flow 2: MC Engine (100 Trials)

```rust
let mc_engine = ExecutionMcEngine {
    num_trials: 100,
    slippage_dist: Arc::new(HistoricalSlippage {
        samples: vec![3.2, 4.1, 5.8, 7.2, 12.4, 15.3],  // Historical data
    }),
    adverse_selection: AdverseSelectionModel::moderate(),  // skew=0.5
    queue_depth: QueueDepthModel { fill_probability: 0.75 },
};

let mc_result = mc_engine.run(&candidate, &data, &orders);

println!("MC Results (100 trials):");
println!("  Sharpe Ratio:");
println!("    P10 (pessimistic): {:.2}", mc_result.sharpe_p10);
println!("    P50 (median):      {:.2}", mc_result.sharpe_p50);
println!("    P90 (optimistic):  {:.2}", mc_result.sharpe_p90);
println!();
println!("  Max Drawdown:");
println!("    P10 (best):   {:.1}%", mc_result.dd_p10 * 100.0);
println!("    P50 (median): {:.1}%", mc_result.dd_p50 * 100.0);
println!("    P90 (worst):  {:.1}%", mc_result.dd_p90 * 100.0);
```

**Output:**

```
MC Results (100 trials):
  Sharpe Ratio:
    P10 (pessimistic): 0.92
    P50 (median):      1.15
    P90 (optimistic):  1.38

  Max Drawdown:
    P10 (best):   -14.2%
    P50 (median): -18.7%
    P90 (worst):  -24.1%
```

---

### Flow 3: L2 → L3 Promotion

```rust
let l3_filter = L3PromotionFilter::default();

// L2 results (10 survivors)
let l2_results = vec![
    ("Donchian(20, 2.0x)", 1.15, -0.16, 0.75),
    ("Donchian(25, 2.5x)", 1.10, -0.18, 0.72),
    ("MA_Cross(50, 200)", 0.75, -0.15, 0.68),  // Low Sharpe
    ("RSI_Mean(14)", 0.95, -0.24, 0.70),       // Excessive DD
    // ...
];

let mut l3_candidates = Vec::new();

for (name, sharpe, dd, stability) in l2_results {
    let l2_result = WalkForwardResult {
        oos_sharpe: sharpe,
        oos_max_drawdown: dd,
        stability,
    };

    let (promote, reason) = l3_filter.should_promote(&l2_result);
    if promote {
        println!("✓ L2 → L3: {}: {}", name, reason);
        l3_candidates.push(name);
    } else {
        println!("✗ Rejected at L2: {}: {}", name, reason);
    }
}

println!("\nL2 → L3: {} / {} promoted ({:.1}% filtered)",
    l3_candidates.len(),
    l2_results.len(),
    100.0 * (1.0 - l3_candidates.len() as f64 / l2_results.len() as f64));

// Run L3 (MC 100 trials) on survivors
for name in l3_candidates {
    let mc_result = mc_engine.run(&get_candidate(name), &data, &orders);
    println!("\nL3 MC Result: {}", name);
    println!("  P10: {:.2}  P50: {:.2}  P90: {:.2}",
        mc_result.sharpe_p10,
        mc_result.sharpe_p50,
        mc_result.sharpe_p90);
}
```

**Output:**

```
✓ L2 → L3: Donchian(20, 2.0x): Sharpe 1.15, DD -16.0%, Stability 75%
✓ L2 → L3: Donchian(25, 2.5x): Sharpe 1.10, DD -18.0%, Stability 72%
✗ Rejected at L2: MA_Cross(50, 200): OOS Sharpe (0.75) < min (0.80)
✗ Rejected at L2: RSI_Mean(14): OOS DD (-24.0%) > max (-20.0%)
...

L2 → L3: 3 / 10 promoted (70.0% filtered)

L3 MC Result: Donchian(20, 2.0x)
  P10: 0.92  P50: 1.15  P90: 1.38

L3 MC Result: Donchian(25, 2.5x)
  P10: 0.85  P50: 1.10  P90: 1.35
```

---

### Flow 4: Regime-Conditional Slippage

```rust
let regime_slippage = RegimeConditionalSlippage {
    low_vol: Box::new(GaussianSlippage { mean_bps: 2.0, std_bps: 0.5 }),
    normal_vol: Box::new(GaussianSlippage { mean_bps: 5.0, std_bps: 1.5 }),
    high_vol: Box::new(GaussianSlippage { mean_bps: 12.0, std_bps: 4.0 }),
};

// During backtest:
for bar in data.bars() {
    let context = BarContext {
        date: bar.date,
        vix: bar.vix,  // From auxiliary data
    };

    let slippage_bps = regime_slippage.sample(&mut rng, &context);

    // Low vol day (VIX=10): slippage ≈ 2 bps
    // Normal day (VIX=18): slippage ≈ 5 bps
    // High vol day (VIX=40): slippage ≈ 12 bps
}
```

---

### Flow 5: Full Promotion Ladder (L1 → L2 → L3)

```
[1000 candidates]
     ↓
 L1: Fixed 70/30 (deterministic)
     Cost: 1000 backtests
     Time: 10 seconds
     ↓
 [8 survivors (99.2% filtered)]
     ↓
 L2: Walk-Forward 40 windows
     Cost: 8 × 40 = 320 backtests
     Time: 2 minutes
     ↓
 [3 survivors (62.5% filtered)]
     ↓
 L3: MC 100 trials
     Cost: 3 × 100 = 300 backtests
     Time: 5 minutes
     ↓
 [Best candidate selected]

Total cost: 1620 backtests (vs 100,000 without ladder)
Savings: 98.4%
Total time: ~7 minutes (vs 17 hours without ladder)
```

---

## TUI Integration

### MC Report Screen

```
╭─────────────────────────────────────────────────────────────╮
│ Execution Monte Carlo Results (L3)                         │
├─────────────────────────────────────────────────────────────┤
│ Candidate: Donchian(N=20, stop=2.0x ATR)                   │
│ Trials: 100                                                 │
│ Slippage: Historical (mean 6.2 bps)                        │
│ Adverse Selection: Moderate (skew=0.5)                     │
│ Queue Depth: 75% fill probability                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│ Performance Metrics (Percentiles):                         │
│ ╭────────────────┬──────┬──────┬──────╮                    │
│ │ Metric         │ P10  │ P50  │ P90  │                    │
│ ├────────────────┼──────┼──────┼──────┤                    │
│ │ Sharpe Ratio   │ 0.92 │ 1.15 │ 1.38 │                    │
│ │ Max Drawdown   │ -14% │ -19% │ -24% │                    │
│ │ CAGR           │ 12%  │ 15%  │ 18%  │                    │
│ │ Win Rate       │ 48%  │ 52%  │ 56%  │                    │
│ │ Num Trades     │ 82   │ 87   │ 93   │                    │
│ ╰────────────────┴──────┴──────┴──────╯                    │
│                                                             │
│ Equity Curve (Fan Chart):                                  │
│   220K ┤        ╭────────────────────────── P90 (optimistic)
│   200K ┤      ╭─┼───────────────────────
│   180K ┤    ╭───┼──────────────────────── P50 (median)
│   160K ┤  ╭─────┼──────────
│   140K ┤╭───────┼──────────────────────── P10 (pessimistic)
│   120K ┼────────┴────────────────────────
│   100K ┴────────────────────────────────
│        2010   2012   2014   2016   2018   2020            │
│                                                             │
│ Interpretation:                                            │
│ • P50 (median) is your "expected" outcome                  │
│ • P10-P90 band shows uncertainty (wider = more variable)   │
│ • If P10 is still acceptable, strategy is robust           │
│                                                             │
│ [Space] Continue  [R] Re-run with different params  [Q] Quit│
╰─────────────────────────────────────────────────────────────╯
```

### L2 vs L3 Comparison Table

```
╭─────────────────────────────────────────────────────────────╮
│ L2 (Walk-Forward) vs L3 (Monte Carlo) Comparison           │
├─────────────────────────────────────────────────────────────┤
│ ╭──────────┬────────┬──────────┬─────────┬──────────╮      │
│ │ Level    │ Sharpe │ Max DD   │ CAGR    │ Trades   │      │
│ ├──────────┼────────┼──────────┼─────────┼──────────┤      │
│ │ L2 (Det) │ 1.20   │ -18%     │ 16%     │ 92       │      │
│ │ L3 (P50) │ 1.15   │ -19%     │ 15%     │ 87       │      │
│ │ L3 (P10) │ 0.92   │ -24%     │ 12%     │ 82       │ ← Worst
│ │ L3 (P90) │ 1.38   │ -14%     │ 18%     │ 93       │ ← Best
│ ╰──────────┴────────┴──────────┴─────────┴──────────╯      │
│                                                             │
│ Degradation (L2 → L3):                                     │
│   Sharpe:  -4.2% (1.20 → 1.15)  ✓ Acceptable             │
│   Max DD:  +5.6% (-18% → -19%)  ✓ Acceptable             │
│                                                             │
│ Conclusion:                                                │
│   L3 results consistent with L2 (low degradation).        │
│   Strategy is robust to execution uncertainty.            │
│                                                             │
│   ✓ PROMOTE TO L4 (Path Monte Carlo)                      │
╰─────────────────────────────────────────────────────────────╯
```

---

## Completion Criteria (22 items)

### Architecture & Core Traits (5 items)

- [ ] **SlippageDistribution trait** defined (sample, expected_bps)
- [ ] **AdverseSelectionModel struct** defined (skew_factor, adjust_limit_fill)
- [ ] **QueueDepthModel struct** defined (fill_probability, sample_fill_fraction)
- [ ] **ExecutionMcTrial struct** defined (single trial with seed)
- [ ] **ExecutionMcEngine struct** defined (run N trials, aggregate)

### Slippage Distributions (5 items)

- [ ] **FixedSlippage** implemented (deterministic, L1)
- [ ] **HistoricalSlippage** implemented (bootstrap sampling)
- [ ] **GaussianSlippage** implemented (Normal distribution)
- [ ] **UniformSlippage** implemented (conservative, equal weight)
- [ ] **RegimeConditionalSlippage** implemented (VIX-based regimes)

### Execution Models (4 items)

- [ ] **Adverse selection for limit buys** (fill biased toward high)
- [ ] **Adverse selection for limit sells** (fill biased toward low)
- [ ] **Queue depth sampling** (binary fill/no-fill)
- [ ] **Volume-based partial fills** (large orders get partial fills)

### MC Trial Engine (4 items)

- [ ] **Single trial is deterministic** (given seed)
- [ ] **Different seeds produce different outcomes**
- [ ] **Parallel trial execution** (Rayon, 4-8× speedup)
- [ ] **Aggregate results → percentiles** (P10/P50/P90)

### L3 Promotion Filter (2 items)

- [ ] **L3PromotionFilter** checks (min Sharpe, max DD, min stability)
- [ ] **Batch filtering** (L2 results → L3 candidates)

### TUI & Reporting (2 items)

- [ ] **MC percentiles table** (P10/P50/P90 for Sharpe/DD/CAGR)
- [ ] **Equity curve fan chart** (P10/P50/P90 curves, shaded band)

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
M8   [..............ᗧ] 100% (complete)
M9   [..............ᗧ] 100% (complete) ← JUST COMPLETED
M10  [ᗧ··············] 0%   (not started)
M11  [ᗧ··············] 0%   (not started)
M12  [ᗧ··············] 0%   (not started)

Meta-plan status: 11/12 milestones enhanced (92%)
```

---

## Why M9 Matters

Traditional backtesting uses **fixed slippage** (e.g., 5 bps). This is unrealistic:

❌ **Real slippage varies** (2 bps on calm days, 20 bps on volatile days)
❌ **Limit orders have adverse selection** (fill when price moves against you)
❌ **Not all limits execute** (queue depth, partial fills)
❌ **Fixed slippage = optimistic bias** (overstates real performance)

### M9's Solution:

✅ **Sample from distributions** (Historical, Gaussian, Regime-conditional)
✅ **Adverse selection** (limits biased toward unfavorable prices)
✅ **Queue depth model** (some limits don't fill)
✅ **Monte Carlo trials** (100 paths → distribution of outcomes)
✅ **Uncertainty bands** (P10/P50/P90 show range of outcomes)

### Key Insight:

Instead of one optimistic Sharpe (1.20), you get a **distribution**:

- **P10 (pessimistic):** 0.92 (10th percentile — bad luck)
- **P50 (median):** 1.15 (expected outcome)
- **P90 (optimistic):** 1.38 (90th percentile — good luck)

**Decision rule:** If **P10 is still acceptable**, the strategy is robust!

---

## Cost vs Realism Trade-Off

| Level | Method                  | Cost (per candidate) | Realism          |
|-------|-------------------------|----------------------|------------------|
| L1    | Fixed slippage          | 1× (cheap)           | Low (optimistic) |
| L2    | Walk-forward 40 windows | 40× (moderate)       | Medium           |
| **L3**| **MC 100 trials**       | **100×** (expensive) | **High**         |
| L4    | Path MC + L3            | 500× (very expensive)| Very high        |

**M9 makes L3 affordable** via promotion ladder:

- **1000 candidates** → L1 → **8 survivors**
- **8 survivors** → L2 → **3 survivors**
- **3 survivors** → L3 (MC 100 trials)

**Total cost:** 1000 (L1) + 320 (L2) + 300 (L3) = **1620 backtests**
**Without ladder:** 1000 × 100 = **100,000 backtests**
**Savings:** **98.4%** 🎉

---

## Next Steps

**M10 (Path Monte Carlo)** is next. This milestone covers:

- **Intrabar ambiguity resolution** (when both SL and TP could trigger)
- **Path sampling** (sample micro-paths consistent with OHLC)
- **Adversarial path policy** (worst-case ordering for stress testing)
- **Path MC trials** (combine with execution MC from M9)
- **L4 promotion filter** (L3 survivors → earn path MC)

**Estimated LOC:** ~600 lines
**Complexity:** High (path generation + OHLC consistency constraints)

---

## Options

1. **Continue immediately** with **M10 (Path Monte Carlo)**
2. **Pause for review** (now 11/12 milestones = 92% complete)
3. **Skip to M11 (Bootstrap)** or **M12 (Profiling/Optimization)**
4. **Adjust approach** (different format, focus areas, etc.)

What would you like to do next?
