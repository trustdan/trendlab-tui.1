# Extension Guide

How to add new signals, position managers, execution models, and signal filters to TrendLab.

All four component types follow the same pattern:

1. Implement the trait in `trendlab-core/src/components/<type>/`
2. Register it in the factory (`trendlab-core/src/components/factory.rs`)
3. Wire up its required indicators in `required_indicators()`
4. Add it to the YOLO sampler pool (`trendlab-core/src/components/sampler.rs`)
5. Write tests (unit + look-ahead + NaN injection)

---

## Adding a New Signal

Signals live in `trendlab-core/src/components/signal/`. Each signal is a struct that implements the `SignalGenerator` trait.

### Step 1: Implement the trait

Create a new file or add to an existing file in `signal/`:

```rust
use crate::components::signal::{SignalGenerator, SignalEvent, SignalDirection};
use crate::domain::Bar;
use crate::indicators::IndicatorValues;

pub struct MySignal {
    lookback: usize,
}

impl MySignal {
    pub fn new(lookback: usize) -> Self {
        Self { lookback }
    }
}

impl SignalGenerator for MySignal {
    fn name(&self) -> &str {
        "my_signal"
    }

    fn warmup_bars(&self) -> usize {
        self.lookback
    }

    fn evaluate(
        &self,
        bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> Option<SignalEvent> {
        if bar_index < self.warmup_bars() {
            return None;
        }

        // Read precomputed indicator values
        let sma = indicators.get("sma_20", bar_index)?;
        let close = bars[bar_index].close;

        // Signal logic: only use data up to bar_index (no look-ahead)
        if close > sma {
            Some(SignalEvent {
                bar_index,
                direction: SignalDirection::Long,
                strength: 1.0,
            })
        } else {
            None
        }
    }
}
```

**Key rules:**
- `evaluate()` must only use `bars[0..=bar_index]` — never look ahead
- Return `None` if `bar_index < warmup_bars()`
- Return `None` if any required indicator value is `NaN`
- The signal must be `Send + Sync`

### Step 2: Export from signal module

In `signal/mod.rs`, add:

```rust
mod my_signal;
pub use my_signal::MySignal;
```

### Step 3: Register in the factory

In `factory.rs`, add a match arm to `create_signal()`:

```rust
"my_signal" => {
    let lookback = param_usize(config, "lookback", 20);
    Ok(Box::new(MySignal::new(lookback)))
}
```

### Step 4: Wire up indicators

In `required_indicators()`, add a match arm for the signal's indicator needs:

```rust
"my_signal" => {
    let period = param_usize(signal, "lookback", 20);
    indicators.insert(Box::new(Sma::new(period)));
}
```

The factory deduplicates indicators automatically — if another component already needs `sma_20`, only one instance is created.

### Step 5: Add to the YOLO sampler

In `sampler.rs`, add the signal type and its parameter ranges to the sampling pool so YOLO mode can discover it.

### Step 6: Write tests

Required tests for every signal:

1. **Unit test** — verify the signal fires on known data
2. **Look-ahead contamination test** — compute on bars 1–100 vs 1–200, assert bars 1–100 are identical
3. **NaN injection test** — inject NaN into a bar, assert no signal fires on that bar

---

## Adding a New Position Manager

Position managers live in `trendlab-core/src/components/pm/`. They implement the `PositionManager` trait.

### The trait

```rust
pub trait PositionManager: Send + Sync {
    fn name(&self) -> &str;

    fn on_bar(
        &self,
        position: &Position,
        bar: &Bar,
        bar_index: usize,
        market_status: MarketStatus,
        indicators: &IndicatorValues,
    ) -> OrderIntent;
}
```

`on_bar()` is called every bar for each open position. It returns an `OrderIntent`:

- `OrderIntent::Hold` — no action
- `OrderIntent::UpdateStop { price }` — move the stop (ratchet enforced: can only tighten)
- `OrderIntent::ExitMarket` — exit at next bar's open

### Implementation pattern

```rust
pub struct MyPm {
    trail_pct: f64,
}

impl PositionManager for MyPm {
    fn name(&self) -> &str { "my_pm" }

    fn on_bar(
        &self,
        position: &Position,
        bar: &Bar,
        bar_index: usize,
        _market_status: MarketStatus,
        _indicators: &IndicatorValues,
    ) -> OrderIntent {
        let highest = position.highest_close.unwrap_or(bar.close);
        let stop = highest * (1.0 - self.trail_pct);
        OrderIntent::UpdateStop { price: stop }
    }
}
```

**Key rules:**
- Stops must obey the ratchet invariant (the engine enforces this, but don't propose loosening)
- On void bars (`MarketStatus::Closed`), increment time counters but don't emit price-dependent orders
- Register in `create_pm()` in `factory.rs`
- Wire indicators in `required_indicators()` if needed

---

## Adding a New Execution Model

Execution models live in `trendlab-core/src/components/execution/`. They implement:

```rust
pub trait ExecutionModel: Send + Sync {
    fn name(&self) -> &str;
    fn entry_order_type(&self, signal: &SignalEvent, bar: &Bar, instrument: &Instrument) -> OrderType;
    fn path_policy(&self) -> PathPolicy;
    fn gap_fill_policy(&self) -> GapFillPolicy;
    fn cost_model(&self) -> &CostModel;
}
```

Key methods:
- `entry_order_type()` — decides what kind of order to place (MOO, Stop, Limit, etc.)
- `path_policy()` — how to resolve intrabar ambiguity (WorstCase, BestCase, or PathMC)
- `gap_fill_policy()` — what happens when price gaps through a stop (fill at open vs trigger)
- `cost_model()` — slippage + commission parameters

Use `ExecutionPreset` to construct standard cost models:

| Preset | Slippage | Commission | Path Policy |
|--------|----------|------------|-------------|
| Frictionless | 0 bps | 0 bps | Deterministic |
| Realistic | 5 bps | 5 bps | WorstCase |
| Hostile | 20 bps | 15 bps | WorstCase |
| Optimistic | 2 bps | 2 bps | BestCase |

---

## Adding a New Signal Filter

Signal filters live in `trendlab-core/src/components/filter/`. They implement:

```rust
pub trait SignalFilter: Send + Sync {
    fn name(&self) -> &str;
    fn evaluate(
        &self,
        signal: &SignalEvent,
        bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> SignalEvaluation;
}
```

A `SignalEvaluation` contains:
- `verdict: FilterVerdict` — `Pass` or `Block`
- Filter-specific state for diagnostics

Example:

```rust
pub struct MyFilter {
    threshold: f64,
}

impl SignalFilter for MyFilter {
    fn name(&self) -> &str { "my_filter" }

    fn evaluate(
        &self,
        _signal: &SignalEvent,
        _bars: &[Bar],
        bar_index: usize,
        indicators: &IndicatorValues,
    ) -> SignalEvaluation {
        let value = indicators.get("my_indicator", bar_index).unwrap_or(f64::NAN);
        if value.is_nan() || value < self.threshold {
            SignalEvaluation::blocked("my_filter", value)
        } else {
            SignalEvaluation::passed("my_filter", value)
        }
    }
}
```

Register in `create_filter()` in `factory.rs`, wire indicators in `required_indicators()`.

---

## File Checklist

For any new component, touch these files:

| File | Action |
|------|--------|
| `components/<type>/my_component.rs` | Implement the trait |
| `components/<type>/mod.rs` | `pub use` the new struct |
| `components/factory.rs` | Add match arm in `create_*()` |
| `components/factory.rs` | Add indicator wiring in `required_indicators()` |
| `components/sampler.rs` | Add to YOLO sampling pool |
| `tests/` | Unit test, look-ahead test, NaN test |

---

## Testing Commands

```bash
# Run all tests
cargo test --workspace

# Run tests for core only
cargo test -p trendlab-core

# Run a specific test
cargo test my_signal_fires_on_breakout -- --nocapture

# Run property tests
cargo test -p trendlab-core -- property

# Run benchmarks
cargo bench -p trendlab-core
```
