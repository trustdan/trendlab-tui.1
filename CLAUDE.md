# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## TrendLab v3 — Claude Code Project Context

You are helping build **TrendLab v3**, a Rust-based, research-grade trend-following backtesting engine with a terminal UI (Ratatui).

This project exists to solve two chronic backtesting failure modes:

1. **Strategy stickiness** (exits "chase" highs and never let you exit), and
2. **Execution bias** (forced "signal on close → fill next open" assumptions that distort strategy families).

The core redesign is a strict, composable, event-driven pipeline.

## Project Status

**Current Phase:** Phase 1 — Repo Bootstrap

See [trendlab-v4-new-build-plan.md](trendlab-v4-new-build-plan.md) for the full development plan (Phases 1–14, with Phase 10 split into 10a/10b/10c, ~20–22 weeks solo).

### Workspace Structure (Phase 1 deliverable)

```text
trendlab-v3/
├── Cargo.toml                  # workspace root
├── trendlab-core/              # engine (domain, signals, execution, PM)
├── trendlab-runner/            # sweeps, caching, leaderboards
├── trendlab-tui/               # Ratatui interface
├── trendlab-cli/               # CLI wrapper
└── data/                       # canonical Parquet cache
```

### Common Commands

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

# Run benchmarks (Criterion, added in Phase 14)
cargo bench -p trendlab-core
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

**Void bar policy:** When a symbol has a NaN/missing bar, the market status is `Closed` for that symbol. Equity carries forward, pending orders are not checked, PM increments time counters but emits no price-dependent intents, and indicators/signals are not evaluated.

### C) Decision / Placement / Fill Timeline

Signals evaluated at bar T's close may only use data up to and including bar T. Orders generated from those signals execute on bar T+1 (next-bar-open by default). No order may execute on the same bar whose data generated the signal, unless using explicit intrabar logic (deferred).

### D) Look-Ahead Contamination Guard

No indicator value at bar t may depend on price data from bar t+1 or later. Every indicator and signal must have a look-ahead contamination test: compute on a truncated series (bars 1–100) and on the full series (bars 1–200), assert bars 1–100 are identical. This is mandatory and must pass before any phase gate.

### E) NaN Propagation Guard

Invalid or NaN input must never generate a trade. If a bar contains NaN in any OHLCV field, indicators produce NaN, signals produce no event, and the event loop skips order checks for that symbol on that bar. The invalid-bar rate is tracked per-run and flagged if >10% for any symbol.

### F) Deterministic RNG Hierarchy

A master seed generates deterministic sub-seeds for each (run_id, symbol, iteration) tuple. Sub-seeds are derived independently of thread scheduling order. Results must be identical regardless of thread count. Verified by running YOLO with 1 thread and 8 threads and asserting identical outputs.

### G) "Promotion Ladder" for realism vs speed

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
- Property tests for invariants (no double fill, OCO consistency, equity accounting, ratchet monotonicity)
- Look-ahead contamination tests for every indicator and signal (truncated vs full series)
- NaN injection tests for every indicator and signal (NaN input → no trade)
- Golden regression tests for stable benchmarks
- All core domain types must be `Send + Sync`

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
