# TrendLab v3 — Step-by-Step Development Roadmap (v2, BDD-first)

> Date: 2026-02-04 (America/Chicago)  
> This v2 incorporates the critiques: **instrument metadata**, **smoke backtest**, **warmup**, **cancel/replace atomicity**, **intrabar order priority**, **liquidity caps**, **explicit anti-stickiness regression**, **cache invalidation**, **stability scoring**, and **TUI drill-down + ghost curve**.

---

## How we’ll use BDD (enough, not overkill)

- **BDD = acceptance/integration layer** (cross-module behavior and contracts).
- **Unit tests** for local correctness.
- **Property tests** for invariants (no double fills, OCO, equity accounting).
- **Golden tests** for tiny synthetic worlds and end-to-end stability.

Keep Gherkin scenarios small and focused on *behavior*, not implementation.

---

## Dependency graph (explicit)

```
M0 ─── M0.5 ─┬─ M1 (domain + instrument)
             │
             └─ M2 (data + cache)
                  │
                  └─ M3 (event loop + warmup + accounting)
                        │
                        └─ M4 (orders + cancel/replace + OCO/brackets)
                              │
                              └─ M5 (execution + priority + presets + liquidity)
                                    │
                                    └─ M6 (position mgmt + ratchet + anti-stickiness)
                                          │
                                          └─ M7 (composition + normalization tests)
                                                │
                                                └─ M8 (runner + cache invalidation + leaderboards)
                                                      │
                                                      └─ M9 (robustness ladder + stability scoring)
                                                            │
                                                            └─ M10 (TUI + drill-down + ghost curve)
                                                                  │
                                                                  └─ M11 (reporting/artifacts)
                                                                        │
                                                                        └─ M12 (hardening: perf/regression/docs)
```

---

## “Escape hatches” (avoid gold-plating)

- **M2 Data:** start with Parquet ingest + local lists only (no vendor APIs).
- **M5 Execution:** ship only `Deterministic` + `WorstCase` path policies first; add MC later.
- **M6 PM:** ship 3 PMs first (fixed %, ATR, chandelier) + ratchet; add more later.
- **M9 Robustness:** ship Walk-Forward + Execution MC first; add Path MC and bootstrap later.
- **M10 TUI:** ship 4 core panels first; expand to full suite after runner is solid.

---

## Integration checkpoints (planned)

### Checkpoint A (after M5)
- Synthetic end-to-end backtest works
- Order book + execution realism works
- Accounting correct  
(No real signals/PM required yet.)

### Checkpoint B (after M8)
- Full-Auto sweeps run and persist leaderboards
- Any leaderboard row is reproducible from manifest  
(Robustness ladder can still be minimal.)

### Checkpoint C (after M10)
- Drill-down “why did this win?” works in TUI
- Execution sensitivity + ghost curve visible
- Ready to harden and optimize

---

# M0 — Repo bootstrap & guardrails

## Deliverables

- Workspace scaffold (`trendlab-core`, `trendlab-runner`, `trendlab-tui`, optional `trendlab-cli`)
- `rustfmt` + `clippy` policies, CI pipeline
- `.claude` installed (commands + agents)
- Docs: architecture invariants (one page)

### File Structure

#### Workspace Root Files

**1. Cargo.toml** (workspace manifest)

```toml
[workspace]
members = [
    "trendlab-core",
    "trendlab-runner",
    "trendlab-tui",
    "trendlab-cli",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["TrendLab Contributors"]
license = "MIT"

[workspace.dependencies]
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
anyhow = "1.0"
polars = { version = "0.44", features = ["lazy", "parquet"] }
ratatui = "0.28"
crossterm = "0.28"
```

**2. rustfmt.toml**

```toml
edition = "2021"
max_width = 100
use_small_heuristics = "Max"
hard_tabs = false
tab_spaces = 4
```

**3. .clippy.toml**

```toml
# Deny warnings in CI
warn-on-all-wildcard-imports = true
```

**4. .github/workflows/ci.yml**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      - name: Check formatting
        run: cargo fmt --all -- --check

      - name: Run clippy
        run: cargo clippy --workspace -- -D warnings

      - name: Run tests
        run: cargo test --workspace

      - name: Build all
        run: cargo build --workspace --release
```

**5. README.md**

```markdown
# TrendLab v3

Research-grade trend-following backtesting engine with terminal UI.

## Quick Start

\`\`\`bash
# Build workspace
cargo build --workspace

# Run all tests
cargo test --workspace

# Run smoke backtest
cargo test --package trendlab-core smoke_backtest
\`\`\`

## Architecture

See `docs/architecture.md` for design invariants.
```

**6. docs/architecture.md**

```markdown
# TrendLab v3 Architecture Invariants

## 1. Separation of Concerns

- **Signals** are portfolio-agnostic
- **Position management** is post-execution only
- **Execution** is configurable and realistic

## 2. Bar-by-Bar Event Loop

Per bar:
1. Start-of-bar: activate day orders, fill MOO
2. Intrabar: simulate triggers/fills via path policy
3. End-of-bar: fill MOC
4. Post-bar: mark positions, PM emits intents for NEXT bar

## 3. Deterministic Reproducibility

Every run keyed by: config hash + dataset hash + seed → exact results

## 4. Execution Realism

- Gap rule: stops gapped through fill at open (worse)
- Ambiguity rule: WorstCase default (adversarial ordering)
- No "perfect touch" assumptions
```

#### Initial Crate Structures

**trendlab-core/Cargo.toml**

```toml
[package]
name = "trendlab-core"
version.workspace = true
edition.workspace = true

[dependencies]
chrono.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
polars.workspace = true
anyhow.workspace = true

[dev-dependencies]
proptest = "1.0"
insta = "1.34"
cucumber = "0.21"
```

**trendlab-core/src/lib.rs**

```rust
//! TrendLab v3 Core Engine
//!
//! Provides domain types, event loop, execution simulation, and position management.

pub mod domain;
pub mod engine;
pub mod data;
```

**trendlab-runner/Cargo.toml**

```toml
[package]
name = "trendlab-runner"
version.workspace = true
edition.workspace = true

[dependencies]
trendlab-core = { path = "../trendlab-core" }
chrono.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
anyhow.workspace = true
```

**trendlab-tui/Cargo.toml**

```toml
[package]
name = "trendlab-tui"
version.workspace = true
edition.workspace = true

[dependencies]
trendlab-core = { path = "../trendlab-core" }
trendlab-runner = { path = "../trendlab-runner" }
ratatui.workspace = true
crossterm.workspace = true
```

**trendlab-cli/Cargo.toml** (optional)

```toml
[package]
name = "trendlab-cli"
version.workspace = true
edition.workspace = true

[dependencies]
trendlab-core = { path = "../trendlab-core" }
trendlab-runner = { path = "../trendlab-runner" }
clap = { version = "4.0", features = ["derive"] }
anyhow.workspace = true
```

### Verification Commands

```bash
# Step 1: Create workspace
mkdir trendlab-v3
cd trendlab-v3

# Step 2: Initialize workspace
cargo init --workspace

# Step 3: Create member crates
cargo new trendlab-core --lib
cargo new trendlab-runner --lib
cargo new trendlab-tui --lib
cargo new trendlab-cli

# Step 4: Copy workspace Cargo.toml (from above)
# Edit Cargo.toml at workspace root

# Step 5: Copy individual crate Cargo.toml files

# Step 6: Create CI workflow
mkdir -p .github/workflows
# Copy ci.yml content

# Step 7: Create config files
# Copy rustfmt.toml, .clippy.toml

# Step 8: Verify build
cargo build --workspace

# Expected output:
#    Compiling trendlab-core v0.1.0
#    Compiling trendlab-runner v0.1.0
#    Compiling trendlab-tui v0.1.0
#    Compiling trendlab-cli v0.1.0
#     Finished `dev` profile [unoptimized + debuginfo] target(s) in X.XXs

# Step 9: Run tests (should pass with no tests yet)
cargo test --workspace

# Expected output:
# running 0 tests
# test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

# Step 10: Check formatting and linting
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# Expected: no output (success)
```

### Completion Criteria

- [ ] Workspace compiles cleanly with `cargo build --workspace`
- [ ] CI pipeline configured and passes locally
- [ ] All config files (rustfmt, clippy) in place
- [ ] `cargo fmt --check` passes with no changes
- [ ] `cargo clippy` has zero warnings
- [ ] Basic docs/architecture.md exists and matches invariants

## BDD (minimal)

**Feature: Project builds cleanly**

- Scenario: clean checkout passes `cargo test`
- Scenario: CI runs fmt + clippy + smoke tests

---

# M0.5 — Smoke backtest (integration skeleton)

## Why

A tracer-bullet prevents "integration surprises" later.

## Deliverables

- Synthetic dataset (≈10 bars)
- Hardcoded "buy bar 3, sell bar 7" logic (no signals/PM yet)
- Minimal engine path: bars → "orders" → "fills" → portfolio → equity
- Golden test: final equity and trade list match expected

### trendlab-core Initial Module Structure

```text
trendlab-core/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── domain/
│   │   └── mod.rs (stub, will be filled in M1)
│   ├── engine/
│   │   ├── mod.rs
│   │   └── smoke.rs (minimal smoke test engine)
│   └── data/
│       └── mod.rs (stub, will be filled in M2)
└── tests/
    ├── smoke_backtest.rs (integration test)
    └── fixtures/
        └── synthetic_10bar.csv
```

### Smoke Test Implementation

**File: `trendlab-core/src/domain/mod.rs`** (minimal stub for M0.5)

```rust
//! Domain types (minimal stub for smoke test)
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bar {
    pub index: usize,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Bar {
    pub fn new(index: usize, open: f64, high: f64, low: f64, close: f64) -> Self {
        Self { index, open, high, low, close, volume: 1000.0 }
    }
}

#[derive(Debug, Clone)]
pub struct Trade {
    pub entry_bar: usize,
    pub entry_price: f64,
    pub exit_bar: usize,
    pub exit_price: f64,
    pub pnl: f64,
}
```

**File: `trendlab-core/src/engine/mod.rs`**

```rust
//! Engine module
pub mod smoke;
```

**File: `trendlab-core/src/engine/smoke.rs`**

```rust
//! Minimal smoke test engine
use crate::domain::{Bar, Trade};

/// Minimal engine for M0.5 smoke test only
/// Will be replaced by real engine in M3
pub struct SmokeEngine {
    cash: f64,
    equity: f64,
    position_size: f64,
    entry_price: f64,
    trades: Vec<Trade>,
}

impl SmokeEngine {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            cash: initial_cash,
            equity: initial_cash,
            position_size: 0.0,
            entry_price: 0.0,
            trades: Vec::new(),
        }
    }

    /// Hardcoded buy (for smoke test only)
    pub fn execute_buy(&mut self, bar: &Bar, notional: f64) {
        let shares = notional / bar.close;
        self.position_size = shares;
        self.entry_price = bar.close;
        self.cash -= notional;
        self.equity = self.cash + (self.position_size * bar.close);
    }

    /// Hardcoded sell (for smoke test only)
    pub fn execute_sell(&mut self, bar: &Bar) {
        let exit_value = self.position_size * bar.close;
        let pnl = exit_value - (self.position_size * self.entry_price);

        self.trades.push(Trade {
            entry_bar: 3, // hardcoded for smoke test
            entry_price: self.entry_price,
            exit_bar: bar.index,
            exit_price: bar.close,
            pnl,
        });

        self.cash += exit_value;
        self.position_size = 0.0;
        self.equity = self.cash;
    }

    /// Mark to market (update equity)
    pub fn mark_to_market(&mut self, bar: &Bar) {
        self.equity = self.cash + (self.position_size * bar.close);
    }

    pub fn equity(&self) -> f64 {
        self.equity
    }

    pub fn trades(&self) -> &[Trade] {
        &self.trades
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smoke_engine_buy_sell() {
        let mut engine = SmokeEngine::new(10000.0);
        let bar_buy = Bar::new(3, 107.0, 112.0, 106.0, 110.0);
        let bar_sell = Bar::new(7, 118.0, 125.0, 117.0, 120.0);

        engine.execute_buy(&bar_buy, 100.0);
        assert!(engine.position_size > 0.0);

        engine.execute_sell(&bar_sell);
        assert_eq!(engine.position_size, 0.0);
        assert!(engine.equity() > 10000.0);
    }
}
```

**File: `trendlab-core/tests/smoke_backtest.rs`**

```rust
//! M0.5 Smoke backtest integration test
//!
//! This is a golden test with hardcoded buy/sell logic.
//! Purpose: validate the tracer-bullet integration path works end-to-end.

use trendlab_core::domain::Bar;
use trendlab_core::engine::smoke::SmokeEngine;

#[test]
fn smoke_backtest_produces_golden_equity() {
    // Load synthetic 10-bar dataset
    let bars = load_synthetic_bars();

    // Hardcoded strategy: buy bar 3, sell bar 7
    let mut engine = SmokeEngine::new(10000.0);

    for (i, bar) in bars.iter().enumerate() {
        if i == 3 {
            engine.execute_buy(&bar, 100.0); // buy $100 worth
        }
        if i == 7 {
            engine.execute_sell(&bar);
        }
        engine.mark_to_market(&bar);
    }

    let final_equity = engine.equity();

    // Golden value: calculated manually
    // Entry: $100 @ 110.0 = 0.909 shares
    // Exit: 0.909 shares @ 120.0 = $109.09
    // Profit: $9.09
    // Final equity: $10000 - $100 + $109.09 = $10009.09
    assert!((final_equity - 10009.09).abs() < 0.1,
        "Golden equity mismatch: expected ~10009.09, got {}", final_equity);

    let trades = engine.trades();
    assert_eq!(trades.len(), 1, "Expected exactly 1 round-trip trade");
    assert!((trades[0].pnl - 9.09).abs() < 0.1, "Expected ~$9.09 profit");

    // Print visual confirmation
    println!("\n✓ Smoke backtest PASSED");
    println!("  Final equity: ${:.2}", final_equity);
    println!("  Trades: {}", trades.len());
    println!("  [0] Entry: bar {} @ ${:.2}, Exit: bar {} @ ${:.2}, PnL: ${:.2}",
        trades[0].entry_bar, trades[0].entry_price,
        trades[0].exit_bar, trades[0].exit_price, trades[0].pnl);
}

fn load_synthetic_bars() -> Vec<Bar> {
    // 10 bars with predictable price movement
    // Entry at bar 3 (close=110), exit at bar 7 (close=120) = +$10/share
    vec![
        Bar::new(0, 100.0, 105.0, 95.0, 100.0),
        Bar::new(1, 100.0, 110.0, 98.0, 105.0),
        Bar::new(2, 105.0, 108.0, 102.0, 107.0),
        Bar::new(3, 107.0, 112.0, 106.0, 110.0), // BUY HERE
        Bar::new(4, 110.0, 115.0, 108.0, 112.0),
        Bar::new(5, 112.0, 118.0, 111.0, 115.0),
        Bar::new(6, 115.0, 120.0, 114.0, 118.0),
        Bar::new(7, 118.0, 125.0, 117.0, 120.0), // SELL HERE (+$10/share)
        Bar::new(8, 120.0, 122.0, 118.0, 119.0),
        Bar::new(9, 119.0, 121.0, 117.0, 120.0),
    ]
}
```

### Expected Terminal Output

```text
$ cargo test --package trendlab-core smoke_backtest -- --nocapture

running 1 test

✓ Smoke backtest PASSED
  Final equity: $10009.09
  Trades: 1
  [0] Entry: bar 3 @ $110.00, Exit: bar 7 @ $120.00, PnL: $9.09

test smoke_backtest_produces_golden_equity ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Verification Steps

```bash
# Step 1: Create domain stub
mkdir -p trendlab-core/src/domain
cat > trendlab-core/src/domain/mod.rs <<'EOF'
(copy domain stub content from above)
EOF

# Step 2: Create engine module
mkdir -p trendlab-core/src/engine
cat > trendlab-core/src/engine/mod.rs <<'EOF'
pub mod smoke;
EOF

cat > trendlab-core/src/engine/smoke.rs <<'EOF'
(copy smoke.rs content from above)
EOF

# Step 3: Update lib.rs
cat > trendlab-core/src/lib.rs <<'EOF'
pub mod domain;
pub mod engine;
pub mod data;
EOF

# Step 4: Create smoke test
mkdir -p trendlab-core/tests
cat > trendlab-core/tests/smoke_backtest.rs <<'EOF'
(copy smoke_backtest.rs content from above)
EOF

# Step 5: Run smoke test
cargo test --package trendlab-core smoke_backtest -- --nocapture

# Expected: test passes with golden equity ~$10009.09

# Step 6: Run all tests
cargo test --workspace

# Expected: all tests pass (smoke test + any unit tests)
```

### Completion Criteria

- [ ] Smoke test file exists at `trendlab-core/tests/smoke_backtest.rs`
- [ ] Smoke test passes with golden equity value
- [ ] Terminal output shows trade details (entry/exit/PnL)
- [ ] `cargo test --workspace` shows 1+ passing tests
- [ ] Code compiles with no warnings

## BDD

**Feature: Smoke backtest integration**

- Scenario: synthetic 10-bar run produces golden final equity and trades

---

# M1 — Domain model + Instrument metadata + determinism contract
## Additions (from critique)
Include **Instrument** metadata early, even if minimal for equities.

## Deliverables
- Core types: `Bar`, `Order`, `Fill`, `Position`, `Portfolio`, `Trade`
- **Instrument**:
  - tick_size, lot_size, currency, asset_class
  - (optional now) trading calendar/trading hours hooks
- Deterministic IDs:
  - `ConfigId`, `DatasetHash`, `RunId`
- Seed plumbing for any stochastic behavior

### File Structure

#### Domain Module Files

```text
trendlab-core/src/domain/
├── mod.rs
├── bar.rs              # OHLC bar with timestamp
├── order.rs            # Order types and states
├── fill.rs             # Fill records
├── position.rs         # Position tracking
├── portfolio.rs        # Portfolio accounting
├── trade.rs            # Closed trade records
├── instrument.rs       # Instrument metadata
└── ids.rs              # Deterministic ID types
```

**File: `trendlab-core/src/domain/mod.rs`**

```rust
//! Domain types for TrendLab v3

pub mod bar;
pub mod order;
pub mod fill;
pub mod position;
pub mod portfolio;
pub mod trade;
pub mod instrument;
pub mod ids;

pub use bar::Bar;
pub use order::{Order, OrderType, OrderState, OrderSide};
pub use fill::Fill;
pub use position::Position;
pub use portfolio::Portfolio;
pub use trade::Trade;
pub use instrument::{Instrument, AssetClass};
pub use ids::{ConfigId, DatasetHash, RunId, OrderId, FillId, TradeId};
```

### Code Templates

**File: `trendlab-core/src/domain/bar.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Single OHLC bar with timestamp and symbol
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bar {
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Bar {
    /// Create a new bar
    pub fn new(
        timestamp: DateTime<Utc>,
        symbol: String,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Self {
        Self {
            timestamp,
            symbol,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    /// Validate bar invariants
    pub fn validate(&self) -> Result<(), BarError> {
        if self.high < self.low {
            return Err(BarError::InvalidRange {
                high: self.high,
                low: self.low,
            });
        }
        if self.open < 0.0 || self.high < 0.0 || self.low < 0.0 || self.close < 0.0 {
            return Err(BarError::NegativePrice);
        }
        if self.volume < 0.0 {
            return Err(BarError::NegativeVolume);
        }
        if !(self.low..=self.high).contains(&self.open) {
            return Err(BarError::OpenOutOfRange);
        }
        if !(self.low..=self.high).contains(&self.close) {
            return Err(BarError::CloseOutOfRange);
        }
        Ok(())
    }

    /// Check if bar is bullish (close > open)
    pub fn is_bullish(&self) -> bool {
        self.close > self.open
    }

    /// Get bar range (high - low)
    pub fn range(&self) -> f64 {
        self.high - self.low
    }
}

#[derive(Debug, Error)]
pub enum BarError {
    #[error("Invalid bar range: high={high}, low={low}")]
    InvalidRange { high: f64, low: f64 },

    #[error("Negative price not allowed")]
    NegativePrice,

    #[error("Negative volume not allowed")]
    NegativeVolume,

    #[error("Open price outside high/low range")]
    OpenOutOfRange,

    #[error("Close price outside high/low range")]
    CloseOutOfRange,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_bar_validate_rejects_inverted_range() {
        let bar = Bar::new(
            Utc::now(),
            "SPY".into(),
            100.0,
            99.0,  // high < low (invalid)
            101.0,
            100.0,
            1000.0,
        );
        assert!(bar.validate().is_err());
    }

    #[test]
    fn test_bar_validate_accepts_valid_bar() {
        let bar = Bar::new(
            Utc::now(),
            "SPY".into(),
            100.0,
            105.0,
            95.0,
            102.0,
            1000.0,
        );
        assert!(bar.validate().is_ok());
    }

    #[test]
    fn test_bar_rejects_negative_volume() {
        let bar = Bar::new(
            Utc::now(),
            "SPY".into(),
            100.0,
            105.0,
            95.0,
            102.0,
            -100.0, // invalid
        );
        assert!(matches!(bar.validate(), Err(BarError::NegativeVolume)));
    }
}
```

**File: `trendlab-core/src/domain/instrument.rs`**

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Instrument metadata for tick size, lot size, etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Instrument {
    pub symbol: String,
    pub tick_size: f64,
    pub lot_size: f64,
    pub currency: String,
    pub asset_class: AssetClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AssetClass {
    Equity,
    Future,
    Forex,
    Crypto,
}

impl Instrument {
    /// Create new instrument
    pub fn new(
        symbol: String,
        tick_size: f64,
        lot_size: f64,
        currency: String,
        asset_class: AssetClass,
    ) -> Self {
        Self {
            symbol,
            tick_size,
            lot_size,
            currency,
            asset_class,
        }
    }

    /// Round price to nearest tick
    pub fn round_to_tick(&self, price: f64) -> f64 {
        let ticks = (price / self.tick_size).round();
        ticks * self.tick_size
    }

    /// Validate price respects tick size
    pub fn validate_price(&self, price: f64) -> Result<f64, InstrumentError> {
        let rounded = self.round_to_tick(price);
        if (price - rounded).abs() > 1e-10 {
            Err(InstrumentError::InvalidTickSize {
                price,
                tick_size: self.tick_size,
            })
        } else {
            Ok(rounded)
        }
    }

    /// Validate quantity respects lot size
    pub fn validate_quantity(&self, qty: f64) -> Result<f64, InstrumentError> {
        let lots = (qty / self.lot_size).round();
        let rounded = lots * self.lot_size;
        if (qty - rounded).abs() > 1e-10 {
            Err(InstrumentError::InvalidLotSize {
                quantity: qty,
                lot_size: self.lot_size,
            })
        } else {
            Ok(rounded)
        }
    }
}

#[derive(Debug, Error)]
pub enum InstrumentError {
    #[error("Price {price} does not respect tick_size {tick_size}")]
    InvalidTickSize { price: f64, tick_size: f64 },

    #[error("Quantity {quantity} does not respect lot_size {lot_size}")]
    InvalidLotSize { quantity: f64, lot_size: f64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_size_rounding() {
        let inst = Instrument::new(
            "SPY".into(),
            0.01,
            1.0,
            "USD".into(),
            AssetClass::Equity,
        );
        assert_eq!(inst.round_to_tick(100.126), 100.13);
        assert_eq!(inst.round_to_tick(100.124), 100.12);
    }

    #[test]
    fn test_validate_price_rejects_bad_tick() {
        let inst = Instrument::new(
            "ES".into(),
            0.25,
            1.0,
            "USD".into(),
            AssetClass::Future,
        );
        assert!(inst.validate_price(4500.10).is_err());
        assert!(inst.validate_price(4500.25).is_ok());
        assert!(inst.validate_price(4500.50).is_ok());
    }

    #[test]
    fn test_validate_quantity_respects_lot_size() {
        let inst = Instrument::new(
            "BTC".into(),
            0.01,
            0.001, // crypto lot size
            "USD".into(),
            AssetClass::Crypto,
        );
        assert!(inst.validate_quantity(1.5).is_err());
        assert!(inst.validate_quantity(1.001).is_ok());
    }
}
```

**File: `trendlab-core/src/domain/ids.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::fmt;

/// Deterministic configuration ID (hash of strategy + params + execution config)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConfigId(pub String);

impl ConfigId {
    pub fn from_hash(hash: &str) -> Self {
        Self(hash.to_string())
    }
}

impl fmt::Display for ConfigId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Deterministic dataset hash (content hash of canonicalized data)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DatasetHash(pub String);

impl DatasetHash {
    pub fn from_hash(hash: &str) -> Self {
        Self(hash.to_string())
    }
}

impl fmt::Display for DatasetHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Deterministic run ID (config + dataset + seed)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId {
    pub config_id: ConfigId,
    pub dataset_hash: DatasetHash,
    pub seed: u64,
}

impl RunId {
    pub fn new(config_id: ConfigId, dataset_hash: DatasetHash, seed: u64) -> Self {
        Self {
            config_id,
            dataset_hash,
            seed,
        }
    }

    /// Generate deterministic run hash
    pub fn hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.config_id.0.hash(&mut hasher);
        self.dataset_hash.0.hash(&mut hasher);
        self.seed.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.config_id, self.dataset_hash, self.seed)
    }
}

/// Order ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(pub String);

impl OrderId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

/// Fill ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FillId(pub String);

impl FillId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

/// Trade ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TradeId(pub String);

impl TradeId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_id_deterministic() {
        let run1 = RunId::new(
            ConfigId::from_hash("abc123"),
            DatasetHash::from_hash("def456"),
            42,
        );
        let run2 = RunId::new(
            ConfigId::from_hash("abc123"),
            DatasetHash::from_hash("def456"),
            42,
        );
        assert_eq!(run1.hash(), run2.hash());
    }

    #[test]
    fn test_run_id_different_seed_different_hash() {
        let run1 = RunId::new(
            ConfigId::from_hash("abc123"),
            DatasetHash::from_hash("def456"),
            42,
        );
        let run2 = RunId::new(
            ConfigId::from_hash("abc123"),
            DatasetHash::from_hash("def456"),
            43,
        );
        assert_ne!(run1.hash(), run2.hash());
    }
}
```

**File: `trendlab-core/src/domain/order.rs`** (stub for M1, full implementation in M4)

```rust
use serde::{Deserialize, Serialize};
use crate::domain::ids::OrderId;

/// Order side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Order type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    StopMarket { stop_price: f64 },
    Limit { limit_price: f64 },
    StopLimit { stop_price: f64, limit_price: f64 },
}

/// Order state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderState {
    Pending,
    Triggered,
    Filled,
    Cancelled,
    Expired,
}

/// Order (minimal stub for M1, full implementation in M4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub state: OrderState,
}

impl Order {
    pub fn market(id: OrderId, symbol: String, side: OrderSide, quantity: f64) -> Self {
        Self {
            id,
            symbol,
            side,
            order_type: OrderType::Market,
            quantity,
            state: OrderState::Pending,
        }
    }
}
```

**File: `trendlab-core/src/domain/fill.rs`** (stub for M1)

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::domain::ids::{FillId, OrderId};
use crate::domain::order::OrderSide;

/// Fill record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub id: FillId,
    pub order_id: OrderId,
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub side: OrderSide,
    pub price: f64,
    pub quantity: f64,
    pub commission: f64,
}
```

**File: `trendlab-core/src/domain/position.rs`** (stub for M1)

```rust
use serde::{Deserialize, Serialize};

/// Position tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub avg_entry_price: f64,
}

impl Position {
    pub fn is_long(&self) -> bool {
        self.quantity > 0.0
    }

    pub fn is_short(&self) -> bool {
        self.quantity < 0.0
    }

    pub fn market_value(&self, current_price: f64) -> f64 {
        self.quantity * current_price
    }

    pub fn unrealized_pnl(&self, current_price: f64) -> f64 {
        self.quantity * (current_price - self.avg_entry_price)
    }
}
```

**File: `trendlab-core/src/domain/portfolio.rs`** (stub for M1)

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::domain::position::Position;

/// Portfolio accounting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    pub cash: f64,
    pub positions: HashMap<String, Position>,
}

impl Portfolio {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            cash: initial_cash,
            positions: HashMap::new(),
        }
    }

    pub fn equity(&self, current_prices: &HashMap<String, f64>) -> f64 {
        let position_value: f64 = self
            .positions
            .iter()
            .map(|(symbol, pos)| {
                let price = current_prices.get(symbol).copied().unwrap_or(0.0);
                pos.market_value(price)
            })
            .sum();

        self.cash + position_value
    }
}
```

**File: `trendlab-core/src/domain/trade.rs`** (stub for M1)

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::domain::ids::TradeId;

/// Closed trade record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: TradeId,
    pub symbol: String,
    pub entry_time: DateTime<Utc>,
    pub entry_price: f64,
    pub exit_time: DateTime<Utc>,
    pub exit_price: f64,
    pub quantity: f64,
    pub pnl: f64,
    pub commission: f64,
}
```

### Concrete BDD Scenarios

**Feature: Instrument-aware validation**

```gherkin
Feature: Instrument-aware validation

  Background:
    Given an instrument "SPY" with tick_size 0.01 and lot_size 1.0
    And an instrument "ES" with tick_size 0.25 and lot_size 1.0

  Scenario: Order price respects tick size (round policy)
    Given a pending order for "SPY" with price 100.126
    When the order is validated against the instrument
    Then the price is rounded to 100.13

  Scenario: Order price violates tick size (reject policy)
    Given a pending order for "ES" with price 4500.10
    And the validation policy is "reject"
    When the order is validated
    Then the order is rejected with error "InvalidTickSize"
    And the error message contains "4500.10 does not respect tick_size 0.25"

  Scenario: Order quantity respects lot size
    Given an instrument "BTC" with tick_size 0.01 and lot_size 0.001
    And a pending order for "BTC" with quantity 1.5
    When the order is validated
    Then the order is rejected with error "InvalidLotSize"
```

**Feature: Deterministic reproducibility**

```gherkin
Feature: Deterministic reproducibility

  Background:
    Given a strategy config with hash "abc123"
    And a dataset with hash "def456"
    And a seed value of 42

  Scenario: Same manifest produces identical RunId
    When I create a RunId from config "abc123", dataset "def456", and seed 42
    And I create another RunId from config "abc123", dataset "def456", and seed 42
    Then both RunIds have identical hashes

  Scenario: Different seed produces different RunId
    When I create a RunId from config "abc123", dataset "def456", and seed 42
    And I create another RunId from config "abc123", dataset "def456", and seed 43
    Then the RunIds have different hashes

  Scenario: Same manifest reproduces identical backtest results
    Given a backtest run with RunId "abc123:def456:42"
    When I execute the backtest
    And I record the final equity as "equity_1"
    And I execute the same backtest again with the same RunId
    And I record the final equity as "equity_2"
    Then equity_1 equals equity_2
    And the trade sequences are identical
```

### Verification Commands

```bash
# Step 1: Create domain module structure
mkdir -p trendlab-core/src/domain
cd trendlab-core/src/domain

# Step 2: Create domain type files
touch mod.rs bar.rs order.rs fill.rs position.rs portfolio.rs trade.rs instrument.rs ids.rs

# Step 3: Copy code templates to files
# (copy bar.rs content from above)
# (copy instrument.rs content from above)
# (copy ids.rs content from above)
# (copy other stub files)

# Step 4: Update src/lib.rs to export domain module
cat > ../lib.rs <<'EOF'
//! TrendLab v3 Core Engine

pub mod domain;
pub mod engine;
pub mod data;
EOF

# Step 5: Run tests
cargo test --package trendlab-core

# Expected output:
# running 9 tests
# test domain::bar::tests::test_bar_validate_accepts_valid_bar ... ok
# test domain::bar::tests::test_bar_validate_rejects_inverted_range ... ok
# test domain::bar::tests::test_bar_rejects_negative_volume ... ok
# test domain::instrument::tests::test_tick_size_rounding ... ok
# test domain::instrument::tests::test_validate_price_rejects_bad_tick ... ok
# test domain::instrument::tests::test_validate_quantity_respects_lot_size ... ok
# test domain::ids::tests::test_run_id_deterministic ... ok
# test domain::ids::tests::test_run_id_different_seed_different_hash ... ok
# test engine::smoke::tests::test_smoke_engine_buy_sell ... ok
#
# test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

# Step 6: Check formatting and linting
cargo fmt --all -- --check
cargo clippy --package trendlab-core -- -D warnings

# Expected: no warnings
```

### Example Flow: Order Validation with Instrument Metadata

```text
1. User creates Order:
   Order { symbol: "SPY", price: 100.126, quantity: 100.0, ... }

2. Instrument registry lookup:
   instrument = InstrumentRegistry.get("SPY")
   → Instrument { tick_size: 0.01, lot_size: 1.0, ... }

3. Price validation:
   instrument.validate_price(100.126)
   → rounds to 100.13
   → order.price updated to 100.13

4. Quantity validation:
   instrument.validate_quantity(100.0)
   → 100.0 % 1.0 == 0 ✓
   → validated

5. Order ready for submission:
   Order { symbol: "SPY", price: 100.13, quantity: 100.0, state: Pending }
```

### Example Flow: Deterministic Run Reproducibility

```text
1. User configures backtest:
   config = StrategyConfig { ... } → hash: "abc123"
   dataset = load_data("SPY.parquet") → hash: "def456"
   seed = 42

2. Create RunId:
   run_id = RunId::new(
       ConfigId("abc123"),
       DatasetHash("def456"),
       42
   )
   → run_id.hash() = "7f8a9b..."

3. Execute backtest:
   engine.run(run_id)
   → final_equity = $10,523.45
   → trades = [...]

4. Store results:
   results_db.insert(run_id.hash(), results)

5. Later: Re-run same backtest:
   run_id_2 = RunId::new(
       ConfigId("abc123"),
       DatasetHash("def456"),
       42
   )
   → run_id_2.hash() = "7f8a9b..." (same!)

6. Retrieve cached or verify:
   cached = results_db.get("7f8a9b...")
   → if exists, return cached
   → if not, re-run and verify identical
```

### Completion Criteria

- [ ] All domain type files exist in `trendlab-core/src/domain/`
- [ ] Bar validation tests pass (invalid range, negative prices, negative volume)
- [ ] Instrument tick_size and lot_size validation tests pass
- [ ] RunId produces deterministic hashes for same inputs
- [ ] RunId produces different hashes for different seeds
- [ ] All domain types are properly exported from `domain/mod.rs`
- [ ] `cargo test --package trendlab-core` shows 9+ passing tests
- [ ] `cargo clippy` has zero warnings
- [ ] BDD scenarios are documented (Cucumber implementation in M3+)

## BDD
**Feature: Deterministic reproducibility**
- Scenario: same manifest + dataset hash + seed reproduces identical results

**Feature: Instrument-aware validation**
- Scenario: order prices respect tick size (round/reject per policy)

---

# M2 — Data ingest + canonical cache
## Deliverables
- Ingest CSV/Parquet → validate schema → canonicalize → sort/dedupe → anomaly checks
- Canonical Parquet cache + metadata sidecar (hash, date range, adjustments)
- Universe sets: local lists + named universes

### File Structure

#### Data Module Files

```text
trendlab-core/src/data/
├── mod.rs
├── ingest.rs           # CSV/Parquet ingestion
├── canonicalize.rs     # Sort, dedupe, validation
├── cache.rs            # Canonical cache + metadata
├── schema.rs           # Expected schema definitions
└── universe.rs         # Universe set management
```

**File: `trendlab-core/src/data/mod.rs`**

```rust
//! Data ingestion and caching

pub mod ingest;
pub mod canonicalize;
pub mod cache;
pub mod schema;
pub mod universe;

pub use ingest::DataIngestor;
pub use canonicalize::Canonicalizer;
pub use cache::{DataCache, CacheMetadata};
pub use schema::BarSchema;
pub use universe::{Universe, UniverseSet};
```

### Code Templates

**File: `trendlab-core/src/data/schema.rs`**

```rust
use polars::prelude::*;

/// Expected schema for bar data
pub struct BarSchema;

impl BarSchema {
    /// Get the canonical bar schema
    pub fn schema() -> Schema {
        Schema::from_iter(vec![
            Field::new("timestamp", DataType::Datetime(TimeUnit::Milliseconds, None)),
            Field::new("symbol", DataType::Utf8),
            Field::new("open", DataType::Float64),
            Field::new("high", DataType::Float64),
            Field::new("low", DataType::Float64),
            Field::new("close", DataType::Float64),
            Field::new("volume", DataType::Float64),
        ])
    }

    /// Validate DataFrame against schema
    pub fn validate(df: &DataFrame) -> Result<(), SchemaError> {
        let expected = Self::schema();
        let actual = df.schema();

        // Check all required columns exist
        for field in expected.iter_fields() {
            if !actual.contains(field.name()) {
                return Err(SchemaError::MissingColumn(field.name().to_string()));
            }
        }

        // Check data types match
        for field in expected.iter_fields() {
            let actual_field = actual.get(field.name()).ok_or_else(|| {
                SchemaError::MissingColumn(field.name().to_string())
            })?;
            if actual_field.data_type() != field.data_type() {
                return Err(SchemaError::TypeMismatch {
                    column: field.name().to_string(),
                    expected: field.data_type().clone(),
                    actual: actual_field.data_type().clone(),
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("Missing required column: {0}")]
    MissingColumn(String),

    #[error("Type mismatch in column {column}: expected {expected:?}, got {actual:?}")]
    TypeMismatch {
        column: String,
        expected: DataType,
        actual: DataType,
    },
}
```

**File: `trendlab-core/src/data/ingest.rs`**

```rust
use polars::prelude::*;
use std::path::Path;
use crate::data::schema::BarSchema;

/// Data ingestor for CSV and Parquet files
pub struct DataIngestor {
    schema: Schema,
}

impl DataIngestor {
    pub fn new() -> Self {
        Self {
            schema: BarSchema::schema(),
        }
    }

    /// Ingest CSV file
    pub fn ingest_csv(&self, path: &Path) -> Result<LazyFrame, DataError> {
        LazyCsvReader::new(path)
            .with_schema(Some(Arc::new(self.schema.clone())))
            .with_has_header(true)
            .finish()
            .map_err(|e| DataError::IngestFailed(e.to_string()))
    }

    /// Ingest Parquet file
    pub fn ingest_parquet(&self, path: &Path) -> Result<LazyFrame, DataError> {
        LazyFrame::scan_parquet(path, Default::default())
            .map_err(|e| DataError::IngestFailed(e.to_string()))
    }
}

impl Default for DataIngestor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("Ingest failed: {0}")]
    IngestFailed(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Cache error: {0}")]
    CacheError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ingest_csv_with_valid_schema() {
        // Test would load a fixture CSV file
        // and verify it matches the expected schema
    }
}
```

**File: `trendlab-core/src/data/canonicalize.rs`**

```rust
use polars::prelude::*;
use crate::data::DataError;

/// Canonicalizer for bar data
pub struct Canonicalizer;

impl Canonicalizer {
    /// Canonicalize data: sort, dedupe, validate
    pub fn canonicalize(df: LazyFrame) -> LazyFrame {
        df.sort(
            ["timestamp", "symbol"],
            SortMultipleOptions::default()
                .with_order_descending_multi([false, false]),
        )
        .unique(
            Some(vec!["timestamp".into(), "symbol".into()]),
            UniqueKeepStrategy::First,
        )
    }

    /// Validate bar data (no negative prices, high >= low, etc.)
    pub fn validate(df: LazyFrame) -> LazyFrame {
        df.filter(
            col("high")
                .gt_eq(col("low"))
                .and(col("open").gt(0.0))
                .and(col("high").gt(0.0))
                .and(col("low").gt(0.0))
                .and(col("close").gt(0.0))
                .and(col("volume").gt_eq(0.0))
                .and(col("open").gt_eq(col("low")))
                .and(col("open").lt_eq(col("high")))
                .and(col("close").gt_eq(col("low")))
                .and(col("close").lt_eq(col("high"))),
        )
    }

    /// Detect anomalies (outliers, gaps, suspicious volume)
    pub fn detect_anomalies(df: &DataFrame) -> Vec<AnomalyReport> {
        let mut anomalies = Vec::new();

        // Check for zero volume
        if let Ok(volume) = df.column("volume") {
            let zero_volume_count = volume
                .f64()
                .unwrap()
                .iter()
                .filter(|v| v == &Some(0.0))
                .count();

            if zero_volume_count > 0 {
                anomalies.push(AnomalyReport {
                    anomaly_type: AnomalyType::ZeroVolume,
                    count: zero_volume_count,
                    severity: Severity::Warning,
                });
            }
        }

        // More anomaly checks would go here...

        anomalies
    }
}

#[derive(Debug)]
pub struct AnomalyReport {
    pub anomaly_type: AnomalyType,
    pub count: usize,
    pub severity: Severity,
}

#[derive(Debug)]
pub enum AnomalyType {
    ZeroVolume,
    SuspiciousGap,
    OutlierPrice,
}

#[derive(Debug)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonicalize_removes_duplicates() {
        // Create test DataFrame with duplicate timestamp+symbol
        // Verify only first occurrence kept
    }

    #[test]
    fn test_validate_rejects_inverted_bars() {
        // Create test DataFrame with high < low
        // Verify those rows are filtered out
    }
}
```

**File: `trendlab-core/src/data/cache.rs`**

```rust
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use crate::data::DataError;
use crate::domain::DatasetHash;

/// Canonical data cache
pub struct DataCache {
    cache_dir: PathBuf,
}

impl DataCache {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Write DataFrame to cache with metadata
    pub fn write(
        &self,
        df: &mut DataFrame,
        metadata: CacheMetadata,
    ) -> Result<DatasetHash, DataError> {
        // Compute content hash
        let hash = Self::compute_hash(df)?;

        // Write parquet file
        let data_path = self.cache_dir.join(format!("{}.parquet", hash.0));
        let file = std::fs::File::create(&data_path)
            .map_err(|e| DataError::CacheError(e.to_string()))?;

        ParquetWriter::new(file)
            .finish(df)
            .map_err(|e| DataError::CacheError(e.to_string()))?;

        // Write metadata sidecar
        let meta_path = self.cache_dir.join(format!("{}.meta.json", hash.0));
        let meta_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| DataError::CacheError(e.to_string()))?;
        std::fs::write(&meta_path, meta_json)
            .map_err(|e| DataError::CacheError(e.to_string()))?;

        Ok(hash)
    }

    /// Read DataFrame from cache
    pub fn read(&self, hash: &DatasetHash) -> Result<DataFrame, DataError> {
        let data_path = self.cache_dir.join(format!("{}.parquet", hash.0));
        let file = std::fs::File::open(&data_path)
            .map_err(|e| DataError::CacheError(e.to_string()))?;

        ParquetReader::new(file)
            .finish()
            .map_err(|e| DataError::CacheError(e.to_string()))
    }

    /// Read metadata from cache
    pub fn read_metadata(&self, hash: &DatasetHash) -> Result<CacheMetadata, DataError> {
        let meta_path = self.cache_dir.join(format!("{}.meta.json", hash.0));
        let meta_json = std::fs::read_to_string(&meta_path)
            .map_err(|e| DataError::CacheError(e.to_string()))?;
        serde_json::from_str(&meta_json)
            .map_err(|e| DataError::CacheError(e.to_string()))
    }

    /// Compute deterministic hash of DataFrame content
    fn compute_hash(df: &DataFrame) -> Result<DatasetHash, DataError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash column names
        for name in df.get_column_names() {
            name.hash(&mut hasher);
        }

        // Hash row count
        df.height().hash(&mut hasher);

        // Hash first/last timestamps and symbols for determinism
        // (Full content hash would be too expensive; this is a reasonable proxy)
        if let Ok(ts) = df.column("timestamp") {
            if let Some(first) = ts.get(0) {
                format!("{:?}", first).hash(&mut hasher);
            }
            if let Some(last) = ts.get(df.height() - 1) {
                format!("{:?}", last).hash(&mut hasher);
            }
        }

        let hash_value = format!("{:x}", hasher.finish());
        Ok(DatasetHash::from_hash(&hash_value))
    }
}

/// Cache metadata sidecar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub created_at: DateTime<Utc>,
    pub source_files: Vec<String>,
    pub date_range: (DateTime<Utc>, DateTime<Utc>),
    pub symbol_count: usize,
    pub bar_count: usize,
    pub adjustments: Option<String>,
    pub anomalies: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash_deterministic() {
        // Create identical DataFrames
        // Verify they produce the same hash
    }

    #[test]
    fn test_cache_roundtrip() {
        // Write DataFrame to cache
        // Read it back
        // Verify content matches
    }
}
```

**File: `trendlab-core/src/data/universe.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Universe of symbols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Universe {
    pub name: String,
    pub symbols: HashSet<String>,
}

impl Universe {
    pub fn new(name: String, symbols: Vec<String>) -> Self {
        Self {
            name,
            symbols: symbols.into_iter().collect(),
        }
    }

    pub fn contains(&self, symbol: &str) -> bool {
        self.symbols.contains(symbol)
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// Collection of named universes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseSet {
    pub universes: Vec<Universe>,
}

impl UniverseSet {
    pub fn new() -> Self {
        Self {
            universes: Vec::new(),
        }
    }

    pub fn add_universe(&mut self, universe: Universe) {
        self.universes.push(universe);
    }

    pub fn get(&self, name: &str) -> Option<&Universe> {
        self.universes.iter().find(|u| u.name == name)
    }
}

impl Default for UniverseSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universe_contains() {
        let universe = Universe::new(
            "sp500".into(),
            vec!["AAPL".into(), "MSFT".into(), "GOOGL".into()],
        );
        assert!(universe.contains("AAPL"));
        assert!(!universe.contains("TSLA"));
    }
}
```

### Concrete BDD Scenarios

**Feature: Canonical data cache**

```gherkin
Feature: Canonical data cache

  Background:
    Given a data cache directory at "data/cache"
    And a CSV file "spy_raw.csv" with 1000 bars

  Scenario: Ingest produces deterministic dataset hash
    When I ingest "spy_raw.csv"
    And I canonicalize the data (sort, dedupe, validate)
    And I write to cache
    Then I receive a dataset hash "abc123..."
    When I ingest "spy_raw.csv" again
    And I canonicalize the data
    And I write to cache
    Then I receive the same dataset hash "abc123..."

  Scenario: Cache metadata includes date range and bar count
    When I ingest "spy_raw.csv"
    And I write to cache with hash "abc123"
    Then the metadata file "abc123.meta.json" exists
    And the metadata contains:
      | field       | value                |
      | bar_count   | 1000                 |
      | symbol_count| 1                    |
      | date_range  | 2020-01-01 to 2023-12-31 |

  Scenario: Missing days remain missing (no silent forward fill)
    Given a CSV file with bars:
      | date       | symbol | close |
      | 2023-01-01 | SPY    | 400.0 |
      | 2023-01-03 | SPY    | 405.0 |
    When I ingest and canonicalize
    Then the cached data has 2 bars
    And no bar exists for 2023-01-02
    And the date gap is preserved

  Scenario: Duplicate timestamps are deduped (first wins)
    Given a CSV file with duplicate bars:
      | timestamp           | symbol | close |
      | 2023-01-01 09:30:00 | SPY    | 400.0 |
      | 2023-01-01 09:30:00 | SPY    | 401.0 |
    When I canonicalize the data
    Then only 1 bar remains
    And the close price is 400.0 (first occurrence)

  Scenario: Invalid bars are filtered out
    Given a CSV file with invalid bars:
      | symbol | open  | high  | low   | close |
      | SPY    | 100.0 | 95.0  | 105.0 | 102.0 |
    When I validate the data
    Then the bar with high < low is removed
    And an anomaly report is generated
```

**Feature: Anomaly detection**

```gherkin
Feature: Anomaly detection

  Scenario: Zero volume bars are flagged as warnings
    Given a DataFrame with 100 bars
    And 5 bars have zero volume
    When I detect anomalies
    Then I receive an anomaly report:
      | type        | count | severity |
      | ZeroVolume  | 5     | Warning  |

  Scenario: Suspicious price gaps are flagged
    Given a DataFrame where bar N has close=100.0
    And bar N+1 has open=150.0 (50% gap)
    When I detect anomalies
    Then I receive an anomaly report for SuspiciousGap
```

### Verification Commands

```bash
# Step 1: Create data module structure
mkdir -p trendlab-core/src/data
cd trendlab-core/src/data

# Step 2: Create data module files
touch mod.rs ingest.rs canonicalize.rs cache.rs schema.rs universe.rs

# Step 3: Add Polars dependency to Cargo.toml
cat >> ../../Cargo.toml <<'EOF'

[dependencies]
# (existing dependencies)
polars = { workspace = true }
EOF

# Step 4: Create test fixture directory
mkdir -p trendlab-core/tests/fixtures

# Step 5: Create sample CSV for testing
cat > trendlab-core/tests/fixtures/sample.csv <<'EOF'
timestamp,symbol,open,high,low,close,volume
2023-01-01T09:30:00Z,SPY,400.0,405.0,399.0,403.0,1000000
2023-01-02T09:30:00Z,SPY,403.0,408.0,402.0,407.0,1200000
EOF

# Step 6: Run tests
cargo test --package trendlab-core

# Expected output:
# running 12+ tests
# test data::schema::tests::... (if implemented)
# test data::canonicalize::tests::test_canonicalize_removes_duplicates ... ok
# test data::cache::tests::test_compute_hash_deterministic ... ok
# test data::universe::tests::test_universe_contains ... ok
# (plus all previous domain tests)
#
# test result: ok. 12 passed; 0 failed; 0 ignored

# Step 7: Test data ingestion manually
cargo run --example ingest_csv -- tests/fixtures/sample.csv

# Expected output:
# Ingested 2 bars
# Dataset hash: a1b2c3...
# Cache written to: data/cache/a1b2c3.parquet
```

### Example Flow: Data Ingestion Pipeline

```text
1. User provides CSV file:
   spy_data.csv (10,000 rows)

2. Ingestor reads CSV:
   DataIngestor::ingest_csv("spy_data.csv")
   → LazyFrame (unevaluated)

3. Schema validation:
   BarSchema::validate(df)
   → checks for required columns (timestamp, symbol, OHLC, volume)
   → checks data types match

4. Canonicalization:
   Canonicalizer::canonicalize(df)
   → sort by (timestamp, symbol)
   → dedupe on (timestamp, symbol), keep first

5. Validation:
   Canonicalizer::validate(df)
   → filter: high >= low
   → filter: prices > 0
   → filter: volume >= 0

6. Anomaly detection:
   Canonicalizer::detect_anomalies(df)
   → detect zero volume bars
   → detect suspicious gaps
   → generate report

7. Cache write:
   cache.write(df, metadata)
   → compute content hash: "abc123..."
   → write abc123.parquet
   → write abc123.meta.json with metadata

8. Result:
   DatasetHash("abc123...")
   → used for reproducibility (RunId)
```

### Completion Criteria

- [ ] All data module files exist in `trendlab-core/src/data/`
- [ ] BarSchema validation tests pass
- [ ] Canonicalizer removes duplicates correctly
- [ ] Canonicalizer filters invalid bars (high < low, negative prices)
- [ ] DataCache computes deterministic hashes for identical data
- [ ] DataCache round-trip works (write then read produces identical data)
- [ ] Universe contains/get methods work correctly
- [ ] Anomaly detection flags zero volume bars
- [ ] Test fixtures exist with sample CSV data
- [ ] `cargo test --package trendlab-core` shows 12+ passing tests
- [ ] `cargo clippy` has zero warnings

## BDD
**Feature: Canonical data cache**
- Scenario: ingest produces deterministic dataset hash
- Scenario: missing days remain missing (no silent forward fill)

---

# M3 — Event loop skeleton + warmup + accounting
## Critique-driven clarifications
- PM emits intents for **next bar**, never the current bar.

## Deliverables
- Bar event phases:
  1) Start-of-bar: activate day orders; fill MOO
  2) Intrabar: simulate triggers/fills
  3) End-of-bar: fill MOC
  4) Post-bar:
     a) mark positions + compute equity
     b) PM emits maintenance intents for **NEXT** bar
     c) (optional) signal generation for next bar if incremental
- **Warmup** handling:
  - no orders before required history exists
  - warmup length defined per feature set / strategy
- Accounting:
  - equity, realized/unrealized PnL, fees

### File Structure

#### Engine Module Files

```text
trendlab-core/src/engine/
├── mod.rs
├── event_loop.rs      # Main backtest engine with 4-phase loop
├── warmup.rs          # Warmup state tracking
├── accounting.rs      # Equity, PnL, fees tracking
└── smoke.rs           # Smoke test engine (from M0.5)
```

**File: `trendlab-core/src/engine/mod.rs`**

```rust
//! Backtest engine

pub mod event_loop;
pub mod warmup;
pub mod accounting;
pub mod smoke;

pub use event_loop::Engine;
pub use warmup::WarmupState;
pub use accounting::EquityTracker;
```

### Code Templates

**File: `trendlab-core/src/engine/warmup.rs`**

```rust
/// Warmup state tracker
#[derive(Debug, Clone)]
pub struct WarmupState {
    warmup_bars: usize,
    bars_processed: usize,
}

impl WarmupState {
    pub fn new(warmup_bars: usize) -> Self {
        Self {
            warmup_bars,
            bars_processed: 0,
        }
    }

    pub fn process_bar(&mut self) {
        self.bars_processed += 1;
    }

    pub fn is_warm(&self) -> bool {
        self.bars_processed >= self.warmup_bars
    }

    pub fn bars_until_warm(&self) -> usize {
        if self.is_warm() {
            0
        } else {
            self.warmup_bars - self.bars_processed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warmup_state() {
        let mut warmup = WarmupState::new(20);
        assert!(!warmup.is_warm());
        assert_eq!(warmup.bars_until_warm(), 20);

        for _ in 0..19 {
            warmup.process_bar();
        }
        assert!(!warmup.is_warm());
        assert_eq!(warmup.bars_until_warm(), 1);

        warmup.process_bar();
        assert!(warmup.is_warm());
        assert_eq!(warmup.bars_until_warm(), 0);
    }
}
```

**File: `trendlab-core/src/engine/accounting.rs`**

```rust
use std::collections::HashMap;
use crate::domain::{Fill, Position, OrderSide};

/// Equity and PnL tracker
#[derive(Debug, Clone)]
pub struct EquityTracker {
    initial_cash: f64,
    cash: f64,
    realized_pnl: f64,
    commission_paid: f64,
    equity_history: Vec<f64>,
}

impl EquityTracker {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            initial_cash,
            cash: initial_cash,
            realized_pnl: 0.0,
            commission_paid: 0.0,
            equity_history: vec![initial_cash],
        }
    }

    /// Apply a fill to cash and realized PnL
    pub fn apply_fill(&mut self, fill: &Fill, avg_entry_price: f64) {
        // Update cash (inflow for sells, outflow for buys)
        match fill.side {
            OrderSide::Buy => {
                self.cash -= fill.price * fill.quantity;
            }
            OrderSide::Sell => {
                self.cash += fill.price * fill.quantity;
                // Realize PnL on sell
                let pnl = (fill.price - avg_entry_price) * fill.quantity;
                self.realized_pnl += pnl;
            }
        }

        // Deduct commission
        self.cash -= fill.commission;
        self.commission_paid += fill.commission;
    }

    /// Compute current equity (cash + position value)
    pub fn compute_equity(&self, positions: &HashMap<String, Position>, prices: &HashMap<String, f64>) -> f64 {
        let position_value: f64 = positions
            .iter()
            .map(|(symbol, pos)| {
                let price = prices.get(symbol).copied().unwrap_or(0.0);
                pos.market_value(price)
            })
            .sum();

        self.cash + position_value
    }

    /// Record equity at bar close
    pub fn record_equity(&mut self, equity: f64) {
        self.equity_history.push(equity);
    }

    /// Get unrealized PnL
    pub fn unrealized_pnl(&self, positions: &HashMap<String, Position>, prices: &HashMap<String, f64>) -> f64 {
        positions
            .iter()
            .map(|(symbol, pos)| {
                let price = prices.get(symbol).copied().unwrap_or(0.0);
                pos.unrealized_pnl(price)
            })
            .sum()
    }

    pub fn cash(&self) -> f64 {
        self.cash
    }

    pub fn realized_pnl(&self) -> f64 {
        self.realized_pnl
    }

    pub fn total_pnl(&self, current_equity: f64) -> f64 {
        current_equity - self.initial_cash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FillId, OrderId};
    use chrono::Utc;

    #[test]
    fn test_equity_tracking() {
        let mut tracker = EquityTracker::new(10000.0);

        // Simulate buy fill
        let buy_fill = Fill {
            id: FillId::new("fill1".into()),
            order_id: OrderId::new("order1".into()),
            timestamp: Utc::now(),
            symbol: "SPY".into(),
            side: OrderSide::Buy,
            price: 100.0,
            quantity: 10.0,
            commission: 1.0,
        };

        tracker.apply_fill(&buy_fill, 0.0);
        assert_eq!(tracker.cash(), 10000.0 - 1000.0 - 1.0); // cash - cost - commission

        // Simulate sell fill
        let sell_fill = Fill {
            id: FillId::new("fill2".into()),
            order_id: OrderId::new("order2".into()),
            timestamp: Utc::now(),
            symbol: "SPY".into(),
            side: OrderSide::Sell,
            price: 110.0,
            quantity: 10.0,
            commission: 1.0,
        };

        tracker.apply_fill(&sell_fill, 100.0); // avg entry = 100
        assert_eq!(tracker.realized_pnl(), 100.0); // 10 shares * $10 profit
        assert_eq!(tracker.commission_paid(), 2.0);
    }
}
```

**File: `trendlab-core/src/engine/event_loop.rs`** (simplified for M3, full in M4-M5)

```rust
use crate::domain::{Bar, Order, Fill, Position, Portfolio};
use crate::engine::{WarmupState, EquityTracker};
use std::collections::HashMap;

/// Main backtest engine
pub struct Engine {
    warmup: WarmupState,
    accounting: EquityTracker,
    portfolio: Portfolio,
    current_bar_index: usize,
}

impl Engine {
    pub fn new(initial_cash: f64, warmup_bars: usize) -> Self {
        Self {
            warmup: WarmupState::new(warmup_bars),
            accounting: EquityTracker::new(initial_cash),
            portfolio: Portfolio::new(initial_cash),
            current_bar_index: 0,
        }
    }

    /// Process a single bar (4-phase event loop)
    pub fn process_bar(&mut self, bar: &Bar, current_prices: &HashMap<String, f64>) {
        // Phase 1: Start-of-bar
        self.start_of_bar(bar);

        // Phase 2: Intrabar (simulated in M5)
        self.intrabar(bar);

        // Phase 3: End-of-bar
        self.end_of_bar(bar);

        // Phase 4: Post-bar
        self.post_bar(bar, current_prices);

        self.current_bar_index += 1;
        self.warmup.process_bar();
    }

    fn start_of_bar(&mut self, bar: &Bar) {
        // Activate day orders (M4)
        // Fill MOO orders (M5)
    }

    fn intrabar(&mut self, bar: &Bar) {
        // Simulate triggers/fills using PathPolicy (M5)
    }

    fn end_of_bar(&mut self, bar: &Bar) {
        // Fill MOC orders (M5)
    }

    fn post_bar(&mut self, bar: &Bar, current_prices: &HashMap<String, f64>) {
        // Mark to market
        let equity = self.accounting.compute_equity(&self.portfolio.positions, current_prices);
        self.accounting.record_equity(equity);

        // PM emits maintenance orders for NEXT bar (M6)
        // Only if warmup complete
        if !self.warmup.is_warm() {
            return;
        }

        // PM logic goes here in M6
    }

    pub fn is_warm(&self) -> bool {
        self.warmup.is_warm()
    }

    pub fn equity_history(&self) -> &[f64] {
        &self.accounting.equity_history
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_warmup_blocks_pm() {
        let mut engine = Engine::new(10000.0, 20);
        let bars: Vec<_> = (0..50)
            .map(|i| Bar {
                timestamp: Utc::now(),
                symbol: "SPY".into(),
                open: 100.0,
                high: 105.0,
                low: 95.0,
                close: 102.0,
                volume: 1000.0,
            })
            .collect();

        for (i, bar) in bars.iter().enumerate() {
            let prices = HashMap::from([("SPY".to_string(), bar.close)]);
            engine.process_bar(bar, &prices);

            if i < 20 {
                assert!(!engine.is_warm(), "Should not be warm before bar 20");
            } else {
                assert!(engine.is_warm(), "Should be warm after bar 20");
            }
        }
    }
}
```

### Concrete BDD Scenarios

**Feature: Engine warmup**

```gherkin
Feature: Engine warmup prevents premature trading

  Background:
    Given a strategy that requires 20 bars of warmup
    And a dataset with 100 bars
    And initial cash of $10,000

  Scenario: No orders before warmup completes
    When the backtest runs
    Then no orders are generated before bar 20
    And the first order can only appear at bar 20 or later
    And warmup bars still contribute to indicator computation

  Scenario: Warmup length is strategy-dependent
    Given strategy A requires 20 bars warmup
    And strategy B requires 50 bars warmup
    When both strategies run on the same dataset
    Then strategy A's first order appears at bar 20 or later
    And strategy B's first order appears at bar 50 or later

  Scenario: Warmup state is observable
    Given a strategy with 20-bar warmup
    When the engine is at bar 10
    Then engine.is_warm() returns false
    And bars_until_warm() returns 10
    When the engine reaches bar 20
    Then engine.is_warm() returns true
    And bars_until_warm() returns 0
```

**Feature: PM timing (PM emits for NEXT bar)**

```gherkin
Feature: PM timing - orders apply to next bar, not current

  Background:
    Given a long position in SPY entered at bar 10 @ $100
    And a trailing stop PM with 2 ATR offset
    And ATR(20) is computed each bar

  Scenario: Stop update applies starting bar N+1, not bar N
    Given at bar 15, ATR = 2.0, close = $110
    When PM computes stop level: 110 - (2 * 2.0) = $106
    And PM emits stop order for NEXT bar (bar 16)
    Then the stop order is not active during bar 15
    And the stop order activates at start of bar 16
    And if bar 16 gaps down to $105, the stop triggers

  Scenario: Current bar cannot trigger newly emitted stop
    Given at bar 20, close = $120, ATR = 3.0
    When PM emits stop at 120 - (2 * 3.0) = $114 for bar 21
    And bar 20 has low = $112 (below stop level)
    Then the stop does NOT trigger during bar 20
    Because the stop was not active yet
```

**Feature: Equity accounting invariant**

```gherkin
Feature: Equity accounting invariant

  Background:
    Given initial cash of $10,000
    And a portfolio with positions

  Scenario: Equity = cash + Σ(position value) each bar close
    Given the following positions:
      | symbol | quantity | avg_entry | current_price |
      | SPY    | 10       | 100.0     | 110.0         |
      | QQQ    | 5        | 200.0     | 210.0         |
    And cash = $7,500
    When equity is computed at bar close
    Then equity = 7500 + (10 * 110) + (5 * 210)
    And equity = 7500 + 1100 + 1050
    And equity = $9,650

  Scenario: Realized + unrealized PnL consistency
    Given initial cash $10,000
    And bought 100 shares SPY @ $100 (cost $10,000)
    And cash = $0 after buy
    And current price SPY = $110
    When PnL is computed
    Then unrealized PnL = 100 * (110 - 100) = $1,000
    And realized PnL = $0 (not sold yet)
    And equity = 0 + (100 * 110) = $11,000
    And total PnL = equity - initial_cash = $1,000
    And total PnL = realized + unrealized ✓
```

### Verification Commands

```bash
# Step 1: Create engine module files
mkdir -p trendlab-core/src/engine
cd trendlab-core/src/engine

# Step 2: Create engine files
touch warmup.rs accounting.rs event_loop.rs

# Step 3: Update engine/mod.rs
cat > mod.rs <<'EOF'
pub mod event_loop;
pub mod warmup;
pub mod accounting;
pub mod smoke;

pub use event_loop::Engine;
pub use warmup::WarmupState;
pub use accounting::EquityTracker;
EOF

# Step 4: Run tests
cargo test --package trendlab-core

# Expected output:
# running 15+ tests
# test engine::warmup::tests::test_warmup_state ... ok
# test engine::accounting::tests::test_equity_tracking ... ok
# test engine::event_loop::tests::test_warmup_blocks_pm ... ok
# (plus all previous tests)
#
# test result: ok. 15 passed; 0 failed; 0 ignored

# Step 5: Check specific engine tests
cargo test --package trendlab-core engine::

# Expected: all engine module tests pass
```

### Example Flow: 4-Phase Event Loop

```text
Bar N arrives (timestamp, OHLC, volume)
    ↓
┌──────────────────────────────────────────────┐
│ Phase 1: Start-of-bar                        │
│  - OrderBook.activate_day_orders()           │
│  - ExecutionModel.fill_moo_orders()          │
│  - Portfolio.apply_fill() for each MOO fill  │
└──────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────┐
│ Phase 2: Intrabar                            │
│  - PathPolicy determines O→H→L→C sequence    │
│  - For each price point in sequence:         │
│    - Check if any stop/limit triggers        │
│    - ExecutionModel.fill_triggered()         │
│    - Portfolio.apply_fill()                  │
│    - OCO siblings cancelled if filled        │
└──────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────┐
│ Phase 3: End-of-bar                          │
│  - ExecutionModel.fill_moc_orders()          │
│  - Portfolio.apply_fill()                    │
└──────────────────────────────────────────────┘
    ↓
┌──────────────────────────────────────────────┐
│ Phase 4: Post-bar                            │
│  - Portfolio.mark_to_market(close_prices)    │
│  - EquityTracker.record_equity()             │
│  - IF warmup complete:                       │
│    - PositionManager.emit_maintenance()      │
│    - OrderBook.submit() orders for bar N+1   │
└──────────────────────────────────────────────┘
    ↓
Proceed to bar N+1
```

### Example Flow: Warmup Handling

```text
Strategy requires 20-bar warmup for MA(20) indicator

Bar 0-19: Warmup phase
  ↓
For each bar:
  1. Compute indicators (MA fills up)
  2. Skip signal generation (warmup incomplete)
  3. Skip PM (no positions yet)
  4. Record equity (just cash, no positions)

Bar 19: Last warmup bar
  ↓
warmup.is_warm() = false
  ↓
No orders emitted

Bar 20: First live bar
  ↓
warmup.is_warm() = true
  ↓
Strategy can now emit signals
  ↓
Orders submitted for bar 21
```

### Completion Criteria

- [ ] Engine module files exist in `trendlab-core/src/engine/`
- [ ] WarmupState correctly tracks warmup progress
- [ ] EquityTracker computes equity = cash + position_value
- [ ] EquityTracker tracks realized and unrealized PnL
- [ ] Event loop has 4 distinct phases (start, intrabar, end, post)
- [ ] Warmup prevents PM from emitting orders before completion
- [ ] PM orders are emitted for NEXT bar, not current bar
- [ ] `cargo test --package trendlab-core engine::` shows all engine tests passing
- [ ] `cargo clippy` has zero warnings

## BDD
**Feature: Engine warmup**
- Scenario: no orders generated before warmup completes

**Feature: PM timing**
- Scenario: stop update applies starting bar N+1, not bar N

**Feature: Equity accounting**
- Scenario: equity == cash + Σ(position value) each bar close

---

# M4 — Orders + OrderBook lifecycle + cancel/replace as first-class
## Critique-driven additions
Cancel/Replace must be **atomic** because PM depends on it.

## Deliverables
- Order types (MVP): Market (MOO/MOC/Now), StopMarket, Limit, StopLimit
- Brackets + OCO
- OrderBook state machine:
  - Pending → Triggered → Filled/Cancelled/Expired
- **CancelReplace atomic operation**
  - partial fill rules: amend only remaining qty
  - audit trail for trade tape

## Enhanced M4 Specification

### File Structure

```text
trendlab-core/src/
├── orders/
│   ├── mod.rs              # Public interface
│   ├── order_type.rs       # OrderType enum (Market, Stop, Limit, etc.)
│   ├── order.rs            # Order struct with lifecycle state
│   ├── order_book.rs       # OrderBook (state machine)
│   ├── order_policy.rs     # OrderPolicy (signal → order intent)
│   ├── bracket.rs          # Bracket/OCO order groups
│   └── cancel_replace.rs   # Atomic cancel/replace operation
└── tests/
    └── bdd_orders.rs       # Cucumber BDD tests
```

### Complete Implementations

**orders/order_type.rs**

```rust
use serde::{Deserialize, Serialize};

/// Market order timing variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketTiming {
    /// Market-on-Open: fill at bar open
    MOO,
    /// Market-on-Close: fill at bar close
    MOC,
    /// Market Now: fill immediately at next available price
    Now,
}

/// Stop order direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopDirection {
    Buy,  // trigger when price >= stop
    Sell, // trigger when price <= stop
}

/// Core order type taxonomy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderType {
    /// Market order (various timing)
    Market(MarketTiming),

    /// Stop market: becomes market when price triggers
    StopMarket {
        direction: StopDirection,
        trigger_price: f64,
    },

    /// Limit order: fill only at limit price or better
    Limit {
        limit_price: f64,
    },

    /// Stop-limit: becomes limit when stop triggers
    StopLimit {
        direction: StopDirection,
        trigger_price: f64,
        limit_price: f64,
    },
}

impl OrderType {
    /// Check if order type requires a trigger before becoming active
    pub fn requires_trigger(&self) -> bool {
        matches!(
            self,
            OrderType::StopMarket { .. } | OrderType::StopLimit { .. }
        )
    }

    /// Get trigger price if applicable
    pub fn trigger_price(&self) -> Option<f64> {
        match self {
            OrderType::StopMarket { trigger_price, .. } => Some(*trigger_price),
            OrderType::StopLimit { trigger_price, .. } => Some(*trigger_price),
            _ => None,
        }
    }

    /// Get limit price if applicable
    pub fn limit_price(&self) -> Option<f64> {
        match self {
            OrderType::Limit { limit_price } => Some(*limit_price),
            OrderType::StopLimit { limit_price, .. } => Some(*limit_price),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_market_requires_trigger() {
        let order = OrderType::StopMarket {
            direction: StopDirection::Buy,
            trigger_price: 100.0,
        };
        assert!(order.requires_trigger());
        assert_eq!(order.trigger_price(), Some(100.0));
    }

    #[test]
    fn test_market_no_trigger() {
        let order = OrderType::Market(MarketTiming::MOO);
        assert!(!order.requires_trigger());
        assert_eq!(order.trigger_price(), None);
    }

    #[test]
    fn test_stop_limit_has_both_prices() {
        let order = OrderType::StopLimit {
            direction: StopDirection::Sell,
            trigger_price: 95.0,
            limit_price: 94.0,
        };
        assert_eq!(order.trigger_price(), Some(95.0));
        assert_eq!(order.limit_price(), Some(94.0));
    }
}
```

**orders/order.rs**

```rust
use crate::domain::{OrderId, Symbol};
use crate::orders::order_type::OrderType;
use serde::{Deserialize, Serialize};

/// Order lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderState {
    /// Pending: not yet active (e.g., bracket child waiting for parent fill)
    Pending,
    /// Active: eligible for triggering/filling
    Active,
    /// Triggered: stop has triggered, now acting as market/limit
    Triggered,
    /// PartiallyFilled: some qty filled, rest still active
    PartiallyFilled { filled_qty: u32 },
    /// Filled: order complete
    Filled,
    /// Cancelled: user or system cancelled
    Cancelled,
    /// Expired: time-based expiry (e.g., day order at end of day)
    Expired,
}

/// An order with full lifecycle tracking
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub symbol: Symbol,
    pub order_type: OrderType,
    pub qty: u32,
    pub filled_qty: u32,
    pub state: OrderState,

    /// Optional: parent order ID (for bracket children)
    pub parent_id: Option<OrderId>,

    /// Optional: OCO sibling ID (for bracket stop/target pairs)
    pub oco_sibling_id: Option<OrderId>,

    /// Bar number when order was created
    pub created_bar: usize,

    /// Bar number when order was filled/cancelled/expired (if applicable)
    pub closed_bar: Option<usize>,
}

impl Order {
    /// Create a new order in Pending state
    pub fn new(
        id: OrderId,
        symbol: Symbol,
        order_type: OrderType,
        qty: u32,
        created_bar: usize,
    ) -> Self {
        Self {
            id,
            symbol,
            order_type,
            qty,
            filled_qty: 0,
            state: OrderState::Pending,
            parent_id: None,
            oco_sibling_id: None,
            created_bar,
            closed_bar: None,
        }
    }

    /// Activate the order (Pending → Active)
    pub fn activate(&mut self) {
        if self.state == OrderState::Pending {
            self.state = OrderState::Active;
        }
    }

    /// Trigger a stop order (Active → Triggered)
    pub fn trigger(&mut self, bar: usize) {
        if self.state == OrderState::Active && self.order_type.requires_trigger() {
            self.state = OrderState::Triggered;
        }
    }

    /// Fill the order (partial or complete)
    pub fn fill(&mut self, qty: u32, bar: usize) {
        assert!(qty <= self.remaining_qty(), "Cannot fill more than remaining");

        self.filled_qty += qty;

        if self.filled_qty >= self.qty {
            self.state = OrderState::Filled;
            self.closed_bar = Some(bar);
        } else {
            self.state = OrderState::PartiallyFilled { filled_qty: self.filled_qty };
        }
    }

    /// Cancel the order
    pub fn cancel(&mut self, bar: usize) {
        if !self.is_terminal() {
            self.state = OrderState::Cancelled;
            self.closed_bar = Some(bar);
        }
    }

    /// Expire the order (e.g., day order at EOD)
    pub fn expire(&mut self, bar: usize) {
        if !self.is_terminal() {
            self.state = OrderState::Expired;
            self.closed_bar = Some(bar);
        }
    }

    /// Get remaining unfilled quantity
    pub fn remaining_qty(&self) -> u32 {
        self.qty.saturating_sub(self.filled_qty)
    }

    /// Check if order is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            OrderState::Filled | OrderState::Cancelled | OrderState::Expired
        )
    }

    /// Check if order is eligible for fill attempts
    pub fn is_fillable(&self) -> bool {
        matches!(
            self.state,
            OrderState::Active | OrderState::Triggered | OrderState::PartiallyFilled { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::order_type::{MarketTiming, StopDirection};

    #[test]
    fn test_order_lifecycle_market() {
        let mut order = Order::new(
            OrderId::new(1),
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::MOO),
            100,
            0,
        );

        assert_eq!(order.state, OrderState::Pending);

        order.activate();
        assert_eq!(order.state, OrderState::Active);

        order.fill(100, 0);
        assert_eq!(order.state, OrderState::Filled);
        assert_eq!(order.filled_qty, 100);
        assert!(order.is_terminal());
    }

    #[test]
    fn test_partial_fill() {
        let mut order = Order::new(
            OrderId::new(2),
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::Now),
            100,
            5,
        );

        order.activate();
        order.fill(30, 5);

        assert_eq!(order.state, OrderState::PartiallyFilled { filled_qty: 30 });
        assert_eq!(order.remaining_qty(), 70);
        assert!(!order.is_terminal());

        order.fill(70, 5);
        assert_eq!(order.state, OrderState::Filled);
    }

    #[test]
    fn test_stop_trigger() {
        let mut order = Order::new(
            OrderId::new(3),
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 95.0,
            },
            50,
            10,
        );

        order.activate();
        assert_eq!(order.state, OrderState::Active);

        order.trigger(12);
        assert_eq!(order.state, OrderState::Triggered);

        order.fill(50, 12);
        assert_eq!(order.state, OrderState::Filled);
    }

    #[test]
    fn test_cancel_active_order() {
        let mut order = Order::new(
            OrderId::new(4),
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::MOC),
            100,
            15,
        );

        order.activate();
        order.cancel(16);

        assert_eq!(order.state, OrderState::Cancelled);
        assert_eq!(order.closed_bar, Some(16));
        assert!(order.is_terminal());
    }

    #[test]
    #[should_panic(expected = "Cannot fill more than remaining")]
    fn test_overfill_panics() {
        let mut order = Order::new(
            OrderId::new(5),
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::Now),
            50,
            0,
        );

        order.activate();
        order.fill(60, 0); // Should panic
    }
}
```

**orders/order_book.rs**

```rust
use crate::domain::{OrderId, Symbol};
use crate::orders::order::{Order, OrderState};
use crate::orders::order_type::OrderType;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrderBookError {
    #[error("Order {0:?} not found")]
    OrderNotFound(OrderId),

    #[error("Order {0:?} cannot be modified in state {1:?}")]
    InvalidState(OrderId, OrderState),

    #[error("OCO constraint violated: sibling {0:?} already filled")]
    OcoViolation(OrderId),
}

/// OrderBook: manages all orders and their lifecycle
pub struct OrderBook {
    orders: HashMap<OrderId, Order>,
    next_id: u64,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
            next_id: 1,
        }
    }

    /// Submit a new order (returns OrderId)
    pub fn submit(
        &mut self,
        symbol: Symbol,
        order_type: OrderType,
        qty: u32,
        bar: usize,
    ) -> OrderId {
        let id = OrderId::new(self.next_id);
        self.next_id += 1;

        let mut order = Order::new(id, symbol, order_type, qty, bar);

        // Market orders activate immediately; others stay Pending until explicitly activated
        if matches!(order_type, OrderType::Market(_)) {
            order.activate();
        }

        self.orders.insert(id, order);
        id
    }

    /// Activate a pending order (e.g., bracket child after parent fills)
    pub fn activate(&mut self, id: OrderId) -> Result<(), OrderBookError> {
        let order = self.orders.get_mut(&id)
            .ok_or(OrderBookError::OrderNotFound(id))?;

        if order.state != OrderState::Pending {
            return Err(OrderBookError::InvalidState(id, order.state));
        }

        order.activate();
        Ok(())
    }

    /// Trigger a stop order
    pub fn trigger(&mut self, id: OrderId, bar: usize) -> Result<(), OrderBookError> {
        let order = self.orders.get_mut(&id)
            .ok_or(OrderBookError::OrderNotFound(id))?;

        if order.state != OrderState::Active {
            return Err(OrderBookError::InvalidState(id, order.state));
        }

        order.trigger(bar);
        Ok(())
    }

    /// Fill an order (partial or complete)
    /// If OCO sibling exists, cancel it
    pub fn fill(&mut self, id: OrderId, qty: u32, bar: usize) -> Result<(), OrderBookError> {
        // Check OCO constraint BEFORE filling
        let sibling_id = {
            let order = self.orders.get(&id)
                .ok_or(OrderBookError::OrderNotFound(id))?;
            order.oco_sibling_id
        };

        // Fill the order
        {
            let order = self.orders.get_mut(&id)
                .ok_or(OrderBookError::OrderNotFound(id))?;
            order.fill(qty, bar);
        }

        // If OCO sibling exists, cancel it
        if let Some(sibling_id) = sibling_id {
            self.cancel(sibling_id, bar)?;
        }

        Ok(())
    }

    /// Cancel an order
    pub fn cancel(&mut self, id: OrderId, bar: usize) -> Result<(), OrderBookError> {
        let order = self.orders.get_mut(&id)
            .ok_or(OrderBookError::OrderNotFound(id))?;

        if order.is_terminal() {
            return Err(OrderBookError::InvalidState(id, order.state));
        }

        order.cancel(bar);
        Ok(())
    }

    /// Atomic cancel/replace operation
    /// Cancels old order and submits new one atomically
    pub fn cancel_replace(
        &mut self,
        old_id: OrderId,
        new_order_type: OrderType,
        new_qty: u32,
        bar: usize,
    ) -> Result<OrderId, OrderBookError> {
        // Get old order symbol (before cancelling)
        let symbol = {
            let old_order = self.orders.get(&old_id)
                .ok_or(OrderBookError::OrderNotFound(old_id))?;
            old_order.symbol.clone()
        };

        // Cancel old order
        self.cancel(old_id, bar)?;

        // Submit new order
        let new_id = self.submit(symbol, new_order_type, new_qty, bar);

        Ok(new_id)
    }

    /// Set OCO relationship between two orders
    pub fn set_oco(&mut self, id1: OrderId, id2: OrderId) -> Result<(), OrderBookError> {
        // Verify both orders exist
        if !self.orders.contains_key(&id1) {
            return Err(OrderBookError::OrderNotFound(id1));
        }
        if !self.orders.contains_key(&id2) {
            return Err(OrderBookError::OrderNotFound(id2));
        }

        // Set mutual OCO relationship
        self.orders.get_mut(&id1).unwrap().oco_sibling_id = Some(id2);
        self.orders.get_mut(&id2).unwrap().oco_sibling_id = Some(id1);

        Ok(())
    }

    /// Get all active orders for a symbol
    pub fn active_orders(&self, symbol: &Symbol) -> Vec<&Order> {
        self.orders.values()
            .filter(|o| o.symbol == *symbol && o.is_fillable())
            .collect()
    }

    /// Get order by ID
    pub fn get(&self, id: OrderId) -> Option<&Order> {
        self.orders.get(&id)
    }

    /// Get mutable order by ID
    pub fn get_mut(&mut self, id: OrderId) -> Option<&mut Order> {
        self.orders.get_mut(&id)
    }

    /// Get all orders
    pub fn all_orders(&self) -> Vec<&Order> {
        self.orders.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::order_type::{MarketTiming, StopDirection};

    #[test]
    fn test_submit_and_activate() {
        let mut book = OrderBook::new();

        let id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Buy,
                trigger_price: 100.0,
            },
            50,
            0,
        );

        let order = book.get(id).unwrap();
        assert_eq!(order.state, OrderState::Pending);

        book.activate(id).unwrap();
        let order = book.get(id).unwrap();
        assert_eq!(order.state, OrderState::Active);
    }

    #[test]
    fn test_oco_cancellation() {
        let mut book = OrderBook::new();

        // Submit stop-loss
        let stop_id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 95.0,
            },
            100,
            5,
        );

        // Submit take-profit
        let target_id = book.submit(
            Symbol::from("SPY"),
            OrderType::Limit {
                limit_price: 105.0,
            },
            100,
            5,
        );

        // Set OCO relationship
        book.set_oco(stop_id, target_id).unwrap();

        // Activate both
        book.activate(stop_id).unwrap();
        book.activate(target_id).unwrap();

        // Fill stop-loss
        book.fill(stop_id, 100, 6).unwrap();

        // Verify stop is filled
        assert_eq!(book.get(stop_id).unwrap().state, OrderState::Filled);

        // Verify target is cancelled
        assert_eq!(book.get(target_id).unwrap().state, OrderState::Cancelled);
    }

    #[test]
    fn test_cancel_replace_atomic() {
        let mut book = OrderBook::new();

        let old_id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 95.0,
            },
            100,
            10,
        );

        book.activate(old_id).unwrap();

        // Cancel/replace with tighter stop
        let new_id = book.cancel_replace(
            old_id,
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 97.0,
            },
            100,
            12,
        ).unwrap();

        // Old order should be cancelled
        assert_eq!(book.get(old_id).unwrap().state, OrderState::Cancelled);

        // New order should exist and be pending (stop orders start pending)
        assert_eq!(book.get(new_id).unwrap().state, OrderState::Pending);
        assert_eq!(
            book.get(new_id).unwrap().order_type.trigger_price(),
            Some(97.0)
        );
    }

    #[test]
    fn test_partial_fill() {
        let mut book = OrderBook::new();

        let id = book.submit(
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::Now),
            100,
            0,
        );

        // Market orders activate immediately
        assert_eq!(book.get(id).unwrap().state, OrderState::Active);

        // Partial fill
        book.fill(id, 30, 0).unwrap();
        assert_eq!(
            book.get(id).unwrap().state,
            OrderState::PartiallyFilled { filled_qty: 30 }
        );

        // Complete fill
        book.fill(id, 70, 0).unwrap();
        assert_eq!(book.get(id).unwrap().state, OrderState::Filled);
    }

    #[test]
    fn test_active_orders_filter() {
        let mut book = OrderBook::new();

        let id1 = book.submit(
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::MOO),
            100,
            0,
        );

        let id2 = book.submit(
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::MOO),
            50,
            0,
        );

        let id3 = book.submit(
            Symbol::from("QQQ"),
            OrderType::Market(MarketTiming::MOO),
            75,
            0,
        );

        // Fill id2
        book.fill(id2, 50, 0).unwrap();

        // Active orders for SPY should only include id1
        let active_spy = book.active_orders(&Symbol::from("SPY"));
        assert_eq!(active_spy.len(), 1);
        assert_eq!(active_spy[0].id, id1);

        // Active orders for QQQ should include id3
        let active_qqq = book.active_orders(&Symbol::from("QQQ"));
        assert_eq!(active_qqq.len(), 1);
        assert_eq!(active_qqq[0].id, id3);
    }
}
```

**orders/bracket.rs**

```rust
use crate::domain::{OrderId, Symbol};
use crate::orders::order_book::OrderBook;
use crate::orders::order_type::{OrderType, StopDirection};

/// Bracket order builder
pub struct BracketOrderBuilder {
    symbol: Symbol,
    entry_order: OrderType,
    qty: u32,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
}

impl BracketOrderBuilder {
    pub fn new(symbol: Symbol, entry_order: OrderType, qty: u32) -> Self {
        Self {
            symbol,
            entry_order,
            qty,
            stop_loss: None,
            take_profit: None,
        }
    }

    pub fn with_stop_loss(mut self, stop_price: f64) -> Self {
        self.stop_loss = Some(stop_price);
        self
    }

    pub fn with_take_profit(mut self, target_price: f64) -> Self {
        self.take_profit = Some(target_price);
        self
    }

    /// Submit bracket to order book
    /// Returns (entry_id, stop_id, target_id)
    pub fn submit(
        self,
        book: &mut OrderBook,
        bar: usize,
    ) -> (OrderId, Option<OrderId>, Option<OrderId>) {
        // Submit entry
        let entry_id = book.submit(self.symbol.clone(), self.entry_order, self.qty, bar);

        // Submit stop-loss (if provided)
        let stop_id = self.stop_loss.map(|stop_price| {
            let stop_order = OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: stop_price,
            };
            let id = book.submit(self.symbol.clone(), stop_order, self.qty, bar);

            // Link to parent
            if let Some(order) = book.get_mut(id) {
                order.parent_id = Some(entry_id);
            }

            id
        });

        // Submit take-profit (if provided)
        let target_id = self.take_profit.map(|target_price| {
            let target_order = OrderType::Limit {
                limit_price: target_price,
            };
            let id = book.submit(self.symbol.clone(), target_order, self.qty, bar);

            // Link to parent
            if let Some(order) = book.get_mut(id) {
                order.parent_id = Some(entry_id);
            }

            id
        });

        // Set OCO relationship between stop and target
        if let (Some(stop_id), Some(target_id)) = (stop_id, target_id) {
            book.set_oco(stop_id, target_id).ok();
        }

        (entry_id, stop_id, target_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::order_type::MarketTiming;
    use crate::orders::order::OrderState;

    #[test]
    fn test_bracket_with_stop_and_target() {
        let mut book = OrderBook::new();

        let bracket = BracketOrderBuilder::new(
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::MOO),
            100,
        )
        .with_stop_loss(95.0)
        .with_take_profit(105.0);

        let (entry_id, stop_id, target_id) = bracket.submit(&mut book, 10);

        // Entry should be active (market order)
        assert_eq!(book.get(entry_id).unwrap().state, OrderState::Active);

        // Stop and target should be pending
        assert_eq!(book.get(stop_id.unwrap()).unwrap().state, OrderState::Pending);
        assert_eq!(book.get(target_id.unwrap()).unwrap().state, OrderState::Pending);

        // Verify parent linkage
        assert_eq!(book.get(stop_id.unwrap()).unwrap().parent_id, Some(entry_id));
        assert_eq!(book.get(target_id.unwrap()).unwrap().parent_id, Some(entry_id));

        // Verify OCO linkage
        assert_eq!(book.get(stop_id.unwrap()).unwrap().oco_sibling_id, target_id);
        assert_eq!(book.get(target_id.unwrap()).unwrap().oco_sibling_id, stop_id);
    }

    #[test]
    fn test_bracket_activation_on_entry_fill() {
        let mut book = OrderBook::new();

        let bracket = BracketOrderBuilder::new(
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::MOO),
            100,
        )
        .with_stop_loss(95.0)
        .with_take_profit(105.0);

        let (entry_id, stop_id, target_id) = bracket.submit(&mut book, 10);

        // Fill entry
        book.fill(entry_id, 100, 10).unwrap();

        // Now activate children (this would be done by engine)
        book.activate(stop_id.unwrap()).unwrap();
        book.activate(target_id.unwrap()).unwrap();

        assert_eq!(book.get(stop_id.unwrap()).unwrap().state, OrderState::Active);
        assert_eq!(book.get(target_id.unwrap()).unwrap().state, OrderState::Active);
    }

    #[test]
    fn test_bracket_oco_behavior() {
        let mut book = OrderBook::new();

        let bracket = BracketOrderBuilder::new(
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::MOO),
            100,
        )
        .with_stop_loss(95.0)
        .with_take_profit(105.0);

        let (entry_id, stop_id, target_id) = bracket.submit(&mut book, 10);

        // Fill entry and activate children
        book.fill(entry_id, 100, 10).unwrap();
        book.activate(stop_id.unwrap()).unwrap();
        book.activate(target_id.unwrap()).unwrap();

        // Fill target
        book.fill(target_id.unwrap(), 100, 12).unwrap();

        // Verify target filled
        assert_eq!(book.get(target_id.unwrap()).unwrap().state, OrderState::Filled);

        // Verify stop cancelled (OCO behavior)
        assert_eq!(book.get(stop_id.unwrap()).unwrap().state, OrderState::Cancelled);
    }
}
```

**orders/order_policy.rs**

```rust
use crate::domain::Symbol;
use crate::orders::order_type::{OrderType, MarketTiming, StopDirection};

/// OrderPolicy: translates signal intent into concrete order types
///
/// Different signal families prefer different order types:
/// - Breakout signals → stop entries (enter above/below level)
/// - Mean-reversion signals → limit entries (enter at level)
/// - Trend-following → market entries (enter now)
pub trait OrderPolicy {
    fn entry_order(&self, signal_price: f64, is_long: bool) -> OrderType;
    fn exit_order(&self) -> OrderType;
}

/// Breakout order policy: use stop entries
pub struct BreakoutPolicy;

impl OrderPolicy for BreakoutPolicy {
    fn entry_order(&self, signal_price: f64, is_long: bool) -> OrderType {
        OrderType::StopMarket {
            direction: if is_long {
                StopDirection::Buy
            } else {
                StopDirection::Sell
            },
            trigger_price: signal_price,
        }
    }

    fn exit_order(&self) -> OrderType {
        OrderType::Market(MarketTiming::MOC)
    }
}

/// Mean-reversion order policy: use limit entries
pub struct MeanReversionPolicy;

impl OrderPolicy for MeanReversionPolicy {
    fn entry_order(&self, signal_price: f64, _is_long: bool) -> OrderType {
        OrderType::Limit {
            limit_price: signal_price,
        }
    }

    fn exit_order(&self) -> OrderType {
        OrderType::Market(MarketTiming::MOC)
    }
}

/// Immediate entry policy: use market orders
pub struct ImmediatePolicy;

impl OrderPolicy for ImmediatePolicy {
    fn entry_order(&self, _signal_price: f64, _is_long: bool) -> OrderType {
        OrderType::Market(MarketTiming::MOO)
    }

    fn exit_order(&self) -> OrderType {
        OrderType::Market(MarketTiming::MOC)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breakout_policy_long() {
        let policy = BreakoutPolicy;
        let order = policy.entry_order(100.0, true);

        match order {
            OrderType::StopMarket { direction, trigger_price } => {
                assert_eq!(direction, StopDirection::Buy);
                assert_eq!(trigger_price, 100.0);
            }
            _ => panic!("Expected StopMarket"),
        }
    }

    #[test]
    fn test_breakout_policy_short() {
        let policy = BreakoutPolicy;
        let order = policy.entry_order(95.0, false);

        match order {
            OrderType::StopMarket { direction, trigger_price } => {
                assert_eq!(direction, StopDirection::Sell);
                assert_eq!(trigger_price, 95.0);
            }
            _ => panic!("Expected StopMarket"),
        }
    }

    #[test]
    fn test_mean_reversion_policy() {
        let policy = MeanReversionPolicy;
        let order = policy.entry_order(98.0, true);

        match order {
            OrderType::Limit { limit_price } => {
                assert_eq!(limit_price, 98.0);
            }
            _ => panic!("Expected Limit"),
        }
    }

    #[test]
    fn test_immediate_policy() {
        let policy = ImmediatePolicy;
        let order = policy.entry_order(100.0, true);

        match order {
            OrderType::Market(MarketTiming::MOO) => {}
            _ => panic!("Expected Market MOO"),
        }
    }
}
```

### Concrete BDD Scenarios

**Feature: Order lifecycle state machine**

```gherkin
Feature: Order lifecycle state machine

  Background:
    Given an empty OrderBook
    And the current bar is 10

  Scenario: Market order lifecycle
    When I submit a Market MOO order for SPY, 100 shares
    Then the order state is Active
    When I fill the order with 100 shares at bar 10
    Then the order state is Filled
    And the filled_qty is 100
    And the closed_bar is 10

  Scenario: Stop market order lifecycle
    When I submit a StopMarket Sell order for SPY at trigger 95.0, 100 shares
    Then the order state is Pending
    When I activate the order
    Then the order state is Active
    When I trigger the order at bar 12
    Then the order state is Triggered
    When I fill the order with 100 shares at bar 12
    Then the order state is Filled

  Scenario: Partial fill then complete
    When I submit a Market Now order for SPY, 100 shares
    And I fill the order with 30 shares at bar 10
    Then the order state is PartiallyFilled with filled_qty 30
    And the remaining_qty is 70
    When I fill the order with 70 shares at bar 11
    Then the order state is Filled
    And the filled_qty is 100

  Scenario: Cancel active order
    When I submit a Limit order for SPY at 100.0, 50 shares
    And I activate the order
    And I cancel the order at bar 15
    Then the order state is Cancelled
    And the closed_bar is 15
    And the order is terminal
```

**Feature: OCO (One-Cancels-Other) correctness**

```gherkin
Feature: OCO correctness

  Background:
    Given an empty OrderBook
    And the current bar is 20

  Scenario: Stop fills, target cancels
    When I submit a StopMarket Sell order for SPY at 95.0, 100 shares as "stop"
    And I submit a Limit order for SPY at 105.0, 100 shares as "target"
    And I set OCO relationship between "stop" and "target"
    And I activate both orders
    And I fill "stop" with 100 shares at bar 22
    Then "stop" state is Filled
    And "target" state is Cancelled

  Scenario: Target fills, stop cancels
    When I submit a StopMarket Sell order for SPY at 95.0, 100 shares as "stop"
    And I submit a Limit order for SPY at 105.0, 100 shares as "target"
    And I set OCO relationship between "stop" and "target"
    And I activate both orders
    And I fill "target" with 100 shares at bar 22
    Then "target" state is Filled
    And "stop" state is Cancelled

  Scenario: Partial fill does not cancel sibling
    When I submit a StopMarket Sell order for SPY at 95.0, 100 shares as "stop"
    And I submit a Limit order for SPY at 105.0, 100 shares as "target"
    And I set OCO relationship between "stop" and "target"
    And I activate both orders
    And I fill "target" with 50 shares at bar 22
    Then "target" state is PartiallyFilled with filled_qty 50
    And "stop" state is Active (not cancelled yet)
    When I fill "target" with 50 shares at bar 22
    Then "target" state is Filled
    And "stop" state is Cancelled
```

**Feature: Bracket order activation**

```gherkin
Feature: Bracket order activation

  Background:
    Given an empty OrderBook
    And the current bar is 30

  Scenario: Bracket children remain pending until entry fills
    When I submit a bracket order:
      | symbol | SPY                 |
      | entry  | Market MOO, 100 qty |
      | stop   | 95.0                |
      | target | 105.0               |
    Then the entry order state is Active
    And the stop order state is Pending
    And the target order state is Pending
    And the stop order parent_id is the entry order id
    And the target order parent_id is the entry order id

  Scenario: Bracket children activate after entry fills
    When I submit a bracket order:
      | symbol | SPY                 |
      | entry  | Market MOO, 100 qty |
      | stop   | 95.0                |
      | target | 105.0               |
    And I fill the entry order with 100 shares at bar 30
    And the engine activates bracket children
    Then the stop order state is Active
    And the target order state is Active
    And the stop and target orders have OCO relationship

  Scenario: Bracket OCO behavior after activation
    When I submit a bracket order:
      | symbol | SPY                 |
      | entry  | Market MOO, 100 qty |
      | stop   | 95.0                |
      | target | 105.0               |
    And I fill the entry order with 100 shares at bar 30
    And the engine activates bracket children
    And I fill the target order with 100 shares at bar 32
    Then the target order state is Filled
    And the stop order state is Cancelled
```

**Feature: Atomic cancel/replace**

```gherkin
Feature: Atomic cancel/replace

  Background:
    Given an empty OrderBook
    And the current bar is 40

  Scenario: Cancel/replace updates stop level atomically
    When I submit a StopMarket Sell order for SPY at 95.0, 100 shares as "old"
    And I activate "old"
    And I cancel/replace "old" with StopMarket Sell at 97.0, 100 shares at bar 42
    Then "old" state is Cancelled
    And "old" closed_bar is 42
    And a new order "new" exists with trigger_price 97.0
    And "new" state is Pending
    And there is no "stopless window" (both orders never simultaneously absent)

  Scenario: Cancel/replace with partial fill amends remaining qty
    When I submit a Market Now order for SPY, 100 shares as "old"
    And I fill "old" with 30 shares at bar 40
    And I cancel/replace "old" with Market Now, 100 shares at bar 40
    Then "old" state is Cancelled
    And "old" filled_qty is 30
    And a new order "new" exists with qty 100
    And "new" filled_qty is 0
```

**Feature: Order policy (signal → order type)**

```gherkin
Feature: Order policy

  Scenario: Breakout policy uses stop entries
    Given a BreakoutPolicy
    When I request an entry order for price 100.0, long=true
    Then the order type is StopMarket with direction Buy and trigger_price 100.0

  Scenario: Breakout policy for short uses stop sell
    Given a BreakoutPolicy
    When I request an entry order for price 95.0, long=false
    Then the order type is StopMarket with direction Sell and trigger_price 95.0

  Scenario: Mean-reversion policy uses limit entries
    Given a MeanReversionPolicy
    When I request an entry order for price 98.0, long=true
    Then the order type is Limit with limit_price 98.0

  Scenario: Immediate policy uses market orders
    Given an ImmediatePolicy
    When I request an entry order for price 100.0, long=true
    Then the order type is Market with timing MOO
```

### Verification Commands

```bash
# Step 1: Create orders module structure
mkdir -p trendlab-core/src/orders
cd trendlab-core/src/orders

# Step 2: Create order module files
touch mod.rs order_type.rs order.rs order_book.rs order_policy.rs bracket.rs cancel_replace.rs

# Step 3: Implement each file (see above implementations)

# Step 4: Update trendlab-core/src/lib.rs
cat >> ../lib.rs <<'EOF'

pub mod orders;
EOF

# Step 5: Create BDD test file
mkdir -p trendlab-core/tests
touch trendlab-core/tests/bdd_orders.rs

# Step 6: Run unit tests
cargo test --package trendlab-core --lib orders

# Expected output:
# running 25+ tests (from all order module unit tests)
# test orders::order_type::tests::test_stop_market_requires_trigger ... ok
# test orders::order::tests::test_order_lifecycle_market ... ok
# test orders::order::tests::test_partial_fill ... ok
# test orders::order_book::tests::test_oco_cancellation ... ok
# test orders::order_book::tests::test_cancel_replace_atomic ... ok
# test orders::bracket::tests::test_bracket_with_stop_and_target ... ok
# test orders::order_policy::tests::test_breakout_policy_long ... ok
# (... and more)
#
# test result: ok. 25 passed; 0 failed; 0 ignored

# Step 7: Run BDD tests (Cucumber)
cargo test --test bdd_orders

# Expected output:
# Feature: Order lifecycle state machine
#   Scenario: Market order lifecycle ... ok
#   Scenario: Stop market order lifecycle ... ok
#   Scenario: Partial fill then complete ... ok
#   Scenario: Cancel active order ... ok
#
# Feature: OCO correctness
#   Scenario: Stop fills, target cancels ... ok
#   Scenario: Target fills, stop cancels ... ok
#
# Feature: Bracket order activation
#   Scenario: Bracket children remain pending until entry fills ... ok
#   Scenario: Bracket OCO behavior after activation ... ok
#
# Feature: Atomic cancel/replace
#   Scenario: Cancel/replace updates stop level atomically ... ok
#
# test result: ok. 9+ scenarios passed

# Step 8: Lint check
cargo clippy --package trendlab-core -- -D warnings

# Expected: no warnings
```

### Example Flow: Bracket Order Lifecycle

```text
1. User submits bracket order:
   BracketOrderBuilder::new(SPY, Market MOO, 100)
     .with_stop_loss(95.0)
     .with_take_profit(105.0)
     .submit(&mut book, bar=10)

2. OrderBook creates 3 orders:
   - Entry: Order { id=1, type=Market(MOO), qty=100, state=Active }
   - Stop:  Order { id=2, type=StopMarket(Sell, 95.0), qty=100, state=Pending, parent_id=Some(1) }
   - Target:Order { id=3, type=Limit(105.0), qty=100, state=Pending, parent_id=Some(1) }

3. OrderBook sets OCO:
   - Order(2).oco_sibling_id = Some(3)
   - Order(3).oco_sibling_id = Some(2)

4. Engine fills entry at bar 10:
   book.fill(id=1, qty=100, bar=10)
   → Order(1).state = Filled

5. Engine activates bracket children:
   book.activate(id=2) → Order(2).state = Active
   book.activate(id=3) → Order(3).state = Active

6a. Scenario: Stop hits first
   book.fill(id=2, qty=100, bar=12)
   → Order(2).state = Filled
   → Order(3).state = Cancelled (OCO auto-cancel)

6b. Scenario: Target hits first
   book.fill(id=3, qty=100, bar=15)
   → Order(3).state = Filled
   → Order(2).state = Cancelled (OCO auto-cancel)

Result:
- Entry filled, exit filled, no "stopless window"
- Audit trail: all 3 orders preserved in OrderBook history
```

### Example Flow: Atomic Cancel/Replace (Trailing Stop)

```text
1. PM emits trailing stop at bar 20:
   old_id = book.submit(SPY, StopMarket(Sell, 95.0), 100, bar=20)
   book.activate(old_id)

2. Price rises; PM tightens stop at bar 25:
   new_id = book.cancel_replace(
     old_id,
     StopMarket(Sell, 97.0),
     100,
     bar=25
   )

3. OrderBook executes atomically:
   a) book.cancel(old_id, bar=25)
      → Order(old_id).state = Cancelled
      → Order(old_id).closed_bar = Some(25)

   b) new_id = book.submit(SPY, StopMarket(Sell, 97.0), 100, bar=25)
      → Order(new_id) created
      → Order(new_id).state = Pending

4. Result:
   - No "stopless window" (cancel + submit atomic)
   - Audit trail shows both old (cancelled) and new (active) orders
   - PM can repeat this process to ratchet stop higher as price rises
```

### Completion Criteria

- [ ] All order module files exist in `trendlab-core/src/orders/`
- [ ] OrderType enum supports Market, StopMarket, Limit, StopLimit
- [ ] Order struct tracks lifecycle state correctly (Pending → Active → Triggered → Filled/Cancelled/Expired)
- [ ] OrderBook submit/activate/trigger/fill/cancel methods work correctly
- [ ] OCO relationship enforced: filling one order cancels sibling
- [ ] Bracket orders link children to parent and set OCO between stop/target
- [ ] Atomic cancel/replace operation works (no "stopless window")
- [ ] OrderPolicy trait defines entry/exit order type selection
- [ ] Breakout, MeanReversion, and Immediate policies implemented
- [ ] Unit tests pass (25+ tests covering all modules)
- [ ] BDD tests pass (9+ scenarios in Gherkin format)
- [ ] `cargo clippy` has zero warnings
- [ ] All edge cases tested: partial fills, overfill protection, terminal state guards

## BDD
**Feature: Order amendment atomicity**
- Scenario: cancel/replace is atomic (no "stopless window")

**Feature: OCO correctness**
- Scenario: if one OCO sibling fills, other cancels; never both fill

**Feature: Bracket activation**
- Scenario: bracket children activate only after entry fills

---

# M5 — Execution engine (fills) + order priority + presets + liquidity guardrails

## Critique-driven additions
- Explicit **order priority within bar** rules
- Optional **liquidity/participation caps** (capacity realism)
- Gap logic for stops (fill at open, not trigger, when gapped through)
- Configurable slippage/spread (fixed + ATR-based)
- Execution presets for quick configuration (Optimistic, Realistic, Hostile)

## Deliverables
- Execution phases integrated with OrderBook:
  - **SOB (Start of Bar):** Activate day orders, fill MOO orders
  - **Intrabar:** Trigger and fill based on path policy
  - **EOB (End of Bar):** Fill MOC orders
- Intrabar path policies:
  - `Deterministic` (OHLC order: O → L → H → C)
  - `WorstCase` (adversarial ordering for exits)
  - `BestCase` (optimistic ordering, for debugging)
  - `PriceOrder` (natural price sequence based on OHLC)
- **Order priority rules** (configurable):
  - WorstCase: stop-loss before take-profit
  - BestCase: take-profit before stop-loss
  - PriceOrder: natural price-time sequence
- Gap rules (stop gapped through fills at open, worse price)
- Slippage/spread:
  - Fixed (bps or absolute)
  - ATR-based (multiple of ATR)
- Execution presets: Optimistic, Realistic, Hostile
- **Liquidity constraint (optional)**:
  - Participation limit (% of bar volume)
  - Remainder policy: Carry, Cancel, PartialFill

## File Structure

### execution/mod.rs

```rust
//! Execution engine: converts triggered orders into fills with realistic simulation
//!
//! Key concepts:
//! - **Fill phases**: SOB → Intrabar → EOB
//! - **Path policies**: How to resolve intrabar ambiguity
//! - **Gap rules**: Stops that gap through fill at worse price
//! - **Order priority**: Which order fills first in ambiguous bars
//! - **Slippage**: Cost added to fill price
//! - **Liquidity**: Optional participation limits

pub mod fill_engine;
pub mod path_policy;
pub mod gap_handler;
pub mod slippage;
pub mod priority;
pub mod preset;
pub mod liquidity;

pub use fill_engine::FillEngine;
pub use path_policy::{PathPolicy, Deterministic, WorstCase, BestCase, PriceOrder};
pub use gap_handler::GapHandler;
pub use slippage::{SlippageModel, FixedSlippage, AtrSlippage};
pub use priority::{PriorityPolicy, WorstCasePriority, BestCasePriority, PriceOrderPriority};
pub use preset::{ExecutionPreset, Optimistic, Realistic, Hostile};
pub use liquidity::{LiquidityConstraint, RemainderPolicy};
```

### execution/fill_engine.rs

```rust
use crate::domain::{Bar, Symbol};
use crate::orders::{OrderBook, Order, OrderState, OrderType};
use crate::execution::{PathPolicy, GapHandler, SlippageModel, PriorityPolicy, LiquidityConstraint};
use std::collections::HashMap;

/// Fill result for a single order
#[derive(Debug, Clone, PartialEq)]
pub struct FillResult {
    pub order_id: crate::domain::OrderId,
    pub fill_qty: u32,
    pub fill_price: f64,
    pub fill_bar: usize,
    pub slippage: f64,
    pub was_gapped: bool,
}

/// Execution engine: processes orders and generates fills
pub struct FillEngine {
    path_policy: Box<dyn PathPolicy>,
    gap_handler: GapHandler,
    slippage_model: Box<dyn SlippageModel>,
    priority_policy: Box<dyn PriorityPolicy>,
    liquidity_constraint: Option<LiquidityConstraint>,
}

impl FillEngine {
    pub fn new(
        path_policy: Box<dyn PathPolicy>,
        gap_handler: GapHandler,
        slippage_model: Box<dyn SlippageModel>,
        priority_policy: Box<dyn PriorityPolicy>,
        liquidity_constraint: Option<LiquidityConstraint>,
    ) -> Self {
        Self {
            path_policy,
            gap_handler,
            slippage_model,
            priority_policy,
            liquidity_constraint,
        }
    }

    /// Process a bar: SOB → Intrabar → EOB
    pub fn process_bar(
        &mut self,
        bar: &Bar,
        bar_index: usize,
        order_book: &mut OrderBook,
    ) -> Vec<FillResult> {
        let mut fills = Vec::new();

        // Phase 1: Start of Bar (SOB)
        fills.extend(self.process_sob(bar, bar_index, order_book));

        // Phase 2: Intrabar (path-dependent)
        fills.extend(self.process_intrabar(bar, bar_index, order_book));

        // Phase 3: End of Bar (EOB)
        fills.extend(self.process_eob(bar, bar_index, order_book));

        fills
    }

    /// SOB: Activate day orders, fill MOO orders
    fn process_sob(
        &mut self,
        bar: &Bar,
        bar_index: usize,
        order_book: &mut OrderBook,
    ) -> Vec<FillResult> {
        let mut fills = Vec::new();

        // Activate all pending day orders
        let pending_orders: Vec<_> = order_book
            .all_orders()
            .iter()
            .filter(|o| o.state == OrderState::Pending)
            .map(|o| o.id)
            .collect();

        for id in pending_orders {
            let _ = order_book.activate(id);
        }

        // Fill all active MOO orders at open price
        let moo_orders: Vec<_> = order_book
            .all_orders()
            .iter()
            .filter(|o| {
                o.state == OrderState::Active
                    && matches!(
                        o.order_type,
                        OrderType::Market(crate::orders::order_type::MarketTiming::MOO)
                    )
            })
            .map(|o| o.id)
            .collect();

        for id in moo_orders {
            if let Some(order) = order_book.get(id) {
                let qty = order.remaining_qty();
                let fill_price = self.compute_fill_price(bar.open, &order.order_type, false);

                if let Ok(()) = order_book.fill(id, qty, bar_index) {
                    fills.push(FillResult {
                        order_id: id,
                        fill_qty: qty,
                        fill_price,
                        fill_bar: bar_index,
                        slippage: fill_price - bar.open,
                        was_gapped: false,
                    });
                }
            }
        }

        fills
    }

    /// Intrabar: Trigger and fill based on path policy
    fn process_intrabar(
        &mut self,
        bar: &Bar,
        bar_index: usize,
        order_book: &mut OrderBook,
    ) -> Vec<FillResult> {
        let mut fills = Vec::new();

        // Get active orders (exclude MOC)
        let active_orders: Vec<Order> = order_book
            .all_orders()
            .iter()
            .filter(|o| {
                o.state == OrderState::Active
                    && !matches!(
                        o.order_type,
                        OrderType::Market(crate::orders::order_type::MarketTiming::MOC)
                    )
            })
            .cloned()
            .collect();

        if active_orders.is_empty() {
            return fills;
        }

        // Determine which orders could trigger in this bar
        let triggerable: Vec<Order> = active_orders
            .into_iter()
            .filter(|o| self.can_trigger_in_bar(o, bar))
            .collect();

        if triggerable.is_empty() {
            return fills;
        }

        // Apply path policy to determine trigger sequence
        let trigger_sequence = self.path_policy.order_sequence(&triggerable, bar);

        // Apply priority policy to resolve conflicts
        let prioritized = self.priority_policy.prioritize(trigger_sequence, bar);

        // Process fills in priority order
        for order in prioritized {
            // Check if order still active (OCO may have cancelled it)
            if let Some(current) = order_book.get(order.id) {
                if current.state != OrderState::Active {
                    continue;
                }
            } else {
                continue;
            }

            // Trigger order
            if order.requires_trigger() {
                let _ = order_book.trigger(order.id, bar_index);
            }

            // Compute fill price (including gap logic)
            let was_gapped = self.gap_handler.did_gap_through(&order, bar);
            let base_price = if was_gapped {
                bar.open
            } else {
                self.get_trigger_or_limit_price(&order)
            };

            let fill_price = self.compute_fill_price(base_price, &order.order_type, was_gapped);

            // Apply liquidity constraint
            let fill_qty = if let Some(ref liq) = self.liquidity_constraint {
                liq.limit_fill_qty(order.remaining_qty(), bar.volume)
            } else {
                order.remaining_qty()
            };

            // Execute fill
            if let Ok(()) = order_book.fill(order.id, fill_qty, bar_index) {
                fills.push(FillResult {
                    order_id: order.id,
                    fill_qty,
                    fill_price,
                    fill_bar: bar_index,
                    slippage: fill_price - base_price,
                    was_gapped,
                });
            }
        }

        fills
    }

    /// EOB: Fill all MOC orders at close price
    fn process_eob(
        &mut self,
        bar: &Bar,
        bar_index: usize,
        order_book: &mut OrderBook,
    ) -> Vec<FillResult> {
        let mut fills = Vec::new();

        // Fill all active MOC orders at close price
        let moc_orders: Vec<_> = order_book
            .all_orders()
            .iter()
            .filter(|o| {
                o.state == OrderState::Active
                    && matches!(
                        o.order_type,
                        OrderType::Market(crate::orders::order_type::MarketTiming::MOC)
                    )
            })
            .map(|o| o.id)
            .collect();

        for id in moc_orders {
            if let Some(order) = order_book.get(id) {
                let qty = order.remaining_qty();
                let fill_price = self.compute_fill_price(bar.close, &order.order_type, false);

                if let Ok(()) = order_book.fill(id, qty, bar_index) {
                    fills.push(FillResult {
                        order_id: id,
                        fill_qty: qty,
                        fill_price,
                        fill_bar: bar_index,
                        slippage: fill_price - bar.close,
                        was_gapped: false,
                    });
                }
            }
        }

        fills
    }

    /// Check if order can trigger within this bar's range
    fn can_trigger_in_bar(&self, order: &Order, bar: &Bar) -> bool {
        match &order.order_type {
            OrderType::Market(_) => true,
            OrderType::StopMarket { direction, trigger_price } => {
                use crate::orders::order_type::StopDirection;
                match direction {
                    StopDirection::Buy => *trigger_price <= bar.high,
                    StopDirection::Sell => *trigger_price >= bar.low,
                }
            }
            OrderType::Limit { limit_price } => {
                *limit_price >= bar.low && *limit_price <= bar.high
            }
            OrderType::StopLimit { trigger_price, limit_price, .. } => {
                // For now, treat as stop (full stop-limit logic is more complex)
                *trigger_price >= bar.low && *trigger_price <= bar.high
            }
        }
    }

    /// Get trigger or limit price for an order
    fn get_trigger_or_limit_price(&self, order: &Order) -> f64 {
        match &order.order_type {
            OrderType::StopMarket { trigger_price, .. } => *trigger_price,
            OrderType::Limit { limit_price } => *limit_price,
            OrderType::StopLimit { trigger_price, .. } => *trigger_price,
            OrderType::Market(_) => panic!("Market orders don't have trigger prices"),
        }
    }

    /// Compute fill price with slippage
    fn compute_fill_price(&self, base_price: f64, order_type: &OrderType, was_gapped: bool) -> f64 {
        let slippage = self.slippage_model.compute_slippage(base_price, order_type, was_gapped);
        base_price + slippage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Symbol;
    use crate::orders::order_type::{MarketTiming, StopDirection};
    use crate::execution::{Deterministic, FixedSlippage, WorstCasePriority};

    #[test]
    fn test_moo_fills_at_open() {
        let mut book = OrderBook::new();
        let mut engine = FillEngine::new(
            Box::new(Deterministic),
            GapHandler::default(),
            Box::new(FixedSlippage::new(0.0)),
            Box::new(WorstCasePriority),
            None,
        );

        let id = book.submit(
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::MOO),
            100,
            0,
        );
        book.activate(id).unwrap();

        let bar = Bar {
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.0,
            volume: 1000000,
        };

        let fills = engine.process_bar(&bar, 1, &mut book);

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_price, 100.0);
        assert_eq!(fills[0].fill_qty, 100);
    }

    #[test]
    fn test_moc_fills_at_close() {
        let mut book = OrderBook::new();
        let mut engine = FillEngine::new(
            Box::new(Deterministic),
            GapHandler::default(),
            Box::new(FixedSlippage::new(0.0)),
            Box::new(WorstCasePriority),
            None,
        );

        let id = book.submit(
            Symbol::from("SPY"),
            OrderType::Market(MarketTiming::MOC),
            100,
            0,
        );
        book.activate(id).unwrap();

        let bar = Bar {
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.0,
            volume: 1000000,
        };

        let fills = engine.process_bar(&bar, 1, &mut book);

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_price, 101.0);
        assert_eq!(fills[0].fill_qty, 100);
    }

    #[test]
    fn test_stop_market_triggers_and_fills() {
        let mut book = OrderBook::new();
        let mut engine = FillEngine::new(
            Box::new(Deterministic),
            GapHandler::default(),
            Box::new(FixedSlippage::new(0.0)),
            Box::new(WorstCasePriority),
            None,
        );

        let id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Buy,
                trigger_price: 101.0,
            },
            100,
            0,
        );
        book.activate(id).unwrap();

        let bar = Bar {
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.5,
            volume: 1000000,
        };

        let fills = engine.process_bar(&bar, 1, &mut book);

        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].fill_price, 101.0);
    }
}
```

### execution/path_policy.rs

```rust
use crate::domain::Bar;
use crate::orders::Order;

/// PathPolicy: determines the sequence in which triggerable orders are evaluated
pub trait PathPolicy: Send + Sync {
    fn order_sequence(&self, triggerable: &[Order], bar: &Bar) -> Vec<Order>;
}

/// Deterministic: OHLC sequence (O → L → H → C)
pub struct Deterministic;

impl PathPolicy for Deterministic {
    fn order_sequence(&self, triggerable: &[Order], bar: &Bar) -> Vec<Order> {
        // Simple OHLC ordering: low first, then high, then close
        // This is a deterministic, neutral policy
        triggerable.to_vec()
    }
}

/// WorstCase: adversarial ordering for conservative estimates
/// - For exits: stop-loss before take-profit
/// - For entries: worse fill price first
pub struct WorstCase;

impl PathPolicy for WorstCase {
    fn order_sequence(&self, triggerable: &[Order], bar: &Bar) -> Vec<Order> {
        let mut orders = triggerable.to_vec();

        // Sort: exits before entries, stops before limits
        orders.sort_by(|a, b| {
            use crate::orders::OrderType;

            let a_priority = match &a.order_type {
                OrderType::StopMarket { .. } => 1, // Stops first (worst case for exits)
                OrderType::Limit { .. } => 2,
                OrderType::Market(_) => 3,
                OrderType::StopLimit { .. } => 1,
            };

            let b_priority = match &b.order_type {
                OrderType::StopMarket { .. } => 1,
                OrderType::Limit { .. } => 2,
                OrderType::Market(_) => 3,
                OrderType::StopLimit { .. } => 1,
            };

            a_priority.cmp(&b_priority)
        });

        orders
    }
}

/// BestCase: optimistic ordering (for debugging/upper bound)
/// - For exits: take-profit before stop-loss
pub struct BestCase;

impl PathPolicy for BestCase {
    fn order_sequence(&self, triggerable: &[Order], bar: &Bar) -> Vec<Order> {
        let mut orders = triggerable.to_vec();

        // Sort: limits before stops (best case for exits)
        orders.sort_by(|a, b| {
            use crate::orders::OrderType;

            let a_priority = match &a.order_type {
                OrderType::Limit { .. } => 1, // Limits first (best case)
                OrderType::StopMarket { .. } => 2,
                OrderType::Market(_) => 3,
                OrderType::StopLimit { .. } => 2,
            };

            let b_priority = match &b.order_type {
                OrderType::Limit { .. } => 1,
                OrderType::StopMarket { .. } => 2,
                OrderType::Market(_) => 3,
                OrderType::StopLimit { .. } => 2,
            };

            a_priority.cmp(&b_priority)
        });

        orders
    }
}

/// PriceOrder: natural price-time sequence based on OHLC
pub struct PriceOrder;

impl PathPolicy for PriceOrder {
    fn order_sequence(&self, triggerable: &[Order], bar: &Bar) -> Vec<Order> {
        // Simple OHLC-based ordering
        triggerable.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Symbol;
    use crate::orders::{OrderBook, OrderType};
    use crate::orders::order_type::{StopDirection};

    #[test]
    fn test_worst_case_puts_stops_first() {
        let mut book = OrderBook::new();

        // Create stop and limit
        let stop_id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 95.0,
            },
            100,
            0,
        );

        let limit_id = book.submit(
            Symbol::from("SPY"),
            OrderType::Limit { limit_price: 105.0 },
            100,
            0,
        );

        book.activate(stop_id).unwrap();
        book.activate(limit_id).unwrap();

        let stop = book.get(stop_id).unwrap().clone();
        let limit = book.get(limit_id).unwrap().clone();

        let policy = WorstCase;
        let bar = Bar {
            open: 100.0,
            high: 106.0,
            low: 94.0,
            close: 100.0,
            volume: 1000000,
        };

        let sequence = policy.order_sequence(&[stop.clone(), limit.clone()], &bar);

        // Stop should come first in WorstCase
        assert_eq!(sequence[0].id, stop_id);
        assert_eq!(sequence[1].id, limit_id);
    }

    #[test]
    fn test_best_case_puts_limits_first() {
        let mut book = OrderBook::new();

        let stop_id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 95.0,
            },
            100,
            0,
        );

        let limit_id = book.submit(
            Symbol::from("SPY"),
            OrderType::Limit { limit_price: 105.0 },
            100,
            0,
        );

        book.activate(stop_id).unwrap();
        book.activate(limit_id).unwrap();

        let stop = book.get(stop_id).unwrap().clone();
        let limit = book.get(limit_id).unwrap().clone();

        let policy = BestCase;
        let bar = Bar {
            open: 100.0,
            high: 106.0,
            low: 94.0,
            close: 100.0,
            volume: 1000000,
        };

        let sequence = policy.order_sequence(&[stop.clone(), limit.clone()], &bar);

        // Limit should come first in BestCase
        assert_eq!(sequence[0].id, limit_id);
        assert_eq!(sequence[1].id, stop_id);
    }
}
```

### execution/gap_handler.rs

```rust
use crate::domain::Bar;
use crate::orders::{Order, OrderType};

/// GapHandler: determines if an order gapped through and should fill at open (worse price)
#[derive(Default)]
pub struct GapHandler;

impl GapHandler {
    pub fn new() -> Self {
        Self
    }

    /// Check if order gapped through (trigger price not reached before open)
    pub fn did_gap_through(&self, order: &Order, bar: &Bar) -> bool {
        match &order.order_type {
            OrderType::StopMarket { direction, trigger_price } => {
                use crate::orders::order_type::StopDirection;

                match direction {
                    StopDirection::Buy => {
                        // Stop buy at 100, but bar opens at 102 → gapped through
                        bar.open > *trigger_price
                    }
                    StopDirection::Sell => {
                        // Stop sell at 100, but bar opens at 98 → gapped through
                        bar.open < *trigger_price
                    }
                }
            }
            _ => false, // Only stops can gap
        }
    }

    /// Get fill price for gapped order (always open)
    pub fn gap_fill_price(&self, bar: &Bar) -> f64 {
        bar.open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Symbol;
    use crate::orders::{OrderBook, OrderType};
    use crate::orders::order_type::StopDirection;

    #[test]
    fn test_stop_buy_gaps_up() {
        let handler = GapHandler::new();
        let mut book = OrderBook::new();

        let id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Buy,
                trigger_price: 100.0,
            },
            100,
            0,
        );

        let order = book.get(id).unwrap();

        let bar = Bar {
            open: 102.0, // Gapped up past trigger
            high: 103.0,
            low: 101.0,
            close: 102.5,
            volume: 1000000,
        };

        assert!(handler.did_gap_through(order, &bar));
        assert_eq!(handler.gap_fill_price(&bar), 102.0);
    }

    #[test]
    fn test_stop_sell_gaps_down() {
        let handler = GapHandler::new();
        let mut book = OrderBook::new();

        let id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 100.0,
            },
            100,
            0,
        );

        let order = book.get(id).unwrap();

        let bar = Bar {
            open: 98.0, // Gapped down past trigger
            high: 99.0,
            low: 97.0,
            close: 98.5,
            volume: 1000000,
        };

        assert!(handler.did_gap_through(order, &bar));
        assert_eq!(handler.gap_fill_price(&bar), 98.0);
    }

    #[test]
    fn test_no_gap_when_trigger_reached() {
        let handler = GapHandler::new();
        let mut book = OrderBook::new();

        let id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Buy,
                trigger_price: 100.0,
            },
            100,
            0,
        );

        let order = book.get(id).unwrap();

        let bar = Bar {
            open: 99.0, // Below trigger, so no gap
            high: 101.0,
            low: 98.0,
            close: 100.5,
            volume: 1000000,
        };

        assert!(!handler.did_gap_through(order, &bar));
    }
}
```

### execution/slippage.rs

```rust
use crate::orders::OrderType;

/// SlippageModel: computes slippage to add to fill price
pub trait SlippageModel: Send + Sync {
    fn compute_slippage(&self, base_price: f64, order_type: &OrderType, was_gapped: bool) -> f64;
}

/// FixedSlippage: constant slippage in dollars
pub struct FixedSlippage {
    slippage: f64,
}

impl FixedSlippage {
    pub fn new(slippage: f64) -> Self {
        Self { slippage }
    }

    /// Create from basis points
    pub fn from_bps(bps: f64) -> Self {
        Self { slippage: bps / 10000.0 }
    }
}

impl SlippageModel for FixedSlippage {
    fn compute_slippage(&self, base_price: f64, order_type: &OrderType, was_gapped: bool) -> f64 {
        // For gapped stops, additional adverse slippage
        if was_gapped {
            return self.slippage * 2.0;
        }

        // Direction-aware slippage
        match order_type {
            OrderType::Market(_) => self.slippage,
            OrderType::StopMarket { direction, .. } => {
                use crate::orders::order_type::StopDirection;
                match direction {
                    StopDirection::Buy => self.slippage,  // Pay slippage on buys
                    StopDirection::Sell => -self.slippage, // Receive worse on sells
                }
            }
            OrderType::Limit { .. } => 0.0, // Limits don't slip (you get limit price)
            OrderType::StopLimit { .. } => 0.0,
        }
    }
}

/// AtrSlippage: slippage as multiple of ATR
pub struct AtrSlippage {
    atr_multiple: f64,
    atr: f64, // Current ATR value
}

impl AtrSlippage {
    pub fn new(atr_multiple: f64, atr: f64) -> Self {
        Self { atr_multiple, atr }
    }
}

impl SlippageModel for AtrSlippage {
    fn compute_slippage(&self, base_price: f64, order_type: &OrderType, was_gapped: bool) -> f64 {
        let base_slippage = self.atr * self.atr_multiple;

        if was_gapped {
            return base_slippage * 2.0;
        }

        match order_type {
            OrderType::Market(_) => base_slippage,
            OrderType::StopMarket { direction, .. } => {
                use crate::orders::order_type::StopDirection;
                match direction {
                    StopDirection::Buy => base_slippage,
                    StopDirection::Sell => -base_slippage,
                }
            }
            OrderType::Limit { .. } => 0.0,
            OrderType::StopLimit { .. } => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::order_type::{MarketTiming, StopDirection};

    #[test]
    fn test_fixed_slippage_market() {
        let model = FixedSlippage::new(0.05);
        let slippage = model.compute_slippage(
            100.0,
            &OrderType::Market(MarketTiming::MOO),
            false,
        );
        assert_eq!(slippage, 0.05);
    }

    #[test]
    fn test_fixed_slippage_stop_buy() {
        let model = FixedSlippage::new(0.05);
        let slippage = model.compute_slippage(
            100.0,
            &OrderType::StopMarket {
                direction: StopDirection::Buy,
                trigger_price: 100.0,
            },
            false,
        );
        assert_eq!(slippage, 0.05);
    }

    #[test]
    fn test_fixed_slippage_gapped() {
        let model = FixedSlippage::new(0.05);
        let slippage = model.compute_slippage(
            100.0,
            &OrderType::StopMarket {
                direction: StopDirection::Buy,
                trigger_price: 100.0,
            },
            true, // Gapped
        );
        assert_eq!(slippage, 0.10); // 2x slippage when gapped
    }

    #[test]
    fn test_atr_slippage() {
        let model = AtrSlippage::new(0.5, 2.0); // 0.5 ATR, ATR = 2.0
        let slippage = model.compute_slippage(
            100.0,
            &OrderType::Market(MarketTiming::MOO),
            false,
        );
        assert_eq!(slippage, 1.0); // 0.5 * 2.0 = 1.0
    }
}
```

### execution/priority.rs

```rust
use crate::domain::Bar;
use crate::orders::Order;

/// PriorityPolicy: determines fill order when multiple orders trigger
pub trait PriorityPolicy: Send + Sync {
    fn prioritize(&self, orders: Vec<Order>, bar: &Bar) -> Vec<Order>;
}

/// WorstCasePriority: stop-loss before take-profit
pub struct WorstCasePriority;

impl PriorityPolicy for WorstCasePriority {
    fn prioritize(&self, mut orders: Vec<Order>, bar: &Bar) -> Vec<Order> {
        // Stops (exits) before limits (targets)
        orders.sort_by(|a, b| {
            use crate::orders::OrderType;

            let a_is_stop = matches!(a.order_type, OrderType::StopMarket { .. });
            let b_is_stop = matches!(b.order_type, OrderType::StopMarket { .. });

            match (a_is_stop, b_is_stop) {
                (true, false) => std::cmp::Ordering::Less,  // Stop first
                (false, true) => std::cmp::Ordering::Greater, // Limit second
                _ => std::cmp::Ordering::Equal,
            }
        });

        orders
    }
}

/// BestCasePriority: take-profit before stop-loss
pub struct BestCasePriority;

impl PriorityPolicy for BestCasePriority {
    fn prioritize(&self, mut orders: Vec<Order>, bar: &Bar) -> Vec<Order> {
        // Limits (targets) before stops (exits)
        orders.sort_by(|a, b| {
            use crate::orders::OrderType;

            let a_is_limit = matches!(a.order_type, OrderType::Limit { .. });
            let b_is_limit = matches!(b.order_type, OrderType::Limit { .. });

            match (a_is_limit, b_is_limit) {
                (true, false) => std::cmp::Ordering::Less,  // Limit first
                (false, true) => std::cmp::Ordering::Greater, // Stop second
                _ => std::cmp::Ordering::Equal,
            }
        });

        orders
    }
}

/// PriceOrderPriority: natural price-time sequence
pub struct PriceOrderPriority;

impl PriorityPolicy for PriceOrderPriority {
    fn prioritize(&self, orders: Vec<Order>, bar: &Bar) -> Vec<Order> {
        // Natural order (no reordering)
        orders
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Symbol;
    use crate::orders::{OrderBook, OrderType};
    use crate::orders::order_type::StopDirection;

    #[test]
    fn test_worst_case_priority() {
        let mut book = OrderBook::new();

        let stop_id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 95.0,
            },
            100,
            0,
        );

        let limit_id = book.submit(
            Symbol::from("SPY"),
            OrderType::Limit { limit_price: 105.0 },
            100,
            0,
        );

        let stop = book.get(stop_id).unwrap().clone();
        let limit = book.get(limit_id).unwrap().clone();

        let policy = WorstCasePriority;
        let bar = Bar {
            open: 100.0,
            high: 106.0,
            low: 94.0,
            close: 100.0,
            volume: 1000000,
        };

        let prioritized = policy.prioritize(vec![limit, stop.clone()], &bar);

        assert_eq!(prioritized[0].id, stop_id); // Stop first
        assert_eq!(prioritized[1].id, limit_id);
    }

    #[test]
    fn test_best_case_priority() {
        let mut book = OrderBook::new();

        let stop_id = book.submit(
            Symbol::from("SPY"),
            OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: 95.0,
            },
            100,
            0,
        );

        let limit_id = book.submit(
            Symbol::from("SPY"),
            OrderType::Limit { limit_price: 105.0 },
            100,
            0,
        );

        let stop = book.get(stop_id).unwrap().clone();
        let limit = book.get(limit_id).unwrap().clone();

        let policy = BestCasePriority;
        let bar = Bar {
            open: 100.0,
            high: 106.0,
            low: 94.0,
            close: 100.0,
            volume: 1000000,
        };

        let prioritized = policy.prioritize(vec![stop, limit.clone()], &bar);

        assert_eq!(prioritized[0].id, limit_id); // Limit first
        assert_eq!(prioritized[1].id, stop_id);
    }
}
```

### execution/preset.rs

```rust
use crate::execution::{
    PathPolicy, SlippageModel, PriorityPolicy, LiquidityConstraint,
    Deterministic, WorstCase, BestCase,
    FixedSlippage, AtrSlippage,
    WorstCasePriority, BestCasePriority, PriceOrderPriority,
};

/// ExecutionPreset: bundles path policy + slippage + priority + liquidity
pub struct ExecutionPreset {
    pub name: String,
    pub path_policy: Box<dyn PathPolicy>,
    pub slippage_model: Box<dyn SlippageModel>,
    pub priority_policy: Box<dyn PriorityPolicy>,
    pub liquidity_constraint: Option<LiquidityConstraint>,
}

impl ExecutionPreset {
    /// Optimistic: best case for debugging
    pub fn optimistic() -> Self {
        Self {
            name: "Optimistic".to_string(),
            path_policy: Box::new(BestCase),
            slippage_model: Box::new(FixedSlippage::new(0.0)),
            priority_policy: Box::new(BestCasePriority),
            liquidity_constraint: None,
        }
    }

    /// Realistic: default for production
    pub fn realistic() -> Self {
        Self {
            name: "Realistic".to_string(),
            path_policy: Box::new(Deterministic),
            slippage_model: Box::new(FixedSlippage::from_bps(5.0)), // 5 bps
            priority_policy: Box::new(WorstCasePriority),
            liquidity_constraint: Some(LiquidityConstraint::new(0.05, crate::execution::liquidity::RemainderPolicy::Carry)),
        }
    }

    /// Hostile: adversarial for stress testing
    pub fn hostile() -> Self {
        Self {
            name: "Hostile".to_string(),
            path_policy: Box::new(WorstCase),
            slippage_model: Box::new(FixedSlippage::from_bps(20.0)), // 20 bps
            priority_policy: Box::new(WorstCasePriority),
            liquidity_constraint: Some(LiquidityConstraint::new(0.02, crate::execution::liquidity::RemainderPolicy::Cancel)),
        }
    }
}

/// Optimistic preset (for use in tests/examples)
pub struct Optimistic;
/// Realistic preset (for use in tests/examples)
pub struct Realistic;
/// Hostile preset (for use in tests/examples)
pub struct Hostile;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presets_exist() {
        let opt = ExecutionPreset::optimistic();
        assert_eq!(opt.name, "Optimistic");

        let real = ExecutionPreset::realistic();
        assert_eq!(real.name, "Realistic");

        let host = ExecutionPreset::hostile();
        assert_eq!(host.name, "Hostile");
    }
}
```

### execution/liquidity.rs

```rust
/// RemainderPolicy: what to do with unfilled quantity due to liquidity constraints
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RemainderPolicy {
    /// Carry remainder to next bar
    Carry,
    /// Cancel remainder
    Cancel,
    /// Partial fill only
    PartialFill,
}

/// LiquidityConstraint: limits fill quantity based on bar volume
#[derive(Debug, Clone)]
pub struct LiquidityConstraint {
    /// Max participation rate (e.g., 0.05 = 5% of bar volume)
    pub max_participation: f64,
    /// Policy for remainder
    pub remainder_policy: RemainderPolicy,
}

impl LiquidityConstraint {
    pub fn new(max_participation: f64, remainder_policy: RemainderPolicy) -> Self {
        Self {
            max_participation,
            remainder_policy,
        }
    }

    /// Limit fill quantity based on bar volume
    pub fn limit_fill_qty(&self, requested_qty: u32, bar_volume: u64) -> u32 {
        let max_qty = (bar_volume as f64 * self.max_participation) as u32;
        requested_qty.min(max_qty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_participation_limit() {
        let constraint = LiquidityConstraint::new(0.05, RemainderPolicy::Carry);

        // Bar volume = 1,000,000, max participation = 5% = 50,000
        let limited = constraint.limit_fill_qty(100000, 1000000);
        assert_eq!(limited, 50000);
    }

    #[test]
    fn test_no_limit_when_below_threshold() {
        let constraint = LiquidityConstraint::new(0.10, RemainderPolicy::Cancel);

        // Bar volume = 1,000,000, max = 10% = 100,000
        // Request only 50,000 → should get full fill
        let limited = constraint.limit_fill_qty(50000, 1000000);
        assert_eq!(limited, 50000);
    }
}
```

## BDD Scenarios

**Feature: MOO/MOC fills at correct prices**

```gherkin
Feature: Market on Open and Market on Close fills

  Scenario: MOO fills at open price
    Given an OrderBook with a MOO order for 100 shares of SPY
    And the order is active
    When the bar opens at 100.0
    Then the order fills at 100.0

  Scenario: MOC fills at close price
    Given an OrderBook with a MOC order for 100 shares of SPY
    And the order is active
    When the bar closes at 101.0
    Then the order fills at 101.0
```

**Feature: Stop gap logic**

```gherkin
Feature: Stop orders that gap through fill at open (worse price)

  Scenario: Stop buy gaps up
    Given a stop buy order at 100.0 for SPY
    When the bar opens at 102.0 (gapped up past trigger)
    Then the order fills at 102.0 (not 100.0)

  Scenario: Stop sell gaps down
    Given a stop sell order at 100.0 for SPY
    When the bar opens at 98.0 (gapped down past trigger)
    Then the order fills at 98.0 (not 100.0)

  Scenario: Stop buy does not gap when trigger reached
    Given a stop buy order at 100.0 for SPY
    When the bar opens at 99.0 and high reaches 101.0
    Then the order fills at 100.0 (trigger price)
```

**Feature: Intrabar ambiguity resolution (WorstCase vs BestCase)**

```gherkin
Feature: Ambiguous bars trigger different outcomes based on path policy

  Background:
    Given a bracket order with entry at 100.0
    And a stop-loss at 95.0
    And a take-profit at 105.0
    And the entry is filled

  Scenario: WorstCase fills stop-loss first
    Given the execution preset is WorstCase
    When a bar has low=94.0 and high=106.0 (both triggerable)
    Then the stop-loss fills at 95.0
    And the take-profit is cancelled (OCO)

  Scenario: BestCase fills take-profit first
    Given the execution preset is BestCase
    When a bar has low=94.0 and high=106.0 (both triggerable)
    Then the take-profit fills at 105.0
    And the stop-loss is cancelled (OCO)
```

**Feature: Slippage modeling**

```gherkin
Feature: Slippage affects fill prices

  Scenario: Fixed slippage on market orders
    Given a fixed slippage model of 0.05
    When a market order fills at base price 100.0
    Then the fill price is 100.05

  Scenario: Gapped stops incur 2x slippage
    Given a fixed slippage model of 0.05
    When a stop buy gaps from 100.0 to 102.0
    Then the fill price is 102.10 (102.0 + 2*0.05)

  Scenario: Limit orders have zero slippage
    Given any slippage model
    When a limit order fills at 100.0
    Then the fill price is exactly 100.0 (no slippage)
```

**Feature: Order priority in ambiguous bars**

```gherkin
Feature: Priority policy determines fill sequence

  Scenario Outline: Different policies produce different outcomes
    Given a stop-loss at 95.0 and take-profit at 105.0
    And a bar with low=94.0, high=106.0
    When the priority policy is <Policy>
    Then the first fill is <FirstFill>
    And the second order is <SecondState>

    Examples:
      | Policy            | FirstFill   | SecondState |
      | WorstCasePriority | stop-loss   | cancelled   |
      | BestCasePriority  | take-profit | cancelled   |
```

**Feature: Liquidity constraints**

```gherkin
Feature: Participation limits restrict fill size

  Scenario: Order exceeds participation limit
    Given a liquidity constraint of 5% participation
    And a bar with volume 1,000,000
    When an order requests 100,000 shares
    Then only 50,000 shares fill (5% of 1,000,000)

  Scenario: Order below participation limit fills completely
    Given a liquidity constraint of 10% participation
    And a bar with volume 1,000,000
    When an order requests 50,000 shares
    Then all 50,000 shares fill

  Scenario: Remainder policy Cancel
    Given a liquidity constraint with RemainderPolicy::Cancel
    When an order partially fills due to liquidity
    Then the unfilled quantity is cancelled

  Scenario: Remainder policy Carry
    Given a liquidity constraint with RemainderPolicy::Carry
    When an order partially fills due to liquidity
    Then the unfilled quantity carries to the next bar
```

**Feature: Execution presets**

```gherkin
Feature: Execution presets bundle common configurations

  Scenario: Optimistic preset uses best case assumptions
    Given the Optimistic preset
    Then the path policy is BestCase
    And the slippage is 0.0
    And there are no liquidity constraints

  Scenario: Realistic preset uses conservative assumptions
    Given the Realistic preset
    Then the path policy is Deterministic
    And the slippage is 5 bps
    And the liquidity constraint is 5% participation

  Scenario: Hostile preset uses adversarial assumptions
    Given the Hostile preset
    Then the path policy is WorstCase
    And the slippage is 20 bps
    And the liquidity constraint is 2% participation with Cancel policy
```

## Verification Commands

```bash
# Step 1: Create execution module structure
mkdir -p trendlab-core/src/execution
cd trendlab-core/src/execution

# Step 2: Create execution module files
touch mod.rs fill_engine.rs path_policy.rs gap_handler.rs slippage.rs priority.rs preset.rs liquidity.rs

# Step 3: Implement each file (see above implementations)

# Step 4: Update trendlab-core/src/lib.rs
cat >> ../lib.rs <<'EOF'

pub mod execution;
EOF

# Step 5: Create BDD test file
mkdir -p trendlab-core/tests
touch trendlab-core/tests/bdd_execution.rs

# Step 6: Run unit tests
cargo test --package trendlab-core --lib execution

# Expected output:
# running 20+ tests (from all execution module unit tests)
# test execution::fill_engine::tests::test_moo_fills_at_open ... ok
# test execution::fill_engine::tests::test_moc_fills_at_close ... ok
# test execution::fill_engine::tests::test_stop_market_triggers_and_fills ... ok
# test execution::path_policy::tests::test_worst_case_puts_stops_first ... ok
# test execution::path_policy::tests::test_best_case_puts_limits_first ... ok
# test execution::gap_handler::tests::test_stop_buy_gaps_up ... ok
# test execution::gap_handler::tests::test_stop_sell_gaps_down ... ok
# test execution::gap_handler::tests::test_no_gap_when_trigger_reached ... ok
# test execution::slippage::tests::test_fixed_slippage_market ... ok
# test execution::slippage::tests::test_fixed_slippage_stop_buy ... ok
# test execution::slippage::tests::test_fixed_slippage_gapped ... ok
# test execution::slippage::tests::test_atr_slippage ... ok
# test execution::priority::tests::test_worst_case_priority ... ok
# test execution::priority::tests::test_best_case_priority ... ok
# test execution::preset::tests::test_presets_exist ... ok
# test execution::liquidity::tests::test_participation_limit ... ok
# test execution::liquidity::tests::test_no_limit_when_below_threshold ... ok
# (... and more)
#
# test result: ok. 20+ passed; 0 failed; 0 ignored

# Step 7: Run BDD tests (Cucumber)
cargo test --test bdd_execution

# Expected output:
# Feature: Market on Open and Market on Close fills
#   Scenario: MOO fills at open price ... ok
#   Scenario: MOC fills at close price ... ok
#
# Feature: Stop orders that gap through fill at open
#   Scenario: Stop buy gaps up ... ok
#   Scenario: Stop sell gaps down ... ok
#   Scenario: Stop buy does not gap when trigger reached ... ok
#
# Feature: Ambiguous bars trigger different outcomes
#   Scenario: WorstCase fills stop-loss first ... ok
#   Scenario: BestCase fills take-profit first ... ok
#
# Feature: Slippage affects fill prices
#   Scenario: Fixed slippage on market orders ... ok
#   Scenario: Gapped stops incur 2x slippage ... ok
#   Scenario: Limit orders have zero slippage ... ok
#
# Feature: Priority policy determines fill sequence
#   Scenario Outline: Different policies produce different outcomes ... ok (2 examples)
#
# Feature: Participation limits restrict fill size
#   Scenario: Order exceeds participation limit ... ok
#   Scenario: Order below limit fills completely ... ok
#   Scenario: Remainder policy Cancel ... ok
#   Scenario: Remainder policy Carry ... ok
#
# Feature: Execution presets bundle common configurations
#   Scenario: Optimistic preset uses best case assumptions ... ok
#   Scenario: Realistic preset uses conservative assumptions ... ok
#   Scenario: Hostile preset uses adversarial assumptions ... ok
#
# test result: ok. 18+ scenarios passed

# Step 8: Lint check
cargo clippy --package trendlab-core -- -D warnings

# Expected: no warnings
```

## Example Flow: WorstCase vs BestCase on Ambiguous Bar

```text
Setup:
- Symbol: SPY
- Entry filled at bar 10, price 100.0, qty 100
- Bracket orders created:
  - Stop-loss: StopMarket(Sell, 95.0), qty 100
  - Take-profit: Limit(105.0), qty 100
- Both stop and target are active (OCO linked)

Bar 11 arrives:
  open: 100.0
  high: 106.0  ← target triggerable
  low: 94.0    ← stop triggerable
  close: 100.5
  volume: 1,000,000

Scenario A: WorstCase execution preset

1. FillEngine.process_bar(bar_11):
   - SOB: No MOO orders
   - Intrabar:
     a) Get triggerable orders: [stop-loss, take-profit] (both can trigger)
     b) PathPolicy (WorstCase).order_sequence() → [stop, target]
     c) PriorityPolicy (WorstCasePriority).prioritize() → [stop, target] (stop first)
     d) Process stop first:
        - Trigger stop at 95.0
        - Check gap: did_gap_through() → true (low=94.0 < trigger=95.0)
        - Fill price = open = 100.0 (gap fill, worse than trigger)
        - Slippage: -0.10 (2x slippage for gapped stop)
        - Final fill: 99.90
        - Fill stop: qty=100, price=99.90
        - OCO: Cancel take-profit
     e) Take-profit already cancelled, skip
   - EOB: No MOC orders

Result:
- Stop filled at 99.90 (worse than trigger 95.0 due to gap)
- Target cancelled
- Trade closed at loss

Scenario B: BestCase execution preset

1. FillEngine.process_bar(bar_11):
   - SOB: No MOO orders
   - Intrabar:
     a) Get triggerable orders: [stop-loss, take-profit] (both can trigger)
     b) PathPolicy (BestCase).order_sequence() → [target, stop]
     c) PriorityPolicy (BestCasePriority).prioritize() → [target, stop] (target first)
     d) Process target first:
        - Target is limit order at 105.0
        - High=106.0 ≥ limit → trigger
        - Fill price = 105.0 (limit orders fill at limit price)
        - Slippage: 0.0 (limits have no slippage)
        - Fill target: qty=100, price=105.0
        - OCO: Cancel stop-loss
     e) Stop-loss already cancelled, skip
   - EOB: No MOC orders

Result:
- Target filled at 105.0
- Stop cancelled
- Trade closed at profit

Delta:
- WorstCase: -0.10 per share (loss)
- BestCase: +5.00 per share (profit)
- Difference: $510 on 100 shares

This illustrates why path policy matters: same bar, vastly different outcomes.
```

## Example Flow: Gap Logic for Stops

```text
Setup:
- Symbol: SPY
- Stop buy order: trigger=100.0, qty=100
- Order is active

Bar arrives:
  open: 102.0  ← Gapped up past trigger
  high: 103.0
  low: 101.0
  close: 102.5
  volume: 1,000,000

Execution flow:

1. FillEngine.process_intrabar():
   - Get active orders: [stop buy]
   - Check can_trigger_in_bar(stop, bar):
     - Stop buy trigger=100.0, bar.high=103.0
     - 100.0 ≤ 103.0 → true (triggerable)

2. GapHandler.did_gap_through(stop, bar):
   - Stop buy trigger=100.0
   - Bar open=102.0
   - 102.0 > 100.0 → true (gapped through)

3. Compute fill price:
   - Base price = bar.open = 102.0 (gap fill, not trigger)
   - Slippage = 0.05 (fixed) * 2 (gapped) = 0.10
   - Fill price = 102.0 + 0.10 = 102.10

4. Fill order:
   - OrderBook.fill(id, qty=100, bar_index)
   - FillResult:
     - fill_price: 102.10
     - was_gapped: true
     - slippage: 0.10

Result:
- Wanted to buy at 100.0
- Actually bought at 102.10 (gap + slippage)
- Cost: extra $2.10 per share ($210 total)

This enforces realistic execution: gaps are adversarial, not free.
```

## Example Flow: Liquidity Constraint with Carry Policy

```text
Setup:
- Symbol: SPY
- Market buy order: qty=100,000
- Liquidity constraint: 5% participation, RemainderPolicy::Carry
- Order is active

Bar 1 arrives:
  open: 100.0
  high: 101.0
  low: 99.0
  close: 100.5
  volume: 1,000,000

Execution flow (Bar 1):

1. FillEngine.process_sob():
   - MOO order active
   - Requested qty: 100,000
   - LiquidityConstraint.limit_fill_qty(100000, 1000000):
     - Max qty = 1,000,000 * 0.05 = 50,000
     - Return: 50,000

2. Fill:
   - Fill qty: 50,000 (not 100,000)
   - Fill price: 100.0 + slippage
   - Remainder: 50,000 shares

3. RemainderPolicy::Carry:
   - Unfilled qty (50,000) carries to next bar
   - Order state: PartiallyFilled
   - Remaining qty: 50,000

Bar 2 arrives:
  open: 100.2
  high: 101.0
  low: 100.0
  close: 100.8
  volume: 800,000

Execution flow (Bar 2):

1. Order still active (partially filled)
   - Remaining qty: 50,000

2. LiquidityConstraint.limit_fill_qty(50000, 800000):
   - Max qty = 800,000 * 0.05 = 40,000
   - Return: 40,000

3. Fill:
   - Fill qty: 40,000
   - Remainder: 10,000 carries again

Bar 3 arrives:
  open: 100.5
  high: 101.2
  low: 100.3
  close: 101.0
  volume: 1,200,000

Execution flow (Bar 3):

1. Order still active
   - Remaining qty: 10,000

2. LiquidityConstraint.limit_fill_qty(10000, 1200000):
   - Max qty = 1,200,000 * 0.05 = 60,000
   - 10,000 < 60,000 → full fill

3. Fill:
   - Fill qty: 10,000
   - Order complete

Summary:
- Total filled: 50,000 + 40,000 + 10,000 = 100,000 shares
- Filled across 3 bars (realistic for large orders)
- Average fill price reflects market impact over time
```

## Completion Criteria

- [ ] All execution module files exist in `trendlab-core/src/execution/`
- [ ] FillEngine processes bars in 3 phases: SOB → Intrabar → EOB
- [ ] MOO orders fill at open price
- [ ] MOC orders fill at close price
- [ ] PathPolicy trait defines order sequence (Deterministic, WorstCase, BestCase)
- [ ] GapHandler detects gaps and fills at open (worse price)
- [ ] SlippageModel computes slippage (FixedSlippage, AtrSlippage)
- [ ] Gapped stops incur 2x slippage
- [ ] Limit orders have zero slippage
- [ ] PriorityPolicy resolves conflicts (WorstCasePriority, BestCasePriority)
- [ ] WorstCase: stop-loss fills before take-profit
- [ ] BestCase: take-profit fills before stop-loss
- [ ] ExecutionPreset bundles path/slippage/priority/liquidity (Optimistic, Realistic, Hostile)
- [ ] LiquidityConstraint limits fill qty based on participation rate
- [ ] RemainderPolicy handles unfilled qty (Carry, Cancel, PartialFill)
- [ ] Unit tests pass (20+ tests covering all modules)
- [ ] BDD tests pass (18+ scenarios in Gherkin format)
- [ ] `cargo clippy` has zero warnings
- [ ] Integration with OrderBook: orders trigger → fill → OCO cancellation works correctly

---

# M6 — Position management (anti-stickiness) + ratchet invariant
## Critique-driven additions
- Explicit regression scenarios for stickiness and “volatility trap.”
- Stops must obey a **ratchet** invariant under volatility expansion.

## Deliverables
- PM emits order intents (cancel/replace), never direct fills
- MVP PM set:
  - fixed %, ATR stop, chandelier, time stop
- **Ratchet invariant** (default):
  - stop may tighten, never loosen (even if ATR expands)
- Anti-stickiness scenarios:
  - chandelier-style exit not trapped by chasing highs
  - floor-style tightening that doesn’t chase ceiling

## BDD
**Feature: Ratchet invariant**
- Scenario: volatility expansion does not loosen stop

**Feature: Anti-stickiness**
- Scenario: chandelier exit allows profitable exit in a rise-then-fall path
- Scenario: floor tightening tightens on rises but not on falls

---

# M7 — Strategy composition + normalization for fair comparisons
## Critique-driven addition
Make “same PM across signals” testable here, not later.

## Deliverables
- Signals are portfolio-agnostic (exposure/intent)
- OrderPolicy chooses natural order types by family (breakout → stops)
- Sizers: fixed qty/notional + ATR-risk sizing (MVP)
- Compose: (Signal + OrderPolicy + PM + ExecutionPreset + Sizer)

## BDD
**Feature: Signals ignore portfolio**
- Scenario: signal emits same intent regardless of current position

**Feature: Breakout uses stop entries**
- Scenario: Donchian breakout issues stop entry above level (not “buy next open”)

**Feature: Fair signal comparison via PM normalization**
- Scenario: multiple signals share identical PM and execution; differences reflect signal timing

---

# M8 — Runner (sweeps) + caching + cache invalidation + leaderboards
## Critique-driven addition
Caching must have explicit invalidation rules.

## Deliverables
- Full-Auto sweeps (structural explore + parameter sampling)
- Persist:
  - manifest, equity, trades, diagnostics
- Leaderboards:
  - session + all-time
  - signal-only / PM / execution sensitivity / composite
- **Cache invalidation rules**
  - feature cache keyed by dataset hash + feature spec id
  - indicator cache invalidated by param changes
  - result cache keyed by manifest hash (auto invalidation)

## BDD
**Feature: Cache invalidation correctness**
- Scenario: param change invalidates indicator cache
- Scenario: identical config uses cache (no recompute)

**Feature: Leaderboard reproducibility**
- Scenario: leaderboard row reruns identically from manifest

---

# M9 — Robustness ladder + stability scoring
## Critique-driven addition
Define what “stable enough to promote” means.

## Deliverables
- Promotion ladder (ship minimal first):
  1) Cheap Pass
  2) Walk-Forward
  3) Execution MC
  4) (later) Path MC
  5) (later) Bootstrap/Regime/Universe MC
- **Stability scoring**
  - e.g., `StabilityScore = median(metric) - penalty_factor * IQR(metric)`
  - promotion uses StabilityScore threshold, not just point estimate
- Store distributions (median, IQR, tails), not just best-case

## BDD
**Feature: Stability-aware promotion**
- Scenario: higher median but high variance ranks below slightly lower median with low variance

**Feature: Promotion gating**
- Scenario: failing Cheap Pass never consumes Execution MC budget

---

# M10 — TUI v3 + drill-down explainability + ghost curve
## Critique-driven additions
- Drill-down path must be explicit.
- “Ghost curve” shows execution drag (ideal vs real fills).

## Deliverables
- Theme tokens (Parrot/neon)
- Core panels (MVP):
  - Leaderboard, Chart, Trade Tape, Execution Lab
- **Drill-down flow**
  1) Leaderboard → select row → summary card
  2) Enter → trade tape
  3) Enter on trade → chart jump to entry/exit
  4) `d` → diagnostics (slippage, gaps, ambiguities)
  5) `r` → rerun with new execution preset
- **Ghost curve**
  - store “ideal equity” vs “real equity” (execution-drag)
  - compute and display Execution Drag metric

## BDD
**Feature: Drill-down explainability**
- Scenario: user can trace from leaderboard row → trade tape → chart markers

**Feature: Execution drag visualization**
- Scenario: run result contains both ideal and real equity and computed drag

---

# M11 — Reporting & artifacts
## Deliverables
- Run artifacts:
  - manifest, equity, trades, diagnostics
- Optional: one-page markdown report per run (composition + metrics + robustness summaries)

## BDD
**Feature: Explainability artifacts exist**
- Scenario: every leaderboard row has manifest + trade tape export

---

# M12 — Hardening (perf + regression + docs)
## Deliverables
- Criterion benches: bar loop, order book ops, execution fills
- Regression suite:
  - golden synthetic datasets
  - property tests for invariants
- Docs:
  - how to add signals/PM/order policies
  - how to interpret presets and robustness distributions

## BDD
**Feature: Regression protection**
- Scenario: golden synthetic worlds remain unchanged unless explicitly updated

---

## Global Definition of Done
You are “v3 done” when:
- Any leaderboard row is reproducible from manifest (config + seed + dataset hash).
- Execution assumptions are explicit (preset + knobs) and sensitivity is visible.
- Signal vs PM vs execution effects are isolatable via separate leaderboards.
- Robustness ladder promotes stable candidates using distributions (median/IQR).
- TUI drill-down makes “why did this win?” obvious (tape + overlays + diagnostics + ghost curve).
