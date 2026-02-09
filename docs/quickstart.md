# Quick Start Guide

Clone → first YOLO run in under 5 minutes.

## Prerequisites

- **Rust toolchain** (1.78+): [rustup.rs](https://rustup.rs)
- **Internet connection** (for initial data download; or use bundled fixture)

## 1. Clone and Build

```bash
git clone https://github.com/your-org/trendlab.git
cd trendlab
cargo build --workspace --release
```

The release build takes ~2 minutes on a modern machine.

## 2. Download Market Data

Fetch a small universe to get started fast (3 symbols, 2 years):

```bash
cargo run --release -p trendlab-cli -- download SPY QQQ AAPL \
  --start 2023-01-01 --end 2024-12-31
```

This downloads ~30 KB of daily OHLCV data per symbol and caches it as Parquet in `data/`.

**Offline alternative:** The repo ships with a frozen SPY 2024 fixture in `data/symbol=SPY/2024.parquet` (~12 KB). You can run your first backtest immediately against that data with no network access.

## 3. Run Your First Backtest (CLI)

Using a bundled preset:

```bash
cargo run --release -p trendlab-cli -- run \
  --preset momentum_roc --symbol SPY \
  --start 2024-01-02 --end 2024-12-31
```

Or using a TOML config file:

```bash
cargo run --release -p trendlab-cli -- run \
  --config config/strategies/donchian_breakout.toml
```

You'll see a summary like:

```
=== Backtest Result ===
Symbol:         SPY
Period:         2024-01-02 to 2024-12-31
Bars:           252 (26 warmup)
Signals:        9
Trades:         6

--- Performance ---
Total Return:   12.45%
Sharpe:         1.234
Max Drawdown:   -8.52%
Win Rate:       66.7%
```

Results are saved as JSON + CSV in the `results/` directory.

## 4. Launch the TUI

```bash
cargo run --release -p trendlab-tui
```

The TUI is a six-panel terminal interface:

| Key | Panel |
|-----|-------|
| `1` | Leaderboard — ranked strategies across all symbols |
| `2` | Details — composition, metrics, stickiness diagnostics |
| `3` | Chart — equity curve with ghost curves |
| `4` | YOLO — continuous auto-discovery engine |
| `5` | Data — download and manage market data |
| `6` | Config — settings and universe management |

Navigate with `j`/`k` (up/down), `h`/`l` (left/right), `Enter` to select, `q` to quit.

## 5. Run YOLO Mode

1. Press `4` to open the YOLO panel
2. Select your universe symbols (SPY, QQQ, AAPL)
3. Configure the sliders:
   - **Parameter Jitter** — how much to vary signal/PM parameters (0–100%)
   - **Structural Exploration** — how aggressively to mix component types (0–100%)
4. Press `Enter` to start

YOLO runs backtests continuously in the background, populating the Leaderboard (panel `1`) with the best results ranked by your active risk profile.

## 6. Explore Results

Switch to panel `1` (Leaderboard) to see ranked strategies. Use `j`/`k` to browse, `Enter` to inspect a row in the Details panel (`2`). Press `3` for the equity chart.

Risk profiles cycle with `r`:
- **Aggressive** — optimizes for total return
- **Balanced** — Sharpe-weighted ranking
- **Conservative** — penalizes drawdown heavily
- **Income** — favors consistent, lower-volatility returns

## What's Next

- **More symbols:** Download a larger universe with `trendlab download SPY QQQ AAPL MSFT NVDA AMZN GOOG META TSLA ...`
- **Custom strategies:** Write your own TOML config — see [Configuration Reference](config-reference.md)
- **Extend the engine:** Add new signals, PMs, or filters — see [Extension Guide](extension-guide.md)
- **Cache management:** `trendlab cache status` and `trendlab cache clean --unused-days 90`

## Example TOML Configs

The `config/strategies/` directory ships with 8 ready-to-run examples:

| File | Strategy |
|------|----------|
| `donchian_breakout.toml` | Classic 50-period Donchian channel breakout |
| `bollinger_breakout.toml` | Bollinger band breakout with ADX filter |
| `ma_crossover_trend.toml` | SMA 10/50 crossover with MA regime filter |
| `supertrend_system.toml` | Supertrend flip with breakeven-then-trail |
| `momentum_roc.toml` | Rate-of-change momentum with time-decay stop |
| `breakout_52w.toml` | 52-week high breakout (pure trend) |
| `buy_and_hold.toml` | Buy-and-hold benchmark |
| `mean_reversion_rsi.toml` | Counter-trend TSMOM with fixed stop |
