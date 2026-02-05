# M7: Strategy Composition + Normalization — Specification

**Milestone:** M7 (Strategy Composition + Normalization)
**Status:** In Progress
**Estimated Lines of Code:** ~800
**Dependencies:** M0 (Events), M1 (Signals), M2 (Order Book), M3 (Execution), M4 (Portfolio), M5 (Data), M6 (Position Management)

---

## Objective

Create the **composition layer** that assembles signals, order policies, position managers, execution models, and sizers into **complete, comparable strategies**.

This milestone solves the "apples-to-oranges" comparison problem: different signal families have different natural order types, risk profiles, and exit behaviors. M7 provides:

1. **Portfolio-agnostic signals** (no position size or risk assumptions)
2. **Family-specific order policies** (breakout → stops, mean-reversion → limits)
3. **Normalized risk sizing** (ATR-risk sizing for fair comparisons)
4. **Composition API** (Signal + OrderPolicy + PM + ExecutionPreset + Sizer = Strategy)
5. **Fair comparisons** (normalize PM/execution to compare signals on equal footing)

---

## Core Problems Solved

### Problem 1: Signals Leak Portfolio State

**Bad (coupled):**
```rust
// Signal knows about portfolio/position size — BAD
fn generate_signal(&self, bar: &Bar, portfolio: &Portfolio) -> Option<Order> {
    let current_position = portfolio.position("AAPL")?;
    if self.should_exit() {
        return Some(Order::market_close(current_position.qty));
    }
    None
}
```

**Good (decoupled):**
```rust
// Signal emits exposure intent only — GOOD
fn generate_signal(&self, bar: &Bar) -> SignalIntent {
    if self.should_enter_long() {
        SignalIntent::Long { confidence: 0.8 }
    } else if self.should_exit() {
        SignalIntent::Flat
    } else {
        SignalIntent::Hold
    }
}
```

### Problem 2: Different Signal Families Have Different Natural Order Types

- **Breakout signals** naturally use **stop orders** (enter on momentum)
- **Mean-reversion signals** naturally use **limit orders** (enter on pullbacks)
- Forcing all signals to use the same order type creates unfair comparisons

**Solution:** `OrderPolicy` trait maps `SignalIntent` to appropriate order types per family.

### Problem 3: Different PM Strategies Have Different Risk Profiles

Comparing strategies with different stop distances is unfair:
- Strategy A: 2% fixed stop → risk $2,000/contract
- Strategy B: 5 ATR stop (ATR=10) → risk $5,000/contract

**Solution:** ATR-risk sizing normalizes position size so all strategies risk the same $ amount.

### Problem 4: No Composition API

Building a complete strategy requires manually wiring 5+ components:
```rust
let signal = DonchianBreakout::new(20);
let order_policy = BreakoutOrderPolicy::new();
let pm = AtrStop::new(2.0, true);
let execution = ExecutionPreset::L1_Cheap;
let sizer = AtrRiskSizer::new(10_000.0, 0.01); // $10k account, 1% risk/trade
// ...now what? How do we run this?
```

**Solution:** `StrategyComposer` assembles and runs the full pipeline.

---

## File Structure (7 new files, ~800 lines)

```
trendlab-core/src/composition/
├── mod.rs                      # Module root + exports
├── intent.rs                   # SignalIntent enum (Long/Short/Flat/Hold)
├── order_policy.rs             # OrderPolicy trait + registry
├── policies/                   # Concrete policies
│   ├── breakout_policy.rs      # Breakout → StopMarket orders
│   ├── meanrev_policy.rs       # Mean-rev → Limit orders
│   └── simple_policy.rs        # Market-on-close fallback
├── sizer.rs                    # Sizer trait + FixedQty/FixedNotional/AtrRisk
├── composer.rs                 # StrategyComposer (assembles + runs pipeline)
└── normalization.rs            # PM normalization for fair signal comparisons
```

---

## Part 1: Signal Intent (Portfolio-Agnostic)

### File: `composition/intent.rs` (~100 lines)

**Design:**
- Signals emit **exposure intent** only (no positions, no order details)
- Supports **confidence scores** (0.0-1.0) for position scaling
- Handles **partial exits** (scale out 50%, etc.)

```rust
/// Exposure intent from a signal (portfolio-agnostic).
#[derive(Debug, Clone, PartialEq)]
pub enum SignalIntent {
    /// Enter or increase long exposure.
    Long {
        /// Confidence score [0.0, 1.0]. Used for position scaling.
        confidence: f64,
    },
    /// Enter or increase short exposure.
    Short {
        /// Confidence score [0.0, 1.0].
        confidence: f64,
    },
    /// Close all exposure (exit completely).
    Flat,
    /// Partial exit (scale out by fraction).
    PartialExit {
        /// Fraction to exit [0.0, 1.0] (e.g., 0.5 = close 50%).
        fraction: f64,
    },
    /// No change (hold current position).
    Hold,
}

impl SignalIntent {
    /// Is this a directional entry signal?
    pub fn is_entry(&self) -> bool {
        matches!(self, Self::Long { .. } | Self::Short { .. })
    }

    /// Is this an exit signal?
    pub fn is_exit(&self) -> bool {
        matches!(self, Self::Flat | Self::PartialExit { .. })
    }

    /// Extract confidence (if applicable).
    pub fn confidence(&self) -> Option<f64> {
        match self {
            Self::Long { confidence } | Self::Short { confidence } => Some(*confidence),
            _ => None,
        }
    }
}
```

**Validation:**
- `confidence` must be in [0.0, 1.0]
- `fraction` must be in [0.0, 1.0]

---

## Part 2: Order Policy (Family-Specific Order Types)

### File: `composition/order_policy.rs` (~120 lines)

**Design:**
- Maps `SignalIntent` to concrete `Order` instances
- Different policies for different signal families
- Receives current bar + portfolio state (but signals don't)

```rust
/// Maps signal intent to concrete orders (family-specific).
pub trait OrderPolicy: Send + Sync {
    /// Generate orders from signal intent.
    fn generate_orders(
        &self,
        intent: &SignalIntent,
        symbol: &str,
        bar: &Bar,
        portfolio: &Portfolio,
    ) -> Vec<Order>;

    /// Policy name (for debugging/logging).
    fn name(&self) -> &str;
}

/// Registry of order policies (similar to PmRegistry).
pub struct OrderPolicyRegistry {
    policies: HashMap<String, Arc<dyn OrderPolicy>>,
}

impl OrderPolicyRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            policies: HashMap::new(),
        };
        // Register defaults
        registry.register("breakout", Arc::new(BreakoutOrderPolicy::default()));
        registry.register("meanrev", Arc::new(MeanRevOrderPolicy::default()));
        registry.register("simple", Arc::new(SimpleOrderPolicy::default()));
        registry
    }

    pub fn register(&mut self, name: &str, policy: Arc<dyn OrderPolicy>) {
        self.policies.insert(name.to_string(), policy);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn OrderPolicy>> {
        self.policies.get(name).cloned()
    }
}
```

---

### File: `composition/policies/breakout_policy.rs` (~100 lines)

**Design:**
- Breakout signals → **StopMarket orders** (enter on momentum)
- Stop price = current close (enter if price continues in signal direction)
- Uses ATR for bracket stops (optional)

```rust
/// Order policy for breakout signals (StopMarket entries).
#[derive(Debug, Clone)]
pub struct BreakoutOrderPolicy {
    /// ATR multiple for bracket stops (None = no brackets).
    pub bracket_atr_mult: Option<f64>,
}

impl OrderPolicy for BreakoutOrderPolicy {
    fn generate_orders(
        &self,
        intent: &SignalIntent,
        symbol: &str,
        bar: &Bar,
        portfolio: &Portfolio,
    ) -> Vec<Order> {
        match intent {
            SignalIntent::Long { .. } => {
                // StopMarket buy at current close
                let stop_price = bar.close;
                let mut order = Order::stop_market(symbol, Side::Buy, stop_price);

                // Optional: add bracket stop
                if let Some(atr_mult) = self.bracket_atr_mult {
                    if let Some(atr) = bar.indicators.get("atr") {
                        let stop_loss = bar.close - atr * atr_mult;
                        order = order.with_stop_loss(stop_loss);
                    }
                }
                vec![order]
            }
            SignalIntent::Short { .. } => {
                let stop_price = bar.close;
                let mut order = Order::stop_market(symbol, Side::SellShort, stop_price);

                if let Some(atr_mult) = self.bracket_atr_mult {
                    if let Some(atr) = bar.indicators.get("atr") {
                        let stop_loss = bar.close + atr * atr_mult;
                        order = order.with_stop_loss(stop_loss);
                    }
                }
                vec![order]
            }
            SignalIntent::Flat => {
                // Close at market
                if let Some(pos) = portfolio.position(symbol) {
                    vec![Order::market_close(symbol, pos.qty)]
                } else {
                    vec![]
                }
            }
            SignalIntent::PartialExit { fraction } => {
                if let Some(pos) = portfolio.position(symbol) {
                    let exit_qty = (pos.qty.abs() as f64 * fraction).round() as i64;
                    if exit_qty > 0 {
                        let exit_qty_signed = if pos.qty > 0 { -exit_qty } else { exit_qty };
                        vec![Order::market(symbol, exit_qty_signed)]
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            SignalIntent::Hold => vec![],
        }
    }

    fn name(&self) -> &str {
        "breakout"
    }
}
```

---

### File: `composition/policies/meanrev_policy.rs` (~100 lines)

**Design:**
- Mean-reversion signals → **Limit orders** (enter on pullbacks)
- Limit price = current close - N% (buy below market)
- Similar bracket logic as breakout policy

```rust
/// Order policy for mean-reversion signals (Limit entries).
#[derive(Debug, Clone)]
pub struct MeanRevOrderPolicy {
    /// Limit offset as % of close (e.g., 0.01 = 1% below close for longs).
    pub limit_offset_pct: f64,
    /// ATR multiple for bracket stops (None = no brackets).
    pub bracket_atr_mult: Option<f64>,
}

impl OrderPolicy for MeanRevOrderPolicy {
    fn generate_orders(
        &self,
        intent: &SignalIntent,
        symbol: &str,
        bar: &Bar,
        portfolio: &Portfolio,
    ) -> Vec<Order> {
        match intent {
            SignalIntent::Long { .. } => {
                // Limit buy below current close
                let limit_price = bar.close * (1.0 - self.limit_offset_pct);
                let mut order = Order::limit(symbol, Side::Buy, limit_price);

                if let Some(atr_mult) = self.bracket_atr_mult {
                    if let Some(atr) = bar.indicators.get("atr") {
                        let stop_loss = bar.close - atr * atr_mult;
                        order = order.with_stop_loss(stop_loss);
                    }
                }
                vec![order]
            }
            SignalIntent::Short { .. } => {
                // Limit sell above current close
                let limit_price = bar.close * (1.0 + self.limit_offset_pct);
                let mut order = Order::limit(symbol, Side::SellShort, limit_price);

                if let Some(atr_mult) = self.bracket_atr_mult {
                    if let Some(atr) = bar.indicators.get("atr") {
                        let stop_loss = bar.close + atr * atr_mult;
                        order = order.with_stop_loss(stop_loss);
                    }
                }
                vec![order]
            }
            SignalIntent::Flat | SignalIntent::PartialExit { .. } => {
                // Same as breakout policy (market exit)
                // ... (duplicate exit logic)
                vec![]
            }
            SignalIntent::Hold => vec![],
        }
    }

    fn name(&self) -> &str {
        "meanrev"
    }
}
```

---

### File: `composition/policies/simple_policy.rs` (~60 lines)

**Design:**
- Fallback policy: all entries/exits at **market-on-close**
- No stop/limit intelligence
- Useful for quick testing or "perfect fill" baseline

```rust
/// Simple policy: all orders at market-on-close.
#[derive(Debug, Clone, Default)]
pub struct SimpleOrderPolicy;

impl OrderPolicy for SimpleOrderPolicy {
    fn generate_orders(
        &self,
        intent: &SignalIntent,
        symbol: &str,
        bar: &Bar,
        portfolio: &Portfolio,
    ) -> Vec<Order> {
        match intent {
            SignalIntent::Long { .. } => {
                vec![Order::market(symbol, 1)] // placeholder qty
            }
            SignalIntent::Short { .. } => {
                vec![Order::market(symbol, -1)]
            }
            SignalIntent::Flat => {
                if let Some(pos) = portfolio.position(symbol) {
                    vec![Order::market_close(symbol, pos.qty)]
                } else {
                    vec![]
                }
            }
            SignalIntent::PartialExit { fraction } => {
                // ... (same as other policies)
                vec![]
            }
            SignalIntent::Hold => vec![],
        }
    }

    fn name(&self) -> &str {
        "simple"
    }
}
```

---

## Part 3: Position Sizing

### File: `composition/sizer.rs` (~150 lines)

**Design:**
- Determines position size from signal intent + portfolio state
- Three sizer types: FixedQty, FixedNotional, AtrRisk
- AtrRisk is the **canonical sizer** for fair comparisons

```rust
/// Determines position size from signal intent.
pub trait Sizer: Send + Sync {
    /// Calculate position size (signed: positive = long, negative = short).
    fn calculate_size(
        &self,
        intent: &SignalIntent,
        symbol: &str,
        bar: &Bar,
        portfolio: &Portfolio,
    ) -> i64;

    fn name(&self) -> &str;
}

/// Fixed quantity (e.g., 100 shares per trade).
#[derive(Debug, Clone)]
pub struct FixedQtySizer {
    pub qty: i64,
}

impl Sizer for FixedQtySizer {
    fn calculate_size(
        &self,
        intent: &SignalIntent,
        _symbol: &str,
        _bar: &Bar,
        _portfolio: &Portfolio,
    ) -> i64 {
        match intent {
            SignalIntent::Long { .. } => self.qty,
            SignalIntent::Short { .. } => -self.qty,
            _ => 0,
        }
    }

    fn name(&self) -> &str {
        "fixed_qty"
    }
}

/// Fixed notional (e.g., $10,000 per trade).
#[derive(Debug, Clone)]
pub struct FixedNotionalSizer {
    pub notional: f64,
}

impl Sizer for FixedNotionalSizer {
    fn calculate_size(
        &self,
        intent: &SignalIntent,
        _symbol: &str,
        bar: &Bar,
        _portfolio: &Portfolio,
    ) -> i64 {
        let qty = (self.notional / bar.close).round() as i64;
        match intent {
            SignalIntent::Long { .. } => qty,
            SignalIntent::Short { .. } => -qty,
            _ => 0,
        }
    }

    fn name(&self) -> &str {
        "fixed_notional"
    }
}

/// ATR-risk sizing (normalize risk per trade).
/// Formula: position_size = (account_value * risk_pct) / (atr * atr_mult)
#[derive(Debug, Clone)]
pub struct AtrRiskSizer {
    /// Account value (e.g., $10,000).
    pub account_value: f64,
    /// Risk per trade as % of account (e.g., 0.01 = 1%).
    pub risk_pct: f64,
    /// ATR multiple for stop distance (e.g., 2.0 = 2 ATR stop).
    pub atr_mult: f64,
}

impl Sizer for AtrRiskSizer {
    fn calculate_size(
        &self,
        intent: &SignalIntent,
        _symbol: &str,
        bar: &Bar,
        _portfolio: &Portfolio,
    ) -> i64 {
        let atr = bar.indicators.get("atr").copied().unwrap_or(1.0);
        let risk_dollars = self.account_value * self.risk_pct;
        let stop_distance = atr * self.atr_mult;

        // Avoid divide-by-zero
        if stop_distance < 0.01 {
            return 0;
        }

        let qty = (risk_dollars / stop_distance).round() as i64;

        match intent {
            SignalIntent::Long { confidence } => (qty as f64 * confidence).round() as i64,
            SignalIntent::Short { confidence } => -(qty as f64 * confidence).round() as i64,
            _ => 0,
        }
    }

    fn name(&self) -> &str {
        "atr_risk"
    }
}
```

**Example:**
- Account = $10,000
- Risk per trade = 1% = $100
- ATR = 5, ATR mult = 2.0 → stop distance = 10 points
- Position size = $100 / 10 = 10 shares

If ATR doubles (volatility expands), position size halves → **constant $ risk**.

---

## Part 4: Strategy Composer (Assembly + Execution)

### File: `composition/composer.rs` (~200 lines)

**Design:**
- Assembles all components into a runnable strategy
- Runs the full pipeline: Signal → OrderPolicy → Sizer → Orders → Execution → PM

```rust
/// Composes a complete strategy from components.
pub struct StrategyComposer {
    /// Signal generator (emits SignalIntent).
    pub signal: Arc<dyn SignalGenerator>,
    /// Order policy (maps intent to orders).
    pub order_policy: Arc<dyn OrderPolicy>,
    /// Position sizer.
    pub sizer: Arc<dyn Sizer>,
    /// Position manager (optional, for stops/targets).
    pub pm: Option<Arc<dyn PositionManager>>,
    /// Execution preset (L1/L2/L3/L4/L5).
    pub execution_preset: ExecutionPreset,
}

impl StrategyComposer {
    pub fn new(
        signal: Arc<dyn SignalGenerator>,
        order_policy: Arc<dyn OrderPolicy>,
        sizer: Arc<dyn Sizer>,
    ) -> Self {
        Self {
            signal,
            order_policy,
            sizer,
            pm: None,
            execution_preset: ExecutionPreset::L1_Cheap,
        }
    }

    /// Add position manager.
    pub fn with_pm(mut self, pm: Arc<dyn PositionManager>) -> Self {
        self.pm = Some(pm);
        self
    }

    /// Set execution preset.
    pub fn with_execution(mut self, preset: ExecutionPreset) -> Self {
        self.execution_preset = preset;
        self
    }

    /// Run strategy on a bar.
    /// Returns orders to submit to the order book.
    pub fn process_bar(
        &self,
        symbol: &str,
        bar: &Bar,
        portfolio: &Portfolio,
    ) -> Vec<Order> {
        let mut orders = Vec::new();

        // Step 1: Generate signal intent
        let intent = self.signal.generate_signal(bar);

        // Step 2: Map intent to orders (via order policy)
        let mut policy_orders = self.order_policy.generate_orders(
            &intent,
            symbol,
            bar,
            portfolio,
        );

        // Step 3: Apply position sizing
        for order in &mut policy_orders {
            if order.qty == 1 || order.qty == -1 {
                // Placeholder qty → replace with sizer result
                let sized_qty = self.sizer.calculate_size(&intent, symbol, bar, portfolio);
                order.qty = sized_qty;
            }
        }

        orders.extend(policy_orders);

        // Step 4: PM maintenance orders (stops/targets)
        if let Some(pm) = &self.pm {
            if let Some(pos) = portfolio.position(symbol) {
                let pm_intents = pm.update(pos, bar);
                for pm_intent in pm_intents {
                    match pm_intent {
                        OrderIntent::New(order) => orders.push(order),
                        OrderIntent::Cancel(order_id) => {
                            // Send cancel to order book
                            // (handled by runner, not composer)
                        }
                        OrderIntent::CancelReplace { old_id, new_order } => {
                            // Cancel + new order
                            orders.push(new_order);
                        }
                    }
                }
            }
        }

        orders
    }

    /// Human-readable strategy name.
    pub fn name(&self) -> String {
        format!(
            "{}+{}+{}{}",
            self.signal.name(),
            self.order_policy.name(),
            self.sizer.name(),
            self.pm.as_ref().map_or("".to_string(), |pm| format!("+{}", pm.name())),
        )
    }
}
```

**Example composition:**
```rust
let composer = StrategyComposer::new(
    Arc::new(DonchianBreakout::new(20)),
    Arc::new(BreakoutOrderPolicy::default()),
    Arc::new(AtrRiskSizer::new(10_000.0, 0.01, 2.0)),
)
.with_pm(Arc::new(AtrStop::new(2.0, true)))
.with_execution(ExecutionPreset::L2_WalkForward);

let orders = composer.process_bar("AAPL", &bar, &portfolio);
```

---

## Part 5: PM Normalization (Fair Signal Comparisons)

### File: `composition/normalization.rs` (~70 lines)

**Design:**
- When comparing signals, fix PM + execution to isolate signal quality
- Provides **canonical presets** for fair benchmarking

```rust
/// Canonical PM/execution presets for signal normalization.
#[derive(Debug, Clone, Copy)]
pub enum NormalizedPreset {
    /// No PM, L1 execution (cheapest, deterministic).
    Baseline,
    /// Fixed 2% stop, L1 execution.
    FixedStop2Pct,
    /// 2 ATR stop with ratchet, L1 execution.
    AtrStop2x,
    /// Chandelier exit (anti-stickiness), L2 execution.
    Chandelier,
}

impl NormalizedPreset {
    /// Get PM + execution preset.
    pub fn components(&self) -> (Option<Arc<dyn PositionManager>>, ExecutionPreset) {
        match self {
            Self::Baseline => (None, ExecutionPreset::L1_Cheap),
            Self::FixedStop2Pct => (
                Some(Arc::new(FixedPercentStop::new(0.02))),
                ExecutionPreset::L1_Cheap,
            ),
            Self::AtrStop2x => (
                Some(Arc::new(AtrStop::new(2.0, true))),
                ExecutionPreset::L1_Cheap,
            ),
            Self::Chandelier => (
                Some(Arc::new(ChandelierExit::new(20, 2.0))),
                ExecutionPreset::L2_WalkForward,
            ),
        }
    }

    /// Apply preset to a composer.
    pub fn apply(&self, composer: StrategyComposer) -> StrategyComposer {
        let (pm, exec) = self.components();
        let mut composer = composer.with_execution(exec);
        if let Some(pm) = pm {
            composer = composer.with_pm(pm);
        }
        composer
    }
}
```

**Usage:**
```rust
// Compare 3 signals with normalized PM
let signals = vec![
    Arc::new(DonchianBreakout::new(20)),
    Arc::new(MovingAverageCross::new(50, 200)),
    Arc::new(RSI::new(14, 30.0, 70.0)),
];

for signal in signals {
    let composer = StrategyComposer::new(
        signal,
        Arc::new(BreakoutOrderPolicy::default()),
        Arc::new(AtrRiskSizer::new(10_000.0, 0.01, 2.0)),
    );

    let normalized = NormalizedPreset::AtrStop2x.apply(composer);

    // Run backtest with normalized...
}
```

---

## BDD Scenarios (6 features, 18+ scenarios)

### Feature 1: Signal Intent (Portfolio-Agnostic)

**File:** `tests/bdd_composition_intent.feature`

```gherkin
Feature: Signal Intent (Portfolio-Agnostic)
  Signals emit exposure intent only (no position size or order details).

  Scenario: Long signal with confidence
    Given a signal emits Long intent with confidence 0.8
    Then the intent should be an entry signal
    And the confidence should be 0.8

  Scenario: Flat signal (exit all)
    Given a signal emits Flat intent
    Then the intent should be an exit signal
    And confidence should be None

  Scenario: Partial exit (scale out 50%)
    Given a signal emits PartialExit intent with fraction 0.5
    Then the intent should be an exit signal
    And the exit fraction should be 0.5

  Scenario: Hold (no change)
    Given a signal emits Hold intent
    Then the intent should not be an entry signal
    And the intent should not be an exit signal
```

**Step count:** 4 scenarios, ~15 steps

---

### Feature 2: Order Policy (Family-Specific)

**File:** `tests/bdd_composition_order_policy.feature`

```gherkin
Feature: Order Policy (Family-Specific Order Types)
  Different signal families map to different order types.

  Scenario: Breakout policy uses StopMarket orders
    Given a BreakoutOrderPolicy
    And a Long signal intent
    And current bar close is 100.0
    When I generate orders
    Then I should receive 1 order
    And the order type should be StopMarket
    And the stop price should be 100.0

  Scenario: Mean-reversion policy uses Limit orders
    Given a MeanRevOrderPolicy with 1% offset
    And a Long signal intent
    And current bar close is 100.0
    When I generate orders
    Then I should receive 1 order
    And the order type should be Limit
    And the limit price should be 99.0

  Scenario: Simple policy uses Market orders
    Given a SimpleOrderPolicy
    And a Long signal intent
    When I generate orders
    Then I should receive 1 order
    And the order type should be Market
```

**Step count:** 3 scenarios, ~12 steps

---

### Feature 3: Position Sizing

**File:** `tests/bdd_composition_sizer.feature`

```gherkin
Feature: Position Sizing
  Sizers determine position size from intent + portfolio state.

  Scenario: FixedQtySizer returns constant quantity
    Given a FixedQtySizer with qty 100
    And a Long signal intent
    When I calculate size
    Then the size should be 100

  Scenario: FixedNotionalSizer scales by price
    Given a FixedNotionalSizer with $10,000 notional
    And current bar close is 100.0
    When I calculate size for Long intent
    Then the size should be 100
    When current bar close is 50.0
    Then the size should be 200

  Scenario: AtrRiskSizer normalizes risk
    Given an AtrRiskSizer with $10,000 account, 1% risk, 2x ATR
    And ATR is 5.0
    When I calculate size for Long intent with confidence 1.0
    Then risk per trade should be $100
    And stop distance should be 10 points
    And position size should be 10 shares

  Scenario: AtrRiskSizer scales by confidence
    Given an AtrRiskSizer with $10,000 account, 1% risk, 2x ATR
    And ATR is 5.0
    And base position size would be 10 shares
    When I calculate size for Long intent with confidence 0.5
    Then position size should be 5 shares
```

**Step count:** 4 scenarios, ~18 steps

---

### Feature 4: Strategy Composition

**File:** `tests/bdd_composition_composer.feature`

```gherkin
Feature: Strategy Composition
  Composer assembles signal + policy + sizer + PM into a runnable strategy.

  Scenario: Full pipeline (signal → orders)
    Given a DonchianBreakout signal (20-day)
    And a BreakoutOrderPolicy
    And an AtrRiskSizer ($10k account, 1% risk, 2x ATR)
    And current bar has close=100, ATR=5
    When the signal emits Long intent
    And I process the bar
    Then I should receive 1 order
    And the order type should be StopMarket
    And the stop price should be 100.0
    And the order qty should be 10 shares

  Scenario: PM maintenance orders added
    Given a composed strategy with AtrStop PM (2x ATR)
    And an open long position at entry_price=100
    And current bar has close=110, ATR=5
    When I process the bar
    Then I should receive PM maintenance orders
    And the stop should have tightened to 100 (from initial 90)
```

**Step count:** 2 scenarios, ~10 steps

---

### Feature 5: PM Normalization

**File:** `tests/bdd_composition_normalization.feature`

```gherkin
Feature: PM Normalization (Fair Signal Comparisons)
  Normalize PM + execution to isolate signal quality.

  Scenario: Baseline preset (no PM, L1 execution)
    Given a composer with DonchianBreakout signal
    When I apply NormalizedPreset::Baseline
    Then PM should be None
    And execution preset should be L1_Cheap

  Scenario: AtrStop2x preset
    Given a composer with any signal
    When I apply NormalizedPreset::AtrStop2x
    Then PM should be AtrStop with 2.0 multiplier and ratchet enabled
    And execution preset should be L1_Cheap

  Scenario: Comparing 3 signals with normalized PM
    Given 3 signals: DonchianBreakout, MA_Cross, RSI
    And NormalizedPreset::AtrStop2x
    When I run backtests on same data
    Then PM and execution should be identical across all 3
    And performance differences should reflect signal quality only
```

**Step count:** 3 scenarios, ~12 steps

---

### Feature 6: Confidence Scaling

**File:** `tests/bdd_composition_confidence.feature`

```gherkin
Feature: Confidence Scaling
  Position size scales by signal confidence.

  Scenario: Full confidence (1.0)
    Given an AtrRiskSizer with base size 10 shares
    And a Long signal with confidence 1.0
    When I calculate size
    Then position size should be 10 shares

  Scenario: Half confidence (0.5)
    Given an AtrRiskSizer with base size 10 shares
    And a Long signal with confidence 0.5
    When I calculate size
    Then position size should be 5 shares

  Scenario: Zero confidence (0.0)
    Given an AtrRiskSizer with base size 10 shares
    And a Long signal with confidence 0.0
    When I calculate size
    Then position size should be 0 shares
```

**Step count:** 3 scenarios, ~9 steps

---

## Unit Tests (~30 tests)

### Test Coverage:

1. **SignalIntent** (5 tests):
   - `test_long_intent_is_entry`
   - `test_flat_intent_is_exit`
   - `test_partial_exit_fraction`
   - `test_hold_intent_is_neither`
   - `test_confidence_extraction`

2. **OrderPolicy** (8 tests):
   - `test_breakout_policy_long_stop_market`
   - `test_breakout_policy_short_stop_market`
   - `test_meanrev_policy_long_limit`
   - `test_meanrev_policy_limit_offset`
   - `test_simple_policy_market_orders`
   - `test_policy_registry_defaults`
   - `test_policy_registry_custom`
   - `test_bracket_orders_with_atr`

3. **Sizer** (10 tests):
   - `test_fixed_qty_sizer`
   - `test_fixed_notional_sizer_scaling`
   - `test_atr_risk_sizer_calculation`
   - `test_atr_risk_sizer_confidence_scaling`
   - `test_atr_risk_sizer_zero_atr_guard`
   - `test_atr_risk_sizer_volatility_normalization`
   - `test_sizer_short_position`
   - `test_sizer_partial_exit`
   - `test_sizer_flat_intent`
   - `test_sizer_hold_intent`

4. **Composer** (7 tests):
   - `test_composer_full_pipeline`
   - `test_composer_with_pm`
   - `test_composer_without_pm`
   - `test_composer_name_generation`
   - `test_composer_multiple_orders`
   - `test_composer_pm_maintenance_orders`
   - `test_composer_order_sizing`

5. **Normalization** (5 tests):
   - `test_normalized_preset_baseline`
   - `test_normalized_preset_fixed_stop`
   - `test_normalized_preset_atr_stop`
   - `test_normalized_preset_chandelier`
   - `test_normalized_preset_apply`

---

## Verification Commands

### 1. Create Module Structure

```bash
# Create composition module
mkdir -p trendlab-core/src/composition/policies
cd trendlab-core/src

# Create files
touch composition/mod.rs
touch composition/intent.rs
touch composition/order_policy.rs
touch composition/sizer.rs
touch composition/composer.rs
touch composition/normalization.rs
touch composition/policies/breakout_policy.rs
touch composition/policies/meanrev_policy.rs
touch composition/policies/simple_policy.rs

# Update lib.rs
echo "pub mod composition;" >> lib.rs
```

### 2. Run Unit Tests

```bash
cargo test -p trendlab-core composition:: --lib

# Expected output:
# test composition::intent::test_long_intent_is_entry ... ok
# test composition::intent::test_flat_intent_is_exit ... ok
# test composition::intent::test_partial_exit_fraction ... ok
# test composition::intent::test_hold_intent_is_neither ... ok
# test composition::intent::test_confidence_extraction ... ok
# test composition::order_policy::test_breakout_policy_long_stop_market ... ok
# test composition::order_policy::test_meanrev_policy_long_limit ... ok
# test composition::order_policy::test_simple_policy_market_orders ... ok
# test composition::sizer::test_fixed_qty_sizer ... ok
# test composition::sizer::test_atr_risk_sizer_calculation ... ok
# test composition::sizer::test_atr_risk_sizer_confidence_scaling ... ok
# test composition::composer::test_composer_full_pipeline ... ok
# test composition::composer::test_composer_with_pm ... ok
# test composition::normalization::test_normalized_preset_atr_stop ... ok
# ... (30 total tests)
```

### 3. Run BDD Tests

```bash
cargo test --test bdd_composition_intent
cargo test --test bdd_composition_order_policy
cargo test --test bdd_composition_sizer
cargo test --test bdd_composition_composer
cargo test --test bdd_composition_normalization
cargo test --test bdd_composition_confidence

# Expected output (6 features, 19 scenarios):
# Feature: Signal Intent (Portfolio-Agnostic) ... 4 scenarios (15 steps) ✓
# Feature: Order Policy (Family-Specific) ... 3 scenarios (12 steps) ✓
# Feature: Position Sizing ... 4 scenarios (18 steps) ✓
# Feature: Strategy Composition ... 2 scenarios (10 steps) ✓
# Feature: PM Normalization ... 3 scenarios (12 steps) ✓
# Feature: Confidence Scaling ... 3 scenarios (9 steps) ✓
# Total: 19 scenarios, 76 steps, 0 failures
```

### 4. Clippy Verification

```bash
cargo clippy -p trendlab-core -- -D warnings

# Expected: no warnings
```

### 5. Integration Test (Full Pipeline)

```bash
cargo test -p trendlab-core test_full_composition_pipeline --lib

# Expected:
# Composer: DonchianBreakout+breakout+atr_risk+AtrStop
# Bar 0: Long signal @ 100.0, qty=10, stop=90
# Bar 1: Price 110, stop tightens to 100
# Bar 2: Price 105, stop stays at 100 (ratchet)
# Bar 3: Price 98, filled at 100 (+10 points)
# test test_full_composition_pipeline ... ok
```

---

## Completion Criteria (20 items)

### Architecture & Separation (5 items)

- [ ] `SignalIntent` enum defined (Long/Short/Flat/PartialExit/Hold)
- [ ] Signals emit intent only (no portfolio state access)
- [ ] `OrderPolicy` trait defined (maps intent → orders)
- [ ] `Sizer` trait defined (determines position size)
- [ ] `StrategyComposer` assembles all components

### Order Policies (4 items)

- [ ] `BreakoutOrderPolicy` (StopMarket entries)
- [ ] `MeanRevOrderPolicy` (Limit entries)
- [ ] `SimpleOrderPolicy` (Market-on-close fallback)
- [ ] `OrderPolicyRegistry` with defaults

### Position Sizing (4 items)

- [ ] `FixedQtySizer` implemented
- [ ] `FixedNotionalSizer` implemented
- [ ] `AtrRiskSizer` implemented (risk normalization)
- [ ] Confidence scaling works (0.5 confidence → 50% size)

### Composition & Execution (4 items)

- [ ] `StrategyComposer::process_bar` runs full pipeline
- [ ] PM maintenance orders included in output
- [ ] Strategy name auto-generated from components
- [ ] Composer handles zero-qty edge cases (no divide-by-zero)

### Normalization (3 items)

- [ ] `NormalizedPreset` enum defined (4 presets)
- [ ] Baseline/FixedStop2Pct/AtrStop2x/Chandelier presets work
- [ ] Presets can be applied to any composer

### Testing (5 items)

- [ ] 30+ unit tests pass
- [ ] 6 BDD features, 19+ scenarios pass
- [ ] Integration test: full pipeline (signal → PM → exit) works
- [ ] Clippy clean (no warnings)
- [ ] Golden regression test for ATR-risk sizing formula

---

## Example Flows

### Flow 1: Full Pipeline (Breakout Signal → StopMarket Order → ATR-Risk Sizing → AtrStop PM)

**Setup:**
- Signal: DonchianBreakout(20)
- OrderPolicy: BreakoutOrderPolicy (StopMarket entries)
- Sizer: AtrRiskSizer ($10k account, 1% risk, 2x ATR)
- PM: AtrStop(2.0, ratchet=true)

**Execution:**

**Bar 0: Entry signal**
```
Price: 100.0, ATR: 5.0
Signal: Long (confidence 1.0)
OrderPolicy: StopMarket buy @ 100.0
Sizer: risk=$100, stop_dist=10 → qty=10
PM: No position yet → no maintenance orders
→ Submit: StopMarket buy 10 @ 100.0
```

**Bar 1: Fill + PM initialization**
```
Price: 102.0 (gaps up, stops us in at 100.0)
Fill: Bought 10 @ 100.0
PM: Initialize stop at 90.0 (100 - 2*5)
→ Submit: StopMarket sell 10 @ 90.0 (stop-loss)
```

**Bar 2: Price rises, stop tightens**
```
Price: 110.0, ATR: 5.0
PM: Proposed stop = 100.0 (110 - 2*5)
Ratchet: 100 > 90 → tighten ✓
→ Cancel old stop @ 90, submit new stop @ 100
```

**Bar 3: ATR expands (volatility spike)**
```
Price: 112.0, ATR: 10.0 (doubled!)
PM: Proposed stop = 92.0 (112 - 2*10)
Ratchet: 92 < 100 → BLOCKED (can't loosen)
→ Stop stays at 100
```

**Bar 4: Price falls, hits stop**
```
Price: 98.0 (intraday low hits 100.0)
Fill: Sold 10 @ 100.0 (stop triggered)
P&L: +0 points (breakeven, but ratchet prevented -8 point loss!)
```

**Key insight:** Without ratchet, stop would have loosened to 92, and we'd have lost -8 points instead of breaking even.

---

### Flow 2: Mean-Reversion Signal → Limit Order → Fixed Notional Sizing

**Setup:**
- Signal: RSI(14, oversold=30)
- OrderPolicy: MeanRevOrderPolicy (1% limit offset)
- Sizer: FixedNotionalSizer ($5,000/trade)
- PM: None (exit on signal flip)

**Execution:**

**Bar 0: Oversold signal**
```
Price: 100.0, RSI: 25 (oversold)
Signal: Long (confidence 0.8)
OrderPolicy: Limit buy @ 99.0 (100 * 0.99)
Sizer: $5,000 / 99 = 50 shares (confidence → 40 shares)
→ Submit: Limit buy 40 @ 99.0
```

**Bar 1: Limit fills**
```
Price: 98.0 (low=97, intrabar touches 99)
Fill: Bought 40 @ 99.0 (limit filled on pullback)
```

**Bar 2: RSI normalizes**
```
Price: 102.0, RSI: 45 (neutral)
Signal: Hold
→ No action
```

**Bar 3: RSI overbought**
```
Price: 105.0, RSI: 75 (overbought)
Signal: Flat (exit)
OrderPolicy: Market close
→ Submit: Market sell 40 @ close
Fill: 40 @ 105.0
P&L: +6 points/share = +$240
```

---

### Flow 3: Normalized Signal Comparison

**Scenario:** Compare 3 signals on identical PM/execution to isolate signal quality.

**Setup:**
- Signals: DonchianBreakout(20), MovingAverageCross(50,200), RSI(14)
- Normalized preset: AtrStop2x (2 ATR stop + ratchet, L1 execution)
- Sizer: AtrRiskSizer ($10k, 1% risk)

**Results (on same 252-bar dataset):**

| Signal               | Sharpe | Max DD | Trades | Win% | Avg Win/Loss |
|----------------------|--------|--------|--------|------|--------------|
| DonchianBreakout(20) | 1.2    | -15%   | 12     | 58%  | 2.1:1        |
| MA_Cross(50,200)     | 0.8    | -22%   | 3      | 67%  | 1.8:1        |
| RSI(14)              | 0.4    | -28%   | 45     | 42%  | 1.1:1        |

**Conclusion:** DonchianBreakout wins on this dataset. Comparison is **fair** because:
- All use same PM (AtrStop2x)
- All use same execution (L1_Cheap)
- All use same sizer (ATR-risk normalized)
- Differences reflect signal quality only

---

## Golden Regression Test (ATR-Risk Sizer Formula)

**Test:** `test_atr_risk_sizer_golden`

**Data:**
```rust
let account = 10_000.0;
let risk_pct = 0.01; // 1%
let atr_mult = 2.0;
let atr = 5.0;
let close = 100.0;
let confidence = 1.0;

let sizer = AtrRiskSizer::new(account, risk_pct, atr_mult);
let intent = SignalIntent::Long { confidence };
let bar = Bar { close, indicators: [("atr", 5.0)].into(), .. };

let qty = sizer.calculate_size(&intent, "AAPL", &bar, &portfolio);

assert_eq!(qty, 10); // Golden value
```

**Formula verification:**
```
risk_dollars = 10,000 * 0.01 = 100
stop_distance = 5.0 * 2.0 = 10
qty = 100 / 10 = 10 shares ✓
```

**Regression guard:** If formula changes, this test should break.

---

## Why M7 Matters

M7 is the **composition layer** that makes TrendLab v3 a **fair comparison engine**.

Traditional backtesting platforms suffer from:

1. **Apples-to-oranges comparisons**: Different signals have different natural order types, PM strategies, and risk profiles → comparing raw Sharpe ratios is meaningless.

2. **Coupled components**: Signals know about portfolio state, PM is baked into signal logic, execution is hardcoded → impossible to swap components.

3. **Unfair risk allocation**: Strategy A risks 2% per trade, Strategy B risks 5% → of course B has higher returns (and higher risk!).

M7 solves all three:

✅ **Normalized comparisons** via `NormalizedPreset` (fix PM/execution, vary signals)
✅ **Decoupled components** via trait boundaries (Signal → OrderPolicy → Sizer → PM)
✅ **Fair risk allocation** via `AtrRiskSizer` (constant $ risk per trade)

The `StrategyComposer` is the **rosetta stone** of TrendLab: it translates between all component languages (SignalIntent → Orders → Fills → Positions) while maintaining strict boundaries.

---

## Next Steps

M8 (Walk-Forward + OOS) is next. This milestone covers:

- Train/test splits (in-sample vs out-of-sample)
- Rolling walk-forward windows
- Overfitting detection (IS Sharpe >> OOS Sharpe)
- Promotion ladder L2 filter (cheap candidates → expensive simulation)

This milestone is **critical for robustness**: it prevents curve-fitting and ensures strategies generalize to unseen data.

**Estimated LOC:** ~600 lines
**Complexity:** Medium (mostly accounting + validation logic)

---

## M7 Progress Tracker

```
[ᗧ··············] 0%   (specification complete, implementation pending)
```

**Specification complete!** Ready to implement.
