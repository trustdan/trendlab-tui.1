# TrendLab v3

> Research-grade trend-following backtesting engine with terminal UI

TrendLab v3 is a Rust-based backtesting system designed to solve two chronic failure modes in traditional backtesting:

1. **Strategy stickiness** — exits "chase" highs and never let you exit profitably
2. **Execution bias** — forced "signal on close, fill next open" assumptions that distort strategy families

## What it does

- Downloads real market data from Yahoo Finance and caches it as Parquet
- Composes strategies from four independent components: signal, position manager, execution model, and signal filter
- Runs backtests with realistic execution (gap fills, intrabar ambiguity, configurable slippage)
- YOLO mode continuously discovers strategy configurations across hundreds of symbols
- Ranks discoveries on a cross-symbol leaderboard with risk profiles and confidence grades
- Six-panel TUI with vim-style navigation, equity curve charts, and drill-down diagnostics

## Project Status

**Current Phase:** Phase 1 — Repo Bootstrap

See [Development Plan](trendlab-v4-new-build-plan.md) for the full 14-phase build plan.

| Phase | Description | Week | Status |
|-------|-------------|------|--------|
| 1 | Repo bootstrap | 1 | Not started |
| 2 | Smoke backtest (tracer bullet) | 1 | Not started |
| 3 | Domain model + component traits | 2 | Not started |
| 4 | Data ingest + Yahoo Finance | 3 | Not started |
| 5a | Data pipeline integration | 4 | Not started |
| 5b | Event loop + indicators (13) | 4 | Not started |
| 6 | Orders + order book | 5 | Not started |
| 7 | Execution engine | 6 | Not started |
| 8 | Position management (9 PMs) | 7 | Not started |
| 9 | Strategy composition (10 signals, 4 filters) | 8-9 | Not started |
| 10 | Runner + CLI + YOLO + leaderboards | 10-11 | Not started |
| 11 | Robustness + statistical validation | 12 | Not started |
| 12 | TUI (6-panel layout) | 13-15 | Not started |
| 13 | Reporting + exports | 16 | Not started |
| 14 | Hardening + docs + quick start | 17 | Not started |

## Architecture

### Core design principles

1. **Separation of concerns** — Signals never see portfolio state. Position management never influences signals. Execution is configurable, not hardcoded.

2. **Bar-by-bar event loop** — Deterministic four-phase loop per bar (start-of-bar, intrabar, end-of-bar, post-bar), even though indicators are vectorized with Polars.

3. **Four-component composition** — Every strategy is a combination of signal generator + position manager + execution model + signal filter. Components are independently swappable.

4. **Promotion ladder** — Cheap candidates must earn expensive simulation. Level 1 (deterministic), Level 2 (walk-forward), Level 3 (execution Monte Carlo).

### Canonical flow

```
Signals -> Signal Filter -> Order Policy -> Order Book -> Execution Model -> Portfolio -> Position Manager
```

## Workspace structure

```
trendlab-v3/
├── Cargo.toml          # workspace root
├── trendlab-core/      # engine: domain types, event loop, orders, execution, PM
├── trendlab-runner/    # backtest orchestration, YOLO mode, leaderboards
├── trendlab-tui/       # Ratatui six-panel terminal UI
├── trendlab-cli/       # CLI: download and run commands
└── data/               # Parquet cache (Hive-partitioned by symbol/year)
```

## Quick start (once built)

```bash
# Build
cargo build --workspace

# Download data
cargo run -p trendlab-cli -- download SPY QQQ IWM

# Run a backtest
cargo run -p trendlab-cli -- run --preset donchian-breakout --symbols SPY

# Launch the TUI
cargo run -p trendlab-tui

# Run tests
cargo test --workspace

# Lint
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## What ships

- 10 signal generators (52-week breakout, Donchian, Bollinger, Keltner, Supertrend, Parabolic SAR, MA crossover, TSMOM, ROC, Aroon)
- 9 position managers (ATR trailing, Chandelier, percent trailing, since-entry, frozen reference, time decay, max holding, fixed stop, breakeven-then-trail)
- 4 execution models (next-bar-open, stop, limit, close-on-signal)
- 4 signal filters (none, ADX, MA regime, volatility)
- 13 indicators powering the above
- Cross-symbol leaderboard with 4 risk profiles
- YOLO mode with dual randomization sliders (parameter jitter + structural exploration)
- Block bootstrap confidence grades and walk-forward validation with FDR correction
- Stickiness diagnostics for every backtest run
- Run fingerprinting and JSONL history for meta-analysis

## Code style

- `thiserror` for domain errors, `anyhow` for top-level propagation
- Explicit structs/enums over stringly-typed configs
- Allocation-free hot loops, Polars for vectorized indicator math
- Rayon for symbol-level parallelism, Polars threading for per-backtest

## License

TBD

---

**Last Updated:** 2026-02-06
