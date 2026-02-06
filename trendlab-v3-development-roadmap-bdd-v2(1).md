# TrendLab v3 — Phased Development Plan

> Last updated: 2026-02-06
> This is a planning document only. No code belongs here. Every phase requires writing fresh, fully-thought-out code from scratch.

---

## What "done" looks like

A user clones this repo, downloads real market data with a single CLI command, runs a backtest against a named strategy preset, and opens the TUI to see real results on real data. No synthetic data, no fabricated numbers, no stubs. The full path from clone to first real backtest takes under 5 minutes.

---

## Two-track structure

This plan has two parallel tracks that merge at Phase 9:

- **Track A (Data Pipeline):** Phases 3 and 5a build the system that downloads, caches, and serves real market data.
- **Track B (Engine):** Phases 4 through 8 build the event loop, order system, execution, position management, and strategy composition.
- **Merge point:** Phase 9 (Runner) is the first phase that runs real strategies on real data. Both tracks must be complete before Phase 9 begins.

---

## Testing philosophy

- **BDD scenarios** test cross-module behavior and contracts. Keep them small and focused on observable behavior.
- **Unit tests** verify local correctness within each module.
- **Property tests** enforce invariants (no double fills, OCO consistency, equity accounting identities).
- **Golden tests** lock known-good outputs for regression detection.
- **Real data fixtures** (frozen Parquet files) provide deterministic integration tests starting in Phase 3.

Every phase ends with all existing tests still passing. No phase is done until its tests are green.

---

## Escape hatches (what to defer)

- Start with Yahoo Finance daily OHLC for US equities only. Defer multi-vendor, intraday, and international data.
- Ship three intrabar path policies first (Deterministic, WorstCase, BestCase). Add Monte Carlo path sampling later.
- Ship three to four position managers first. Add more later.
- Ship five signals first. Add more later.
- Ship the CLI with download and run commands first. Add sweep and report later.
- Ship the TUI with four core panels first. Expand after the runner is solid.

---

## Integration checkpoints

### Checkpoint 0 (after Phase 5a) — Real data flows
- The CLI download command retrieves real daily bars from Yahoo Finance
- Bars are cached as Parquet files on disk
- The dataset hash is deterministic for the same data
- The runner can load bars from the Parquet cache

### Checkpoint A (after Phase 6) — Engine correctness
- An end-to-end backtest runs on real SPY data (downloaded and cached)
- The order book and execution model produce realistic fills
- Equity accounting is correct at every bar
- (Hardcoded buy/sell logic is fine here — real signals come in Phase 8)

### Checkpoint B (after Phase 9) — Full pipeline
- Backtests run on a real multi-symbol universe (e.g., SPY + QQQ + IWM)
- The leaderboard shows real performance metrics
- Any leaderboard row is reproducible from its manifest
- The CLI run command produces a result file from a config file

### Checkpoint C (after Phase 11) — Explainability
- The TUI opens with real backtest results (not sample/fake data)
- Drill-down from leaderboard to trade tape to chart works
- Execution sensitivity and ghost curve are visible
- Ready to harden and optimize

---

# Phase 1 — Repo Bootstrap (Week 1)

## Goal
A clean workspace that builds, tests, lints, and is ready for development across all four crates.

## Steps
1. Create the Cargo workspace with four member crates: core, runner, tui, cli
2. Configure rustfmt and clippy policies for the workspace
3. Set up CI to run format check, clippy, and tests on every push
4. Write a one-page architecture invariants document covering the separation of concerns, the canonical pipeline flow, and the bar event loop contract
5. Verify the entire workspace builds and all (zero) tests pass

## Gate
- `cargo build --workspace` succeeds
- `cargo test --workspace` succeeds (even if there are zero tests)
- `cargo clippy --workspace` passes with no warnings
- Architecture invariants document exists and covers the three non-negotiable rules

---

# Phase 2 — Smoke Backtest (Week 1, continued)

## Goal
A tracer bullet that proves bars flow through the system end-to-end: bars in, orders generated, fills simulated, portfolio updated, equity curve out. This prevents "integration surprises" in later phases.

## Steps
1. Create a synthetic dataset of approximately 10 bars with known OHLCV values
2. Write hardcoded logic that buys on bar 3 and sells on bar 7 (no signal or PM system yet)
3. Build the minimal engine path: iterate bars, generate an order, simulate a fill, update the portfolio, record equity
4. Write a golden test that asserts the final equity and the trade list match hand-calculated expected values
5. This is throwaway scaffolding — it will be replaced by real components, but it proves the plumbing works

## Gate
- The golden test passes with exact expected values
- The test is deterministic (same result every run)

---

# Phase 3 — Domain Model (Week 2)

## Goal
Define every core type the system will use. This is the vocabulary of the entire application. Every later phase builds on these types.

## Steps
1. Design and implement the core market data type (bar with OHLCV, timestamp, symbol)
2. Design and implement order-related types: orders (with all fields needed for the order book), fills, and the various order type variants (market, stop, limit, stop-limit, brackets, OCO)
3. Design and implement position and portfolio types with fields for tracking cash, holdings, realized/unrealized PnL
4. Design and implement the trade record type that captures a complete round-trip trade with entry/exit details and signal traceability fields
5. Design and implement instrument metadata: tick size, lot size, currency, asset class, and a tick rounding policy that is side-aware (buy orders round one direction, sell orders round the other)
6. Design the deterministic ID system: config hash, dataset hash, and run ID, all using BLAKE3. Canonical serialization must sort keys. Symbol universes must iterate in deterministic order.
7. Add seed plumbing so any stochastic component can be seeded for reproducibility
8. Define the signal abstraction as a trait — signals take bar history and produce signal events (timestamp, symbol, directional intent, strength). Signals must never reference portfolio state. This trait is defined here but not implemented until Phase 8.
9. Define the indicator abstraction as a trait — indicators take bar history and produce a numeric series. Indicators are pure functions. Defined here, implemented in Phase 5b.
10. Write unit tests for every type: construction, serialization round-trips, deterministic hashing, tick rounding edge cases, and the signal-must-not-see-portfolio contract

## Gate
- All core types are defined with full serialization support
- Deterministic hashing: same config/data always produces the same hash
- Tick rounding is side-aware and tested for edge cases
- Signal and indicator trait contracts are defined and documented with tests
- All tests pass

---

# Phase 4 — Data Ingest + Yahoo Finance (Week 3)

## Goal
Build the complete data pipeline: download real market data from Yahoo Finance, validate it, cache it as Parquet, and serve it to the rest of the system. After this phase, we have real data on disk.

## Steps
1. Design a data provider abstraction that can fetch daily OHLCV bars for a given symbol and date range. This must be a trait so we can swap implementations and mock for tests.
2. Implement the Yahoo Finance provider. Research the best approach (yahoo_finance_api crate vs direct HTTP to Yahoo's API). Handle: rate limiting, exponential backoff retries, response parsing, error handling for network failures, invalid symbols, and empty responses.
3. Handle corporate actions: use Yahoo's adjusted close for split/dividend adjustment. Store adjustment metadata alongside the cached data.
4. Build the ingest pipeline: raw data from any source (Yahoo download, CSV import, Parquet import) goes through validation (schema check, OHLCV sanity), canonicalization (consistent format), sorting, deduplication, and anomaly detection.
5. Build the multi-symbol time alignment system: given bars for multiple symbols, align them to a common timeline. Define a policy for missing bars (forward-fill vs strict NaN). Validate that all symbols end up with the same bar count and aligned timestamps.
6. Build the Parquet cache layer: each symbol gets its own Parquet file plus a metadata sidecar file (hash, date range, source, adjustment info). Implement incremental updates — if the cache already covers the requested range, skip the download; if it's stale, download only the missing bars and append.
7. Build the CLI download command: accepts one or more symbols, optional start/end dates, and a force-redownload flag. Downloads, validates, caches, and prints a summary of what was downloaded.
8. Build named universe lists (e.g., a set of symbols like "the S&P 500 tickers") using deterministic-order collections
9. Write integration tests: download a symbol, verify the Parquet file is valid; re-download and verify cache is used (no HTTP call); extend the date range and verify only new bars are fetched; simulate network failure and verify the cache is not corrupted; try an invalid symbol and verify a clear error.
10. Create a frozen test fixture: download ~252 bars of SPY data, save as a Parquet file in the test fixtures directory. This file will be used by all future integration tests for deterministic results.

## Gate
- CLI download command successfully retrieves real SPY data from Yahoo Finance
- Data is cached as Parquet and survives process restart
- Cache hit skips the network call
- Incremental update appends only missing bars
- Frozen SPY fixture exists for integration tests
- All tests pass

---

# Phase 5a — Data Pipeline Integration (Week 4, Track A)

## Goal
Verify that the runner can load real data from the Parquet cache. This closes the gap between "we have data on disk" and "the backtest engine can use it."

## Steps
1. Build the bar loading function in the runner: given a list of symbols, load their Parquet files from the cache directory and return aligned bar data
2. Define the fallback policy clearly: if cached data exists, use it; if not and network is available, auto-download and cache; if no network, generate synthetic bars but emit an explicit warning that is visible to the user. Never silently substitute synthetic data for real data.
3. Write an integration test that uses the frozen SPY fixture: load bars from Parquet, run a trivial backtest, verify the result uses real prices (not synthetic)
4. Write an integration test for the fallback path: verify synthetic data produces a visible warning
5. Verify the CLI download command works end-to-end from scratch (clean cache directory)

## Gate
- **Checkpoint 0 passes**: CLI downloads real data, bars load from cache, dataset hash is deterministic, runner uses real prices
- All tests pass

---

# Phase 5b — Event Loop + Indicators (Week 4, Track B)

## Goal
Build the bar-by-bar event loop that is the heart of the backtesting engine. Also implement the core indicators that signals and position managers will need later.

## Steps
1. Implement the bar event loop with four phases per bar:
   - Start-of-bar: activate day orders, fill market-on-open orders
   - Intrabar: simulate trigger checks and fills for stop/limit orders
   - End-of-bar: fill market-on-close orders
   - Post-bar: mark-to-market positions, compute equity, then let the position manager emit maintenance orders for the NEXT bar (never the current bar)
2. Implement warmup handling: the engine must not generate any orders before the required indicator history exists. The warmup length equals the longest indicator lookback in the current strategy. Allow per-strategy warmup overrides.
3. Implement equity accounting: at every bar close, equity must equal cash plus the sum of all position values. Track realized PnL, unrealized PnL, and fees separately.
4. Implement the core indicators:
   - Simple moving average
   - Exponential moving average
   - Average true range
   - Relative strength index
   - Donchian channel (highest high / lowest low over a lookback window)
5. Implement the indicator precompute step: before the bar loop begins, compute all indicators using vectorized operations (Polars). Feed per-bar indicator values into the event loop so indicators are not recomputed on every bar.
6. Write unit tests for each indicator against known price series with hand-calculated expected values
7. Write tests for the warmup contract: no orders before warmup completes; warmup length respects indicator lookback
8. Write a property test for the equity accounting identity: equity == cash + position values, every bar, no exceptions

## Gate
- Event loop processes bars in the correct four-phase order
- Warmup prevents premature orders
- All five indicators produce correct values against known test data
- Equity accounting identity holds at every bar in every test
- All tests pass

---

# Phase 6 — Orders + Order Book (Week 5)

## Goal
Build the order book state machine and all order types. This is the infrastructure that execution and position management depend on.

## Steps
1. Implement all MVP order types: market (open, close, immediate), stop-market, limit, stop-limit
2. Implement bracket orders and OCO (one-cancels-other) groups
3. Build the order book state machine: orders transition through pending, triggered, filled, cancelled, and expired states. Define clear rules for each transition.
4. Implement atomic cancel/replace: when a position manager wants to adjust a stop price, the old order is cancelled and the new order is placed in a single atomic operation with no "stopless window." If the order was partially filled, only the remaining quantity is amended.
5. Build an audit trail so every order state transition is recorded for the trade tape
6. Write unit tests for every order type and state transition
7. Write a property test: OCO siblings can never both fill
8. Write a property test: bracket children only activate after the entry order fills
9. Write a test for cancel/replace atomicity: at no point during an amendment is the position unprotected

## Gate
- All order types work correctly
- OCO invariant holds: one fill cancels the other, always
- Bracket activation is correct
- Cancel/replace is atomic
- All tests pass

---

# Phase 7 — Execution Engine (Week 6)

## Goal
Build the fill simulation engine that determines how and when orders get filled given OHLC bar data. This is where execution realism lives.

## Steps
1. Integrate execution phases with the order book and the event loop from Phase 5b
2. Implement the three intrabar path policies:
   - Deterministic: OHLC ordering, no ambiguity
   - WorstCase (default): adversarial ordering — when both stop-loss and take-profit could trigger on the same bar, assume the worse outcome happened first
   - BestCase: optimistic ordering, for sensitivity comparison
3. Implement order priority rules: how to resolve which orders execute first within a single bar. WorstCase should execute stop-losses before take-profits. Make this configurable.
4. Implement gap rules: if price gaps through a stop level at the open, fill at the open price (which is worse than the trigger price), not at the trigger.
5. Implement slippage and spread modeling: start with fixed basis points. Design it so ATR-scaled slippage can be added later without restructuring.
6. Define four named execution presets that bundle a path policy, slippage amount, and commission rate. Include: a zero-cost frictionless preset, a realistic preset, a hostile/pessimistic preset, and an optimistic preset. These presets must map to a configuration structure that can be persisted and reproduced.
7. Implement optional liquidity constraints: a participation limit (percentage of bar volume) with a policy for what happens to the remainder (carry to next bar or cancel)
8. Write tests: stop gapped through fills at the worse price; ambiguous bars produce different outcomes under different path policies; same data with different presets produces different results; liquidity constraint produces partial fills
9. Run a full backtest on the frozen SPY fixture with hardcoded buy/sell logic using each execution preset and verify the results differ

## Gate
- **Checkpoint A passes**: end-to-end backtest on real SPY data, order book + execution working, accounting correct
- Three path policies produce measurably different results on the same data
- Gap fills are realistic (fill at open, not trigger)
- All tests pass

---

# Phase 8 — Position Management (Week 7)

## Goal
Build the position management system that adjusts stops and targets after entry, including the ratchet invariant that prevents the stickiness failure mode.

## Steps
1. Build the position manager interface: PMs emit order intents (cancel/replace requests), never direct fills. They operate after the post-bar mark-to-market step and their orders apply to the NEXT bar.
2. Implement the MVP position manager set:
   - Fixed percentage stop (e.g., sell if price drops X% from entry)
   - ATR-based trailing stop (tighten stop as price moves favorably, using ATR for distance)
   - Chandelier exit (trailing stop from the highest high, using ATR multiplier)
   - Time-based stop (exit after N bars regardless of price)
3. Implement the ratchet invariant: by default, a stop may tighten (move closer to current price on winning trades) but may NEVER loosen (move further away), even if volatility (ATR) expands. This is the core anti-stickiness mechanism.
4. Write specific anti-stickiness regression tests:
   - A chandelier-style exit must not get trapped by endlessly chasing a rising price — it must allow profitable exits when price reverses
   - A floor-style tightening must tighten on price rises but must not chase ceilings or loosen on price drops
5. Write a property test for the ratchet invariant: across any price path, the stop level must be monotonically non-decreasing for long positions and monotonically non-increasing for short positions
6. Test that PM intents are correctly translated into cancel/replace orders via the order book

## Gate
- All four position managers work correctly
- Ratchet invariant holds under volatility expansion (property test)
- Anti-stickiness scenarios pass (chandelier doesn't get trapped, floor doesn't chase)
- PM-generated orders are atomic cancel/replace operations
- All tests pass

---

# Phase 9 — Strategy Composition (Week 8)

## Goal
Build the five concrete signals, the factory system that wires configuration to runtime objects, the strategy preset system, and the TOML configuration file format. After this phase, users can define backtests via config files.

## Steps
1. Implement the five MVP signal generators, each implementing the signal trait from Phase 3:
   - Moving average crossover (trend following family)
   - Donchian breakout (breakout family)
   - RSI mean reversion (mean reversion family)
   - Momentum rotation / ranking (momentum family)
   - Buy-and-hold baseline (benchmark family)
2. Each signal must be thoroughly tested against known data with hand-verified expected outputs. Each signal must be completely portfolio-agnostic — verify this with a test that shows the same signal output regardless of current position state.
3. Build the factory system: a function that takes a signal configuration variant and returns a working signal object. The same factory pattern for order policies and position sizers. This is the bridge between serializable configuration and live runtime objects.
4. Build the strategy preset system: named compositions that bundle a specific signal, order policy, position manager, execution preset, and position sizer into a single ready-to-run configuration. Ship at least three presets: one trend-following, one breakout, one mean-reversion.
5. Design and implement the TOML configuration file format. A user should be able to define a complete backtest (strategy, execution settings, universe, date range, capital) in a single TOML file without writing any Rust code. The config file must parse into the existing run configuration structure.
6. Build the composition rules: signals produce directional intent, order policies translate intent into appropriate order types (breakout strategies use stop entries, mean-reversion strategies use limit entries), position sizers determine quantity, and position managers handle exits.
7. Write tests: every signal produces non-empty output on 252 bars of real SPY data; each preset produces actual trades; TOML config files parse correctly; the factory wiring is complete (no config variant that fails to instantiate)
8. Ship at least three example TOML config files that demonstrate different strategy types

## Gate
- All five signals produce verified output on test data
- Signals are demonstrably portfolio-agnostic
- Factory system converts every config variant into a working runtime object
- At least three strategy presets produce real trades on real data
- TOML config files parse and run successfully
- All tests pass

---

# Phase 10 — Runner + CLI + Metrics (Week 9-10)

## Goal
Build the runner that orchestrates complete backtests, the CLI that makes it usable, the performance metrics system, and the leaderboard. This is the merge point where real data meets real strategies. After this phase, users can run real backtests from the command line.

## Steps
1. Build the runner's real data loading path: load bars from the Parquet cache, not from synthetic generation. If cache is missing, attempt to download; if that fails, use synthetic with an explicit visible warning. The runner must NEVER silently use fake data.
2. Build trade extraction from the engine: the runner must collect actual trade records from the event loop, including entry/exit times, prices, quantities, PnL, commissions, slippage, and signal trace fields (what signal triggered this trade, what order type was used, what fill conditions occurred). The runner must never return an empty trade list when the strategy actually traded.
3. Design and implement the full performance metrics system. Research the standard definitions for each metric (Sharpe, Sortino, Calmar, max drawdown, win rate, profit factor, average trade return, average win/loss, trade count, average holding period). Implement each metric from first principles with proper edge case handling (what happens with zero trades, all-winning trades, zero variance, etc.). Do not copy formulas from this document — look up the canonical definitions and implement them with full understanding.
4. Build the fitness function system: a configurable selector for which metric to optimize/sort by, defaulting to Sharpe ratio.
5. Build the CLI run command: accepts either a TOML config file path or a named preset plus a symbol universe. Runs the backtest, saves the result as JSON to a results directory, and prints a summary to the terminal.
6. Build the sweep engine: given a configuration and parameter ranges, run multiple backtests exploring the parameter space. Persist all results (manifest, equity, trades, diagnostics).
7. Build the leaderboard system: session-scoped and all-time. Sortable by any fitness metric. Supports signal-only, PM-only, execution sensitivity, and composite views.
8. Implement cache invalidation: result cache keyed by manifest hash, indicator cache invalidated by parameter changes, feature cache keyed by dataset hash.
9. Write integration tests: run a real backtest on SPY with a real strategy, verify trades are extracted, verify all metrics are computed and non-NaN, verify the result file is written, verify the leaderboard entry is reproducible from its manifest.
10. Run each of the three strategy presets on real data and verify they all produce meaningful results.

## Gate
- **Checkpoint B passes**: real multi-symbol backtests work, leaderboard shows real metrics, CLI produces result files, results are reproducible
- Trade extraction produces real trades with signal trace information
- All performance metrics are correctly implemented with proper edge case handling
- At least three presets produce meaningful results on real data
- All tests pass

---

# Phase 11 — Robustness Ladder (Week 11)

## Goal
Build the promotion ladder that stress-tests strategy candidates, preventing overfitting and giving confidence that results are stable.

## Steps
1. Build the promotion ladder framework with levels that increase in computational cost. A strategy must pass each level before advancing to the next.
2. Implement Level 1 (Cheap Pass): deterministic intrabar policy with fixed slippage. This is the fast screening pass.
3. Implement Level 2 (Walk-Forward): train/test split with out-of-sample evaluation. The strategy must perform acceptably on data it wasn't optimized on.
4. Implement Level 3 (Execution Monte Carlo): vary slippage, spread, and commission parameters across multiple runs. Sample from distributions, not just run the baseline N times. The key requirement is that each MC run must produce genuinely different outcomes — if all runs produce identical results, the implementation is broken.
5. Design and implement a stability scoring system that considers both the median performance and the variance across Monte Carlo runs. A strategy with slightly lower median but much lower variance should rank above a strategy with higher median but wild swings. Use proper statistical methods — research them and implement from first principles.
6. Store full distribution data (median, IQR, tails) for each robustness level, not just best-case point estimates.
7. Stub the later levels (Path MC, Bootstrap) with clear placeholder markers but do not implement them yet. These are escape-hatched for now.
8. Write tests: failing Level 1 prevents advancement to Level 2 (no budget wasted); Execution MC runs produce non-identical results (actual parameter variation); the stability scoring system correctly penalizes high-variance strategies

## Gate
- Three-level promotion ladder works end-to-end
- Execution MC produces genuine variance (not N identical runs)
- Stability scoring penalizes variance appropriately
- A strategy must pass each level before advancing
- All tests pass

---

# Phase 12 — TUI (Weeks 12-13)

## Goal
Build the terminal UI that displays real backtest results with full drill-down explainability. The TUI must show real data by default and never pretend to have results when it doesn't.

## Steps
1. Build the TUI data pipeline: on startup, load backtest results from the results directory. If no results exist, show an informative empty state message telling the user how to run their first backtest. Implement a --demo flag that loads sample data for UI development/testing only, clearly labeled as demo data.
2. Build the theme system using semantic tokens: background, accent, positive (green), negative (pink), warning (orange), neutral (purple), muted text. All widgets reference semantic tokens, not hardcoded colors.
3. Build the core panels (ship four first, expand later):
   - **Leaderboard**: sortable by any fitness metric, session/all-time toggle, shows strategy name and key metrics per row
   - **Equity chart**: line chart with trade entry/exit markers, ghost curve overlay showing ideal vs real equity
   - **Trade tape**: full trade list with signal trace columns showing why each trade happened (signal intent, order type, fill context), slippage and gap indicators
   - **Run manifest viewer**: full configuration display for the selected run
4. Build the drill-down navigation flow: leaderboard row selection leads to a summary card, then into trade tape, then chart with markers, with keyboard shortcuts to jump between diagnostics, execution lab, sensitivity panel, manifest, and robustness views.
5. Build the execution lab panel: four preset buttons that trigger a rerun with a different execution configuration and display results side-by-side for comparison.
6. Build the sensitivity panel: a cross-preset comparison table showing metrics across presets, color-coded relative to the baseline.
7. Build the robustness ladder panel: level progress indicators, promoted/rejected status, stability scores, and distribution visualizations.
8. Build the diagnostics panel: per-trade analysis of entry/exit slippage, gap flags, and ambiguity resolutions.
9. Build the candle chart panel: OHLC rendering with order overlay lines showing stop and limit prices, togglable with the equity chart.
10. Build TUI-triggered backtests: the user can select a strategy preset, configure it, and launch a backtest from within the TUI. A background thread runs the backtest while the TUI shows a progress indicator. The result appears in the leaderboard when complete.
11. Build the ghost curve system: store both "ideal equity" (zero-friction fills) and "real equity" (actual fills with slippage/commission). Compute the execution drag as a percentage and dollar amount. Flag large drag values as a "death crossing."
12. Write tests: TUI with no results shows the empty state (not fake data); TUI with real results shows correct metrics in the leaderboard; demo flag shows sample data with a clear demo indicator; drill-down navigation from leaderboard to trade tape to chart works correctly.

## Gate
- **Checkpoint C passes**: TUI opens with real results, drill-down works, execution sensitivity visible, ghost curve visible
- No fake data shown by default
- All nine panels are functional
- Drill-down navigation works end-to-end
- All tests pass

---

# Phase 13 — Reporting + Exports (Week 14)

## Goal
Build the reporting system that produces exportable artifacts from backtest results.

## Steps
1. Define the per-run artifact set: manifest, equity curve, trade list, and diagnostics, all persisted as JSON
2. Build JSON export of the full backtest result structure (must be deserializable back into the same types — round-trip test)
3. Build CSV export of the trade tape with all columns including signal trace fields
4. Build a markdown report generator for a single run: composition summary, key metrics, robustness summary
5. Build a comparison report generator: two strategies side-by-side with a metrics table and equity curve overlay
6. Write tests: JSON export round-trips correctly; CSV export contains all columns; every leaderboard row has exportable artifacts

## Gate
- All export formats produce valid output
- JSON round-trips without data loss
- CSV includes signal trace columns
- All tests pass

---

# Phase 14 — Hardening + Docs + Quick Start (Week 15)

## Goal
Lock down performance, add regression protection with real data, write user documentation, and ship example configurations.

## Steps
1. Add Criterion benchmarks for the hot paths: bar event loop, order book operations, execution fill simulation
2. Build the real-data golden test: use the frozen SPY fixture from Phase 4, run a known strategy with known parameters, and assert the exact equity curve and trade list. This test must break if anyone changes the engine's behavior.
3. Verify all existing golden tests (synthetic) still pass
4. Add property tests for all remaining invariants: no double fills, OCO consistency, equity accounting, ratchet monotonicity
5. Write the Quick Start guide: a document that takes a new user from clone to first real backtest in under 5 minutes (clone, download data, run a preset, view in TUI)
6. Write the extension guide: how to add new signals, position managers, and order policies
7. Write the configuration reference: all TOML options documented
8. Ship three to five example TOML configuration files demonstrating different strategy types (trend following, breakout, mean reversion, buy-and-hold benchmark)
9. Run the full test suite one final time and fix any remaining issues

## Gate
- Benchmarks exist for all hot paths
- Real-data golden test passes with exact expected values
- Quick start guide works from scratch (clone to real backtest)
- Example configs all produce valid results
- Full test suite passes
- All Definition of Done criteria are met

---

## Global Definition of Done

You are "v3 done" when all of the following are true:

1. **Real data flows.** A user can download real market data from Yahoo Finance with a single CLI command.
2. **Real strategies run.** The system ships with three or more working strategy presets that produce real trades on real data.
3. **Reproducible.** Any leaderboard row can be reproduced from its manifest (configuration + seed + dataset hash).
4. **Explicit execution.** Execution assumptions are explicit (named presets with specific parameters) and sensitivity is visible via side-by-side comparison.
5. **Isolatable.** Signal effects, position management effects, and execution effects can be isolated and compared independently.
6. **Robust.** The robustness ladder promotes stable candidates using distribution statistics, not just point estimates.
7. **Explainable.** The TUI drill-down makes "why did this win?" obvious through trade tape, chart overlays, diagnostics, and ghost curve.
8. **No fake data by default.** The TUI shows real results when they exist and an honest empty state when they don't. Sample data requires an explicit --demo flag.
9. **CLI works.** The CLI supports download and run commands (sweep and report are later additions).
10. **Approachable.** A new user can go from clone to first real backtest in under 5 minutes by following the Quick Start guide.
