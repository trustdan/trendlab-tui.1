# Configuration Reference

Complete reference for all TrendLab TOML configuration options.

---

## TOML Config File Format

Every backtest is defined by a TOML file with four sections: `[backtest]`, `[signal]`, `[position_manager]`, `[execution_model]`, and an optional `[signal_filter]`.

```toml
[backtest]
symbol = "SPY"
start_date = "2020-01-02"
end_date = "2024-12-31"
initial_capital = 100000.0
trading_mode = "long_only"
position_size_pct = 1.0

[signal]
type = "donchian_breakout"
[signal.params]
entry_lookback = 50.0

[position_manager]
type = "atr_trailing"
[position_manager.params]
atr_period = 14.0
multiplier = 3.0

[execution_model]
type = "next_bar_open"
[execution_model.params]
preset = 1.0

[signal_filter]
type = "no_filter"
```

---

## [backtest] Section

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `symbol` | string | yes | — | Ticker symbol (e.g., "SPY", "QQQ") |
| `start_date` | string | yes | — | Start date in YYYY-MM-DD format |
| `end_date` | string | yes | — | End date in YYYY-MM-DD format |
| `initial_capital` | float | no | 100000.0 | Starting portfolio cash |
| `trading_mode` | string | no | "long_only" | One of: `long_only`, `short_only`, `long_short` |
| `position_size_pct` | float | no | 1.0 | Fraction of capital allocated per trade (0.0–1.0) |

---

## Signal Types

### `breakout_52w` — 52-Week High Breakout

Fires when close exceeds the highest close over the lookback period.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lookback` | usize | 252 | Number of bars to look back for the high |
| `threshold_pct` | float | 0.0 | Minimum % above the lookback high to trigger |

### `donchian_breakout` — Donchian Channel Breakout

Fires when close exceeds the Donchian upper band.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `entry_lookback` | usize | 50 | Donchian channel period |

### `bollinger_breakout` — Bollinger Band Breakout

Fires when close exceeds the upper Bollinger band.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `period` | usize | 20 | Bollinger band period |
| `std_multiplier` | float | 2.0 | Standard deviation multiplier |

### `keltner_breakout` — Keltner Channel Breakout

Fires when close exceeds the upper Keltner channel.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `ema_period` | usize | 20 | EMA period for the center line |
| `atr_period` | usize | 10 | ATR period for channel width |
| `multiplier` | float | 1.5 | ATR multiplier for channel width |

### `supertrend` — Supertrend Flip

Fires when the Supertrend indicator flips direction.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `period` | usize | 10 | ATR period for Supertrend calculation |
| `multiplier` | float | 3.0 | ATR multiplier for bands |

### `parabolic_sar` — Parabolic SAR Flip

Fires when the Parabolic SAR flips from above to below price (or vice versa).

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `af_start` | float | 0.02 | Initial acceleration factor |
| `af_step` | float | 0.02 | Acceleration factor increment |
| `af_max` | float | 0.20 | Maximum acceleration factor |

### `ma_crossover` — Moving Average Crossover

Fires when the fast MA crosses above the slow MA.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `fast_period` | usize | 10 | Fast moving average period |
| `slow_period` | usize | 50 | Slow moving average period |
| `ma_type` | float | 0.0 | 0.0 = SMA, 1.0 = EMA |

### `tsmom` — Time-Series Momentum

Fires when current price is above the price N bars ago (positive momentum).

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lookback` | usize | 20 | Number of bars to compare against |

### `roc_momentum` — Rate of Change Momentum

Fires when the rate of change exceeds the threshold.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `period` | usize | 12 | ROC calculation period |
| `threshold_pct` | float | 0.0 | Minimum ROC % to trigger (0.0 = any positive) |

### `aroon_crossover` — Aroon Crossover

Fires when Aroon Up crosses above Aroon Down.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `period` | usize | 25 | Aroon calculation period |

---

## Position Manager Types

### `atr_trailing` — ATR Trailing Stop

Trailing stop at N x ATR below the highest close since entry.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `atr_period` | usize | 14 | ATR calculation period |
| `multiplier` | float | 3.0 | ATR multiplier for stop distance |

### `percent_trailing` — Percent Trailing Stop

Trailing stop N% below the highest close since entry.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `trail_pct` | float | 0.05 | Trailing distance as fraction (0.05 = 5%) |

### `chandelier` — Chandelier Exit

N x ATR below the highest high since entry (not highest close).

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `atr_period` | usize | 22 | ATR calculation period |
| `multiplier` | float | 3.0 | ATR multiplier |

### `fixed_stop_loss` — Fixed Stop Loss

Fixed stop N% below entry price. Never moves.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `stop_pct` | float | 0.02 | Stop distance as fraction (0.02 = 2%) |

### `breakeven_then_trail` — Breakeven Then Trail

Moves stop to breakeven once profit reaches the trigger threshold, then trails.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `breakeven_trigger_pct` | float | 0.02 | Profit % to trigger breakeven move |
| `trail_pct` | float | 0.03 | Trailing distance after breakeven |

### `time_decay` — Time Decay Stop

Stop tightens each bar. Distance starts at `initial_pct` and decays by `decay_per_bar` per bar, floored at `min_pct`.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `initial_pct` | float | 0.10 | Initial stop distance (10%) |
| `decay_per_bar` | float | 0.005 | Decay per bar (0.5% per bar) |
| `min_pct` | float | 0.02 | Minimum stop distance (2%) |

### `frozen_reference` — Frozen Reference Stop

Stop at N% below entry price. Never moves (no ratcheting).

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `exit_pct` | float | 0.05 | Stop distance from entry (5%) |

### `since_entry_trailing` — Since Entry Trailing

Stop at N% below entry, trailing upward as entry-adjusted reference rises.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `exit_pct` | float | 0.05 | Stop distance (5%) |

### `max_holding_period` — Max Holding Period

Exits after a fixed number of bars regardless of price.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_bars` | usize | 20 | Maximum bars to hold |

### `no_op` — No-Op (Hold Forever)

No position management. Position is held until a reversal signal or end of data.

No parameters.

---

## Execution Model Types

### `next_bar_open` — Next Bar Open

Market-on-open order at the next bar after the signal.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `preset` | float | 1.0 | Execution preset (see below) |

### `stop_entry` — Stop Entry

Stop order placed at a breakout level. Fills when price trades through.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `preset` | float | 1.0 | Execution preset |

### `close_on_signal` — Close on Signal

Market-on-close order at the signal bar.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `preset` | float | 1.0 | Execution preset |

### `limit_entry` — Limit Entry

Limit order placed below the signal trigger price.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `preset` | float | 1.0 | Execution preset |
| `offset_bps` | float | 25.0 | Offset below trigger in basis points |

### Execution Presets

The `preset` parameter controls slippage, commission, and intrabar path resolution:

| Value | Name | Slippage | Commission | Path Policy |
|-------|------|----------|------------|-------------|
| 0.0 | Frictionless | 0 bps | 0 bps | Deterministic |
| 1.0 | Realistic | 5 bps | 5 bps | WorstCase |
| 2.0 | Hostile | 20 bps | 15 bps | WorstCase |
| 3.0 | Optimistic | 2 bps | 2 bps | BestCase |

**Realistic** (1.0) is the default and recommended for research.

---

## Signal Filter Types

### `no_filter` — No Filter

All signals pass through unfiltered.

No parameters.

### `adx_filter` — ADX Trend Strength Filter

Only allows signals when ADX exceeds a threshold (strong trend).

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `period` | usize | 14 | ADX calculation period |
| `threshold` | float | 25.0 | Minimum ADX value to pass signals |

### `ma_regime` — Moving Average Regime Filter

Only allows signals when price is above (or below) a long-term MA.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `period` | usize | 200 | MA period |
| `direction` | float | 0.0 | 0.0 = above MA (uptrend), 1.0 = below MA (downtrend) |

### `volatility_filter` — Volatility Range Filter

Only allows signals when ATR% (ATR / close) is within a specified range.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `period` | usize | 14 | ATR calculation period |
| `min_pct` | float | 0.5 | Minimum ATR% to pass |
| `max_pct` | float | 5.0 | Maximum ATR% to pass |

---

## Named Presets (CLI)

These presets are available via `trendlab run --preset <name>`:

| Preset Name | Signal | PM | Execution | Filter |
|-------------|--------|----|-----------|--------|
| `donchian_trend` | donchian_breakout (50) | atr_trailing (14, 3.0) | stop_entry | no_filter |
| `bollinger_breakout` | bollinger_breakout (20, 2.0) | percent_trailing (5%) | next_bar_open | adx_filter (14, 25) |
| `ma_crossover` | ma_crossover (10/50 SMA) | chandelier (22, 3.0) | next_bar_open | ma_regime (200) |
| `momentum_roc` | roc_momentum (12, 0%) | time_decay (10%/0.5%/2%) | next_bar_open | volatility_filter |
| `supertrend` | supertrend (10, 3.0) | breakeven_then_trail (2%/3%) | next_bar_open | no_filter |

---

## YOLO Configuration

YOLO mode settings are configured in the TUI (panel 4) and persisted across restarts:

| Setting | Description |
|---------|-------------|
| Universe | List of symbols to explore |
| Parameter Jitter (0–100%) | How much to randomize signal/PM parameters around defaults |
| Structural Exploration (0–100%) | How aggressively to mix different component types |
| Risk Profile | Ranking metric: Aggressive, Balanced, Conservative, Income |
| Thread Settings | `outer_threads` (parallel symbols), `polars_threads` (per-indicator) |

---

## Universe Configuration

The universe of tradeable symbols is defined in `config/universe.toml`:

```toml
[universe]
symbols = ["SPY", "QQQ", "AAPL", "MSFT", "NVDA"]
```

Or via the TUI Data panel (panel 5) where symbols can be added/removed interactively.

---

## Data Cache

Market data is cached as Parquet files in the `data/` directory:

```
data/
  symbol=SPY/
    2024.parquet    # Hive-partitioned by year
    meta.json       # Cache metadata
  symbol=QQQ/
    ...
```

Manage the cache with CLI commands:

```bash
# View cache status
trendlab cache status

# Preview what would be cleaned
trendlab cache clean --unused-days 90

# Actually clean
trendlab cache clean --unused-days 90 --confirm
```
