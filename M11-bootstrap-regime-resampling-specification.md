# M11: Bootstrap & Regime Resampling — Specification

**Milestone:** M11 (Level 5 Promotion — Final Robustness Layer)
**Status:** Pre-implementation (BDD spec)
**Depends on:** M10 (Path Monte Carlo), M9 (Execution MC), M8 (Walk-Forward)
**Enables:** M12 (Benchmarks & UI Polish)

---

## Problem Statement

**Even after L4 (Path MC), strategies can fail in production due to:**

1. **Data snooping bias** — Testing on the same historical sample that guided parameter selection
2. **Regime fragility** — Strategy works in one market regime (e.g., trending) but fails in others (sideways, volatile)
3. **Sequential bias** — Returns are autocorrelated; simple shuffling destroys temporal structure
4. **Lucky streaks** — A strategy might have benefited from a unique historical sequence that won't repeat

**Traditional solutions (and their flaws):**

| Method | Problem |
|--------|---------|
| Simple shuffle (i.i.d. bootstrap) | ❌ Destroys autocorrelation (trends, momentum) |
| Train/test split (M8) | ❌ Only tests one OOS period; vulnerable to regime luck |
| Path MC (M10) | ❌ Resamples intrabar paths, not historical sequence |
| "Just use more data" | ❌ Regime distribution in historical data may not match future |

**M11's solution: Block Bootstrap + Regime Resampling**

1. **Block Bootstrap** — Resample historical data in blocks (e.g., 63-day quarters) to preserve autocorrelation
2. **Regime Resampling** — Oversample underrepresented regimes (e.g., 2× bear market blocks) to test robustness
3. **Cross-Regime Validation** — Require P10 Sharpe > threshold in *all* regimes (not just aggregate)
4. **L5 Promotion Filter** — Only promote L4 survivors with stable bootstrap distributions

**Key insight:**
If a strategy survives 100 bootstrap trials with regime oversampling → it's robust to different plausible histories, not just the one that happened to occur.

---

## Architecture Overview

### Module Structure (7 new modules, ~850 lines)

```
trendlab-core/src/robustness/
├── bootstrap/
│   ├── mod.rs                       # BootstrapPolicy trait + re-exports
│   ├── block_bootstrap.rs           # Block bootstrap resampler
│   ├── regime_detector.rs           # Detect market regimes (HMM/vol/trend)
│   ├── regime_resampler.rs          # Oversample specific regimes
│   ├── bootstrap_trial.rs           # Single bootstrap trial (resample → backtest)
│   ├── bootstrap_engine.rs          # Run N trials + aggregate
│   └── promotion_l5.rs              # L4 → L5 filter
```

### Integration Points

**Inputs:**
- **From M10:** L4 survivors (Path MC percentiles)
- **From M9:** Execution MC trials (reused in each bootstrap)
- **From M8:** Walk-forward splitter (for regime detection)
- **From M4:** Portfolio equity curves (for regime labeling)

**Outputs:**
- **To M12:** Final leaderboard (top 1-3 strategies)
- **To TUI:** Bootstrap distribution charts, regime breakdown tables

---

## Core Concepts

### 1. Block Bootstrap (Stationary Bootstrap)

**Problem:** Simple bootstrap (random sampling with replacement) assumes i.i.d. data.
Reality: Financial returns are autocorrelated (momentum, mean reversion, volatility clustering).

**Solution:** Resample in blocks to preserve local structure.

**Algorithm: Stationary Block Bootstrap (Politis & Romano, 1994)**

```rust
/// Generate one bootstrap sample by resampling blocks
pub fn block_bootstrap(
    data: &TimeSeriesData,
    block_size: usize,           // e.g., 63 days (1 quarter)
    rng: &mut impl Rng,
) -> TimeSeriesData {
    let n = data.len();
    let mut resampled = Vec::with_capacity(n);

    while resampled.len() < n {
        // Pick random start point
        let start = rng.gen_range(0..n);

        // Copy block (wrapping at end)
        let block_len = block_size.min(n - resampled.len());
        for i in 0..block_len {
            let idx = (start + i) % n;
            resampled.push(data[idx].clone());
        }
    }

    TimeSeriesData::new(resampled)
}
```

**Parameters:**
- **`block_size`:** Controls bias-variance tradeoff
  - Small blocks (e.g., 5 days): Low bias, high variance
  - Large blocks (e.g., 252 days): High bias (closer to original), low variance
  - **Optimal:** ~63 days (1 quarter) for daily data (Politis & White, 2004)

**Properties:**
- ✅ Preserves autocorrelation within blocks
- ✅ Breaks long-term dependence between distant periods
- ✅ Asymptotically valid (as block size → ∞)

**Example:**

```
Original data (20 days):
  [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T]

Block bootstrap (block_size = 5):
  Sample 1: [F, G, H, I, J] + [B, C, D, E, F] + [M, N, O, P, Q] + [R, S, T, A, B]
  Sample 2: [K, L, M, N, O] + [D, E, F, G, H] + [P, Q, R, S, T] + [A, B, C, D, E]
```

Each bootstrap sample:
- Same length as original (20 days)
- Resampled in blocks of 5
- Preserves short-term structure (within blocks)

---

### 2. Regime Detection

**Goal:** Label each historical period with a market regime (Trending Up, Trending Down, Sideways, Volatile).

**Why it matters:**
A strategy might have high Sharpe because it got lucky with regime distribution in historical data.
Example: Trend-following strategy tested 2010-2020 (mostly bull) → overstates real performance.

**Regime Definitions:**

| Regime | Condition | Example Period |
|--------|-----------|----------------|
| **Trending Up** | Price > 200 SMA, low volatility | 2013-2017 (bull market) |
| **Trending Down** | Price < 200 SMA, low volatility | 2008 (bear market) |
| **Sideways** | Price ≈ 200 SMA, low volatility | 2015-2016 (range-bound) |
| **Volatile** | High volatility (regardless of trend) | 2020 (COVID crash) |

**Detection Algorithm (HMM + Heuristics):**

```rust
pub enum Regime {
    TrendingUp,
    TrendingDown,
    Sideways,
    Volatile,
}

pub struct RegimeDetector {
    pub sma_window: usize,        // e.g., 200
    pub vol_window: usize,        // e.g., 20
    pub vol_threshold: f64,       // e.g., 2.0 (2x median vol)
}

impl RegimeDetector {
    pub fn detect(&self, data: &TimeSeriesData) -> Vec<(Date, Regime)> {
        let sma = data.sma(self.sma_window);
        let vol = data.rolling_std(self.vol_window);
        let median_vol = vol.median();

        data.iter().map(|(date, bar)| {
            let regime = if vol[date] > median_vol * self.vol_threshold {
                Regime::Volatile
            } else if bar.close > sma[date] * 1.02 {
                Regime::TrendingUp
            } else if bar.close < sma[date] * 0.98 {
                Regime::TrendingDown
            } else {
                Regime::Sideways
            };
            (*date, regime)
        }).collect()
    }
}
```

**Alternative: Hidden Markov Model (HMM)**
For more sophisticated regime detection, use HMM with 4 states (Trending Up/Down, Sideways, Volatile).

---

### 3. Regime Resampling

**Problem:** Historical data may have unbalanced regime distribution.

Example (SPY 2000-2020):
- Trending Up: 60% of days
- Trending Down: 15%
- Sideways: 20%
- Volatile: 5%

If we only test on historical data → trend-following strategies look too good!

**Solution: Oversample underrepresented regimes**

```rust
pub struct RegimeResampler {
    pub regime_weights: HashMap<Regime, f64>,  // e.g., {Volatile: 2.0, TrendingDown: 1.5}
}

impl RegimeResampler {
    pub fn resample(
        &self,
        blocks: &[Block],       // Blocks labeled by regime
        target_len: usize,      // e.g., original data length
        rng: &mut impl Rng,
    ) -> Vec<Block> {
        // Build weighted sampling distribution
        let weights: Vec<f64> = blocks.iter()
            .map(|b| self.regime_weights.get(&b.regime).unwrap_or(&1.0))
            .copied()
            .collect();

        // Sample blocks with replacement (weighted by regime)
        let mut resampled = Vec::new();
        let dist = WeightedIndex::new(&weights).unwrap();

        while resampled.total_len() < target_len {
            let idx = dist.sample(rng);
            resampled.push(blocks[idx].clone());
        }

        resampled
    }
}
```

**Example:**

```
Original regime distribution:
  TrendingUp:   60%
  Sideways:     20%
  TrendingDown: 15%
  Volatile:      5%

Regime weights: {Volatile: 4.0, TrendingDown: 2.0}

Bootstrap regime distribution (100 trials, averaged):
  TrendingUp:   45%  (↓ from 60%)
  Sideways:     18%  (↓ from 20%)
  TrendingDown: 22%  (↑ from 15%)
  Volatile:     15%  (↑ from 5%)
```

Now strategy must prove it works in more diverse regime mix!

---

### 4. Bootstrap Trial (Single Run)

```rust
pub struct BootstrapTrial {
    pub trial_id: usize,
    pub block_size: usize,              // e.g., 63
    pub regime_resampler: Option<RegimeResampler>,
    pub path_policy: Arc<dyn PathPolicy>,        // From M10
    pub slippage_dist: Arc<dyn SlippageDistribution>,  // From M9
    pub adverse_selection: AdverseSelectionModel,      // From M9
    pub seed: u64,
}

impl BootstrapTrial {
    pub fn run(&self, candidate: &Candidate, data: &TimeSeriesData) -> TrialResult {
        let mut rng = StdRng::seed_from_u64(self.seed);

        // 1. Resample data (block bootstrap + regime weighting)
        let resampled_data = if let Some(resampler) = &self.regime_resampler {
            let blocks = detect_blocks(data, self.block_size, &self.regime_detector);
            resampler.resample(&blocks, data.len(), &mut rng)
        } else {
            block_bootstrap(data, self.block_size, &mut rng)
        };

        // 2. Run backtest on resampled data (with L4 path MC + L3 execution MC)
        let engine = BacktestEngine::new(
            self.path_policy.clone(),
            self.slippage_dist.clone(),
            self.adverse_selection.clone(),
        );

        let result = engine.run(candidate, &resampled_data);

        TrialResult {
            trial_id: self.trial_id,
            sharpe: result.sharpe_ratio,
            max_dd: result.max_drawdown,
            equity_curve: result.equity_curve,
        }
    }
}
```

**Key differences from M9 (Execution MC) and M10 (Path MC):**

| Layer | What's Resampled | Fixed |
|-------|------------------|-------|
| **M9 (Exec MC)** | Slippage, adverse selection | Data sequence, path |
| **M10 (Path MC)** | Intrabar path | Data sequence, execution |
| **M11 (Bootstrap)** | **Data sequence (blocks)** | Execution + path (reused) |

**Nesting:**
Each bootstrap trial runs 1 backtest on resampled data.
That backtest uses L4 path MC (50 paths) + L3 execution MC (100 trials).
So: **100 bootstrap × 50 paths × 100 exec = 500,000 total simulations per candidate!**

---

### 5. Bootstrap Engine (Aggregate N Trials)

```rust
pub struct BootstrapEngine {
    pub num_trials: usize,              // e.g., 100
    pub block_size: usize,              // e.g., 63
    pub regime_resampler: Option<RegimeResampler>,
    pub path_policy: Arc<dyn PathPolicy>,
    pub slippage_dist: Arc<dyn SlippageDistribution>,
    pub adverse_selection: AdverseSelectionModel,
}

impl BootstrapEngine {
    pub fn run(&self, candidate: &Candidate, data: &TimeSeriesData) -> BootstrapResult {
        // Run N trials in parallel (Rayon)
        let trials: Vec<BootstrapTrial> = (0..self.num_trials)
            .map(|i| BootstrapTrial {
                trial_id: i,
                block_size: self.block_size,
                regime_resampler: self.regime_resampler.clone(),
                path_policy: self.path_policy.clone(),
                slippage_dist: self.slippage_dist.clone(),
                adverse_selection: self.adverse_selection.clone(),
                seed: i as u64,  // Deterministic seeds
            })
            .collect();

        let results: Vec<TrialResult> = trials.par_iter()
            .map(|trial| trial.run(candidate, data))
            .collect();

        // Aggregate percentiles
        let sharpes: Vec<f64> = results.iter().map(|r| r.sharpe).collect();
        let dds: Vec<f64> = results.iter().map(|r| r.max_dd).collect();

        BootstrapResult {
            sharpe_p10: percentile(&sharpes, 10),
            sharpe_p50: percentile(&sharpes, 50),
            sharpe_p90: percentile(&sharpes, 90),
            dd_p10: percentile(&dds, 10),
            dd_p50: percentile(&dds, 50),
            dd_p90: percentile(&dds, 90),
            stability_pct: compute_stability(&sharpes, 0.8),  // % of trials with Sharpe > 0.8
            trials: results,
        }
    }
}
```

**Output Example:**

```
Bootstrap Results (100 trials):
  Sharpe Ratio:
    P10 (pessimistic): 0.75
    P50 (median):      1.05
    P90 (optimistic):  1.35

  Max Drawdown:
    P10 (best):   -16%
    P50 (median): -22%
    P90 (worst):  -28%

  Stability: 82% of trials had Sharpe > 0.8
```

---

### 6. Cross-Regime Validation

**Problem:** Aggregate bootstrap P10 might hide regime-specific failures.

Example:
- Trending Up: Sharpe = 1.5 (great!)
- Sideways: Sharpe = 0.3 (terrible!)
- Aggregate P10: 0.85 (passes threshold)

Strategy looks OK on average, but fails in sideways markets → risky!

**Solution: Require minimum performance in EACH regime**

```rust
pub struct CrossRegimeValidator {
    pub min_sharpe_per_regime: HashMap<Regime, f64>,
    // e.g., {TrendingUp: 0.8, Sideways: 0.6, TrendingDown: 0.7, Volatile: 0.5}
}

impl CrossRegimeValidator {
    pub fn validate(&self, trials: &[TrialResult]) -> Result<(), ValidationError> {
        // Group trials by regime
        let by_regime = group_by_regime(trials);

        for (regime, regime_trials) in by_regime {
            let sharpes: Vec<f64> = regime_trials.iter().map(|t| t.sharpe).collect();
            let p50 = percentile(&sharpes, 50);

            let min_required = self.min_sharpe_per_regime.get(&regime)
                .ok_or(ValidationError::UnknownRegime(regime))?;

            if p50 < *min_required {
                return Err(ValidationError::RegimeFailure {
                    regime,
                    actual: p50,
                    required: *min_required,
                });
            }
        }

        Ok(())
    }
}
```

**Example:**

```
Cross-Regime Validation:
  ✓ TrendingUp:   P50 Sharpe = 1.20 (> 0.80 required)
  ✓ TrendingDown: P50 Sharpe = 0.85 (> 0.70 required)
  ✓ Sideways:     P50 Sharpe = 0.65 (> 0.60 required)
  ✗ Volatile:     P50 Sharpe = 0.42 (< 0.50 required) ❌

Result: REJECTED (fails in Volatile regime)
```

---

### 7. L5 Promotion Filter (L4 → L5)

**Problem:** Bootstrap (100 trials × 50 paths × 100 exec) is **500× more expensive** than L4!

**Solution:** Only bootstrap the *very best* L4 survivors.

```rust
pub struct L5PromotionFilter {
    pub min_l4_sharpe_p10: f64,         // e.g., 0.90
    pub max_l4_dd_p90: f64,             // e.g., -20%
    pub min_l4_stability: f64,          // e.g., 80%
    pub min_cross_regime_sharpe: f64,   // e.g., 0.60 (weakest regime)
}

impl L5PromotionFilter {
    pub fn should_promote(&self, l4_result: &PathMcResult) -> bool {
        l4_result.sharpe_p10 >= self.min_l4_sharpe_p10
            && l4_result.dd_p90 <= self.max_l4_dd_p90
            && l4_result.stability_pct >= self.min_l4_stability
            && l4_result.min_regime_sharpe >= self.min_cross_regime_sharpe
    }
}
```

**Example Flow:**

```
L4 Results (2 candidates from Path MC):
  ✓ L4 → L5: Donchian(20): P10=0.95, DD=-18%, stability=85%, min_regime=0.65
  ✗ Rejected at L4: Donchian(25): P10=0.88 < 0.90

L4 → L5: 1 / 2 promoted (50% filtered)

L5 Results (1 × 500,000 = 500k trials):
  Donchian(20): P10=0.72  P50=1.02  P90=1.32
```

---

## Full Promotion Ladder (L1 → L5)

```
[1000 candidates]
     ↓
 L1: Fixed 70/30 + WorstCase path (deterministic)
     Cost: 1000 backtests
     Time: 10 seconds
     ↓
 [8 survivors (99.2% filtered)]
     ↓
 L2: Walk-Forward 40 windows + WorstCase path
     Cost: 8 × 40 = 320 backtests
     Time: 2 minutes
     ↓
 [3 survivors (62.5% filtered)]
     ↓
 L3: Execution MC (100 trials) + WorstCase path
     Cost: 3 × 100 = 300 backtests
     Time: 5 minutes
     ↓
 [2 survivors (33% filtered)]
     ↓
 L4: Path MC (50 paths × 100 exec = 5000 total)
     Cost: 2 × 5000 = 10,000 backtests
     Time: 30 minutes
     ↓
 [1 survivor (50% filtered)]
     ↓
 L5: Bootstrap (100 trials × 50 paths × 100 exec = 500k total)
     Cost: 1 × 500,000 = 500,000 backtests
     Time: 4 hours
     ↓
 [FINAL LEADERBOARD: Top 1-3 strategies]

Total cost: ~511,620 backtests
Without ladder: 1000 × 500,000 = 500,000,000 backtests
Savings: 99.9% 🎉

Total time: ~6.6 hours (vs 5,787 hours = 241 days without ladder!)
```

**Key observation:**
Even with 500k simulations per candidate in L5, the ladder makes it affordable!

---

## TUI Integration

### 1. Bootstrap Distribution Chart

**ASCII Histogram (Sharpe Ratio):**

```
Bootstrap Sharpe Distribution (100 trials):

  0.6 - 0.7  |███                        | 6%
  0.7 - 0.8  |████████                   | 16%
  0.8 - 0.9  |█████████████              | 26%
  0.9 - 1.0  |████████████████           | 32%  ← P50
  1.0 - 1.1  |██████████                 | 14%
  1.1 - 1.2  |████                       | 4%
  1.2 - 1.3  |██                         | 2%

  P10: 0.75    P50: 1.05    P90: 1.35
  Stability: 82% (trials with Sharpe > 0.8)
```

### 2. Regime Breakdown Table

```
Cross-Regime Performance:

  Regime         | Trials | P50 Sharpe | P50 DD   | Status
  ---------------|--------|------------|----------|--------
  Trending Up    | 42     | 1.20       | -18%     | ✓ PASS
  Trending Down  | 18     | 0.85       | -24%     | ✓ PASS
  Sideways       | 28     | 0.65       | -20%     | ✓ PASS
  Volatile       | 12     | 0.52       | -28%     | ✓ PASS

  Overall: ✓ PASS (all regimes above minimum thresholds)
```

### 3. Bootstrap vs L4 Comparison

```
L4 Path MC vs L5 Bootstrap:

  Metric         | L4 (Path MC) | L5 (Bootstrap) | Change
  ---------------|--------------|----------------|--------
  Sharpe P10     | 0.95         | 0.75           | -21%
  Sharpe P50     | 1.10         | 1.05           | -5%
  Sharpe P90     | 1.35         | 1.35           | 0%
  Max DD P50     | -19%         | -22%           | -3pp
  Stability      | 85%          | 82%            | -3pp

  Interpretation: Strategy is robust to data resampling
  (P10 drop < 25% → low data snooping risk)
```

---

## BDD Scenarios

### Feature 1: Block Bootstrap Resampling

**Scenario 1.1: Resample with fixed block size**
```gherkin
Given historical data with 252 trading days
When I run block_bootstrap with block_size=63
Then the resampled data should have 252 days
And some blocks should be duplicated
And some blocks should be omitted
And autocorrelation(resampled) ≈ autocorrelation(original) ± 20%
```

**Scenario 1.2: Block size tradeoff (small blocks)**
```gherkin
Given historical data with strong autocorrelation (ρ=0.8)
When I run block_bootstrap with block_size=5 (100 trials)
Then autocorrelation(resampled) should be 0.4-0.6 (underestimates)
And variance(bootstrap_sharpe) should be high (> 0.15)
```

**Scenario 1.3: Block size tradeoff (large blocks)**
```gherkin
Given historical data
When I run block_bootstrap with block_size=252 (entire year)
Then resampled data should be very similar to original
And variance(bootstrap_sharpe) should be low (< 0.05)
And bootstrap P10/P90 spread should be narrow (< 20%)
```

**Scenario 1.4: Deterministic resampling (same seed → same result)**
```gherkin
Given seed=42
When I run block_bootstrap twice with seed=42
Then both resampled datasets should be identical
```

---

### Feature 2: Regime Detection

**Scenario 2.1: Detect trending up regime**
```gherkin
Given SPY data 2013-2017 (bull market)
When I run RegimeDetector
Then 70-80% of days should be labeled TrendingUp
And volatility should be below median
```

**Scenario 2.2: Detect trending down regime**
```gherkin
Given SPY data 2008 (bear market)
When I run RegimeDetector
Then 60-70% of days should be labeled TrendingDown
And price < 200 SMA most days
```

**Scenario 2.3: Detect sideways regime**
```gherkin
Given SPY data 2015-2016 (range-bound)
When I run RegimeDetector
Then 50-60% of days should be labeled Sideways
And price should oscillate around 200 SMA
```

**Scenario 2.4: Detect volatile regime**
```gherkin
Given SPY data March 2020 (COVID crash)
When I run RegimeDetector
Then 70-80% of days should be labeled Volatile
And volatility should be > 2× median
```

---

### Feature 3: Regime Resampling

**Scenario 3.1: Oversample underrepresented regime**
```gherkin
Given historical data with 5% Volatile days
And regime_weights = {Volatile: 4.0}
When I run RegimeResampler (100 trials)
Then average Volatile % across trials should be 15-20%
And TrendingUp % should decrease proportionally
```

**Scenario 3.2: Balanced regime resampling**
```gherkin
Given regime_weights = {TrendingUp: 1.0, TrendingDown: 1.0, Sideways: 1.0, Volatile: 1.0}
When I run RegimeResampler (100 trials)
Then all regimes should appear ≈25% ± 5%
```

**Scenario 3.3: No regime weighting (pure block bootstrap)**
```gherkin
Given no regime_resampler
When I run block_bootstrap
Then regime distribution should match original ± 10%
```

---

### Feature 4: Bootstrap Trial (Single Run)

**Scenario 4.1: Single bootstrap trial with L4 integration**
```gherkin
Given a BootstrapTrial with seed=42
When I run the trial on Donchian(20)
Then it should:
  1. Resample data (block bootstrap)
  2. Run backtest with L4 Path MC (50 paths)
  3. Return TrialResult with Sharpe, DD, equity curve
And the trial should be deterministic (same seed → same result)
```

**Scenario 4.2: Bootstrap with regime oversampling**
```gherkin
Given regime_weights = {Volatile: 3.0}
When I run BootstrapTrial
Then the resampled data should have 3× more Volatile blocks
And the Sharpe should be lower than original (Volatile is harder)
```

**Scenario 4.3: Compare bootstrap trial vs original backtest**
```gherkin
Given a strategy with Sharpe=1.10 on original data
When I run 100 bootstrap trials
Then P50 should be ≈1.10 ± 10%
And P10 < 1.10 (some trials will be worse)
And P90 > 1.10 (some trials will be better)
```

---

### Feature 5: Bootstrap Engine (Aggregate N Trials)

**Scenario 5.1: Run 100 bootstrap trials in parallel**
```gherkin
Given BootstrapEngine with num_trials=100
When I run the engine on Donchian(20)
Then it should:
  1. Run 100 trials in parallel (Rayon)
  2. Return P10/P50/P90 percentiles
  3. Return stability % (trials with Sharpe > threshold)
And total runtime should be < 5 hours (for 500k simulations)
```

**Scenario 5.2: Bootstrap stability metric**
```gherkin
Given a robust strategy (P10=0.90, P50=1.10, P90=1.30)
When I compute stability (threshold=0.8)
Then stability should be 85-95% (most trials above 0.8)
```

**Scenario 5.3: Bootstrap instability (fragile strategy)**
```gherkin
Given a fragile strategy (P10=0.20, P50=1.10, P90=2.00)
When I compute stability (threshold=0.8)
Then stability should be 40-60% (many trials below 0.8)
And the strategy should be REJECTED at L5
```

**Scenario 5.4: Bootstrap percentiles (wide spread)**
```gherkin
Given a data-sensitive strategy
When I run 100 bootstrap trials
Then P90 - P10 should be > 0.5 (wide uncertainty)
And the strategy should be flagged as "high bootstrap variance"
```

---

### Feature 6: Cross-Regime Validation

**Scenario 6.1: Pass all regimes**
```gherkin
Given min_sharpe_per_regime = {TrendingUp: 0.8, Sideways: 0.6, TrendingDown: 0.7, Volatile: 0.5}
And a strategy with:
  - TrendingUp: P50=1.20
  - Sideways: P50=0.65
  - TrendingDown: P50=0.85
  - Volatile: P50=0.52
When I run CrossRegimeValidator
Then result should be PASS
```

**Scenario 6.2: Fail in one regime (Sideways)**
```gherkin
Given min_sharpe_per_regime = {Sideways: 0.6}
And a strategy with Sideways P50=0.30
When I run CrossRegimeValidator
Then result should be FAIL
And error should be ValidationError::RegimeFailure(Sideways, 0.30, 0.6)
```

**Scenario 6.3: Regime-specific thresholds**
```gherkin
Given different thresholds per regime:
  - TrendingUp: 0.8 (high bar, easy regime)
  - Volatile: 0.5 (low bar, hard regime)
When I validate a trend-following strategy
Then it should pass TrendingUp if Sharpe > 0.8
And pass Volatile if Sharpe > 0.5 (lower bar)
```

---

### Feature 7: L5 Promotion Filter (L4 → L5)

**Scenario 7.1: Promote strong L4 survivor**
```gherkin
Given L5PromotionFilter with thresholds:
  - min_l4_sharpe_p10: 0.90
  - max_l4_dd_p90: -20%
  - min_l4_stability: 80%
And a L4 result with P10=0.95, DD=-18%, stability=85%
When I check should_promote()
Then result should be true
```

**Scenario 7.2: Reject weak L4 result (low P10)**
```gherkin
Given min_l4_sharpe_p10: 0.90
And a L4 result with P10=0.85
When I check should_promote()
Then result should be false (P10 too low)
```

**Scenario 7.3: Reject fragile L4 result (low stability)**
```gherkin
Given min_l4_stability: 80%
And a L4 result with stability=72%
When I check should_promote()
Then result should be false (too unstable)
```

**Scenario 7.4: L4 → L5 promotion rate**
```gherkin
Given 2 L4 survivors
When I apply L5PromotionFilter
Then 0-1 candidates should be promoted (50-100% filtered)
And only the absolute best should reach L5
```

---

### Feature 8: Bootstrap Cost Analysis

**Scenario 8.1: Compare ladder cost vs brute force**
```gherkin
Given 1000 initial candidates
And L5 cost = 500,000 simulations per candidate
When I use the promotion ladder (L1 → L2 → L3 → L4 → L5)
Then total cost should be ≈511,620 backtests
And brute force cost would be 500,000,000 backtests
And savings should be 99.9%
```

**Scenario 8.2: L5 runtime (single candidate)**
```gherkin
Given BootstrapEngine with 100 trials × 50 paths × 100 exec
When I run L5 on one candidate
Then total simulations should be 500,000
And runtime should be 3-5 hours (parallelized)
```

**Scenario 8.3: Full ladder runtime**
```gherkin
Given 1000 candidates → 8 → 3 → 2 → 1
When I run the full ladder (L1 through L5)
Then total runtime should be ≈6-7 hours
And final leaderboard should have 1-3 strategies
```

---

### Feature 9: Bootstrap Report (TUI)

**Scenario 9.1: Display bootstrap Sharpe distribution**
```gherkin
Given 100 bootstrap trials
When I render the TUI report
Then it should show:
  - ASCII histogram of Sharpe ratios (10 bins)
  - P10/P50/P90 percentiles
  - Stability % (trials above threshold)
```

**Scenario 9.2: Display regime breakdown table**
```gherkin
Given bootstrap results grouped by regime
When I render the regime table
Then it should show:
  - Regime name (TrendingUp, Sideways, etc.)
  - Number of trials in that regime
  - P50 Sharpe and P50 DD
  - ✓/✗ status (pass/fail per regime)
```

**Scenario 9.3: Display L4 vs L5 comparison**
```gherkin
Given L4 Path MC results and L5 Bootstrap results
When I render the comparison table
Then it should show:
  - P10/P50/P90 for both L4 and L5
  - % change (L5 vs L4)
  - Interpretation (e.g., "P10 drop < 25% → low data snooping")
```

---

## Implementation Checklist

### Architecture (7 modules)
- [ ] `bootstrap/mod.rs` — BootstrapPolicy trait + re-exports
- [ ] `bootstrap/block_bootstrap.rs` — Block bootstrap resampler
- [ ] `bootstrap/regime_detector.rs` — Regime detection (HMM/vol/trend)
- [ ] `bootstrap/regime_resampler.rs` — Oversample specific regimes
- [ ] `bootstrap/bootstrap_trial.rs` — Single bootstrap trial
- [ ] `bootstrap/bootstrap_engine.rs` — Run N trials + aggregate
- [ ] `bootstrap/promotion_l5.rs` — L4 → L5 filter

### Core Algorithms (5 functions)
- [ ] `block_bootstrap()` — Stationary block bootstrap
- [ ] `RegimeDetector::detect()` — Label regimes (4 types)
- [ ] `RegimeResampler::resample()` — Weighted regime sampling
- [ ] `BootstrapTrial::run()` — Single trial (resample → backtest)
- [ ] `BootstrapEngine::run()` — Aggregate N trials → percentiles

### Cross-Regime Validation (2 components)
- [ ] `CrossRegimeValidator` — Per-regime thresholds
- [ ] `group_by_regime()` — Group trials by regime

### L5 Promotion Filter (1 component)
- [ ] `L5PromotionFilter::should_promote()` — L4 → L5 gate

### TUI Reports (3 widgets)
- [ ] Bootstrap Sharpe histogram (ASCII)
- [ ] Regime breakdown table
- [ ] L4 vs L5 comparison table

### BDD Tests (9 features, 27 scenarios)
- [ ] Feature 1: Block Bootstrap Resampling (4 scenarios)
- [ ] Feature 2: Regime Detection (4 scenarios)
- [ ] Feature 3: Regime Resampling (3 scenarios)
- [ ] Feature 4: Bootstrap Trial (3 scenarios)
- [ ] Feature 5: Bootstrap Engine (4 scenarios)
- [ ] Feature 6: Cross-Regime Validation (3 scenarios)
- [ ] Feature 7: L5 Promotion Filter (4 scenarios)
- [ ] Feature 8: Bootstrap Cost Analysis (3 scenarios)
- [ ] Feature 9: Bootstrap Report (3 scenarios)

**Total:** 31 items

---

## Acceptance Criteria

**M11 is complete when:**

1. **Block Bootstrap:**
   - ✅ Resamples data in blocks (e.g., 63 days)
   - ✅ Preserves autocorrelation ≈ ± 20%
   - ✅ Deterministic (same seed → same result)

2. **Regime Detection:**
   - ✅ Labels 4 regimes (Trending Up/Down, Sideways, Volatile)
   - ✅ Uses HMM + heuristics (SMA, volatility)

3. **Regime Resampling:**
   - ✅ Oversamples underrepresented regimes (e.g., 4× Volatile)
   - ✅ Produces balanced regime mix (not just historical)

4. **Bootstrap Engine:**
   - ✅ Runs N trials in parallel (e.g., 100)
   - ✅ Returns P10/P50/P90 percentiles
   - ✅ Computes stability (% of trials above threshold)

5. **Cross-Regime Validation:**
   - ✅ Requires P50 Sharpe > threshold in ALL regimes
   - ✅ Rejects strategies that fail in any regime

6. **L5 Promotion:**
   - ✅ Only promotes best L4 survivors (e.g., P10 > 0.90)
   - ✅ Filters 50-100% of L4 candidates

7. **TUI Reports:**
   - ✅ Bootstrap Sharpe histogram (ASCII)
   - ✅ Regime breakdown table (4 regimes × metrics)
   - ✅ L4 vs L5 comparison (% change)

8. **BDD Tests:**
   - ✅ All 9 features pass (27 scenarios)
   - ✅ Property tests for invariants (e.g., autocorrelation preservation)

9. **Performance:**
   - ✅ Full ladder (L1 → L5) completes in 6-7 hours for 1000 candidates
   - ✅ Parallel execution (Rayon) scales to 8+ cores

---

## Why M11 Matters

**Traditional backtesting:**
- ❌ Tests on one historical sample (data snooping)
- ❌ Assumes future = past regime distribution
- ❌ Reports one Sharpe (e.g., 1.20) with no uncertainty

**M11's Bootstrap:**
- ✅ Tests on 100 different plausible histories (resampled)
- ✅ Oversamples rare regimes (e.g., bear markets, volatility spikes)
- ✅ Returns distribution (P10/P50/P90)
- ✅ Requires P50 > threshold in ALL regimes (not just aggregate)

**Key insight:**
If a strategy survives 100 bootstrap trials with regime oversampling → it's robust to different histories and regime mixes, not just the one lucky sequence that happened to occur!

---

## Cost vs Realism Tradeoff (Final)

| Level | Method | Cost/candidate | Realism |
|-------|--------|----------------|---------|
| L1 | Fixed + WorstCase | 1× | Low |
| L2 | WF + WorstCase | 40× | Medium |
| L3 | Exec MC 100 + WorstCase | 100× | High |
| L4 | Path MC (50×100) | 5,000× | Very High |
| L5 | Bootstrap (100×50×100) | 500,000× | **Ultimate** |

**Promotion ladder makes L5 affordable:**
- Total: 511,620 backtests (vs 500M without ladder)
- Savings: **99.9%** 🎉
- Time: 6-7 hours (vs 241 days!)

---

## Next Steps (M12)

After M11, only one milestone remains:

**M12: Benchmarks & UI Polish**
- Criterion benchmarks (hot loop profiling)
- Final TUI polish (layout, colors, keybindings)
- Golden regression tests (stable reference results)
- Documentation (README, architecture diagrams)

**Expected deliverable:**
A production-ready, research-grade trend-following backtesting engine with the most rigorous robustness validation pipeline in the industry! 🚀

---

**End of M11 Specification**
