# TrendLab: Technical Write-Up

## What TrendLab Is

TrendLab is a research-grade trend-following backtest laboratory written in Rust with Polars as the data engine. It runs as a terminal UI (ratatui) with a panel-based interface and vim-style keyboard navigation. The app is designed for one thing: discovering which trend-following strategy configurations work across many symbols and time periods, and validating that they're not overfit.

It is **not** a live-trading system. It favors correctness, reproducibility, and speed of experimentation over latency or execution.

---

## The UI: Panel-Based Terminal Interface

The TUI is organized into six panels, accessible via number keys `1`-`6` or `Tab`/`Shift+Tab`:

| Panel | Key | Purpose |
|-------|-----|---------|
| **Data** | `1` | Sector/ticker hierarchy, data fetching |
| **Strategy** | `2` | Strategy type selection and parameter tuning |
| **Sweep** | `3` | Config panel for YOLO mode (parameter grids, exploration settings) |
| **Results** | `4` | Leaderboard with ranked strategies |
| **Chart** | `5` | Equity curve visualization (ASCII in terminal) |
| **Help** | `6` / `?` | In-app keyboard shortcuts and feature docs |

Every panel uses vim-style navigation: `j`/`k` for up/down, `h`/`l` for collapse/expand or decrease/increase values, `Space` for toggle, `Enter` to confirm. This is consistent across all panels and makes the app feel fast to navigate.

### Startup Flow

On launch, TrendLab:
1. Scans the local `data/parquet/` directory for cached market data
2. Loads the universe from `configs/universe.toml` (a sector-organized list of ~500 S&P 500 tickers)
3. Auto-selects all tickers
4. Randomizes initial strategy parameters (using a non-repeatable seed by default)
5. Lands on the Config/Sweep panel, ready for YOLO mode

The randomization at startup is intentional - it prevents anchoring on "default" parameters and encourages exploration from the first launch. You can disable it with `TRENDLAB_RANDOM_DEFAULTS=0` or pin a seed with `TRENDLAB_RANDOM_SEED=12345`.

---

## Data Panel: Fetching and Caching

The Data panel (`1`) presents tickers organized by GICS sector (Technology, Healthcare, Finance, Energy, etc.) in a two-level tree:

- **Sector view**: Navigate sectors with `j`/`k`, expand with `l`/`Enter`
- **Ticker view**: Toggle individual tickers with `Space`, select all with `a`, deselect with `n`
- **Search**: Press `s` to search any Yahoo Finance symbol not in the default universe

### Data Pipeline

When you press `f` (fetch), the app sends a `FetchData` command to a background worker thread. The worker:

1. Constructs a Yahoo Finance chart URL for each symbol
2. Fetches daily OHLCV JSON via async HTTP (`ureq`)
3. Parses the JSON response, extracting timestamps, open, high, low, close, adjusted close, and volume
4. Converts to `Vec<Bar>` structs with proper timezone handling
5. Writes partitioned Parquet files to `data/parquet/daily/symbol=AAPL/year=2024/data.parquet`

The Parquet layout uses Hive-style partitioning (`symbol=` / `year=`), which enables Polars predicate pushdown - when scanning for SPY 2020-2025, Polars can skip all other symbol directories and year partitions entirely without reading them.

### Caching Rules

- Data is cached locally; re-fetches only happen with `--force` or if the cache is missing
- Raw provider responses are stored in `data/raw/`
- Normalized Parquet in `data/parquet/`
- The entire `data/` directory is gitignored

A green dot (`●`) in the UI indicates a ticker has cached data available.

---

## The Polars Backtest Engine

This is the core of TrendLab. The backtest engine is built on Polars DataFrames and uses a hybrid vectorized/sequential approach.

### Why Polars?

Polars provides:
- **Lazy evaluation**: `scan_parquet()` enables predicate and projection pushdown at the I/O level. TrendLab *always* uses lazy scanning, never eager reads.
- **Expression API**: Indicator calculations (ATR, Donchian channels, Bollinger bands, moving averages) are expressed as Polars column expressions, which Polars can parallelize and optimize.
- **Zero-copy**: DataFrames use Arrow memory format, avoiding serialization overhead between indicator computation and signal generation.

### Backtest Architecture: Vectorized + Sequential

The backtest runs in two phases:

**Phase 1 - Vectorized (Polars expressions)**:
The strategy's `add_strategy_columns()` method adds indicator columns and raw signal columns to the LazyFrame. For example, a Donchian breakout strategy adds:
- `entry_upper` / `entry_lower` (Donchian channel boundaries)
- `raw_entry` (boolean: `close > entry_upper`)
- `raw_exit` (boolean: `close < exit_lower`)

This is fully vectorized and parallelized by Polars.

**Phase 2 - Sequential state machine**:
The position state machine (`apply_position_state_machine`) must be sequential because position state depends on previous position state. It runs a single pass through the DataFrame computing:

- **Position state**: -1 (Short), 0 (Flat), 1 (Long)
- **Signal filtering**: Only allow entry when flat, exit when in position
- **Fill execution**: Next-bar-open with configurable slippage
- **Fee calculation**: Per-side basis points
- **Equity tracking**: Cash + mark-to-market position value
- **Overlay stops**: ATR trailing, Chandelier, percent trail, time-decay stops

The output is a DataFrame with all these columns appended, from which fills, trades, and equity curves are extracted.

### Fill Model

```
Signal computed on bar close → Fill executed on NEXT bar open
```

- Slippage is applied directionally: buyers pay more (open * (1 + slippage_bps/10000)), sellers receive less
- Fees are charged per-side in basis points
- Both are configurable via `CostModel`

### Trading Modes

- **Long-only** (default): Buy signals open long positions, sell signals close them
- **Short-only**: Sell-short signals open positions, buy-to-cover closes
- **Long/Short**: Both directions simultaneously

---

## Strategy System: Components Architecture

TrendLab has a compositional strategy architecture. Instead of monolithic strategies, each strategy is composed from four orthogonal components:

### 1. Signal Generators (`SignalGenerator` trait)
When to consider entering or exiting. Available types:

| Signal | Description | Key Parameters |
|--------|-------------|----------------|
| **52-Week Breakout** | Price exceeds N-day high * threshold | lookback (50-252), entry_pct (0.90-1.0), max_age (1-5) |
| **Donchian Breakout** | Classic channel breakout | entry_lookback (20-100), exit_lookback (10-50) |
| **Bollinger Breakout** | Price exceeds Bollinger band | period (10-50), multiplier (1.5-3.0) |
| **Keltner Breakout** | Price exceeds Keltner channel | ema_period (10-50), atr_period (5-20), multiplier (1.5-3.0) |
| **Supertrend** | ATR-based trend direction flip | period (7-21), multiplier (2.0-4.0) |
| **Parabolic SAR** | Wilder's parabolic stop and reverse | af_start (0.01-0.04), af_step (0.01-0.04), af_max (0.1-0.3) |
| **MA Crossover** | Fast MA crosses above/below slow MA | fast_period (10-50), slow_period (50-200), ma_type (SMA/EMA) |
| **Momentum (TSMOM)** | Time-series momentum | lookback (63-252 trading days) |
| **ROC Momentum** | Rate of change exceeds threshold | period (10-50), threshold (0-10%) |
| **Aroon Crossover** | Aroon Up crosses above Aroon Down | period (14-50) |

### 2. Position Managers (`PositionManager` trait)
How to manage stops and trails once in a position:

| Manager | Description | Key Parameters |
|---------|-------------|----------------|
| **ATR Trailing Stop** | Trail at high - ATR * multiplier | period (7-21), multiplier (2.0-5.0) |
| **Chandelier Exit** | ATR from highest high since entry | period (14-44), multiplier (2.0-4.0) |
| **Percent Trailing** | Trail at fixed % from high | trail_pct (5-20%) |
| **Since-Entry Trailing** | Exit at % below highest since entry | exit_pct (80-95%) |
| **Frozen Reference Exit** | Exit at fixed % below entry-time reference | exit_pct (80-95%) |
| **Time Decay Stop** | Stop tightens over time | initial_pct, decay_per_bar, min_pct |
| **Max Holding Period** | Force exit after N bars | max_bars (20-100) |
| **Fixed Stop** | Simple stop-loss | stop_pct (5-15%) |
| **Breakeven Then Trail** | Move stop to breakeven, then trail | breakeven_pct, trail_pct |

### 3. Execution Models (`ExecutionModel` trait)
How orders get filled:

| Model | Description |
|-------|-------------|
| **Next Bar Open** | Market order at next bar's open (default) |
| **Stop Order** | Fill at stop level with gap policy (fill_at_open, fill_at_level, worst_of) |
| **Limit Order** | Fill at limit price |
| **Close On Signal** | Fill at signal bar's close |

### 4. Signal Filters (`SignalFilter` trait)
Gate entry signals based on market conditions:

| Filter | Description | Key Parameters |
|--------|-------------|----------------|
| **No Filter** | Pass-through (baseline) | - |
| **ADX Filter** | Only enter when ADX > threshold | period (7-21), threshold (20-35) |
| **MA Regime Filter** | Only enter when above/below MA | period (50-200), direction (above/below) |
| **Volatility Filter** | Only enter when volatility in range | atr_period, min_atr_pct, max_atr_pct |

### Why This Matters

The component architecture enables **fair comparison**. You can test "Do 52-week breakout entries with ATR trailing stops beat 52-week breakout entries with percent trailing stops?" - holding the entry signal constant while varying only the exit management. This is impossible with monolithic strategies.

It also enables **structural Monte Carlo** in YOLO mode (see below) - randomly sampling combinations of components to discover winning assemblies.

---

## YOLO Mode: The Main Event

YOLO mode is TrendLab's continuous auto-optimization engine. It runs indefinitely, testing strategy configurations across all selected symbols, maintaining a live leaderboard of the best discoveries.

### YOLO Config Parameters

The YOLO config modal (press `Enter` on the Sweep panel) has two key sliders plus additional settings:

#### Dual Randomization Sliders

| Slider | Default | Range | What It Controls |
|--------|---------|-------|------------------|
| **Parameter Jitter** | 30% | 0-100% | How much to randomize parameters *within* known strategy structures. At 0%, default parameters are used. At 100%, full random within valid ranges. |
| **Structural Explore** | 15% | 0-100% | Probability of sampling random *component combinations* (signal + position manager + execution + filters) vs testing traditional fixed strategies. At 0%, only legacy strategies run. At 100%, every iteration is a random component assembly. |

The structural exploration slider implements a non-linear schedule:
- **0-30%**: Mostly parameter variation, 0-10% chance of structural swap
- **30-60%**: Balanced exploration, 10-50% structural swap probability
- **60-100%**: Full structural exploration, 50-90% swap probability

#### Additional Settings

| Setting | Default | Description |
|---------|---------|-------------|
| **Start Date** | 5 years ago | Backtest period start |
| **End Date** | Today | Backtest period end |
| **WF Sharpe Threshold** | 0.25 | Minimum average Sharpe before walk-forward validation is triggered |
| **Sweep Depth** | Quick | Parameter grid density (Quick/Normal/Deep) |
| **Warmup Iterations** | 50 | Iterations of pure exploration before winner exploitation begins |
| **Combo Mode** | 2-Way | Multi-strategy combo testing: None, 2-Way, 2+3-Way, All |
| **Combo Warmup** | 10 | Iterations before combo strategies start |
| **Polars Threads** | Auto | Thread cap per individual backtest |
| **Outer Threads** | Auto | Thread cap for symbol-level parallelism (Rayon) |
| **Max Iterations** | Unlimited | Hard stop for the YOLO run |

### How YOLO Iterations Work

Each YOLO iteration:

1. **Selects strategies**: Based on the dual sliders, either picks from known strategy types (with jittered parameters) or samples a random component combination from the pools
2. **Runs backtests**: For each selected symbol, executes the backtest through the Polars engine
3. **Computes metrics**: CAGR, Sharpe, Sortino, Calmar, max drawdown, win rate, profit factor, turnover, consecutive win/loss streaks
4. **Updates leaderboards**: Both per-symbol and cross-symbol aggregated results
5. **Persists history**: Writes fingerprints to JSONL files in `artifacts/yolo_history/`
6. **Repeats**: Until you press `Esc` or hit max_iterations

The structural sampler (`YoloSampler`) uses:
- **Weighted random selection**: Components have weights (e.g., ATR Trailing Stop has weight 2.0 - it's chosen more often because it's a key differentiator)
- **Deterministic seeding**: `StdRng::seed_from_u64` ensures reproducibility
- **Fingerprinting**: Each run produces a `YoloRunFingerprint` with config_hash (structure only) and full_hash (structure + params) for deduplication and analysis

### Combo Strategies

Beyond single strategies, YOLO mode can test multi-strategy combinations:
- **2-Way combos**: Every other iteration combines two strategies
- **3-Way combos**: Every 6th iteration
- **4-Way combos**: Enabled with "All" combo mode

Combined equity curves are computed by merging the individual strategies' signals.

---

## The Leaderboard

The Results panel (`4`) displays a ranked leaderboard of discovered strategy configurations.

### Dual Scope: Session vs All-Time

- **Session** (`t` to toggle): Shows only results from the current app launch. Resets on restart.
- **All-Time**: Persistent leaderboard saved to disk. Accumulates across all sessions.

Each session gets a unique ID (e.g., `20260206T143025`) for tracking provenance.

### Per-Symbol Leaderboard

The per-symbol leaderboard (`Leaderboard`) maintains the top N strategies ranked by Sharpe ratio. It features:

- **Deduplication**: Same config_hash (strategy_type + config + symbol) replaces only if the new run has a better Sharpe
- **Bounded size**: Defaults to 500 entries max per scope
- **Sort by Sharpe**: Descending, with 1-based ranking

Each `LeaderboardEntry` contains:
- Strategy type and full configuration
- Symbol and sector
- Full `Metrics` struct (CAGR, Sharpe, Sortino, drawdown, win rate, profit factor, turnover, streak data)
- Equity curve (Vec<f64>) and corresponding timestamps
- Discovery metadata (iteration number, session ID, timestamp)
- **Confidence grade**: Block bootstrap analysis of the Sharpe ratio's significance
- **Walk-forward validation**: Mean OOS Sharpe, Sharpe degradation ratio, % profitable folds, p-values
- **Stickiness diagnostics**: Median holding period, p95 hold bars, exit trigger rate

### Cross-Symbol Leaderboard

The `CrossSymbolLeaderboard` aggregates results across all tested symbols for each strategy configuration. This answers: "Which config worked best across the entire universe?"

`AggregatedMetrics` computed for each config:
- Average / min / max Sharpe across symbols
- Geometric mean CAGR (rewards consistency over outlier performance)
- Hit rate (fraction of symbols where the strategy was profitable)
- Worst max drawdown
- Average number of trades
- Tail risk: CVaR 95%, skewness, kurtosis, downside deviation ratio
- Regime concentration penalty

### Risk Profiles for Ranking

The leaderboard supports four risk profiles (cycle with `p`), each weighting metrics differently:

| Profile | Emphasis | Best For |
|---------|----------|----------|
| **Balanced** | Equal across all metrics | Default exploration |
| **Conservative** | Tail risk, drawdown, consistency | Risk-averse strategies |
| **Aggressive** | Returns, Sharpe, hit rate | Maximum performance |
| **TrendOptions** | Hit rate, consecutive losses, OOS Sharpe | Options traders using trend signals for entries |

Each profile defines weights across 10 metrics (avg Sharpe, OOS Sharpe, hit rate, min Sharpe, max drawdown, CVaR, avg duration, max consecutive losses, walk-forward grade, regime concentration). The weights sum to 1.0.

### Ranking Metrics

You can rank the cross-symbol leaderboard by:
- `AvgSharpe` - Average Sharpe across all symbols (default)
- `MinSharpe` - Conservative: worst-case performance
- `GeoMeanCagr` - Geometric mean of (1 + CAGR) - 1
- `HitRate` - Fraction of profitable symbols
- `MeanOosSharpe` - Anti-overfit: out-of-sample Sharpe from walk-forward
- `CompositeScore(RiskProfile)` - Weighted percentile ranking

---

## Statistical Validation

TrendLab goes beyond simple metrics with several layers of statistical validation:

### Stickiness Diagnostics

A key finding during development: some strategy configurations produce "sticky" positions - trades that are held for excessively long periods because exit conditions keep moving away from the price. The `StickinessMetrics` detect this:

- **Median hold bars**: If > 200 trading days (~10 months), flagged as sticky
- **P95 hold bars**: 95th percentile holding period
- **% over 60/120 bars**: Fraction of trades held > 3/6 months
- **Exit trigger rate**: How often the exit signal actually fires while in position (< 1% = very sticky)
- **Reference chase ratio**: Inverse of exit trigger rate (high = exit keeps running away)

### Bootstrap Confidence Grades

Each leaderboard entry gets a confidence grade computed via **block bootstrap resampling**:

1. Compute daily returns from the equity curve
2. Run stationary block bootstrap (preserving autocorrelation structure of returns)
3. Build a confidence interval for the annualized Sharpe ratio
4. Grade:
   - **High**: CI lower bound > 0.5, CI width < 1.5
   - **Medium**: CI lower bound > 0, CI width < 3.0
   - **Low**: Everything else
   - **Insufficient**: < 30 data points

Block bootstrap is specifically chosen over IID bootstrap because financial returns exhibit serial dependence (autocorrelation, volatility clustering). Block bootstrap produces wider, more realistic confidence intervals.

### Cross-Symbol Confidence

For cross-symbol results, a separate bootstrap grades the mean per-symbol Sharpe ratio, with guardrails:
- At least 10 symbols required
- Worst symbol Sharpe must be > -0.25 for Medium, > 0 for High
- At least 70-80% of symbols must be profitable

### Walk-Forward Validation

Promising configs (those exceeding the WF Sharpe threshold) undergo walk-forward validation:
- Split data into multiple time folds
- Train on in-sample (IS), test on out-of-sample (OOS)
- Compute degradation ratio: mean_oos_sharpe / mean_is_sharpe (close to 1.0 = good generalization)
- FDR correction (Benjamini-Hochberg) across all tested configs to account for multiple comparisons

---

## Performance Metrics

The `Metrics` struct captures everything computed for each backtest run:

| Metric | Description |
|--------|-------------|
| `total_return` | End-to-end return as decimal (0.25 = 25%) |
| `cagr` | Compound annual growth rate |
| `sharpe` | Annualized Sharpe ratio (252 trading days, 0% risk-free) |
| `sortino` | Like Sharpe but only penalizes downside volatility |
| `max_drawdown` | Peak-to-trough drawdown as positive percentage |
| `calmar` | CAGR / Max Drawdown |
| `win_rate` | Winning trades / total trades |
| `profit_factor` | Gross profit / gross loss (capped at 999.99) |
| `num_trades` | Total number of round-trip trades |
| `turnover` | Annual traded notional / average capital |
| `max_consecutive_losses` | Worst losing streak |
| `max_consecutive_wins` | Best winning streak |
| `avg_losing_streak` | Mean length of losing streaks |

Sharpe and Sortino use population standard deviation (dividing by N, not N-1) and assume 252 trading days for annualization.

---

## YOLO History and Run Fingerprinting

Every YOLO run produces a `YoloRunFingerprint` capturing:

- **Identity**: Unique run ID (UUID), timestamp, random seed
- **Configuration**: Symbol, date range, randomization percentage
- **Component breakdown**: Signal type + params, position manager type + params, execution model + params, filter types + params
- **Derived hashes**: `config_hash` (structure only, for grouping similar combos) and `full_hash` (structure + params)
- **Results**: Full metrics, optional stickiness diagnostics, trade count

These fingerprints are persisted as JSONL (one JSON object per line) in `artifacts/yolo_history/`. The `YoloHistory` struct provides indexed querying:
- By config hash (same structural combination)
- By signal type
- By position manager type
- By any component type

And statistical summaries:
- `component_stats()`: Mean/median/p25/p75 Sharpe and win rate per component type
- `top_by_sharpe(n)`: Top N runs overall
- `most_robust_structures(min_runs)`: Structures with highest win rate (minimum sample size)

This enables post-hoc analysis like "ATR Trailing Stop has mean Sharpe 0.35 across 500 runs vs Percent Trailing at 0.22" - answering which components contribute most to performance.

---

## Charts

The Chart panel (`5`) renders equity curves as ASCII line charts in the terminal using ratatui drawing primitives. Features:

- View modes: Equity curve, drawdown overlay
- Multiple curves: Compare top leaderboard entries side-by-side
- Crosshair mode for inspecting specific dates
- Volume subplot toggle

Charts are populated from the `equity_curve` and `dates` fields stored in leaderboard entries.

---

## The Worker Thread

All heavy computation happens on a background worker thread, keeping the UI responsive. The worker communicates via channels (`mpsc`):

**Commands (TUI -> Worker)**:
- `SearchSymbols`: Autocomplete Yahoo Finance tickers
- `FetchData`: Download and cache OHLCV data
- `LoadCachedData`: Load from local Parquet (no network)
- `StartSweep` / `StartMultiSweep`: Parameter sweeps
- `StartMultiStrategySweep`: All strategies across all symbols
- `StartYolo`: YOLO mode iterations
- Various Parquet-direct variants (skip Vec<Bar> intermediary)

**Results (Worker -> TUI)**:
- Search results, data loaded confirmations
- Sweep results with per-config metrics
- Leaderboard updates
- Progress updates (% complete, current symbol)

The worker uses:
- **Rayon** for symbol-level parallelism (outer loop)
- **Polars threading** for per-backtest parallelism (inner loop)
- **Atomic cancellation flag** for responsive Esc handling

---

## Crate Architecture

```
trendlab-core     -- Domain types, strategies, indicators, metrics, backtest engine, leaderboard
trendlab-engine   -- App state, navigation, worker thread, YOLO state machine
trendlab-tui      -- ratatui rendering, panel implementations, keyboard handling
trendlab-cli      -- Command-line interface (alternative to TUI)
trendlab-bdd      -- Cucumber-rs BDD test runner + Gherkin feature files
```

The separation is intentional:
- `core` has zero UI dependencies and can be used as a library
- `engine` manages state but doesn't render
- `tui` is pure rendering + input handling
- `bdd` runs behavioral tests using the same `core` library

---

## What's Particularly Well-Designed

1. **The component architecture**: Decomposing strategies into signal/position/execution/filter is the right abstraction. It enables fair comparison and structural discovery that monolithic strategies can't.

2. **YOLO mode's dual sliders**: The parameter jitter + structural explore sliders give intuitive control over the exploration-exploitation tradeoff without requiring the user to understand the underlying probability schedules.

3. **Stickiness diagnostics**: Detecting pathological holding patterns is a real problem in trend-following research. Most backtesters don't have this.

4. **Block bootstrap confidence**: Using block (not IID) bootstrap for Sharpe significance is statistically correct for time-series data. Most retail tools get this wrong.

5. **Run fingerprinting and history**: The JSONL history with component-level attribution enables meta-research - studying which building blocks contribute to performance.

6. **The Polars integration**: Lazy scanning with predicate pushdown for I/O, expression API for indicators, and single-pass sequential state machine for fills. It's the right hybrid approach.

7. **Risk profiles**: Having Balanced/Conservative/Aggressive/TrendOptions presets for weighted ranking means different users can optimize for different objectives without touching weights directly.

---

## What Didn't Work As Intended

The CLAUDE.md notes that "the nuts and bolts on the inside didn't really work as intended." Based on the code, the likely issues include:

1. **The sequential state machine bottleneck**: The position state machine must be sequential (position at bar N depends on position at bar N-1). In a 30-year backtest across 500 symbols, this single-threaded inner loop dominates runtime despite Polars handling everything else efficiently.

2. **Structural YOLO's coverage problem**: With 10+ signal types, 9 position managers, 4 execution models, and 4 filters, the combinatorial space is enormous (~1440 structural combinations before parameters). At 15% structural explore, most iterations still use traditional strategies, meaning the structural space is sparsely sampled.

3. **Leaderboard accumulation complexity**: The dual session/all-time leaderboard with config-hash deduplication, cross-symbol aggregation, and FDR-corrected p-values creates a lot of moving parts. The debug logging in `update_leaderboards()` suggests this was an area of active debugging.

4. **The stickiness problem**: The docs reference extensive investigation into "sticky" strategies. The exit reference chase ratio and multiple overlay stop types (ATR trailing, Chandelier, time-decay) all exist to combat this, suggesting it was a persistent challenge.

5. **Cost model sensitivity**: Some configs that look great without costs may be mediocre with realistic slippage and fees, especially high-turnover strategies. The current cost model is simple (fixed bps) and may not capture market impact for larger positions.
