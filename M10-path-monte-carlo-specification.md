# M10: Path Monte Carlo — Specification

**Status:** Pre-implementation (spec complete)
**Milestone:** M10 of 12
**Dependencies:** M9 (Execution Monte Carlo)
**Estimated complexity:** High (Path generation + MC sampling)

---

## Overview

M10 implements **Path Monte Carlo (Path MC)** to resolve the **intrabar ambiguity problem**.

### The Intrabar Ambiguity Problem

Given daily OHLC data:
- **O** = $100 (open)
- **H** = $105 (high)
- **L** = $98 (low)
- **C** = $102 (close)

**Question:** What order did H and L occur?

Two possible paths:
1. **Path A:** O → H → L → C (high first, then low)
2. **Path B:** O → L → H → C (low first, then high)

**Why it matters:**

Example: Long position with stop-loss at $99 and take-profit at $104.

- **Path A (H first):** Take-profit hits at $104 ✅ → Exit with profit
- **Path B (L first):** Stop-loss hits at $99 ❌ → Exit with loss

**Same bar, opposite outcomes!**

Traditional backtesting assumes a **deterministic path policy** (e.g., "worst case"). This introduces **path bias**.

### M10's Solution: Path Monte Carlo

Instead of assuming one path, **sample from a distribution of plausible paths**:

1. Generate N micro-paths (e.g., 50) that satisfy OHLC constraints
2. Simulate execution on each path (with M9's slippage/adverse selection)
3. Aggregate results → distribution of outcomes (P10/P50/P90)

**Key insight:** If a strategy is robust across all plausible paths, it's more likely to survive real trading.

---

## Architecture

### File Structure (M10 modules)

```
trendlab-core/src/execution/
├── path/                           # M10: Path generation and MC
│   ├── mod.rs                      # PathPolicy trait + re-exports
│   ├── path_policy.rs              # Enum: WorstCase, BestCase, RandomPath, MicroPathMC
│   ├── path_constraints.rs         # Validate OHLC constraints (O→H/L→C)
│   ├── micro_path.rs               # MicroPath struct (60-minute ticks)
│   ├── micro_path_generator.rs     # Generate plausible micro-paths
│   ├── worst_case.rs               # WorstCasePathPolicy (L1, deterministic)
│   ├── best_case.rs                # BestCasePathPolicy (optimistic, debugging)
│   ├── random_path.rs              # RandomPathPolicy (single random path)
│   ├── path_mc.rs                  # PathMcPolicy (N paths + aggregate)
│   ├── path_mc_trial.rs            # Single path trial (deterministic seed)
│   ├── path_mc_engine.rs           # Run N path trials + percentiles
│   └── promotion_l4.rs             # L3 → L4 promotion filter
```

**8 new modules, ~900 lines of code.**

---

## Core Concepts

### 1. Path Constraints

All micro-paths must satisfy:
1. **Start at Open:** `path[0] == bar.open`
2. **End at Close:** `path[last] == bar.close`
3. **Visit High exactly once:** `max(path) == bar.high`
4. **Visit Low exactly once:** `min(path) == bar.low`
5. **No impossible jumps:** `|path[i+1] - path[i]| <= max_jump` (optional)

**Invalid paths are rejected.**

### 2. Micro-Path Representation

A micro-path divides the bar into M sub-intervals (e.g., 60 ticks for 1-minute resolution on a daily bar).

```rust
pub struct MicroPath {
    pub timestamps: Vec<DateTime<Utc>>,  // M+1 timestamps
    pub prices: Vec<f64>,                // M+1 prices
}

impl MicroPath {
    pub fn new(bar: &Bar, num_ticks: usize, seed: u64) -> Self;
    pub fn validate(&self, bar: &Bar) -> Result<(), PathError>;
    pub fn at_time(&self, t: DateTime<Utc>) -> f64;  // Interpolate price at time t
}
```

**Example (5-tick path):**

```
Bar: O=100, H=105, L=98, C=102

MicroPath (5 ticks):
  t=0:    100.0  (open)
  t=0.2:  103.5
  t=0.4:  105.0  (high)
  t=0.6:   98.0  (low)
  t=0.8:  101.0
  t=1.0:  102.0  (close)
```

### 3. Path Policies (Trait)

```rust
pub trait PathPolicy: Send + Sync {
    fn generate_path(
        &self,
        bar: &Bar,
        seed: u64,
    ) -> Result<MicroPath, PathError>;

    fn is_deterministic(&self) -> bool;
    fn name(&self) -> &str;
}
```

### 4. Path Policy Implementations

#### A) WorstCasePathPolicy (L1, deterministic)

**Purpose:** Conservative default for L1/L2 filters.

**Logic:**
- Long positions: assume stop-loss hits before take-profit
- Short positions: assume stop-loss hits before take-profit

**Path construction:**
1. If long: O → L (hit stop) → H → C
2. If short: O → H (hit stop) → L → C

**Pros:** Conservative, fast (deterministic)
**Cons:** Pessimistic bias (understates real performance)

```rust
pub struct WorstCasePathPolicy;

impl PathPolicy for WorstCasePathPolicy {
    fn generate_path(&self, bar: &Bar, _seed: u64) -> Result<MicroPath, PathError> {
        // For long: O → L → H → C
        // For short: O → H → L → C
        // (Requires position direction context)
    }

    fn is_deterministic(&self) -> bool { true }
    fn name(&self) -> &str { "WorstCase" }
}
```

#### B) BestCasePathPolicy (debugging only)

**Purpose:** Optimistic upper bound (for debugging/comparison).

**Logic:**
- Long positions: assume take-profit hits before stop-loss
- Short positions: assume take-profit hits before stop-loss

**Path construction:**
1. If long: O → H (hit target) → L → C
2. If short: O → L (hit target) → H → C

**Pros:** Fast (deterministic)
**Cons:** Optimistic bias (overstates real performance)

**Use case:** Debugging (compare WorstCase vs BestCase → quantify path sensitivity)

```rust
pub struct BestCasePathPolicy;

impl PathPolicy for BestCasePathPolicy {
    fn generate_path(&self, bar: &Bar, _seed: u64) -> Result<MicroPath, PathError> {
        // For long: O → H → L → C
        // For short: O → L → H → C
    }

    fn is_deterministic(&self) -> bool { true }
    fn name(&self) -> &str { "BestCase" }
}
```

#### C) RandomPathPolicy (single random path)

**Purpose:** Quick stochastic check (cheaper than full Path MC).

**Logic:**
- Generate one random micro-path (satisfies OHLC constraints)
- Use Brownian bridge to interpolate between keypoints

**Path construction:**
1. Choose random order for H/L: 50% chance of O→H→L→C, 50% chance of O→L→H→C
2. Use Brownian bridge to smoothly interpolate between O, H/L, C
3. Add noise (optional, controlled by `volatility` parameter)

**Pros:** Stochastic, more realistic than WorstCase
**Cons:** Single path (doesn't capture full distribution)

```rust
pub struct RandomPathPolicy {
    pub num_ticks: usize,      // e.g., 60 (1-minute resolution)
    pub volatility: f64,       // e.g., 0.02 (2% noise)
}

impl PathPolicy for RandomPathPolicy {
    fn generate_path(&self, bar: &Bar, seed: u64) -> Result<MicroPath, PathError> {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);

        // Step 1: Choose H/L order (50/50)
        let high_first = rng.gen_bool(0.5);

        // Step 2: Define keypoints
        let keypoints = if high_first {
            vec![bar.open, bar.high, bar.low, bar.close]
        } else {
            vec![bar.open, bar.low, bar.high, bar.close]
        };

        // Step 3: Brownian bridge interpolation
        let path = brownian_bridge(&keypoints, self.num_ticks, &mut rng, self.volatility);

        // Step 4: Validate constraints
        MicroPath::new(path).validate(bar)?;

        Ok(path)
    }

    fn is_deterministic(&self) -> bool { false }
    fn name(&self) -> &str { "RandomPath" }
}
```

#### D) PathMcPolicy (N paths + aggregate)

**Purpose:** Full Path Monte Carlo (L4 promotion ladder).

**Logic:**
- Generate N random micro-paths (e.g., 50)
- Simulate execution on each path (with M9's slippage/adverse selection)
- Aggregate results → P10/P50/P90 percentiles

**Pros:** Most realistic (captures full path uncertainty)
**Cons:** Most expensive (N × cost of single path)

```rust
pub struct PathMcPolicy {
    pub num_paths: usize,               // e.g., 50
    pub num_ticks_per_path: usize,      // e.g., 60 (1-minute resolution)
    pub volatility: f64,                // e.g., 0.02 (2% Brownian noise)
    pub slippage_dist: Arc<dyn SlippageDistribution>,  // From M9
    pub adverse_selection: AdverseSelectionModel,      // From M9
    pub queue_depth: QueueDepthModel,                  // From M9
}

impl PathPolicy for PathMcPolicy {
    fn generate_path(&self, bar: &Bar, seed: u64) -> Result<MicroPath, PathError> {
        // This trait method returns a SINGLE path (for compatibility)
        // The full MC aggregation is done in PathMcEngine (see below)

        // Generate one random path (deterministic given seed)
        RandomPathPolicy {
            num_ticks: self.num_ticks_per_path,
            volatility: self.volatility,
        }.generate_path(bar, seed)
    }

    fn is_deterministic(&self) -> bool { false }
    fn name(&self) -> &str { "PathMC" }
}
```

---

## Path MC Engine (Aggregation)

The `PathMcEngine` runs N path trials and aggregates results.

### PathMcTrial (Single Path Trial)

```rust
pub struct PathMcTrial {
    pub trial_id: usize,
    pub path_policy: Arc<dyn PathPolicy>,
    pub slippage_dist: Arc<dyn SlippageDistribution>,  // M9
    pub adverse_selection: AdverseSelectionModel,      // M9
    pub queue_depth: QueueDepthModel,                  // M9
    pub seed: u64,
}

impl PathMcTrial {
    pub fn run(
        &self,
        candidate: &Candidate,
        data: &TimeSeriesData,
        orders: &[Order],
    ) -> TrialResult {
        // Step 1: Generate micro-path for each bar
        let mut equity_curve = vec![candidate.initial_capital];
        let mut portfolio = Portfolio::new(candidate.initial_capital);

        for (i, bar) in data.bars.iter().enumerate() {
            let micro_path = self.path_policy.generate_path(bar, self.seed + i as u64)?;

            // Step 2: Simulate execution along micro-path
            for tick in &micro_path.prices {
                // Check if any orders trigger at this tick price
                let triggered_orders = orders.iter()
                    .filter(|o| o.triggers_at(*tick, bar))
                    .collect::<Vec<_>>();

                for order in triggered_orders {
                    // Apply M9's slippage + adverse selection + queue depth
                    let fill = self.simulate_fill(order, bar, *tick)?;
                    portfolio.apply_fill(fill);
                }
            }

            // Step 3: Update equity at end of bar
            portfolio.mark_to_market(bar.close);
            equity_curve.push(portfolio.equity());
        }

        // Step 4: Compute metrics
        let returns = equity_to_returns(&equity_curve);
        let sharpe = sharpe_ratio(&returns, 0.0);
        let max_dd = max_drawdown(&equity_curve);

        Ok(TrialResult {
            trial_id: self.trial_id,
            sharpe,
            max_drawdown: max_dd,
            final_equity: *equity_curve.last().unwrap(),
            equity_curve,
        })
    }

    fn simulate_fill(
        &self,
        order: &Order,
        bar: &Bar,
        trigger_price: f64,
    ) -> Result<Fill, ExecutionError> {
        // M9 integration: sample slippage, adverse selection, queue depth
        let mut rng = ChaCha8Rng::seed_from_u64(self.seed);

        // Sample slippage (bps)
        let slippage_bps = self.slippage_dist.sample(&mut rng);

        // Apply adverse selection (limit orders)
        let adjusted_price = if order.is_limit() {
            self.adverse_selection.adjust_limit_fill(order, bar, trigger_price, &mut rng)
        } else {
            trigger_price
        };

        // Apply slippage
        let fill_price = adjusted_price * (1.0 + slippage_bps / 10000.0 * order.side.sign());

        // Check queue depth (partial fill / no fill)
        let filled = self.queue_depth.check_fill(order, bar, &mut rng);
        if !filled {
            return Err(ExecutionError::NoFill);
        }

        Ok(Fill {
            order_id: order.id,
            price: fill_price,
            quantity: order.quantity,
            timestamp: bar.timestamp,
        })
    }
}
```

### PathMcEngine (Aggregate N Trials)

```rust
pub struct PathMcEngine {
    pub num_paths: usize,               // e.g., 50
    pub num_ticks_per_path: usize,      // e.g., 60
    pub volatility: f64,                // e.g., 0.02
    pub slippage_dist: Arc<dyn SlippageDistribution>,
    pub adverse_selection: AdverseSelectionModel,
    pub queue_depth: QueueDepthModel,
}

impl PathMcEngine {
    pub fn run(
        &self,
        candidate: &Candidate,
        data: &TimeSeriesData,
        orders: &[Order],
    ) -> Result<PathMcResult, ExecutionError> {
        // Run N trials in parallel (Rayon)
        let trials: Vec<PathMcTrial> = (0..self.num_paths)
            .map(|i| PathMcTrial {
                trial_id: i,
                path_policy: Arc::new(RandomPathPolicy {
                    num_ticks: self.num_ticks_per_path,
                    volatility: self.volatility,
                }),
                slippage_dist: Arc::clone(&self.slippage_dist),
                adverse_selection: self.adverse_selection.clone(),
                queue_depth: self.queue_depth.clone(),
                seed: candidate.seed + i as u64,
            })
            .collect();

        let results: Vec<TrialResult> = trials
            .par_iter()
            .map(|trial| trial.run(candidate, data, orders))
            .collect::<Result<Vec<_>, _>>()?;

        // Aggregate: compute percentiles
        let mut sharpes: Vec<f64> = results.iter().map(|r| r.sharpe).collect();
        let mut dds: Vec<f64> = results.iter().map(|r| r.max_drawdown).collect();
        sharpes.sort_by(|a, b| a.partial_cmp(b).unwrap());
        dds.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let p10_idx = (self.num_paths as f64 * 0.10) as usize;
        let p50_idx = (self.num_paths as f64 * 0.50) as usize;
        let p90_idx = (self.num_paths as f64 * 0.90) as usize;

        Ok(PathMcResult {
            num_paths: self.num_paths,
            sharpe_p10: sharpes[p10_idx],
            sharpe_p50: sharpes[p50_idx],
            sharpe_p90: sharpes[p90_idx],
            dd_p10: dds[p10_idx],
            dd_p50: dds[p50_idx],
            dd_p90: dds[p90_idx],
            equity_curves: results.into_iter().map(|r| r.equity_curve).collect(),
        })
    }
}

pub struct PathMcResult {
    pub num_paths: usize,
    pub sharpe_p10: f64,    // 10th percentile (pessimistic)
    pub sharpe_p50: f64,    // Median (expected)
    pub sharpe_p90: f64,    // 90th percentile (optimistic)
    pub dd_p10: f64,        // Best DD
    pub dd_p50: f64,        // Median DD
    pub dd_p90: f64,        // Worst DD
    pub equity_curves: Vec<Vec<f64>>,
}
```

---

## L4 Promotion Filter (L3 → L4)

**Problem:** Path MC (50 paths) is 50× more expensive than L3 (100 execution trials → 5000 total paths)!

**Solution:** Only promote the **best L3 survivors** to L4.

### L4PromotionFilter

```rust
pub struct L4PromotionFilter {
    pub min_l3_sharpe_p10: f64,        // e.g., 0.90 (10th percentile from L3)
    pub max_l3_dd_p90: f64,            // e.g., -20% (90th percentile from L3)
    pub min_l3_stability: f64,         // e.g., 0.75 (75% of trials profitable)
    pub max_l3_uncertainty: f64,       // e.g., 0.30 (P90-P10 spread < 30%)
}

impl L4PromotionFilter {
    pub fn should_promote(&self, l3_result: &ExecutionMcResult) -> (bool, String) {
        // Check L3 P10 Sharpe
        if l3_result.sharpe_p10 < self.min_l3_sharpe_p10 {
            return (false, format!(
                "L3 P10 Sharpe {:.2} < min {:.2}",
                l3_result.sharpe_p10,
                self.min_l3_sharpe_p10
            ));
        }

        // Check L3 P90 DD
        if l3_result.dd_p90 < self.max_l3_dd_p90 {
            return (false, format!(
                "L3 P90 DD {:.1}% > max {:.1}%",
                l3_result.dd_p90 * 100.0,
                self.max_l3_dd_p90 * 100.0
            ));
        }

        // Check L3 stability
        let profitable_trials = l3_result.equity_curves.iter()
            .filter(|curve| curve.last().unwrap() > &l3_result.initial_capital)
            .count();
        let stability = profitable_trials as f64 / l3_result.num_trials as f64;

        if stability < self.min_l3_stability {
            return (false, format!(
                "L3 stability {:.1}% < min {:.1}%",
                stability * 100.0,
                self.min_l3_stability * 100.0
            ));
        }

        // Check L3 uncertainty (P90-P10 spread)
        let uncertainty = (l3_result.sharpe_p90 - l3_result.sharpe_p10) / l3_result.sharpe_p50;
        if uncertainty > self.max_l3_uncertainty {
            return (false, format!(
                "L3 uncertainty {:.1}% > max {:.1}%",
                uncertainty * 100.0,
                self.max_l3_uncertainty * 100.0
            ));
        }

        (true, format!(
            "Promoted: P10={:.2}, P90 DD={:.1}%, stability={:.1}%, uncertainty={:.1}%",
            l3_result.sharpe_p10,
            l3_result.dd_p90 * 100.0,
            stability * 100.0,
            uncertainty * 100.0
        ))
    }
}
```

### Promotion Flow (L1 → L2 → L3 → L4)

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
 L4: Path MC (50 paths × 100 exec trials = 5000 total)
     Cost: 2 × 5000 = 10,000 backtests
     Time: 30 minutes
     ↓
 [Best candidate selected]

Total cost: 1000 + 320 + 300 + 10,000 = 11,620 backtests
Without ladder: 1000 × 5000 = 5,000,000 backtests
Savings: 99.8% 🎉
Total time: ~37 minutes (vs 35 hours without ladder)
```

---

## Brownian Bridge Interpolation

**Problem:** Given keypoints O, H/L, C, generate smooth micro-path.

**Solution:** Brownian bridge (constrained random walk).

### Algorithm

```rust
pub fn brownian_bridge(
    keypoints: &[f64],         // [O, H, L, C] or [O, L, H, C]
    num_ticks: usize,          // e.g., 60
    rng: &mut impl Rng,
    volatility: f64,           // e.g., 0.02 (2% noise)
) -> Vec<f64> {
    let num_segments = keypoints.len() - 1;
    let ticks_per_segment = num_ticks / num_segments;

    let mut path = Vec::with_capacity(num_ticks + 1);

    for seg in 0..num_segments {
        let start = keypoints[seg];
        let end = keypoints[seg + 1];
        let segment_path = bridge_segment(start, end, ticks_per_segment, rng, volatility);

        if seg == 0 {
            path.extend(segment_path);
        } else {
            path.extend(&segment_path[1..]); // Skip duplicate start point
        }
    }

    path
}

fn bridge_segment(
    start: f64,
    end: f64,
    num_ticks: usize,
    rng: &mut impl Rng,
    volatility: f64,
) -> Vec<f64> {
    let mut path = vec![start];
    let drift = (end - start) / num_ticks as f64;

    let normal = Normal::new(0.0, volatility).unwrap();

    for i in 1..num_ticks {
        let expected = start + drift * i as f64;
        let noise = normal.sample(rng) * start;  // Proportional to price level
        let next_price = expected + noise;
        path.push(next_price);
    }

    path.push(end);  // Ensure endpoint is exact
    path
}
```

**Example output:**

```
Keypoints: [100, 105, 98, 102]

Brownian bridge (20 ticks):
  [100.0, 100.8, 101.5, 102.3, 103.1, 103.9, 104.7, 105.0,  ← O → H
   104.2, 103.1, 102.0, 100.9, 99.8, 98.7, 98.0,            ← H → L
   98.5, 99.1, 99.7, 100.4, 101.1, 101.8, 102.0]            ← L → C
```

---

## BDD Test Scenarios (M10)

### Feature 1: Path Constraints Validation (4 scenarios)

**Scenario 1.1: Valid path (O → H → L → C)**

```gherkin
Scenario: Valid path satisfies all constraints
  Given a bar with O=100, H=105, L=98, C=102
  And a micro-path [100, 103, 105, 101, 98, 99, 102]
  When I validate the path constraints
  Then the path is valid
  And start price is 100
  And end price is 102
  And max price is 105
  And min price is 98
```

**Scenario 1.2: Invalid path (doesn't visit high)**

```gherkin
Scenario: Path missing high is rejected
  Given a bar with O=100, H=105, L=98, C=102
  And a micro-path [100, 103, 101, 98, 99, 102]
  When I validate the path constraints
  Then validation fails with "High not reached: max=103 < bar.high=105"
```

**Scenario 1.3: Invalid path (doesn't start at open)**

```gherkin
Scenario: Path with wrong start is rejected
  Given a bar with O=100, H=105, L=98, C=102
  And a micro-path [99, 103, 105, 101, 98, 102]
  When I validate the path constraints
  Then validation fails with "Start price 99 != open 100"
```

**Scenario 1.4: Invalid path (doesn't end at close)**

```gherkin
Scenario: Path with wrong end is rejected
  Given a bar with O=100, H=105, L=98, C=102
  And a micro-path [100, 103, 105, 101, 98, 99, 101]
  When I validate the path constraints
  Then validation fails with "End price 101 != close 102"
```

---

### Feature 2: WorstCase Path Policy (4 scenarios)

**Scenario 2.1: Long position (stop hits first)**

```gherkin
Scenario: WorstCase path for long position hits stop first
  Given a bar with O=100, H=105, L=98, C=102
  And a long position with stop=99, target=104
  When I generate a WorstCase path
  Then the path order is [100 (O), 98 (L), 105 (H), 102 (C)]
  And the stop at 99 triggers before target at 104
```

**Scenario 2.2: Short position (stop hits first)**

```gherkin
Scenario: WorstCase path for short position hits stop first
  Given a bar with O=100, H=105, L=98, C=102
  And a short position with stop=104, target=99
  When I generate a WorstCase path
  Then the path order is [100 (O), 105 (H), 98 (L), 102 (C)]
  And the stop at 104 triggers before target at 99
```

**Scenario 2.3: WorstCase is deterministic**

```gherkin
Scenario: WorstCase path is deterministic (same seed → same path)
  Given a bar with O=100, H=105, L=98, C=102
  When I generate a WorstCase path with seed 42
  And I generate another WorstCase path with seed 99
  Then both paths are identical
```

**Scenario 2.4: WorstCase pessimistic bias (vs BestCase)**

```gherkin
Scenario: WorstCase Sharpe < BestCase Sharpe (quantifies bias)
  Given a Donchian breakout strategy
  And 1000 bars of test data
  When I backtest with WorstCase path policy
  And I backtest with BestCase path policy
  Then WorstCase Sharpe is 0.92
  And BestCase Sharpe is 1.38
  And the bias spread is 50% (quantifies path sensitivity)
```

---

### Feature 3: RandomPath Policy (3 scenarios)

**Scenario 3.1: Random path satisfies constraints**

```gherkin
Scenario: RandomPath generates valid OHLC path
  Given a bar with O=100, H=105, L=98, C=102
  When I generate a RandomPath with seed 42
  Then the path is valid (starts at 100, ends at 102, visits 105 and 98)
  And the path has 60 ticks (1-minute resolution)
```

**Scenario 3.2: Random path is stochastic (different seeds)**

```gherkin
Scenario: RandomPath generates different paths with different seeds
  Given a bar with O=100, H=105, L=98, C=102
  When I generate a RandomPath with seed 42
  And I generate another RandomPath with seed 99
  Then the paths are different (at least 50% of ticks differ)
```

**Scenario 3.3: Random path H/L order is 50/50**

```gherkin
Scenario: RandomPath generates H-first and L-first paths equally
  Given a bar with O=100, H=105, L=98, C=102
  When I generate 1000 RandomPaths with different seeds
  Then approximately 500 paths visit H before L (within 5% tolerance)
  And approximately 500 paths visit L before H (within 5% tolerance)
```

---

### Feature 4: Brownian Bridge Interpolation (4 scenarios)

**Scenario 4.1: Bridge hits all keypoints**

```gherkin
Scenario: Brownian bridge passes through all keypoints
  Given keypoints [100, 105, 98, 102]
  When I generate a Brownian bridge with 60 ticks
  Then tick 0 is 100 (O)
  And tick 20 is 105 (H)
  And tick 40 is 98 (L)
  And tick 60 is 102 (C)
```

**Scenario 4.2: Bridge adds realistic noise**

```gherkin
Scenario: Brownian bridge adds volatility between keypoints
  Given keypoints [100, 105, 98, 102]
  When I generate a Brownian bridge with volatility 0.02
  Then intermediate ticks deviate from linear interpolation
  And max deviation is < 2% (controlled by volatility param)
```

**Scenario 4.3: Bridge is smooth (no jumps)**

```gherkin
Scenario: Brownian bridge has no impossible jumps
  Given keypoints [100, 105, 98, 102]
  When I generate a Brownian bridge with 60 ticks
  Then max tick-to-tick change is < 5% (no gaps)
```

**Scenario 4.4: Bridge is deterministic (same seed)**

```gherkin
Scenario: Brownian bridge is reproducible with same seed
  Given keypoints [100, 105, 98, 102]
  When I generate a bridge with seed 42
  And I generate another bridge with seed 42
  Then both bridges are identical (same noise samples)
```

---

### Feature 5: PathMcTrial (Single Path Trial) (4 scenarios)

**Scenario 5.1: Single path trial executes fills**

```gherkin
Scenario: PathMcTrial simulates fills along micro-path
  Given a Donchian strategy with stop=99, target=104
  And a bar with O=100, H=105, L=98, C=102
  And a micro-path that visits target first [100, 102, 104, ...]
  When I run a PathMcTrial with seed 42
  Then the target order fills at 104
  And the stop order does not fill
  And the final equity is positive
```

**Scenario 5.2: Different path → different fills**

```gherkin
Scenario: PathMcTrial with different path produces different result
  Given a Donchian strategy with stop=99, target=104
  And a bar with O=100, H=105, L=98, C=102
  And PathA visits stop first [100, 98, ...] (stop fills)
  And PathB visits target first [100, 104, ...] (target fills)
  When I run PathMcTrial with PathA
  And I run PathMcTrial with PathB
  Then PathA final equity < initial capital (loss)
  And PathB final equity > initial capital (profit)
```

**Scenario 5.3: Path trial integrates M9 slippage**

```gherkin
Scenario: PathMcTrial applies slippage from M9
  Given a PathMcTrial with GaussianSlippage(mean=5 bps, std=2 bps)
  And a stop order triggers at price 99
  When I run the trial with seed 42
  Then the fill price is 98.95 (99 - 5 bps slippage)
  And slippage is sampled from Gaussian distribution
```

**Scenario 5.4: Path trial is deterministic (same seed)**

```gherkin
Scenario: PathMcTrial is reproducible with same seed
  Given a PathMcTrial with seed 42
  When I run the trial twice
  Then both trials produce identical results (same path, same slippage samples)
```

---

### Feature 6: PathMcEngine (Aggregate N Paths) (4 scenarios)

**Scenario 6.1: PathMcEngine aggregates 50 paths**

```gherkin
Scenario: PathMcEngine runs 50 path trials and aggregates results
  Given a Donchian strategy
  And 1000 bars of test data
  And a PathMcEngine with 50 paths
  When I run the Path MC engine
  Then I receive 50 trial results
  And results include P10/P50/P90 percentiles for Sharpe and DD
```

**Scenario 6.2: Path MC percentiles show uncertainty**

```gherkin
Scenario: Path MC quantifies path uncertainty via percentiles
  Given a Donchian strategy
  When I run PathMcEngine with 50 paths
  Then Sharpe P10 is 0.85 (pessimistic: stops hit first)
  And Sharpe P50 is 1.10 (median)
  And Sharpe P90 is 1.35 (optimistic: targets hit first)
  And uncertainty spread (P90-P10) is 0.50 (59% relative uncertainty)
```

**Scenario 6.3: Path MC runs trials in parallel**

```gherkin
Scenario: PathMcEngine runs trials in parallel for speed
  Given a PathMcEngine with 50 paths
  When I run the engine on 8 CPU cores
  Then total time is ~6× faster than serial execution
  And all 50 trials complete successfully
```

**Scenario 6.4: Path MC is deterministic (same candidate seed)**

```gherkin
Scenario: PathMcEngine is reproducible with same candidate seed
  Given a candidate with seed 42
  And a PathMcEngine with 50 paths
  When I run the engine twice
  Then both runs produce identical P10/P50/P90 results
```

---

### Feature 7: L4 Promotion Filter (4 scenarios)

**Scenario 7.1: Promote strong L3 survivor**

```gherkin
Scenario: L4 filter promotes candidate with strong L3 results
  Given a L3 result with P10 Sharpe=0.95, P90 DD=-18%, stability=80%
  And a L4PromotionFilter with min_p10=0.90, max_dd=-20%, min_stability=75%
  When I check if candidate should promote to L4
  Then promotion is approved
  And reason is "Promoted: P10=0.95, P90 DD=-18%, stability=80%, uncertainty=18%"
```

**Scenario 7.2: Reject low L3 P10 Sharpe**

```gherkin
Scenario: L4 filter rejects candidate with low L3 P10 Sharpe
  Given a L3 result with P10 Sharpe=0.75 (below threshold)
  And a L4PromotionFilter with min_p10=0.90
  When I check if candidate should promote to L4
  Then promotion is rejected
  And reason is "L3 P10 Sharpe 0.75 < min 0.90"
```

**Scenario 7.3: Reject excessive L3 DD uncertainty**

```gherkin
Scenario: L4 filter rejects candidate with high DD uncertainty
  Given a L3 result with DD P10=-10%, DD P90=-35% (large spread)
  And a L4PromotionFilter with max_dd=-20%
  When I check if candidate should promote to L4
  Then promotion is rejected
  And reason is "L3 P90 DD -35% > max -20%"
```

**Scenario 7.4: Batch L3 → L4 promotion (filter most)**

```gherkin
Scenario: L4 filter reduces batch from 3 to 2 candidates
  Given 3 L3 survivors:
    | Candidate       | P10 Sharpe | P90 DD | Stability |
    | Donchian(20)    | 0.95       | -18%   | 80%       |
    | Donchian(25)    | 0.92       | -19%   | 78%       |
    | MA_Cross(50,200)| 0.82       | -16%   | 72%       |
  And a L4PromotionFilter with min_p10=0.90, min_stability=75%
  When I apply the filter to all 3 candidates
  Then 2 candidates promote to L4 (Donchian 20 and 25)
  And 1 candidate is rejected (MA_Cross: P10=0.82 < 0.90)
```

---

### Feature 8: Path MC Cost Analysis (3 scenarios)

**Scenario 8.1: L3 → L4 computational cost**

```gherkin
Scenario: L4 Path MC is 50× more expensive than L3
  Given a L3 Execution MC with 100 trials
  And a L4 Path MC with 50 paths × 100 exec trials = 5000 total
  When I measure computational cost
  Then L3 cost is 100 backtests (baseline)
  And L4 cost is 5000 backtests (50× more expensive)
```

**Scenario 8.2: Full ladder cost (L1 → L2 → L3 → L4)**

```gherkin
Scenario: Full promotion ladder saves 99.8% of compute
  Given 1000 initial candidates
  When I run the full ladder:
    | Level | Method                          | Survivors | Cost per candidate | Total cost |
    | L1    | Fixed 70/30 + WorstCase         | 8         | 1                  | 1000       |
    | L2    | WF 40 windows + WorstCase       | 3         | 40                 | 320        |
    | L3    | MC 100 trials + WorstCase       | 2         | 100                | 300        |
    | L4    | Path MC 50 paths × 100 exec     | 1         | 5000               | 10,000     |
  Then total cost is 11,620 backtests
  And naive cost (no ladder) is 1000 × 5000 = 5,000,000 backtests
  And savings is 99.8%
```

**Scenario 8.3: L4 runtime (2 candidates)**

```gherkin
Scenario: L4 takes 30 minutes for 2 candidates
  Given 2 L4 candidates
  And each runs 5000 trials (50 paths × 100 exec)
  And each trial takes ~0.2 seconds
  When I run L4 in parallel (8 cores)
  Then total time is ~30 minutes
  And time per candidate is ~15 minutes
```

---

### Feature 9: Path MC Report (TUI) (3 scenarios)

**Scenario 9.1: Display Path MC percentiles table**

```gherkin
Scenario: TUI shows Path MC P10/P50/P90 results
  Given a PathMcResult with 50 paths
  When I display the Path MC report in TUI
  Then I see a table:
    | Metric       | P10 (pessimistic) | P50 (median) | P90 (optimistic) |
    | Sharpe Ratio | 0.85              | 1.10         | 1.35             |
    | Max Drawdown | -14%              | -19%         | -24%             |
```

**Scenario 9.2: Equity curve fan chart (P10/P50/P90)**

```gherkin
Scenario: TUI displays equity curve fan chart
  Given a PathMcResult with 50 paths
  When I display the equity curve fan chart
  Then I see 3 curves plotted:
    - P10 curve (pessimistic, lower bound)
    - P50 curve (median, expected)
    - P90 curve (optimistic, upper bound)
  And the shaded area between P10 and P90 shows uncertainty
```

**Scenario 9.3: Compare L3 vs L4 metrics**

```gherkin
Scenario: TUI compares L3 (Execution MC) vs L4 (Path MC)
  Given a L3 result (Execution MC, 100 trials, WorstCase path)
  And a L4 result (Path MC, 50 paths × 100 exec)
  When I display the comparison table
  Then I see:
    | Metric       | L3 (WorstCase path) | L4 (Path MC)   | Difference |
    | Sharpe P10   | 0.92                | 0.85           | -7.6%      |
    | Sharpe P50   | 1.15                | 1.10           | -4.3%      |
    | Sharpe P90   | 1.38                | 1.35           | -2.2%      |
  And the difference shows path bias (WorstCase is not actually worst!)
```

---

## Completion Criteria (M10)

### Architecture & Core Traits (6 items):
- [ ] `PathPolicy` trait (generate_path, is_deterministic, name)
- [ ] `PathConstraints` validator (O→H/L→C, visit H/L exactly once)
- [ ] `MicroPath` struct (timestamps, prices, validate, at_time)
- [ ] `PathMcTrial` struct (single path + M9 integration)
- [ ] `PathMcEngine` struct (N paths + aggregation)
- [ ] `L4PromotionFilter` struct (L3 → L4 filter)

### Path Policy Implementations (4 items):
- [ ] `WorstCasePathPolicy` (L1, deterministic, pessimistic)
- [ ] `BestCasePathPolicy` (debugging, optimistic)
- [ ] `RandomPathPolicy` (single random path, Brownian bridge)
- [ ] `PathMcPolicy` (N paths + aggregate, L4)

### Brownian Bridge (2 items):
- [ ] `brownian_bridge()` function (keypoints → smooth path)
- [ ] `bridge_segment()` function (interpolate with noise)

### Path MC Engine (4 items):
- [ ] Single path trial (deterministic seed)
- [ ] Different paths → different fills
- [ ] Parallel path execution (Rayon)
- [ ] Aggregate results → P10/P50/P90 percentiles

### L4 Promotion Filter (3 items):
- [ ] Check L3 P10 Sharpe threshold
- [ ] Check L3 P90 DD threshold
- [ ] Check L3 stability + uncertainty
- [ ] Batch filtering (L3 → L4)

### TUI & Reporting (3 items):
- [ ] Path MC percentiles table (P10/P50/P90)
- [ ] Equity curve fan chart (P10/P50/P90 curves)
- [ ] L3 vs L4 comparison table

### BDD Tests (9 features):
- [ ] Feature 1: Path constraints validation (4 scenarios)
- [ ] Feature 2: WorstCase path policy (4 scenarios)
- [ ] Feature 3: RandomPath policy (3 scenarios)
- [ ] Feature 4: Brownian bridge interpolation (4 scenarios)
- [ ] Feature 5: PathMcTrial (single path trial) (4 scenarios)
- [ ] Feature 6: PathMcEngine (aggregate N paths) (4 scenarios)
- [ ] Feature 7: L4 promotion filter (4 scenarios)
- [ ] Feature 8: Path MC cost analysis (3 scenarios)
- [ ] Feature 9: Path MC report (TUI) (3 scenarios)

**Total:** 31 completion items

---

## Key Design Decisions

### 1. Default Path Policy: WorstCase (L1/L2/L3)

**Rationale:** WorstCase is fast (deterministic) and conservative (pessimistic bias).

**Usage:**
- L1 (Fixed 70/30): WorstCase path
- L2 (Walk-Forward): WorstCase path
- L3 (Execution MC): WorstCase path (100 trials sample slippage, but path is fixed)

**Why not RandomPath for L1/L2?**
- RandomPath is stochastic → requires multiple runs → slower
- WorstCase is deterministic → single run → faster
- L1/L2 are filters (reject weak candidates) → conservative bias is safer

### 2. Path MC (L4) combines Path uncertainty + Execution uncertainty

**L3 (Execution MC):**
- Fixed path (WorstCase)
- Stochastic execution (sample slippage, adverse selection, queue depth)
- 100 trials → P10/P50/P90 execution outcomes

**L4 (Path MC):**
- Stochastic paths (50 different micro-paths)
- Stochastic execution (100 trials per path)
- 50 × 100 = 5000 total trials → P10/P50/P90 combined outcomes

**Insight:** L4 captures **both** path uncertainty and execution uncertainty!

### 3. Brownian Bridge for Realistic Paths

**Why Brownian bridge?**
- Satisfies OHLC constraints (starts at O, ends at C, visits H/L)
- Smooth paths (no impossible jumps)
- Controlled randomness (volatility parameter)

**Alternatives considered:**
- **Linear interpolation:** Too rigid (no randomness)
- **Geometric Brownian Motion (GBM):** Doesn't guarantee endpoint (C)
- **Cubic splines:** Computationally expensive

**Decision:** Brownian bridge is the best trade-off (realistic + fast).

### 4. Path MC is Optional (Not Required for Most Users)

**When to use L4 (Path MC):**
- Final candidate validation (after L1/L2/L3 ladder)
- Strategies with tight stops/targets (highly path-sensitive)
- Research/publication (need to quantify path uncertainty)

**When to skip L4:**
- L3 is sufficient for most backtests
- Path sensitivity is low (wide stops/targets)
- Computational budget is limited

**Default recommendation:** Run L1 → L2 → L3, then spot-check best candidate with L4.

---

## Example Flows

### Flow 1: Single Random Path (Deterministic Seed)

```rust
let bar = Bar {
    timestamp: Utc::now(),
    open: 100.0,
    high: 105.0,
    low: 98.0,
    close: 102.0,
    volume: 1_000_000,
};

let policy = RandomPathPolicy {
    num_ticks: 60,
    volatility: 0.02,
};

let path = policy.generate_path(&bar, 42)?;

println!("Random path (seed=42, 60 ticks):");
for (i, price) in path.prices.iter().enumerate() {
    println!("  tick {:2}: {:.2}", i, price);
}
```

**Output:**

```
Random path (seed=42, 60 ticks):
  tick  0: 100.00  (open)
  tick  1: 100.75
  tick  2: 101.48
  ...
  tick 20: 105.00  (high)
  ...
  tick 40: 98.00   (low)
  ...
  tick 60: 102.00  (close)
```

### Flow 2: WorstCase vs BestCase (Quantify Path Bias)

```rust
let strategy = DonchianBreakout {
    lookback: 20,
    stop_atr_mult: 2.0,
    target_atr_mult: 3.0,
};

// Backtest with WorstCase path
let worst_case = backtest(&strategy, &data, WorstCasePathPolicy)?;

// Backtest with BestCase path
let best_case = backtest(&strategy, &data, BestCasePathPolicy)?;

println!("Path Bias Analysis:");
println!("  WorstCase Sharpe: {:.2}", worst_case.sharpe);
println!("  BestCase Sharpe:  {:.2}", best_case.sharpe);
println!("  Bias spread:      {:.1}%",
    (best_case.sharpe - worst_case.sharpe) / worst_case.sharpe * 100.0);
```

**Output:**

```
Path Bias Analysis:
  WorstCase Sharpe: 0.92
  BestCase Sharpe:  1.38
  Bias spread:      50.0%

Interpretation: Strategy is HIGHLY path-sensitive!
Recommendation: Use Path MC (L4) to quantify uncertainty.
```

### Flow 3: Full Path MC Engine (50 Paths × 100 Exec Trials)

```rust
let mc_engine = PathMcEngine {
    num_paths: 50,
    num_ticks_per_path: 60,
    volatility: 0.02,
    slippage_dist: Arc::new(GaussianSlippage {
        mean_bps: 5.0,
        std_bps: 2.0,
    }),
    adverse_selection: AdverseSelectionModel::moderate(),
    queue_depth: QueueDepthModel {
        fill_probability: 0.75,
    },
};

let result = mc_engine.run(&candidate, &data, &orders)?;

println!("Path MC Results (50 paths × 100 exec trials = 5000 total):");
println!("  Sharpe Ratio:");
println!("    P10 (pessimistic): {:.2}", result.sharpe_p10);
println!("    P50 (median):      {:.2}", result.sharpe_p50);
println!("    P90 (optimistic):  {:.2}", result.sharpe_p90);
println!();
println!("  Max Drawdown:");
println!("    P10 (best):   {:.1}%", result.dd_p10 * 100.0);
println!("    P50 (median): {:.1}%", result.dd_p50 * 100.0);
println!("    P90 (worst):  {:.1}%", result.dd_p90 * 100.0);
```

**Output:**

```
Path MC Results (50 paths × 100 exec trials = 5000 total):
  Sharpe Ratio:
    P10 (pessimistic): 0.85
    P50 (median):      1.10
    P90 (optimistic):  1.35

  Max Drawdown:
    P10 (best):   -14.2%
    P50 (median): -19.3%
    P90 (worst):  -23.8%

Interpretation: Strategy is robust (P10 still acceptable).
Decision: Promote to production (live trading candidate).
```

### Flow 4: L4 Promotion Filter (L3 → L4)

```rust
// L3 results (Execution MC, 100 trials, WorstCase path)
let l3_result = ExecutionMcResult {
    sharpe_p10: 0.95,
    sharpe_p50: 1.15,
    sharpe_p90: 1.38,
    dd_p10: -0.14,
    dd_p50: -0.19,
    dd_p90: -0.24,
    // ... (stability, equity curves, etc.)
};

// L4 promotion filter
let l4_filter = L4PromotionFilter {
    min_l3_sharpe_p10: 0.90,
    max_l3_dd_p90: -0.20,
    min_l3_stability: 0.75,
    max_l3_uncertainty: 0.30,
};

let (should_promote, reason) = l4_filter.should_promote(&l3_result);

if should_promote {
    println!("✓ L3 → L4: {}", reason);
    // Run L4 Path MC (50 paths × 100 exec)
    let l4_result = path_mc_engine.run(&candidate, &data, &orders)?;
} else {
    println!("✗ Rejected at L3: {}", reason);
}
```

**Output:**

```
✓ L3 → L4: Promoted: P10=0.95, P90 DD=-24%, stability=78%, uncertainty=22%

Running L4 Path MC (50 paths × 100 exec trials)...
[========================================] 100%

L4 Path MC Results:
  Sharpe P10/P50/P90: 0.85 / 1.10 / 1.35
  (vs L3: 0.95 / 1.15 / 1.38)

Analysis: L4 P10 (0.85) is slightly lower than L3 P10 (0.95).
Reason: L3 used WorstCase path (fixed), L4 samples all paths (more realistic).
Decision: L4 result is more trustworthy (captures full uncertainty).
```

---

## Performance Characteristics

### Computational Cost (Per Candidate)

| Level | Method                           | Backtests | Time (est.) |
|-------|----------------------------------|-----------|-------------|
| L1    | Fixed 70/30 + WorstCase          | 1         | 0.01s       |
| L2    | WF 40 windows + WorstCase        | 40        | 2s          |
| L3    | MC 100 trials + WorstCase        | 100       | 5s          |
| L4    | Path MC (50 paths × 100 exec)    | 5,000     | 900s (15m)  |

### Ladder Efficiency (1000 Initial Candidates)

```
[1000 candidates]
     ↓
 L1: 1000 × 0.01s = 10s
     → 8 survivors (99.2% filtered)
     ↓
 L2: 8 × 2s = 16s
     → 3 survivors (62.5% filtered)
     ↓
 L3: 3 × 5s = 15s
     → 2 survivors (33% filtered)
     ↓
 L4: 2 × 900s = 1800s (30 minutes)
     → 1 best candidate
     ↓
Total: 10s + 16s + 15s + 1800s = 1841s (~31 minutes)

Without ladder: 1000 × 900s = 900,000s (~250 hours)
Savings: 99.8% ✅
```

---

## Integration with M9 (Execution Monte Carlo)

**M10 builds on M9:**

- **M9:** Samples slippage, adverse selection, queue depth (execution uncertainty)
- **M10:** Samples intrabar paths (path uncertainty)

**Combined (L4 Path MC):**

```rust
// For each of 50 paths:
for path_seed in 0..50 {
    let micro_path = generate_path(bar, path_seed);

    // For each of 100 execution trials:
    for exec_seed in 0..100 {
        let slippage = slippage_dist.sample(exec_seed);     // M9
        let adverse = adverse_selection.adjust(exec_seed);  // M9
        let filled = queue_depth.check_fill(exec_seed);     // M9

        let fill = simulate_fill(micro_path, slippage, adverse, filled);
        portfolio.apply_fill(fill);
    }
}

// Result: 50 × 100 = 5000 total trials
// Captures BOTH path uncertainty AND execution uncertainty
```

---

## Limitations & Future Work

### Current Limitations (M10)

1. **No microstructure:** Assumes uniform intrabar liquidity (ignores bid-ask bounce)
2. **No regime shifts:** Path generation doesn't condition on volatility regime
3. **No correlation:** Paths for different bars are independent (ignores serial correlation)
4. **Fixed tick resolution:** 60 ticks per bar (1-minute granularity)

### Future Enhancements (Post-M12)

1. **Microstructure simulation:** Model bid-ask bounce, queue dynamics (Level 2 data)
2. **Regime-conditional paths:** VIX-based path volatility (high-vol bars → wider spreads)
3. **Correlated paths:** AR(1) or GARCH(1,1) for serial correlation
4. **Adaptive tick resolution:** Use more ticks for volatile bars, fewer for calm bars
5. **Historical path database:** Bootstrap from actual intraday paths (if available)

---

## Summary

M10 (Path Monte Carlo) is the **final robustness layer** in the promotion ladder.

**What M10 adds:**
1. **Path uncertainty quantification:** Answers "What if the price took a different path?"
2. **Realistic path sampling:** Brownian bridge generates smooth, plausible paths
3. **Combined uncertainty:** L4 captures both path uncertainty (M10) and execution uncertainty (M9)
4. **Conservative filtering:** L4 promotion filter ensures only robust candidates reach L4

**Why M10 matters:**
- Traditional backtests assume a **fixed path** (e.g., WorstCase) → path bias
- Real trading involves **unknown paths** → actual fills may differ
- Path MC samples **all plausible paths** → distribution of outcomes
- If P10 is still acceptable, strategy is **robust to path ambiguity**

**Cost vs realism:**
- L4 is 50× more expensive than L3 (5000 vs 100 backtests)
- But the promotion ladder makes it affordable (only 2 candidates reach L4)
- Total compute: 11,620 backtests (vs 5M without ladder) → **99.8% savings**

**Next milestone:** M11 (Bootstrap & Regime Resampling) — final statistical robustness checks.

---

**M10 Status:** ✅ Specification complete (ready for implementation)
