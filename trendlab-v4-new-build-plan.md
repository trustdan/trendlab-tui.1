# TrendLab v3 — Phased Development Plan

> **Naming note:** The project is "TrendLab v3" (engine architecture version). This plan file is named "v4" because it is the fourth revision of the development plan. All "v3" references below refer to the product being built.

> Last updated: 2026-02-06
> This is a planning document only. No code belongs here. Every phase requires writing fresh, fully-thought-out code from scratch.
> Informed by the v2 technical writeup — the v2 app's UX, component architecture, YOLO mode, and statistical validation were well-designed. This plan recreates those behaviors on a correct engine foundation.

---

## What "done" looks like

A user clones this repo, launches the TUI, navigates to the Data panel, selects tickers by sector, presses a key to fetch real market data from Yahoo Finance with a per-symbol progress bar, switches to the Sweep panel, configures YOLO mode with parameter jitter and structural exploration sliders, hits go, and watches the Results leaderboard fill with real strategy discoveries ranked by Sharpe across hundreds of symbols (with a warm cache; the first-run Quick Start experience in Phase 14 uses 3–5 symbols). The full path from clone to first real YOLO run takes under 5 minutes. No synthetic data, no fabricated numbers, no stubs.

**Timeline estimate:** 20–22 weeks for a solo developer. 17–18 weeks is achievable with a two-person team (one on engine, one on TUI starting at Phase 12). The Phase 10 split (10a/10b/10c) and Phase 11 expansion are the primary schedule additions over the original estimate.

---

## Two-track structure

This plan has two parallel tracks that merge at Phase 10a:

- **Track A (Data Pipeline):** Phases 4 and 5a build the system that downloads, caches, and serves real market data.
- **Track B (Engine):** Phases 5b through 9 build the event loop, order system, execution, position management, and strategy composition.
- **Merge point:** Phase 10a (Runner) is the first phase that runs real strategies on real data. Both tracks must be complete before Phase 10a begins.

### Track interface contract

The two tracks share a single data contract defined in Phase 3:

- **Parquet schema:** columns are `date` (Date), `open` (f64), `high` (f64), `low` (f64), `close` (f64), `volume` (u64), `adj_close` (f64)
- **Sort order:** ascending by date within each symbol partition
- **Timezone convention:** market-local (US Eastern for US equities)
- **Missing bar policy:** NaN for missing bars in aligned multi-symbol datasets. No forward-fill of OHLCV values — a missing bar is missing, not fabricated. Forward-fill is only permitted for reporting/alignment timestamps, never for tradable price data.
- **Partitioning:** Hive-style `symbol=XXX/year=YYYY/` directories

Track A (Phase 4) produces Parquet files conforming to this schema. Track B (Phase 5b) consumes them. Phase 5a verifies the contract holds. A schema validation function is defined in Phase 3 and used by both tracks.

---

## Testing philosophy

- **BDD scenarios** test cross-module behavior and contracts. Keep them small and focused on observable behavior.
- **Unit tests** verify local correctness within each module.
- **Property tests** enforce invariants (no double fills, OCO consistency, equity accounting identities).
- **Golden tests** lock known-good outputs for regression detection.
- **Real data fixtures** (frozen Parquet files) provide deterministic integration tests starting in Phase 4.

Every phase ends with all existing tests still passing. No phase is done until its tests are green.

### Look-ahead contamination guard (project-wide invariant)

No indicator value at bar t may depend on price data from bar t+1 or later. The canonical test: compute the indicator on a truncated series (bars 1–100) and on the full series (bars 1–200), and assert that bars 1–100 produce identical values in both cases. If they differ, there is look-ahead contamination. This test is mandatory for every indicator (Phase 5b) and every signal (Phase 9).

### Decision / placement / fill timeline (project-wide invariant)

Signals evaluated at bar T's close may only use data up to and including bar T. Orders generated from those signals execute on bar T+1 (next-bar-open by default). No order may execute on the same bar whose data generated the signal, unless using explicit intrabar logic (deferred). This is the canonical timing contract that all signal and execution implementations must obey.

### NaN propagation guard (project-wide invariant)

Invalid or NaN input must never generate a trade. If a bar contains NaN in any OHLCV field, indicators computed from that bar produce NaN, signals produce no signal event, and the event loop skips order checks for that symbol on that bar. The invalid-bar rate (fraction of bars that were NaN for each symbol) must be tracked per-run and surfaced in the run result metadata. If the invalid-bar rate exceeds 10% for any symbol, flag the result with a data quality warning. This invariant is tested by injecting NaN bars into a known-good price series and asserting zero trades are generated on or from those bars.

---

## Escape hatches (what to defer)

- Start with Yahoo Finance daily OHLC for US equities only. Defer multi-vendor, intraday, and international data.
- Ship three intrabar path policies first (Deterministic, WorstCase, BestCase). Add Monte Carlo path sampling later.
- Ship all nine position managers from MVP. They are the heart of the anti-stickiness system and cannot be deferred.
- Ship all ten signals from MVP. The v2 app had ten and the structural exploration requires a wide pool to sample from.
- Ship the CLI with download and run commands first. Add sweep and report later.
- Ship the TUI with the full six-panel layout. The panels are the navigation backbone — shipping a partial set creates UX debt.
- Defer combo strategies (2-way, 3-way) until after single-strategy YOLO is solid.
- **Synthetic data policy:** Synthetic bars are a developer-only debug mode. They require an explicit `--synthetic` flag. Results produced on synthetic data are tagged as synthetic and cannot enter the all-time leaderboard. The system must never silently substitute synthetic data for real data. If data is unavailable and `--synthetic` is not set, the operation fails with a clear error message.
- **Position sizing:** Position sizing is implemented as a fixed percent-of-equity parameter on the order intent (default: 100% of available capital per position). This is a known simplification. A composable Sizer trait (fixed-dollar, volatility-targeted, Kelly criterion) is deferred to post-v1. The current approach means every strategy uses the same capital allocation rule, which is sufficient for relative comparison on the leaderboard but not for portfolio-level analysis.
- **Survivorship bias:** The Yahoo Finance data source only includes currently listed tickers. Survivorship bias is a known limitation — backtests on today's S&P 500 constituents will overstate historical performance because they exclude delisted companies. Symbol alias support (FB → META) is deferred. Document this limitation in the architecture invariants and Quick Start guide.

---

## Integration checkpoints

### Checkpoint 0 (after Phase 5a) — Real data flows

- The CLI download command retrieves real daily bars from Yahoo Finance
- Bars are cached as Parquet files on disk with Hive-style partitioning
- The dataset hash is deterministic for the same data
- The runner can load bars from the Parquet cache

### Checkpoint A (after Phase 7) — Engine correctness

- An end-to-end backtest runs on real SPY data (downloaded and cached)
- The order book and execution model produce realistic fills
- Equity accounting is correct at every bar
- (Hardcoded buy/sell logic is fine here — real signals come in Phase 9)

### Checkpoint A.5 (after Phase 7) — Data-to-engine integration

- The Phase 7 execution engine successfully consumes bars loaded from the Phase 4 Parquet cache
- The Parquet schema contract (from the track interface contract) is verified end-to-end
- A mini-runner integration test runs a hardcoded strategy on real cached SPY data through the full execution engine
- This checkpoint prevents discovering data/engine incompatibilities at the Phase 10a merge

### Checkpoint B (after Phase 10c) — Full pipeline

- Backtests run on a real multi-symbol universe (e.g., SPY + QQQ + IWM)
- The leaderboard shows real performance metrics
- Any leaderboard row is reproducible from its manifest
- The CLI run command produces a result file from a config file
- YOLO mode runs continuously and populates the leaderboard with real discoveries

### Checkpoint C (after Phase 12) — Full app

- The TUI opens to the six-panel layout with vim navigation
- Data panel shows sector/ticker tree with cache status indicators
- YOLO mode runs from within the TUI with progress updates
- Results leaderboard shows session and all-time results with risk profile switching
- Chart panel renders equity curves for selected leaderboard entries
- Ready to harden and optimize

---

# Phase 1 — Repo Bootstrap (Week 1)

## Goal

A clean workspace that builds, tests, lints, and is ready for development across all four crates.

## Steps

1. Create the Cargo workspace with four member crates: core, runner, tui, cli
2. Configure rustfmt and clippy policies for the workspace
3. Set up CI to run format check, clippy, and tests on every push
4. Write a one-page architecture invariants document covering the separation of concerns, the canonical pipeline flow, the bar event loop contract, and the decision/placement/fill timeline (signals at bar T close → orders execute on bar T+1). This timeline is the canonical timing contract for the entire project.
5. Verify the entire workspace builds and all (zero) tests pass

## Gate

- `cargo build --workspace` succeeds
- `cargo test --workspace` succeeds (even if there are zero tests)
- `cargo clippy --workspace` passes with no warnings
- Architecture invariants document exists and covers the three non-negotiable rules plus the decision/placement/fill timeline

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

Define every core type the system will use. This is the vocabulary of the entire application. Every later phase builds on these types. The four-component composition model (signal + position manager + execution + filter) is established here.

## Steps

1. Design and implement the core market data type (bar with OHLCV, timestamp, symbol)
2. Design and implement order-related types: orders (with all fields needed for the order book), fills, and the various order type variants (market, stop, limit, stop-limit, brackets, OCO)
3. Design and implement position and portfolio types with fields for tracking cash, holdings, realized/unrealized PnL
4. Design and implement the trade record type that captures a complete round-trip trade with entry/exit details and signal traceability fields
5. Design and implement instrument metadata: tick size, lot size, currency, asset class, and a tick rounding policy that is side-aware (buy orders round one direction, sell orders round the other)
6. Design the deterministic ID system: config hash, dataset hash, and run ID, all using BLAKE3. Canonical serialization must sort keys. Symbol universes must iterate in deterministic order.
7. Add seed plumbing so any stochastic component can be seeded for reproducibility. Design the RNG hierarchy: a master seed generates deterministic sub-seeds for each (run_id, symbol, iteration) tuple. Sub-seeds must be derived independently of thread scheduling order so that results are identical regardless of thread count or execution order. This is critical for YOLO reproducibility under parallelism.
8. Define the Parquet schema contract that both Track A (data pipeline) and Track B (engine) must conform to. Specify exact column names (`date`, `open`, `high`, `low`, `close`, `volume`, `adj_close`), data types (Date, f64, f64, f64, f64, u64, f64), sort order (ascending by date), and timezone convention (market-local). Build a schema validation function that checks any loaded DataFrame against this contract. This is the boundary between the two tracks.
9. Define the four component traits that compose a strategy:
   - **Signal generator**: takes bar history and produces signal events (timestamp, symbol, directional intent, strength, and an optional metadata payload). Signals must never reference portfolio state. The metadata payload carries context for downstream components (e.g., breakout level, reference price, signal bar low) without violating portfolio-agnosticism — the signal describes the market event, not the portfolio.
   - **Position manager**: takes current position state and bar data, emits order intents for the next bar. Must support the ratchet invariant. May read signal metadata from the triggering signal event (e.g., to set an initial stop below the breakout level).
   - **Execution model**: determines how orders get filled (next-bar-open, stop, limit, close-on-signal). Configurable slippage and commission.
   - **Signal filter**: gates entry signals based on market conditions (trend regime, volatility level, momentum strength). A pass-through "no filter" must be the default.
   All four traits are defined here but not implemented until later phases. Signal events are immutable once emitted — they describe a market event, not a downstream decision. The filter accept/reject trace is stored in a separate `SignalEvaluation` record that references the original signal event by ID. The evaluation record captures: the signal event ID, the filter that evaluated it, the verdict (passed, filtered_by_adx, filtered_by_regime, filtered_by_volatility), and the filter's state at evaluation time (e.g., current ADX value). This separation keeps signals pure while preserving full diagnostics for stickiness analysis and the "isolatable" Definition of Done criterion. Evaluation records are attached to trade records and leaderboard entries, not to the signal events themselves.
10. Define the indicator abstraction as a trait — indicators take bar history and produce a numeric series. Indicators are pure functions. Defined here, implemented in Phase 5b.
11. Design the run fingerprinting types: each backtest run gets a config hash (structure only, for grouping similar compositions) and a full hash (structure + parameters, for exact deduplication). These enable the YOLO history and meta-analysis system in Phase 10c.
12. Write unit tests for every type: construction, serialization round-trips, deterministic hashing, tick rounding edge cases, the signal-must-not-see-portfolio contract, Parquet schema validation, RNG sub-seed determinism (same master seed → same sub-seeds regardless of derivation order), and signal evaluation records correctly referencing their source signal events

## Gate

- All core types are defined with full serialization support
- All four component traits are defined with clear contracts
- All core domain types are `Send + Sync` (compile-time bounds check to prevent a retrofit when the TUI worker thread is introduced in Phase 12)
- Deterministic hashing: same config/data always produces the same hash
- Tick rounding is side-aware and tested for edge cases
- Run fingerprinting produces distinct hashes for structurally different vs parametrically different configs
- Parquet schema validation function correctly accepts valid data and rejects malformed data
- RNG sub-seeds are deterministic and scheduling-order-independent
- All tests pass

---

# Phase 4 — Data Ingest + Yahoo Finance (Weeks 3–4)

## Goal

Build the complete data pipeline: download real market data from Yahoo Finance, validate it, cache it as Parquet, and serve it to the rest of the system. After this phase, we have real data on disk. The data layer must support the sector/ticker hierarchy that the TUI's Data panel will display.

**Scope note:** This phase may take 1.5–2 weeks. If it overruns Week 3, the priority order is: (1) download + Parquet cache + frozen fixture (minimum viable for unblocking Track B), (2) validation + anomaly detection + incremental update, (3) universe config + cache status query. Steps 1–3 and 6–7 are critical path; steps 4–5 and 8–9 can slip to early Week 4 without blocking Track B.

## Steps

1. Design a data provider abstraction that can fetch daily OHLCV bars for a given symbol and date range. This must be a trait so we can swap implementations and mock for tests.
2. Implement the Yahoo Finance provider. Research the best approach (yahoo_finance_api crate vs direct HTTP to Yahoo's API). Handle: rate limiting, exponential backoff retries, response parsing, error handling for network failures, invalid symbols, and empty responses. The data provider must return structured error types that distinguish between: network unreachable, rate limited (with retry-after hint), response format changed (parsing failure on previously-working symbol), authentication required, and symbol not found. These structured errors must be displayable in both the CLI and TUI. Note that Yahoo Finance has no official API and is subject to unannounced format changes — the CSV import path (Step 4) is the primary fallback when Yahoo is unavailable. Implement a circuit breaker for the DataFetcher: if the provider returns HTTP 403 Forbidden (IP ban) or repeated 429 Too Many Requests after exhausting retries, the circuit breaker trips and the entire DataFetcher refuses all subsequent requests for the remainder of the session (or a configurable cooldown period, default 30 minutes). When tripped, immediately surface a "Hard Stop: data provider has blocked requests" error to the user. The circuit breaker prevents retry logic from hammering a provider that has already banned the client, which would extend the ban duration.
3. Handle corporate actions: all OHLC columns must be split-adjusted using the same adjustment ratio derived from Yahoo's adjusted close. Compute the adjustment ratio as `adj_close / close` for each bar, then multiply open, high, low, and close by this ratio. Volume is reverse-adjusted (divided by the ratio) to preserve notional volume. The `adj_close` column is retained in the Parquet schema for reference, but all indicator and execution calculations use the fully-adjusted OHLC columns. This prevents the "ATR on raw OHLC with adjusted close" problem where volatility measures mix adjusted and unadjusted price scales. Store the per-bar adjustment ratios as metadata alongside the cached data. Write a consistency test around a known stock split date: verify that ATR computed across the split boundary produces smooth, continuous values (no artificial spike from the raw-to-adjusted transition).
4. Build the ingest pipeline: raw data from any source (Yahoo download, CSV import, Parquet import) goes through validation (schema check, OHLCV sanity), canonicalization (consistent format), sorting, deduplication, and anomaly detection.
5. Build the multi-symbol time alignment system: given bars for multiple symbols, align them to a common timeline. Apply the missing bar policy defined in the track interface contract: strict NaN for missing bars, no forward-fill of tradable price data. Validate that all symbols end up with the same bar count and aligned timestamps.
6. Build the Parquet cache layer using Hive-style partitioning (symbol and year directories) to enable Polars predicate pushdown — when scanning for a single symbol's data, Polars can skip all other symbol directories without reading them. Include a metadata sidecar file per symbol (hash, date range, source, adjustment info). Implement incremental updates — if the cache already covers the requested range, skip the download; if it's stale, download only the missing bars and append. Cache writes must be atomic per symbol: write to a temporary file first, then rename into place. If the process crashes during a batch download of 500 symbols, the next run must detect already-cached symbols and only fetch the remainder. A partially-written Parquet file must never corrupt the cache. On load, validate the integrity of each Parquet file (schema check, row count > 0, no fully-NaN rows where all OHLCV fields are NaN). If a cached file fails validation, quarantine it by renaming to `{filename}.quarantined` and log a warning. The next fetch for that symbol will re-download fresh data. Quarantined files are not silently re-used.
7. Build the CLI download command: accepts one or more symbols, optional start/end dates, and a force-redownload flag. Must report progress during multi-symbol downloads: which symbol is being fetched, how many are complete out of the total, and a summary when done. This progress reporting infrastructure will be reused by the TUI's Data panel later.
8. Build the universe configuration system: a sector-organized list of tickers (GICS sectors: Technology, Healthcare, Finance, Energy, etc.) stored as a TOML config file. Each sector contains its member tickers. The universe config supports: loading predefined universes (e.g., S&P 500 tickers organized by sector), adding custom symbols, and selecting/deselecting individual tickers or entire sectors. The sector hierarchy is essential — the TUI's Data panel will display it as a two-level tree.
9. Build a cache status query function: given a list of symbols, quickly report which have cached data, what date range is cached, and which need downloading. This will power the "green dot" indicators in the TUI's Data panel.
10. Write integration tests: download a symbol, verify the Parquet file is valid; re-download and verify cache is used (no HTTP call); extend the date range and verify only new bars are fetched; simulate network failure and verify the cache is not corrupted; try an invalid symbol and verify a clear error.
11. Create a frozen test fixture: download ~252 bars of SPY data, save as a Parquet file in the test fixtures directory. This file will be used by all future integration tests for deterministic results.

## Gate

- CLI download command successfully retrieves real SPY data from Yahoo Finance
- Data is cached as Parquet with Hive-style partitioning and survives process restart
- Cache hit skips the network call
- Incremental update appends only missing bars
- Universe config loads sector/ticker hierarchy from TOML
- Cache status query correctly reports which symbols have data
- Progress reporting works during multi-symbol downloads
- Frozen SPY fixture exists for integration tests
- All tests pass

---

# Phase 5a — Data Pipeline Integration (Week 4, Track A)

## Goal

Verify that the runner can load real data from the Parquet cache. This closes the gap between "we have data on disk" and "the backtest engine can use it."

## Steps

1. Build the bar loading function in the runner: given a list of symbols, load their Parquet files from the cache directory and return aligned bar data. Use Polars lazy scanning with predicate pushdown for efficient I/O.
2. Define the fallback policy clearly: if cached data exists, use it; if not and network is available, auto-download and cache; if no network and `--synthetic` flag is set, generate synthetic bars with an explicit visible warning and tag the results as synthetic. If no network and no `--synthetic` flag, the operation **fails with a clear error message**. The system must never silently substitute synthetic data for real data. Synthetic results cannot enter the all-time leaderboard.
3. Write an integration test that uses the frozen SPY fixture: load bars from Parquet, run a trivial backtest, verify the result uses real prices (not synthetic)
4. Write an integration test for the fallback path: verify that without `--synthetic` the operation fails cleanly; verify that with `--synthetic` the results are tagged
5. Verify the CLI download command works end-to-end from scratch (clean cache directory)

## Gate

- **Checkpoint 0 passes**: CLI downloads real data, bars load from cache, dataset hash is deterministic, runner uses real prices
- All tests pass

---

# Phase 5b — Event Loop + Indicators (Week 4, Track B)

## Goal

Build the bar-by-bar event loop that is the heart of the backtesting engine. Also implement the full indicator library that signals, position managers, and filters will need later. The v2 app's ten signal types each depend on different indicators — we need the full set here.

## Steps

1. Implement the bar event loop with four phases per bar:
   - Start-of-bar: activate day orders, fill market-on-open orders
   - Intrabar: simulate trigger checks and fills for stop/limit orders
   - End-of-bar: fill market-on-close orders
   - Post-bar: mark-to-market positions, compute equity, then let the position manager emit maintenance orders for the NEXT bar (never the current bar). **Critical timing rule:** post-bar PM processing must see all fills from all prior phases of the current bar (including end-of-bar MOC fills and close-on-signal fills). The order book must be updated with end-of-bar fills *before* the PM runs at post-bar. If a close-on-signal fill exits a position at end-of-bar, the PM must NOT emit a maintenance order for that now-closed position.
2. Implement the void bar policy: when the event loop encounters a NaN/missing bar for a symbol (where all OHLCV fields are NaN, as produced by the multi-symbol alignment in Phase 4), the following rules apply:
   - The symbol's market status is `MarketStatus::Closed` for that bar. Add `MarketStatus` as an enum variant (`Open`, `Closed`) to the bar type or as a per-symbol-per-bar flag.
   - Equity for that symbol is marked-to-market using the previous bar's close price (carry forward the last known value). No PnL change is recorded.
   - Pending orders (stops, limits, brackets) are NOT checked against NaN prices. They remain pending and carry to the next valid bar.
   - The position manager's `on_bar` method receives the `MarketStatus::Closed` variant, allowing it to increment time-based counters (e.g., max holding period) without seeing a price update. The PM must NOT emit any price-dependent order intents on a closed bar.
   - Indicators that depend on the NaN bar produce NaN for that bar (per the NaN propagation invariant). Signals are not evaluated.
   - Write a test: insert 3 consecutive NaN bars into a known price series for one symbol while another symbol trades normally. Verify: no orders are checked on NaN bars, equity carries forward, time-based PM counters still advance, and the system resumes normal operation on the next valid bar.
3. Implement warmup handling: the engine must not generate any orders before the required indicator history exists. The warmup length equals the longest indicator lookback in the current strategy. Allow per-strategy warmup overrides.
4. Implement equity accounting: at every bar close, equity must equal cash plus the sum of all position values. Track realized PnL, unrealized PnL, and fees separately.
5. Implement the full indicator library. These indicators are needed by the ten signals, nine position managers, and four signal filters defined in later phases:
   - Simple moving average (needed by MA crossover signal, MA regime filter)
   - Exponential moving average (needed by MA crossover signal, Keltner channel)
   - Average true range (needed by ATR trailing stop, Chandelier exit, Keltner channel, volatility filter, Supertrend)
   - Relative strength index (needed by RSI mean reversion signal)
   - Donchian channel — highest high / lowest low over a lookback window (needed by Donchian breakout signal, 52-week breakout)
   - Bollinger bands — moving average plus/minus standard deviation multiplier (needed by Bollinger breakout signal)
   - Keltner channel — EMA plus/minus ATR multiplier (needed by Keltner breakout signal)
   - Supertrend — ATR-based directional indicator that flips between support and resistance (needed by Supertrend signal)
   - Parabolic SAR — Wilder's acceleration factor system (needed by Parabolic SAR signal)
   - Aroon — measures time since highest high and lowest low as a percentage (needed by Aroon crossover signal)
   - Rate of change — percentage price change over N bars (needed by ROC momentum signal)
   - ADX / directional movement index (needed by ADX signal filter)
   - Momentum — simple lookback return (needed by time-series momentum signal)
6. Implement the indicator precompute step: before the bar loop begins, compute all indicators once. Polars lazy expressions are the default and preferred approach. However, indicators that are inherently sequential/stateful (e.g., Parabolic SAR, Supertrend) may be computed sequentially in a controlled loop if the Polars implementation would be incorrect or unmaintainable. The requirement is that indicators are computed *once before the bar loop* — not that they must use Polars. Document which indicators use which computation method. Feed per-bar indicator values into the event loop so indicators are not recomputed on every bar. This is the hybrid vectorized/sequential approach: Polars handles the vectorized indicator math, the bar loop handles the sequential position state machine.
7. Write unit tests for each indicator against known price series. For simple indicators (SMA, EMA, RSI), hand-calculated expected values are sufficient. For complex indicators (Parabolic SAR, Supertrend, Aroon), test against external reference outputs from a trusted source (e.g., TA-Lib, pandas_ta). Document the reference source for each indicator's test data.
8. Write a look-ahead contamination test for every indicator: compute the indicator on a truncated series (bars 1–100) and on the full series (bars 1–200). Assert that the values for bars 1–100 are identical in both cases. If they differ, there is look-ahead bias. This test is mandatory for every indicator and must pass before the phase gate.
9. Write an integration test for each indicator: verify that the Polars-precomputed values match a naive bar-by-bar loop computation for every bar. This catches off-by-one errors in lookback window alignment that unit tests may miss.
10. Write tests for the warmup contract: no orders before warmup completes; warmup length respects indicator lookback
11. Write a property test for the equity accounting identity: equity == cash + position values, every bar, no exceptions

## Gate

- Event loop processes bars in the correct four-phase order
- Void bar policy correctly handles NaN bars (no fills, equity carry, PM time advance, pending orders preserved)
- Post-bar PM processing sees end-of-bar fills (close-on-signal interaction tested)
- Warmup prevents premature orders
- All thirteen indicators produce correct values against known test data
- Look-ahead contamination test passes for every indicator (truncated vs full series)
- Precomputed values match bar-by-bar loop computation for every indicator
- Equity accounting identity holds at every bar in every test
- All tests pass

---

# Phase 6 — Orders + Order Book (Week 5)

## Goal

Build the order book state machine and all order types. This is the infrastructure that execution and position management depend on.

## Steps

1. Implement all MVP order types: market (open, close, immediate), stop-market, limit, stop-limit
2. Implement bracket orders and OCO (one-cancels-other) groups
3. Build the order book state machine: orders transition through pending, triggered, filled, cancelled, and expired states. Define clear rules for each transition. Define cancellation semantics per order state: a pending order can be cancelled cleanly; a triggered-but-not-filled order can be cancelled; a partially-filled order cancellation affects only the unfilled remainder. The audit trail must record the cancellation reason for each cancelled order.
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
4. Implement gap rules: if price gaps through a stop level at the open, fill at the open price (which is worse than the trigger price), not at the trigger. Support three gap policies: fill at open, fill at trigger level, fill at worst of the two.
5. Implement slippage and spread modeling: start with fixed basis points applied directionally (buyers pay more, sellers receive less). Design it so ATR-scaled slippage can be added later without restructuring.
6. Implement per-side fee calculation in basis points, configurable via a cost model.
7. Define four named execution presets that bundle a path policy, slippage amount, and commission rate. Include: a zero-cost frictionless preset, a realistic preset, a hostile/pessimistic preset, and an optimistic preset. These presets must map to a configuration structure that can be persisted and reproduced.
8. Implement the four execution model types from the component architecture:
   - Next-bar-open (default): market order fills at the next bar's open price
   - Stop order: fill at stop level with gap policy
   - Limit order: fill at limit price if reached
   - Close-on-signal: fill at the signal bar's close
9. Implement optional liquidity constraints: a participation limit (percentage of bar volume) with a policy for what happens to the remainder (carry to next bar or cancel)
10. Write tests: stop gapped through fills at the worse price; ambiguous bars produce different outcomes under different path policies; same data with different presets produces different results; liquidity constraint produces partial fills; directional slippage is applied correctly; entry and exit triggers in the same bar produce correct results across all three path policies (common in breakout systems — verify the system does not gamify same-bar entry+exit to produce unrealistic results)
11. Extend the equity accounting property test from Phase 5b to cover partial fills. Under carry policy: equity == cash + position market value + pending carry value. Under cancel policy: equity == cash + position market value (cancelled remainder has no value). Test both policies explicitly.
12. Run a full backtest on the frozen SPY fixture with hardcoded buy/sell logic using each execution preset and verify the results differ

## Gate

- **Checkpoint A passes**: end-to-end backtest on real SPY data, order book + execution working, accounting correct
- Three path policies produce measurably different results on the same data
- All four execution model types work correctly
- Gap fills are realistic (fill at open, not trigger)
- All tests pass

---

# Phase 8 — Position Management (Week 7)

## Goal

Build the full position management system with all nine PM types. The v2 app identified stickiness as the core failure mode of trend-following backtests — positions that never exit because the stop keeps chasing the price. Every PM must obey the ratchet invariant, and stickiness must be measurable.

## Steps

1. Build the position manager interface: PMs emit order intents (cancel/replace requests), never direct fills. They operate after the post-bar mark-to-market step and their orders apply to the NEXT bar.
2. Implement all nine position managers:
   - **ATR trailing stop**: trail at the highest high minus ATR times a multiplier. Key parameters: ATR period, multiplier.
   - **Chandelier exit**: similar to ATR trailing but measured from the highest high since entry, not the recent high. Key parameters: period, multiplier.
   - **Percent trailing stop**: trail at a fixed percentage below the highest high since entry. Key parameter: trail percentage.
   - **Since-entry trailing**: exit at a percentage below the highest price since entry. Key parameter: exit percentage threshold.
   - **Frozen reference exit**: exit at a fixed percentage below the reference price at entry time (the stop never moves — it's frozen at entry). Key parameter: exit percentage.
   - **Time decay stop**: the stop tightens over time, starting wide and narrowing on each bar. Key parameters: initial percentage, decay per bar, minimum percentage.
   - **Max holding period**: force exit after N bars regardless of price. Key parameter: max bars.
   - **Fixed stop-loss**: simple stop at a fixed percentage below entry. Key parameter: stop percentage.
   - **Breakeven then trail**: first move stop to breakeven once a profit threshold is reached, then switch to trailing. Key parameters: breakeven trigger percentage, trail percentage.
3. Implement the ratchet invariant: by default, a stop may tighten (move closer to current price on winning trades) but may NEVER loosen (move further away), even if volatility (ATR) expands. This is the core anti-stickiness mechanism.
4. Build stickiness diagnostic metrics that are computed for every backtest run:
   - Median holding period in bars
   - P95 holding period (95th percentile)
   - Percentage of trades held longer than 60 bars (roughly 3 months) and 120 bars (roughly 6 months)
   - Exit trigger rate: how often the exit signal actually fires while in a position (low rate = sticky)
   - Reference chase ratio: inverse of exit trigger rate (high = the exit keeps running away from the price)
   These metrics will be displayed in the TUI's Results panel and used to flag pathological configurations.
5. Write specific anti-stickiness regression tests:
   - A chandelier-style exit must not get trapped by endlessly chasing a rising price — it must allow profitable exits when price reverses
   - A floor-style tightening must tighten on price rises but must not chase ceilings or loosen on price drops
   - The frozen reference exit must never move its stop (by definition)
   - The time decay stop must converge toward the current price over time
6. Write a property test for the ratchet invariant: across any price path, the stop level must be monotonically non-decreasing for long positions and monotonically non-increasing for short positions. The test generator must include price paths with large gaps (5%+ overnight moves). Verify that a gap through the stop does not cause the stop to widen — the position should be exited at the gap-open price (per Phase 7 gap rules), not at the stop level. Verify that volatility expansion after a gap does not loosen the ratchet.
7. Test that PM intents are correctly translated into cancel/replace orders via the order book

## Gate

- All nine position managers work correctly
- Ratchet invariant holds under volatility expansion (property test)
- Anti-stickiness scenarios pass (chandelier doesn't get trapped, floor doesn't chase, frozen never moves, time decay converges)
- Stickiness diagnostics are computed and produce expected values for known pathological vs healthy configurations
- PM-generated orders are atomic cancel/replace operations
- All tests pass

---

# Phase 9 — Strategy Composition (Week 8-9)

## Goal

Build the ten concrete signals, four signal filters, the factory system that wires configuration to runtime objects, the strategy preset system, and the TOML configuration file format. The four-component composition model (signal + PM + execution + filter) must be fully wired. After this phase, any combination of the four component types can be assembled and run.

## Steps

1. Implement the ten MVP signal generators, each implementing the signal trait from Phase 3:
   - **52-week breakout**: price exceeds the N-day high times a threshold. Key parameters: lookback period, entry percentage threshold, max signal age.
   - **Donchian breakout**: classic channel breakout — long when price exceeds the upper channel. Key parameters: entry lookback, exit lookback. **Note:** The "exit when price breaks the lower channel" behavior is implemented as a PM type (channel-based exit), not as part of the signal. Signals must remain portfolio-agnostic and entry-only. The Donchian signal pairs naturally with a Donchian channel exit PM or an ATR trailing stop.
   - **Bollinger breakout**: price exceeds the upper Bollinger band. Key parameters: period, standard deviation multiplier.
   - **Keltner breakout**: price exceeds the upper Keltner channel. Key parameters: EMA period, ATR period, multiplier.
   - **Supertrend**: ATR-based directional flip indicator. Key parameters: period, multiplier.
   - **Parabolic SAR**: Wilder's parabolic stop-and-reverse system. Key parameters: acceleration factor start, step, and max.
   - **MA crossover**: fast moving average crosses above/below slow moving average. Key parameters: fast period, slow period, MA type (simple vs exponential).
   - **Time-series momentum (TSMOM)**: buy when lookback return is positive. Key parameter: lookback period in trading days.
   - **ROC momentum**: rate of change exceeds a threshold. Key parameters: period, entry threshold percentage.
   - **Aroon crossover**: long when Aroon Up crosses above Aroon Down. Key parameter: period.
2. Implement the four signal filters, each implementing the signal filter trait from Phase 3:
   - **No filter**: pass-through baseline (always allows entry). This is the default.
   - **ADX filter**: only allow entry when ADX exceeds a threshold (confirms trending conditions). Key parameters: ADX period, threshold.
   - **MA regime filter**: only allow entry when price is above (or below) a long-term moving average. Key parameters: period, direction (above or below).
   - **Volatility filter**: only allow entry when ATR-based volatility is within a specified range. Key parameters: ATR period, minimum and maximum ATR percentages.
3. Each signal and filter must be thoroughly tested against known data with hand-verified expected outputs. Each signal must be completely portfolio-agnostic — verify this with a test that shows the same signal output regardless of current position state.
4. Build the factory system: a function that takes a component configuration variant and returns a working component object. One factory for each of the four component types (signal, position manager, execution model, signal filter). This is the bridge between serializable configuration and live runtime objects. Every config variant must successfully instantiate — no dead branches. Write a hash stability golden test: define a known configuration, compute its config hash and full hash, and assert they match hardcoded expected values. This test breaks loudly if the serialization format changes. Config hashing requires additive-only schema changes — new fields must have defaults and must be appended, not inserted. Write an exhaustive factory coverage test: iterate every variant of every component enum (signal type, PM type, execution model type, signal filter type), generate a default configuration for each variant, serialize it to TOML, deserialize it back, pass it through the corresponding factory, and verify a valid runtime object is returned. This test guarantees 100% coverage of the config-to-runtime mapping and catches dead branches, missing match arms, and serialization drift.
5. Build the strategy preset system: named compositions that bundle a specific signal, position manager, execution model, and signal filter into a single ready-to-run configuration. Ship at least five presets covering different strategy families (trend following, breakout, mean reversion, momentum, and a buy-and-hold benchmark).
6. Build the random component sampler for YOLO mode: given the pools of available signals (10), position managers (9), execution models (4), and signal filters (4), randomly sample a complete composition. Components should have configurable weights (some components may be sampled more frequently because they are more interesting). The sampler must be seeded for reproducibility. The sampler must also enforce compatibility constraints before emitting a composition. Define a static compatibility rules table: some signal-execution pairings are invalid (e.g., a mean-reversion signal that relies on limit entry should not be paired with a next-bar-open-only execution model, because the limit price logic would be lost), and some PM types may be incompatible with certain signal types (e.g., a Donchian channel exit PM is specifically designed for Donchian breakout signals and produces undefined behavior with unrelated signals). Invalid compositions must be rejected by the sampler and re-sampled, not entered into the leaderboard. The compatibility table is a static configuration maintained alongside the factory system. Write a test: verify that the sampler never produces a composition flagged as invalid by the compatibility rules, even after 10,000 samples. The compatibility rules table also applies to user-configured compositions (TOML config files and TUI Strategy panel selections), not only to the YOLO sampler. When a user manually selects an incompatible pairing, the system issues a warning but allows the run to proceed — users may have legitimate reasons for testing unusual combinations. The warning includes the specific incompatibility reason (e.g., "Limit entry execution model paired with a signal that does not emit limit prices — orders will fall back to market-on-open"). Write a test: load a TOML config with a known-incompatible pairing, verify a warning is emitted, and verify the backtest still runs to completion.
7. Design and implement the TOML configuration file format. A user should be able to define a complete backtest (signal, position manager, execution model, filter, universe, date range, capital, cost model) in a single TOML file without writing any Rust code. The config file must parse into the existing run configuration structure.
8. Build the composition rules: signals produce directional intent, filters gate whether the intent is allowed, execution models determine the order type (breakout strategies naturally pair with stop entries, mean-reversion with limit entries), position sizing determines quantity (fixed percent-of-equity with a configurable percentage parameter on the order intent — see escape hatches), and position managers handle exits via cancel/replace.
9. Support three trading modes: long-only (default), short-only, and long/short.
10. Write tests: every signal produces non-empty output on 252 bars of real SPY data; every filter correctly gates signals; each preset produces actual trades; TOML config files parse correctly; the factory wiring is complete for all four component types; the random sampler produces valid compositions; trading modes work correctly.
11. Ship at least five example TOML config files demonstrating different strategy types.

## Gate

- All ten signals produce verified output on test data
- All four signal filters work correctly
- Signals are demonstrably portfolio-agnostic
- Factory system converts every config variant (all four component types) into a working runtime object
- Random component sampler produces valid, seeded, reproducible compositions that respect compatibility constraints
- At least five strategy presets produce real trades on real data
- TOML config files parse and run successfully
- All tests pass

---

# Phase 10a — Runner + Metrics + CLI (Week 10)

## Goal

Build the runner that orchestrates complete backtests, the full performance metrics system, and the CLI run command. This is the merge point where real data meets real strategies. After this sub-phase, a single backtest runs on real data and produces correct metrics.

**Capital assumption:** Each symbol backtest uses the same fixed initial capital (default: $100,000). Cross-symbol results are per-symbol, not portfolio-level. Portfolio-level capital allocation (risk parity, equal weight, Kelly) is deferred.

## Steps

1. Build the runner's real data loading path: load bars from the Parquet cache, not from synthetic generation. Use Polars lazy scanning with predicate pushdown. Support a `--offline` flag that disables all network access: if cache is missing and `--offline` is set, fail immediately with a clear error ("symbol XXX not in cache; run without --offline to fetch"). This enables strict deterministic reruns with no side effects. The `--offline` and `--synthetic` flags are mutually exclusive. If cache is missing and `--offline` is not set, attempt to download; if that fails and `--synthetic` flag is set, use synthetic with an explicit visible warning. Without `--synthetic`, fail with a clear error. The runner must NEVER silently use fake data.
2. Build trade extraction from the engine: the runner must collect actual trade records from the event loop, including entry/exit times, prices, quantities, PnL, commissions, slippage, and signal trace fields (what signal triggered this trade, what order type was used, what fill conditions occurred). The runner must never return an empty trade list when the strategy actually traded.
3. Design and implement the full performance metrics system. Research the standard definitions and implement from first principles with proper edge case handling:
   - Total return, CAGR, Sharpe ratio, Sortino ratio, Calmar ratio
   - Max drawdown, win rate, profit factor (capped for edge cases)
   - Number of trades, turnover (annual traded notional / average capital)
   - Max consecutive wins and losses, average losing streak length
   - Do not copy formulas — look up canonical definitions and understand the math before implementing. Handle zero-trade, all-winning, and zero-variance edge cases gracefully.
   - **Implementation note:** Performance metrics are pure functions (return series in, scalar out). They have no dependencies on the runner, the data pipeline, or the event loop. They can be developed and unit-tested independently at any point during development, even before Phase 10a. They are placed here because this is where they are first wired up to real backtest output, but the implementation and test work can be pulled forward to earlier phases if convenient.
4. Build the fitness function system: a configurable selector for which metric to optimize/sort by, defaulting to Sharpe ratio.
5. Build the CLI run command: accepts either a TOML config file path or a named preset plus a symbol universe. Runs the backtest, saves the result as JSON to a results directory, and prints a summary to the terminal.
6. Write integration tests: run a real backtest on SPY with a real strategy, verify trades are extracted, verify all metrics are computed and non-NaN, verify the result file is written.
7. Run each of the five strategy presets on real SPY data and verify they all produce meaningful results with distinct characteristics.

## Gate

- A single backtest runs on real data from the Parquet cache and produces correct, non-NaN metrics
- Trade extraction produces real trades with signal trace information
- All performance metrics are correctly implemented with proper edge case handling
- The CLI `run` command accepts a TOML config and produces a result file
- At least five presets produce meaningful results on real data
- All tests pass

---

# Phase 10b — YOLO Engine + Per-Symbol Leaderboard (Week 11)

## Goal

Build YOLO mode (the continuous auto-discovery engine) with dual-slider randomization, per-symbol leaderboard, error recovery, and progress reporting.

## Steps

1. Build YOLO mode — the continuous auto-optimization engine that runs indefinitely, testing strategy configurations across all selected symbols and maintaining a live leaderboard of discoveries. Each YOLO iteration: selects strategies (based on the dual sliders), runs backtests for each selected symbol, computes metrics, updates the leaderboard, and repeats until stopped.
2. Build the dual randomization control system:
   - **Parameter jitter slider** (0-100%): controls how much to randomize parameters within known strategy structures. At 0%, default parameters are used. At 100%, full random within each parameter's valid range.
   - **Structural exploration slider** (0-100%): controls the probability of sampling random component combinations (signal + PM + execution + filter) vs testing traditional fixed strategies. The slider should follow a non-linear schedule — low values mostly do parameter variation with rare structural swaps, middle values balance exploration and exploitation, high values aggressively explore novel component combinations.
3. Build the full YOLO configuration system with all user-adjustable settings:
   - Start date and end date for the backtest period
   - Walk-forward Sharpe threshold: minimum average Sharpe before walk-forward validation is triggered
   - Sweep depth: parameter grid density (quick, normal, deep)
   - Warmup iterations: number of iterations of pure exploration before winner exploitation begins
   - Combo mode: none, 2-way, 2+3-way, all (deferred to later — stub for now but include the setting)
   - Polars thread cap: limit threads per individual backtest
   - Outer thread cap: limit threads for symbol-level parallelism (Rayon)
   - **Threading mutual exclusion rule:** if `outer_thread_cap > 1` (Rayon parallelism across symbols), force `polars_thread_cap = 1`. Polars internal parallelism is only useful when running a single backtest in isolation. When YOLO mode runs multiple symbols in parallel via Rayon, nested Polars threading causes CPU oversubscription, excessive context switching, and worse throughput than sequential execution. The YOLO configuration UI must enforce this constraint: adjusting one cap automatically clamps the other. Document this rule in the configuration reference (Phase 14). Write a test: verify that setting `outer_thread_cap = 4` automatically sets `polars_thread_cap = 1`.
   - Max iterations: hard stop for the YOLO run (default: unlimited)
4. Build YOLO error recovery: each YOLO iteration must run inside an error boundary (`catch_unwind` or Result-based). Failed iterations are: (a) logged with full context (the composition that failed, the symbol, the error message), (b) skipped and not entered into the leaderboard, (c) counted separately. The YOLO progress display must show both success count and error count. NaN/Inf values in metrics must be filtered before leaderboard insertion. A single bad iteration must never crash the YOLO loop or corrupt the leaderboard.
5. Build progress reporting for YOLO mode: current iteration number, current symbol being processed, percentage complete for multi-symbol runs, estimated throughput (iterations per minute), success/error counts. This will be displayed in the TUI's Sweep panel.
6. Build the per-symbol leaderboard: maintains the top N strategies per symbol, ranked by the selected fitness metric. Features: deduplication by config hash (same structure + config + symbol replaces only if new run has a better score), bounded size (default 500 entries), discovery metadata (iteration number, session ID, timestamp).
7. Write a YOLO determinism regression test: run the same YOLO sweep with a fixed seed, fixed universe, and fixed iteration count. Run it twice: once with 1 thread and once with 8 threads. Assert identical config hashes, identical metric values, and identical leaderboard ordering. If this test fails, the RNG hierarchy from Phase 3 is broken.
8. Write integration tests: verify YOLO mode runs for at least 100 iterations and populates the per-symbol leaderboard with distinct entries; verify failed iterations are caught and logged without crashing the loop; verify dual sliders produce observably different exploration behavior at different settings.

## Gate

- YOLO mode runs for 100+ iterations with error handling (failed iterations logged, not crashed)
- Dual sliders produce observably different exploration behavior
- Per-symbol leaderboard populates with distinct, deduplicated entries
- YOLO determinism test passes (same seed → same results across thread counts)
- All tests pass

---

# Phase 10c — Cross-Symbol Leaderboard + History (Week 12)

## Goal

Build the cross-symbol leaderboard with aggregation, the dual-scope system, risk profiles, run fingerprinting, and the JSONL history system.

## Steps

1. Build the cross-symbol leaderboard: aggregates results across all tested symbols for each strategy configuration. For each config, compute: average and min/max Sharpe across symbols, geometric mean CAGR (rewards consistency over outlier performance), hit rate (fraction of symbols where the strategy was profitable), worst max drawdown, average number of trades, and tail risk metrics (CVaR 95%, skewness, kurtosis, downside deviation ratio). **Geometric mean CAGR edge cases:** when any symbol has a negative total return, use log-return-based aggregation. Add a trimmed geometric mean option that excludes the best and worst performing symbols. Add a guardrail: if any symbol produces a CAGR below a configurable catastrophic threshold (e.g., -50%), flag the strategy regardless of aggregate score. **Tail metrics minimum sample:** CVaR 95%, skewness, kurtosis, and downside deviation ratio require a minimum of 252 return observations. Below this threshold, mark as "insufficient" and exclude from composite ranking. **Incremental aggregation:** Cross-symbol aggregate statistics (geometric mean CAGR, CVaR, skewness, kurtosis) must be updated incrementally as new per-symbol results arrive, not recomputed from scratch across the full leaderboard on every update. This prevents TUI lag during active YOLO sessions with large leaderboards.
2. Build the dual-scope system: session leaderboard (resets on app restart) and all-time leaderboard (persisted to disk, accumulates across sessions). Each session gets a unique ID for provenance tracking. The user toggles between scopes.
3. Build the risk profile system for ranking: four named profiles that weight metrics differently when computing a composite ranking score:
    - **Balanced**: equal weight across all metrics (default exploration)
    - **Conservative**: emphasizes tail risk, drawdown, and consistency
    - **Aggressive**: emphasizes returns, Sharpe, and hit rate
    - **TrendOptions**: emphasizes hit rate, consecutive losses, and out-of-sample Sharpe (for options traders using trend signals)
    The user cycles between risk profiles and the leaderboard re-ranks accordingly.
    **Normalization policy:** Before applying profile weights, all metrics must be normalized to a common scale. Use rank-based normalization within the current leaderboard population: for each metric, replace raw values with their percentile rank (0.0 to 1.0) within the set of entries being ranked. This ensures that metrics with different units and scales (Sharpe of 2.0 vs CAGR of 50% vs max drawdown of -15%) contribute proportionally to the composite score. Rank-based normalization is preferred over min-max or z-score because it is robust to outliers and does not assume a distribution shape. **Tradeoff note:** Rank normalization destroys magnitude information — a Sharpe 8.0 strategy appears only incrementally better than a Sharpe 2.0 strategy if they are adjacent in rank. This is intentional for composite scoring (outlier robustness), but users who care about absolute magnitude should use the raw metric sort (Step 4), which preserves full magnitude and is the escape hatch for this tradeoff. Write a test: verify that the composite ranking is invariant under linear rescaling of any single metric.
4. Build the ranking metric selector: the cross-symbol leaderboard can be ranked by average Sharpe, minimum Sharpe (conservative), geometric mean CAGR, hit rate, mean out-of-sample Sharpe (anti-overfit), or composite score for the active risk profile.
5. Build the run fingerprinting system: every YOLO run produces a fingerprint capturing identity (unique run ID, timestamp, seed), configuration (symbol, date range, jitter percentage), component breakdown (signal type + params, PM type + params, execution model + params, filter type + params), derived hashes (config hash for structural grouping, full hash for exact deduplication), and results (full metrics, stickiness diagnostics, trade count).
6. Build the YOLO history system: persist fingerprints as JSONL (one JSON object per line) for efficient append-only storage. Apply a write filter before persisting: only append runs that meet minimum criteria (at least 5 trades AND either positive CAGR or Sharpe > -1.0). Runs that fail to meet these criteria are kept in transient session memory for the current session's leaderboard but are not written to the JSONL file. This prevents overnight YOLO runs (50,000+ iterations) from producing multi-gigabyte history files dominated by junk configurations. The write filter criteria are configurable. Show the YOLO history file size in the progress display so the user can monitor file growth. Index by config hash, signal type, PM type, and any component type. Provide statistical summaries: mean/median/P25/P75 Sharpe and win rate per component type, top N runs by any metric, and most robust structural combinations (highest win rate with minimum sample size). This enables meta-analysis like "which PM contributes most to performance across all tested configurations."
7. Write integration tests: verify the leaderboard entry is reproducible from its manifest; verify cross-symbol aggregation is correct; verify risk profiles re-rank the leaderboard; verify JSONL history persists and is queryable.
8. Run each of the five strategy presets on real multi-symbol data (e.g., SPY + QQQ + IWM) and verify they all produce meaningful cross-symbol results.

## Gate

- **Checkpoint B passes**: real multi-symbol backtests work, leaderboard shows real metrics, CLI produces result files, results are reproducible
- Cross-symbol leaderboard aggregates correctly with proper statistical summaries and edge case handling
- Risk profiles re-rank the leaderboard according to their weighting
- Run fingerprinting and JSONL history persist and are queryable
- At least five presets produce meaningful results on real multi-symbol data
- All tests pass

---

# Phase 11 — Robustness + Statistical Validation (Weeks 13–14)

## Goal

Build the promotion ladder that stress-tests strategy candidates, the walk-forward validation system with multiple-comparison correction, and the block bootstrap confidence grading system. These are the layers that separate "got lucky once" from "reliably works."

## Steps

### Promotion ladder

1. Build the promotion ladder framework with levels that increase in computational cost. A strategy must pass each level before advancing to the next.
2. Implement Level 1 (Cheap Pass): deterministic intrabar policy with fixed slippage. This is the fast screening pass.
3. Implement Level 2 (Walk-Forward): train/test split with out-of-sample evaluation. The strategy must perform acceptably on data it wasn't optimized on. Split data into multiple time folds, train on in-sample, test on out-of-sample. Compute the degradation ratio: mean OOS Sharpe divided by mean in-sample Sharpe (close to 1.0 means good generalization, much less than 1.0 means overfit). **Minimum data guardrails:** walk-forward requires at least 756 bars (3 years) of data, with a minimum of 252 bars per in-sample fold and 63 bars (one quarter) per out-of-sample fold. If data is insufficient, skip the walk-forward level with a "not enough data for validation" flag — do not produce meaningless fold statistics on short data. **Degradation ratio edge cases:** the ratio is unstable when IS Sharpe is near zero and undefined when IS Sharpe is exactly zero. If IS Sharpe < 0.1, use a difference-based metric (OOS Sharpe - IS Sharpe) instead. For negative IS Sharpe, flag as "negative in-sample" and skip the ratio. If IS Sharpe is positive (>= 0.1) but OOS Sharpe is negative, clamp the degradation ratio to 0.0 and flag as "failed OOS" — a strategy that profits in-sample but loses money out-of-sample is the canonical overfit signature, and a mathematically valid but negative ratio would be misleading in ranking contexts.
4. Apply FDR correction (Benjamini-Hochberg) across all configs that undergo walk-forward testing. **Family definition:** The set of configurations subject to a single FDR correction is all configs tested within one YOLO session on the same universe, date range, and execution preset. Different universes, date ranges, or execution presets constitute separate FDR families — this prevents a conservative universe (3 blue-chip symbols) from being penalized by the multiple-comparison burden of a broad universe (500 symbols) tested in a different session. When YOLO mode tests hundreds of configurations, some will pass walk-forward by chance. FDR correction adjusts p-values to account for this multiple-comparison problem. **Null hypothesis specification:** the null hypothesis is "mean OOS Sharpe equals zero." The test statistic is the t-statistic computed from the K fold-level OOS Sharpe values (where K is the number of walk-forward folds). The p-value comes from a one-sided t-test (H1: mean OOS Sharpe > 0). BH correction is applied to these p-values across all configurations tested. Document these choices in a comment block at the top of the FDR implementation. Note: FDR correction operates on the raw fold-level OOS Sharpe values via the t-test described above. It does NOT use the degradation ratio from Step 3. The degradation ratio is a diagnostic metric for human interpretation; the FDR pipeline is the statistical hypothesis test. These are independent systems that happen to consume the same walk-forward fold data. **Statistical caveat:** The t-test on K fold-level OOS Sharpe values is a heuristic ranking tool, not a rigorous hypothesis test. The assumptions of normality, independence, and sufficient sample size are unlikely to hold (Sharpe estimates are noisy and non-normal, folds share boundary effects, and K is typically small). The resulting p-values should be treated as ranking scores for the BH procedure, not as literal false-positive probabilities. To validate that the FDR pipeline behaves reasonably, add a null-simulation sanity check during development: generate 100 random strategies with no predictive power, run them through walk-forward + FDR, and verify that the false discovery rate is controlled at the target level.
5. Implement Level 3 (Execution Monte Carlo): vary slippage, spread, and commission parameters across multiple runs. Sample from distributions, not just run the baseline N times. Each MC run must produce genuinely different outcomes — if all runs produce identical results, the implementation is broken.
6. Design and implement a stability scoring system that considers both the median performance and the variance across Monte Carlo runs. A strategy with slightly lower median but much lower variance should rank above a strategy with higher median but wild swings. Use proper statistical methods — research them and implement from first principles.
7. Store full distribution data (median, IQR, tails) for each robustness level, not just best-case point estimates.
8. Stub the later levels (Path MC, Bootstrap Regime) with clear placeholder markers but do not implement them yet.

### Block bootstrap confidence grades

9. Build the block bootstrap confidence grading system for individual leaderboard entries:
   - Compute daily returns from the equity curve
   - Run stationary block bootstrap (preserving autocorrelation structure — financial returns are NOT independent, so IID bootstrap is wrong). Use a conservative default mean block length of 20 trading days (approximately one month) with a configuration override. If automatic block length selection (Politis-White method) is too complex for the first implementation, defer it to Phase 14 hardening. Document the sensitivity of confidence intervals to this parameter.
   - Build a confidence interval for the annualized Sharpe ratio
   - Assign a grade: High (CI lower bound is strongly positive, CI width is narrow), Medium (CI lower bound is positive, CI width is moderate), Low (CI is wide or lower bound is near zero), Insufficient (too few data points to grade)
   - The specific thresholds should be calibrated during implementation — research what makes sense for daily equity return series
10. Build cross-symbol bootstrap confidence at two levels:
    - **Primary (portfolio-level):** Construct a synthetic portfolio equity curve by equally weighting the per-symbol equity curves. When symbols have different start dates, truncate all equity curves to the common date range (the intersection of all symbols' date ranges). Require a minimum of 252 bars of overlap; if fewer bars overlap, exclude the short-history symbol from the portfolio-level bootstrap and log a warning. Bootstrap daily returns of this portfolio-level equity curve using the same stationary block bootstrap from Step 9. This captures co-movement between symbols (which inflates confidence if ignored). The portfolio-level CI is the primary confidence grade for cross-symbol entries.
    - **Secondary (per-symbol diagnostic):** Bootstrap the mean per-symbol Sharpe with guardrails (minimum number of symbols required, worst symbol Sharpe must meet a minimum, minimum hit rate). This per-symbol bootstrap is a diagnostic that identifies whether performance is concentrated in a few symbols or broadly distributed.
    Display both grades in the leaderboard drill-down. The primary (portfolio-level) grade is shown in the leaderboard row.

### Stickiness integration

11. Integrate the stickiness diagnostics from Phase 8 into leaderboard entries so they are visible in the Results panel. Flag entries with pathological stickiness (very high median hold bars, very low exit trigger rate) with a visual warning.

### Tests

12. Write tests: failing Level 1 prevents advancement to Level 2 (no budget wasted); walk-forward degradation ratio detects overfit configs; FDR correction reduces false positive rate; Execution MC runs produce non-identical results (actual parameter variation); stability scoring correctly penalizes high-variance strategies; block bootstrap produces grades that correctly distinguish high-confidence from low-confidence results; stickiness flags fire for known pathological PM configurations.
13. Write a golden test for the walk-forward system: run walk-forward on a known strategy with known data and known fold parameters, and assert the exact degradation ratio, fold-level Sharpe values, and FDR-adjusted p-values. This locks the behavior and prevents silent changes during refactoring.

## Gate

- Three-level promotion ladder works end-to-end
- Walk-forward validation detects overfit vs generalizing strategies
- FDR correction is applied across multiple comparisons
- Execution MC produces genuine variance (not N identical runs)
- Stability scoring penalizes variance appropriately
- Block bootstrap produces meaningful confidence grades
- Stickiness diagnostics are integrated and visible
- All tests pass

---

# Phase 12 — TUI (Weeks 15-17)

## Goal

Build the terminal UI with the six-panel layout, vim-style keyboard navigation, background worker thread, and real-time YOLO progress. The TUI must feel like the v2 app: fast, keyboard-driven, panels accessible by number keys, consistent j/k/h/l navigation everywhere. It shows real data by default and never pretends to have results when it doesn't.

## Steps

### Architecture: worker thread and channel system

1. Build the background worker thread architecture. All heavy computation (data fetching, backtests, YOLO iterations) must happen on a background thread to keep the UI responsive. Communication uses channels: the TUI sends commands to the worker (fetch data, start YOLO, stop YOLO, run single backtest) and the worker sends results back (data loaded, leaderboard update, progress update, error). The worker creates a private `rayon::ThreadPool` (via `rayon::ThreadPoolBuilder::new().num_threads(outer_thread_cap).build()`) for symbol-level parallelism, leaving the global Rayon pool free for the UI thread. Using the global static Rayon pool would cause TUI rendering stutter during heavy YOLO sweeps because the engine and the UI would compete for the same thread pool. Polars threading for per-backtest parallelism uses its own internal pool (controlled by `polars_thread_cap` from Phase 10b Step 3). Write a test: verify that the worker's Rayon pool is distinct from `rayon::current_num_threads()` on the main thread. An atomic cancellation flag enables responsive stopping when the user presses Escape.

### Panel layout and navigation

2. Build the six-panel layout. Each panel is accessible by number key (1-6) or Tab/Shift+Tab to cycle. The panel structure is:
   - **Panel 1 — Data**: sector/ticker hierarchy, data fetching, cache status
   - **Panel 2 — Strategy**: four-component composition selection, parameter tuning
   - **Panel 3 — Sweep**: YOLO mode configuration and launch
   - **Panel 4 — Results**: leaderboard display with rankings and metrics
   - **Panel 5 — Chart**: equity curve visualization
   - **Panel 6 — Help**: keyboard shortcuts and feature documentation
3. Build the vim-style navigation system that is consistent across ALL panels: j/k for up/down movement, h/l for collapse/expand in trees or decrease/increase for numeric values, Space for toggle, Enter for confirm/drill-in, Escape for back/cancel. This consistency is non-negotiable — every panel must respond to these keys in a predictable way.

### Startup flow

4. Build app state persistence: on exit (and on significant configuration changes), serialize to a local JSON or TOML file: selected tickers, last YOLO configuration (all slider values, date range, thread caps, etc.), active risk profile, active panel, session/all-time toggle state, and welcome overlay dismissed flag. On startup, load this file before rendering the UI. If the file is missing or corrupted, use defaults silently. This prevents the user from losing configuration on every restart.
5. Build the startup sequence: on launch, load the app state persistence file from Step 4 (if it exists), scan the local Parquet cache directory for existing data, load the universe configuration (sector/ticker hierarchy), check which tickers have cached data and mark them. If this is a fresh install with no cache, the Data panel shows all tickers as unfetched and a one-time welcome overlay is displayed: "Press 1 for Data, select tickers with Space, press f to fetch. Press 3 for YOLO mode once you have data." The overlay is dismissed with any key and the dismissal is saved via the persistence system from Step 4. The TUI does NOT show fake data, fabricated results, or sample numbers at startup. If there are no results, the Results panel shows an empty state with instructions.

### Theme and visual feedback

6. Build the theme system using semantic tokens: background (near-black), accent (electric cyan), positive (neon green), negative (hot pink), warning (neon orange), neutral (cool purple), muted text (steel blue). All widgets reference semantic tokens, not hardcoded colors. The active panel's border uses the accent color (electric cyan). Inactive panels use the muted color (steel blue). This provides an unambiguous visual indicator of which panel owns keyboard input.
7. Build a persistent error display mechanism: a status bar at the bottom of the screen that shows the last error message (with a key to dismiss). The worker channel must include an explicit error message type alongside success messages. Errors from data fetching (network failures, rate limiting, format changes) and from backtests (NaN metrics, zero-trade results, panics) must be surfaced to the user, not silently swallowed. In addition to the status bar (which shows only the last error), build an error history overlay accessible from the Help panel (e.g., pressing `e` from Panel 6). The overlay displays the last 50 errors/warnings with timestamps, error category (network, data, engine, NaN metrics), and the context (which symbol, which operation). The overlay scrolls with j/k and dismisses with Escape. This provides a post-mortem trail for long YOLO runs where transient errors would otherwise be lost.

### Panel 1 — Data

8. Build the Data panel with a two-level tree view: sectors at the top level (Technology, Healthcare, Finance, Energy, etc.), tickers nested under each sector. Navigate sectors with j/k, expand/collapse with h/l or Enter. Toggle individual tickers with Space. Select-all with a key, deselect-all with another key. Show a green indicator dot next to each ticker that has cached data.
9. Build the fetch command: when the user presses a key (e.g., f), send a fetch command to the worker for all selected tickers. Show per-symbol progress during the fetch: which symbol is currently downloading, how many are complete out of the total (e.g., "Fetching AAPL... [23/47]"), and a summary when done. The Escape key cancels an in-progress data fetch — the atomic cancellation flag from the worker architecture (Step 1) applies to fetches as well as YOLO runs. Already-downloaded symbols are preserved in cache. The UI must remain responsive during fetching.
10. Build symbol search: a key (e.g., s) opens a search input where the user can type any Yahoo Finance symbol not in the default universe and add it to the selection.

### Panel 2 — Strategy

11. Build the Strategy panel: display the four-component composition (signal generator, position manager, execution model, signal filter). The user navigates between components with j/k and changes the selected type for each component with h/l. When a component type is selected, show its tunable parameters below, adjustable with h/l to decrease/increase values. Parameter ranges should have sensible bounds matching the v2 app's ranges (e.g., ATR trailing stop multiplier 2.0-5.0, Donchian lookback 20-100). If the currently selected component combination is flagged as incompatible by the compatibility rules table (Phase 9 Step 6), display a warning indicator (neon orange border or icon) next to the conflicting components. The warning is informational only — the user may still launch the backtest.
12. Support the three trading modes (long-only, short-only, long/short) as a setting on this panel.

### Panel 3 — Sweep

13. Build the Sweep panel (YOLO configuration). Display all YOLO settings from Phase 10b: the two primary sliders (parameter jitter and structural exploration, adjustable with h/l), start date, end date, walk-forward Sharpe threshold, sweep depth, warmup iterations, combo mode, Polars thread cap, outer thread cap, and max iterations. Each setting is navigable with j/k and adjustable with h/l or direct input.
14. Build the YOLO launch: Enter starts YOLO mode. While running, show the iteration count, current symbol, throughput (iterations per minute), success/error counts, and a rolling status line. Escape stops the run. Cancellation is cooperative: immediately stop scheduling new iterations, show "Stopping..." status, complete the current iteration, then stop. Partial results from completed iterations are preserved.

### Panel 4 — Results

15. Build the Results panel as the leaderboard display. Show the ranked list of discovered strategy configurations with key metrics per row (strategy name, symbol/sector, Sharpe, CAGR, max drawdown, win rate, profit factor, number of trades). Navigate rows with j/k.
16. Build the session/all-time toggle (e.g., t key): switch between showing only results from the current session vs all accumulated results.
17. Build the risk profile cycling (e.g., p key): cycle through Balanced, Conservative, Aggressive, and TrendOptions profiles. The leaderboard re-ranks when the profile changes.
18. Build leaderboard drill-down: Enter on a row shows full detail — complete metrics, stickiness diagnostics, confidence grade, walk-forward results, and the full component composition. Show enough that the user can understand "why did this win?"

### Panel 5 — Chart

**Scope risk note (Steps 19-20):** The Chart panel is the highest scope risk in Phase 12. Terminal-based charting with axes, scaling, and multi-terminal-size support is significantly more work than other panels. Define two tiers:
- **MVP chart (required for gate):** A single equity curve rendered as a simple line chart using ratatui's built-in drawing primitives. Single entry only. No overlays, no side-by-side, no ghost curves.
- **Full chart (stretch goal, not required for gate):** Multiple view modes (equity, drawdown overlay, side-by-side comparison) and ghost curve overlay with execution drag percentage.
Do not block the Phase 12 gate on the full chart. The MVP chart satisfies the "Chart renders equity curves" checkpoint. Ghost curves and comparison views can be added in Phase 14 hardening if time permits.

19. Build the Chart panel MVP: render an equity curve as a line chart using ratatui drawing primitives. The chart displays the equity curve for the currently selected leaderboard entry.
20. (Stretch) Build the full chart features: multiple view modes (equity curve, drawdown overlay, comparison of multiple entries side-by-side) and ghost curve overlay displaying both "ideal equity" (zero-friction fills) and "real equity" (actual fills) with execution drag as a percentage.

### Panel 6 — Help

21. Build the Help panel: display all keyboard shortcuts organized by panel, explain the YOLO sliders, describe the risk profiles, and provide a quick reference for the most common workflows.

### TUI-triggered single backtests

22. Build single-backtest mode: from the Strategy panel, the user can launch a single backtest with the currently configured composition and selected tickers. The worker runs it in the background, the TUI shows a progress indicator (current bar / total bars or percentage — the worker sends periodic progress messages, not just the final result), and the result appears in the leaderboard when complete.

### Tests

23. Write tests: TUI with no results shows the empty state (not fake data); TUI with real results shows correct metrics in the leaderboard; panel switching with number keys works; j/k navigation works in all panels; session/all-time toggle changes displayed results; risk profile cycling re-ranks the leaderboard; active panel border is visually distinct; errors from the worker channel are displayed in the status bar; app state persists across restart (save → exit → relaunch → verify restored).

## Gate

- **Checkpoint C passes**: TUI opens to six-panel layout, vim navigation works everywhere, Data panel shows sector tree with cache status, YOLO runs from TUI with progress, Results shows real ranked results, Chart panel renders equity curves (MVP line chart minimum)
- No fake data shown by default
- Number keys 1-6 switch panels, Tab/Shift+Tab cycles
- j/k/h/l navigation is consistent across all panels
- Active panel is visually distinct (accent-colored border)
- Worker thread keeps UI responsive during computation
- Errors are displayed in the status bar, not silently swallowed
- App state persists across restarts (selected tickers, YOLO config, risk profile, active panel)
- Escape cancels both data fetches and YOLO runs with cooperative semantics
- All tests pass

---

# Phase 13 — Reporting + Exports (Week 18)

## Goal

Build the reporting system that produces exportable artifacts from backtest results.

## Steps

1. Define the per-run artifact set: manifest, equity curve, trade list, and diagnostics, all persisted as JSON. All persisted artifacts (JSON results, JSONL history, leaderboard state) must include a schema version field. On load, reject unknown versions with a clear error message rather than silently misreading them. Migration support is deferred, but the version field must exist from day one.
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

# Phase 14 — Hardening + Docs + Quick Start (Weeks 19–20)

## Goal

Lock down performance, add regression protection with real data, write user documentation, and ship example configurations.

## Steps

1. Add Criterion benchmarks for the hot paths: bar event loop, order book operations, execution fill simulation, indicator precompute, and the sequential position state machine (which is the known bottleneck). Include a thread contention benchmark: run YOLO on 50 symbols with default thread settings and with constrained settings (outer_threads = cores/2, polars_threads = 2). Verify that constrained settings produce equal or better throughput. Set a recommendation in the docs.
2. Build the real-data golden test: use the frozen SPY fixture from Phase 4, run a known strategy with known parameters, and assert the exact equity curve and trade list. This test must break if anyone changes the engine's behavior.
3. Verify all existing golden tests (synthetic) still pass
4. Add property tests for all remaining invariants: no double fills, OCO consistency, equity accounting, ratchet monotonicity
5. Write the Quick Start guide: a document that takes a new user from clone to first real YOLO run in under 5 minutes (clone, build, launch TUI, select tickers, fetch data, configure YOLO, hit go). The guide must specify a tiny default universe for the first-run experience: 3–5 liquid symbols (e.g., SPY, QQQ, AAPL) with a short date range (2 years). This keeps the initial data download under 30 seconds on a typical connection. Alternatively, ship a pre-packaged fixture file (~50KB) containing 2 years of these symbols so the first YOLO run can start immediately without any network access. The fixture is clearly labeled as "Quick Start sample data" and the guide directs users to fetch their full universe afterward.
6. Write the extension guide: how to add new signals, position managers, execution models, and signal filters
7. Write the configuration reference: all TOML options documented, all YOLO settings explained
8. Ship five or more example TOML configuration files demonstrating different strategy types (52-week breakout, Donchian, MA crossover, momentum, mean reversion RSI, buy-and-hold benchmark)
9. Add `trendlab cache status` CLI command (reports total cache size, symbol count, date ranges) and `trendlab cache clean --unused-days N` (removes symbols not accessed recently)
10. Run the full test suite one final time and fix any remaining issues

## Gate

- Benchmarks exist for all hot paths including the sequential bottleneck
- Real-data golden test passes with exact expected values
- Quick start guide works from scratch (clone to YOLO run)
- Example configs all produce valid results
- Full test suite passes
- All Definition of Done criteria are met

---

## Global Definition of Done

You are "v3 done" when all of the following are true:

1. **Real data flows.** A user can download real market data from Yahoo Finance with a single CLI command or from within the TUI's Data panel.
2. **Real strategies run.** The system ships with five or more working strategy presets that produce real trades on real data, plus ten signals, nine PMs, four execution models, and four filters that can be freely composed.
3. **YOLO mode works.** The continuous auto-discovery engine runs from within the TUI, discovers strategy configurations across a multi-symbol universe, and populates the leaderboard with real results. Parameter jitter and structural exploration sliders produce observably different behavior.
4. **Reproducible.** Any leaderboard row can be reproduced from its run fingerprint (configuration + seed + dataset hash).
5. **Explicit execution.** Execution assumptions are explicit (named presets with specific parameters) and sensitivity is visible via side-by-side comparison.
6. **Isolatable.** Signal vs PM vs execution vs filter effects are isolatable via the component architecture and cross-symbol leaderboard.
7. **Robust.** Walk-forward validation with FDR correction separates overfit from generalizing strategies. Execution MC produces real variance. Block bootstrap confidence grades indicate statistical reliability.
8. **Explainable.** The TUI shows full composition details, stickiness diagnostics, confidence grades, and walk-forward results for every leaderboard entry. The Chart panel shows ghost curves with execution drag.
9. **No fake data by default.** The TUI shows real results when they exist and an honest empty state when they don't.
10. **Feels like a real app.** Six-panel layout with number key switching, vim-style j/k/h/l navigation everywhere, responsive UI with background worker, progress bars during data fetch and YOLO runs, risk profile cycling, session/all-time toggle. The keyboard-driven experience matches or exceeds the v2 app.
11. **CLI works.** The CLI supports download and run commands (sweep and report are later additions).
12. **Approachable.** A new user can go from clone to first real YOLO run in under 5 minutes by following the Quick Start guide.
13. **Settings persist.** YOLO configuration, universe selection, active risk profile, and active panel survive a TUI restart. A user who configures a complex sweep does not lose their settings on exit.
14. **Errors are visible.** Network failures, data format changes, NaN metrics, and zero-trade backtests are displayed to the user with actionable context in the TUI status bar, never silently swallowed.
