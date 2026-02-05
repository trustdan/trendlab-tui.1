# TrendLab v3

> **Research-grade trend-following backtesting engine with terminal UI**

TrendLab v3 is a Rust-based backtesting system designed to solve two chronic failure modes in traditional backtesting:

1. **Strategy stickiness** — exits "chase" highs and never let you exit profitably
2. **Execution bias** — forced "signal on close → fill next open" assumptions that distort strategy families

## Why TrendLab v3?

Traditional backtesting tools often produce misleading results because they:
- Bundle signals, position management, and execution into opaque black boxes
- Assume perfect fills at ideal prices (no slippage, no spread)
- Force unnatural entry/exit timing ("buy next open" for all strategies)
- Allow stops to loosen when volatility expands (giving back gains)
- Hide execution assumptions, making strategies impossible to compare fairly

**TrendLab v3 solves this** with:
- ✅ **Strict separation** of signals, position management, and execution
- ✅ **Realistic execution** with gap fills, intrabar ambiguity, and configurable slippage
- ✅ **Anti-stickiness** guarantees (chandelier exits, ratchet invariants)
- ✅ **Promotion ladder** (cheap → expensive simulation, filter early)
- ✅ **Explainable results** (drill-down from leaderboard → trades → chart → diagnostics)

## Project Status

**Current Phase:** Pre-M0 (Planning)

See [Development Roadmap](trendlab-v3-development-roadmap-bdd-v2.md) for the full 12-milestone BDD-driven plan.

### Milestone Overview

| Milestone | Description | Status |
|-----------|-------------|--------|
| **M0** | Repo bootstrap + guardrails | ⬜ Not started |
| **M0.5** | Smoke backtest (integration skeleton) | ⬜ Not started |
| **M1** | Domain model + determinism contract | ⬜ Not started |
| **M2** | Data ingest + canonical cache | ⬜ Not started |
| **M3** | Event loop + warmup + accounting | ⬜ Not started |
| **M4** | Orders + OrderBook lifecycle | ⬜ Not started |
| **M5** | Execution engine + fill simulation | ⬜ Not started |
| **M6** | Position management (anti-stickiness) | ⬜ Not started |
| **M7** | Strategy composition + normalization | ⬜ Not started |
| **M8** | Runner (sweeps) + caching + leaderboards | ⬜ Not started |
| **M9** | Robustness ladder + stability scoring | ⬜ Not started |
| **M10** | TUI v3 + drill-down + ghost curve | ⬜ Not started |
| **M11** | Reporting & artifacts | ⬜ Not started |
| **M12** | Hardening (perf + regression + docs) | ⬜ Not started |

## Architecture Overview

### Core Design Principles

1. **Separation of Concerns**
   - Signals NEVER depend on portfolio state
   - Position management NEVER influences signal generation
   - Execution is configurable and realistic, not hardcoded

2. **Bar-by-Bar Event Loop**
   ```
   For each bar:
     1. Start-of-bar: activate day orders, fill MOO
     2. Intrabar: simulate triggers/fills via path policy
     3. End-of-bar: fill MOC
     4. Post-bar: update portfolio, emit maintenance orders
   ```

3. **Promotion Ladder** (cheap candidates "earn" expensive simulation)
   - Level 1: Cheap Pass (deterministic + worst-case)
   - Level 2: Walk-Forward (train/test splits)
   - Level 3: Execution MC (slippage/spread distributions)
   - Level 4: Path MC (intrabar ambiguity sampling)
   - Level 5: Bootstrap/Regime (block resampling)

### Canonical Flow

```
Signals → Order Policy → Order Book → Execution Model → Portfolio → Position Manager
```

- **Signals** emit intent (Long/Short/Flat) based ONLY on market data
- **Order Policy** translates intent → order type (breakout → stop, mean-reversion → limit)
- **Order Book** manages order lifecycle (pending → triggered → filled → canceled)
- **Execution Model** simulates fills (slippage, spread, gap rules, ambiguity)
- **Portfolio** tracks positions, cash, equity
- **Position Manager** emits maintenance orders for next bar (stops, targets, scaling)

## Project Structure

```
trendlab-v3/                    (workspace root)
├── Cargo.toml                  # Workspace manifest
├── README.md                   # This file
├── trendlab-v3-development-roadmap-bdd-v2.md  # Main development roadmap
├── CLAUDE.md                   # Claude Code project instructions
├── trendlab-core/              # Engine (domain, signals, execution, PM)
├── trendlab-runner/            # Sweeps, caching, leaderboards
├── trendlab-tui/               # Ratatui terminal UI
├── trendlab-cli/               # Optional CLI wrapper
├── data/                       # Canonical Parquet cache
└── docs/                       # Detailed milestone specifications
    ├── M6-position-management-specification.md
    ├── M7-composition-normalization-specification.md
    ├── M8-walkforward-oos-specification.md
    ├── M9-execution-monte-carlo-specification.md
    ├── M10-path-monte-carlo-specification.md
    ├── M11-bootstrap-regime-resampling-specification.md
    └── M12-benchmarks-ui-polish-specification.md
```

## Quick Start (Post-M0)

Once the repository is initialized:

```bash
# Build all workspace members
cargo build --workspace

# Run all tests (unit + integration + BDD)
cargo test --workspace

# Run tests for a specific crate
cargo test -p trendlab-core

# Lint and format
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# Run benchmarks (M12)
cargo bench -p trendlab-core

# Launch TUI (M10+)
cargo run --package trendlab-tui
```

## Development Approach

TrendLab v3 is built using **Behavior-Driven Development (BDD)** with Cucumber.

Every milestone has:
- **Feature specs** (high-level requirements)
- **BDD scenarios** (Given/When/Then)
- **Code templates** (structs, traits, enums)
- **Verification commands** (tests, expected output)
- **Completion criteria** (checklist for "done")

### Example BDD Scenario (M6 - Ratchet Invariant)

```gherkin
Feature: Ratchet invariant prevents volatility trap

  Scenario: Ratchet prevents loosening on volatility spike
    Given long position entered at $100 with stop at $95
    And initial ATR of $5 (2x ATR stop = $95)
    When price rises to $110
    And ATR expands to $10 (market volatility increases)
    Then proposed stop is $90 (110 - 2*10)
    But ratchet blocks it (can't loosen from $95)
    And stop remains at $95
```

## Documentation

### Main Documents

- **[Development Roadmap](trendlab-v3-development-roadmap-bdd-v2.md)** — Complete 12-milestone plan with quick reference cards
- **[CLAUDE.md](CLAUDE.md)** — Project context and guidance for Claude Code
- **[Detailed Specifications](docs/)** — Full implementation specs for M6-M12 (1,000-1,700 lines each)

### Milestone Specifications

Each milestone M6-M12 has a detailed specification file (~1,000-1,700 lines):

- [M6: Position Management](M6-position-management-specification.md) — Anti-stickiness + ratchet invariant
- [M7: Strategy Composition](M7-composition-normalization-specification.md) — Signal × PM × Execution
- [M8: Walk-Forward + OOS](M8-walkforward-oos-specification.md) — Sweeps, caching, leaderboards
- [M9: Execution Monte Carlo](M9-execution-monte-carlo-specification.md) — Robustness ladder, stability scoring
- [M10: Path Monte Carlo](M10-path-monte-carlo-specification.md) — TUI, drill-down, ghost curve
- [M11: Bootstrap & Regime](M11-bootstrap-regime-resampling-specification.md) — Reporting & artifacts
- [M12: Benchmarks & Polish](M12-benchmarks-ui-polish-specification.md) — Performance, regression, docs

## Execution Realism

TrendLab v3 enforces realistic execution with **daily OHLC data** (no intraday):

1. **Gap rule**: If price gaps through a stop, fill at open (worse), not at trigger
2. **Ambiguity rule**: If a bar could hit both stop and target, don't assume best case
   - Default: **WorstCase** policy (adverse ordering)
   - Upgrade: **Path MC** (sample intrabar paths)
3. **Order types are first-class**:
   - Market (MOO/MOC/Now)
   - StopMarket, Limit, StopLimit
   - Brackets/OCO
4. **No "perfect touch" limits by default** (optional adverse selection + queue depth knobs)

## Anti-Stickiness Guarantees

### Problem: Traditional backtests allow exits to "chase" highs

```
Price rises to $120 → chandelier stop rises to $115
Price rises to $125 → chandelier stop rises to $120
Price rises to $130 → chandelier stop rises to $125
Price falls to $110 → you never exit (stop kept chasing)
```

### TrendLab v3 Solution: Snapshot reference levels

```rust
pub struct ChandelierExit {
    lookback: usize,        // e.g., 20 bars
    atr_mult: f64,          // e.g., 2.0
    reference_high: Decimal, // Snapshot at peak, doesn't chase
}

// When price makes new 20-bar high at $120:
//   reference_high = $120 (captured)
//   stop = $120 - 2*ATR = $115
//
// When price falls back:
//   reference_high STAYS $120 (no chasing)
//   exit triggers at $115 (profitable exit)
```

### Ratchet Invariant: Stops never loosen

```rust
impl RatchetState {
    pub fn apply(&mut self, proposed: Decimal) -> Decimal {
        match self.side {
            Side::Long => self.current_level.max(proposed),  // stop only rises
            Side::Short => self.current_level.min(proposed), // stop only falls
        }
    }
}
```

Even if ATR expands (volatility spike), stops remain protected at previous levels.

## Contributing

This project is in active development. Contributions welcome once M0 is complete.

### Development Workflow

1. Read the [Development Roadmap](trendlab-v3-development-roadmap-bdd-v2.md)
2. Check the current milestone status
3. Review the BDD scenarios for the feature you're implementing
4. Write tests first (BDD + unit tests)
5. Implement until tests pass
6. Run full test suite: `cargo test --workspace`
7. Format and lint: `cargo fmt && cargo clippy`

### Code Style

- Use `thiserror` for domain errors; `anyhow` for top-level propagation
- Prefer explicit structs/enums over "stringly typed" configs
- Avoid allocations in hot loops; reuse buffers where possible
- Keep execution loop in Rust; use Polars (LazyFrame) for indicators

## License

TBD

## Contact

TBD

---

**Status:** Pre-M0 (Planning Phase)
**Last Updated:** 2026-02-04
