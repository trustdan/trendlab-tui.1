# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## TrendLab v3 — Claude Code Project Context

You are helping build **TrendLab v3**, a Rust-based, research-grade trend-following backtesting engine with a terminal UI (Ratatui).

This project exists to solve two chronic backtesting failure modes:

1. **Strategy stickiness** (exits "chase" highs and never let you exit), and
2. **Execution bias** (forced "signal on close → fill next open" assumptions that distort strategy families).

The core redesign is a strict, composable, event-driven pipeline.

## Project Status & Initialization

**Current Phase:** Pre-M0 (Repository scaffold not yet created)

The project is in **planning phase**. See [trendlab-v3-development-roadmap-bdd-v2.md](trendlab-v3-development-roadmap-bdd-v2.md) for the full 12-milestone BDD-driven development plan.

### Expected Workspace Structure (M0 deliverable)

```text
trendlab-v3/
├── Cargo.toml                  # workspace root
├── trendlab-core/              # engine (domain, signals, execution, PM)
├── trendlab-runner/            # sweeps, caching, leaderboards
├── trendlab-tui/               # Ratatui interface
├── trendlab-cli/               # optional CLI wrapper
└── data/                       # canonical Parquet cache
```

### Once Initialized: Common Commands

```bash
# Build all workspace members
cargo build --workspace

# Run all tests (unit + integration + property tests)
cargo test --workspace

# Run tests for a specific crate
cargo test -p trendlab-core

# Run a single test by name
cargo test test_name -- --nocaptures

# Lint and format
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# Run benchmarks (Criterion, added in M12)
cargo bench -p trendlab-core

# BDD tests (Cucumber, used throughout development)
cargo test --test bdd_*
```

## Non-Negotiable Architecture Invariants

### A) Separation of Concerns (no leaking)

**Signals must not depend on portfolio state.**
**Position management must not influence signal generation.**
**Execution must be configurable and realistic, not hardcoded.**

**Canonical flow:**
`Signals → Order Policy → Order Book → Execution Model → Portfolio → Position Manager`

If you see code that bundles these together, refactor until the boundaries are clean.

### B) Bar-by-Bar Event Loop (no "vectorized-only backtest" shortcuts)

TrendLab runs a deterministic **bar event loop** (even if indicators are vectorized with Polars).

Per bar:

1. **Start-of-bar:** activate day orders, fill MOO orders
2. **Intrabar:** simulate triggers/fills via path policy
3. **End-of-bar:** fill MOC orders
4. **Post-bar:** update portfolio, then let position manager emit maintenance orders for next bar

### C) "Promotion Ladder" for realism vs speed

Cheap candidates must "earn" expensive simulation.

1. **Level 1 (Cheap Pass):** deterministic intrabar policy + fixed slippage
2. **Level 2 (Walk-Forward):** train/test splits and OOS filters
3. **Level 3 (Execution MC):** sample slippage/spread/adverse selection distributions
4. **Level 4 (Path MC):** intrabar ambiguity Monte Carlo / micro-path sampling
5. **Level 5 (Bootstrap/Regime):** block bootstrap and regime resampling

## Execution Realism Rules (daily OHLC, no intraday data)

1. **Gap rule:** If price gaps through a stop, fill at open (worse), not at trigger.
2. **Ambiguity rule:** If a bar could have hit both stop-loss and take-profit, do not assume best case.
   - Default policy: **WorstCase** (adverse ordering), unless explicitly using Path MC.
3. **Order types are first-class:**
   - Market (MOO/MOC/Now)
   - StopMarket
   - Limit
   - StopLimit
   - Brackets/OCO
4. **No "perfect touch" limits by default:**
   - optional **adverse selection** and **queue depth** knobs.

## Code Style & Performance Expectations

### Rust conventions

- Use `thiserror` for domain errors; `anyhow` for top-level propagation.
- Prefer explicit structs/enums over "stringly typed" configs.
- Avoid allocations in hot loops; reuse buffers and pre-allocate where possible.
- Keep execution loop in Rust; use Polars (LazyFrame) for indicators/precompute features.

### Testing expectations

- Unit tests for each module (signals/orders/execution/portfolio/pm)
- Property tests for invariants (no double fill, OCO consistency, equity accounting)
- Golden regression tests for stable benchmarks

## TUI Theme & Progress Bars

### Parrot / Neon Theme Tokens

Use semantic tokens (not hardcoded colors in widgets):

- Background: near-black / deep charcoal
- Accent: electric cyan
- Positive: neon green
- Negative: hot pink
- Warning: neon orange
- Neutral: cool purple
- Muted text: steel blue

### Progress bars (Pacman-style)

For multi-step responses or long plans, show a pacman bar like:

`[ᗧ··············] 0%  (scoping)`
`[..ᗧ············] 20% (interfaces)`
`[......ᗧ········] 45% (core loop)`
`[.............ᗧ·] 95% (tests)`
`[..............ᗧ] 100% (done)`

Rules:

- 16 pellets (·) + 1 pacman (ᗧ). Keep it mono-width.
- Include a short stage label in parentheses.
- Use this for *planning / build steps*, not for short answers.

## Commands Available (Claude Code)

- /project:architecture — overall design decisions, module boundaries, invariants
- /project:signals — signal generator design (pure, vectorizable)
- /project:orders — order types, order policy, order book state machine
- /project:execution — fill simulation, path policies, slippage/adverse selection
- /project:position-mgmt — trailing stops, targets, scaling, stickiness avoidance
- /project:robustness — walk-forward, MC, bootstrap, overfitting defenses
- /project:benchmark — profiling, hot loop optimization, perf regression
- /project:data — data ingest, cleaning, adjustments, alignment
- /project:testing — unit/integration/proptest/golden tests; invariants
