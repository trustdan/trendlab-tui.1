# TrendLab v3 — Ground-Up Rebuild Plan (Overview)

> Goal: rebuild TrendLab as a **research-grade trend-following lab** with **realistic order simulation**, clean separations (alpha vs risk vs execution), and a Parrot-inspired high-contrast TUI.

---

## 0) North Star

### What “better” means this time
- **Strategies are not bundles**: you can swap Signal / Order Policy / Execution Model / Position Manager independently.
- **Execution is first-class**: stop/limit/market orders, gaps, slippage, intrabar ambiguity handling, OCO brackets.
- **Rigor by default**: walk-forward, regime testing, universe resampling, and execution/path sensitivity are built into the workflow.
- **Research UX**: leaderboards + visual diagnostics make it obvious *why* something won.

### Non-goals (explicit)
- No live brokerage integration (evergreen “research tool” boundary).
- No high-frequency microstructure or level-2 simulation.
- No ML training pipeline (keep it strategy research + robust validation).

---

## 1) Architecture at 10,000 ft

### Crates / packages
- `trendlab-core`
  - data types, indicators, order simulation, backtest engine, metrics, validation
- `trendlab-runner`
  - sweeps, “Full-Auto” continuous search, experiment tracking, persistence
- `trendlab-tui`
  - Ratatui UI, panels, charts, theme system, input bindings
- `trendlab-cli` (optional but recommended)
  - headless runs, CI regression tests, batch experiments, export artifacts

### Separation of concerns (the “pipeline”)
1. **Alpha / Intent**
   - emits “desired exposure” or “trade intent” (no fills, no portfolio knowledge)
2. **Order Policy (Synthesis)**
   - converts intent into *orders* (market/stop/limit/brackets) + timing rules
3. **Execution Model**
   - simulates realistic fills from orders + OHLCV bars (gaps, slippage, ambiguity)
4. **Position / Risk Management**
   - manages stops/targets, trailing logic, time exits, scaling rules (stateful)
5. **Portfolio + Accounting**
   - positions, cash, fees, equity curve, per-trade ledger

---

## 2) Data Layer (fast + reproducible)

### Data sources
- Primary: daily OHLCV (and optionally hourly later)
- Pluggable fetchers (Yahoo, Stooq, local CSV/Parquet imports)

### Data pipeline
- Normalize → Parquet cache → lazy scan
- Deterministic schema: `timestamp, open, high, low, close, volume, adj_close?`
- Version metadata written into artifacts (so results are reproducible)

### “Universe” management
- Universe sets (542 tickers, sector sets, custom lists)
- Universe Monte Carlo: sample subsets to reduce overfit-to-universe

---

## 3) Backtest Engine v3 (event loop, not vector math)

### Core loop (bar-based event phases)
For each symbol and bar:
1. **Start-of-bar**
   - activate orders that become live now (latency rules)
   - execute MOO-type orders
2. **Intrabar simulation**
   - apply intrabar path policy (deterministic/worst-case/randomized)
   - trigger stop/limit/bracket orders
   - resolve OCO/OSO interactions
3. **End-of-bar**
   - execute MOC-type orders (if supported)
   - update position manager state (e.g., trailing stops for next bar)
4. **Accounting**
   - record fills, update positions/cash/equity, update risk stats

### Determinism and reproducibility
- Engine runs deterministically given:
  - dataset hash + config id + RNG seed (for stochastic execution/path models)
- Every run writes a compact “run manifest” (seed, rules, versions)

---

## 4) Orders & Execution Simulation (the “money layer”)

### Minimum order types (MVP)
- Market (MOO/MOC variants)
- Stop Market
- Limit
- Stop Limit (optional but useful)
- Brackets / OCO (stop-loss + take-profit linked)

### Execution configuration knobs (make assumptions explicit)
- Spread model (bps or fixed)
- Slippage model:
  - constant bps
  - volatility-scaled (ATR/range-based)
  - distributional (sampled; seed-controlled)
- Gap rule:
  - stop fills at open if gapped through
  - stop-limit may fail on gaps (realistic fragility)
- Intrabar policy:
  - deterministic (O→H→L→C or O→L→H→C)
  - worst-case when ambiguous (conservative)
  - stochastic path sampling (contender-stage robustness)
- Partial fills (optional, later):
  - cap fill size to % of bar volume
- Order lifetime:
  - GTC / GTD / day-only

### Execution presets (ship these as named profiles)
- `Optimistic` (teaching/debugging)
- `Realistic` (default)
- `Hostile` (adversarial fills, conservative ambiguity)
- `BreakoutFocused` (proper stop behavior + gap realism)

---

## 5) Strategy Composition Model (fair comparisons)

### Traits / interfaces (conceptual)
- `AlphaModel`: produces intent (exposure target or entry/exit intent)
- `OrderPolicy`: turns intent into concrete orders
- `PositionManager`: maintains stops/targets/trailing/time exits; may emit maintenance orders
- `Sizer`: converts intent into quantity (risk-based sizing)
- `RiskLimits`: max leverage, max positions, sector caps, etc.
- `ExecutionModel`: fills orders given bars and config

### Config identity
- Every component has a config struct
- Compose into a canonical `ConfigId` (stable hash)
- Enables:
  - caching
  - leaderboard keys
  - experiment reproducibility

---

## 6) Metrics, Diagnostics, and “Why did this win?”

### Core metrics
- CAGR, Sharpe/Sortino, Max DD, Calmar, win rate, PF
- Trade stats: avg win/loss, expectancy, hold time, MAE/MFE

### Diagnostics (first-class artifacts)
- Trade tape (orders → fills)
- Order overlay on candles (entry/exit markers)
- Slippage/gap impact summary
- Sensitivity report:
  - performance across execution presets
  - optional path sampling variance

---

## 7) Validation & Robustness Pipeline (promotion ladder)

### Stages
1. **Cheap screen**
   - deterministic execution + fixed intrabar policy
2. **Walk-forward**
   - rolling splits; score stability + degradation
3. **Regime tests**
   - volatility regimes, crash periods, rate regimes (as defined)
4. **Universe resampling**
   - repeated random subsets
5. **Execution sensitivity**
   - slippage/gap distributions, “hostile” mode
6. **Optional: path sampling**
   - intrabar stochastic mode for finalists only

---

## 8) Search / Optimization (Full-Auto v3)

### Two-axis randomness (keep the proven UX)
- **Parameter Jitter**
- **Structural Explore** (signals × PM × execution × order policy)

### Sampling
- Prefer Latin Hypercube / Sobol for continuous params
- Enumerate structural combos, then sample within each

### Caching + pruning
- Cache indicator columns by parameter set where possible
- Early-stop bad configs (e.g., catastrophic DD, too-few trades, etc.)

---

## 9) Persistence & Experiment Tracking

### Artifacts
- `artifacts/leaderboards/*.json` (session + all-time)
- `artifacts/runs/<run_id>/manifest.json`
- `artifacts/runs/<run_id>/equity.parquet`
- `artifacts/runs/<run_id>/trades.parquet`
- `.json.backup` on every write (crash safety)

### Reproducibility contract
Each leaderboard entry stores:
- config id + component ids
- dataset hash + date range
- execution preset + parameters
- RNG seed (if any stochastic behavior)

---

## 10) TUI v3 (Parrot-inspired + research-first)

### Preserve (proven)
- Vim-style navigation
- Worker thread for long tasks + progress updates
- Terminal charts (candles, equity, overlays)
- Session vs all-time leaderboards
- Risk profiles weighting

### New first-class UI features
- **Execution Lab panel**
  - preset selector + key knobs + quick stress tests
- **Trade Tape panel**
  - inspect order lifecycle + fill reasons
- **Sensitivity panel**
  - “same config under 4 execution presets” mini-comparison
- **Run manifest viewer**
  - shows the exact rules used for any leaderboard row

### Theme system
- Semantic tokens: `accent`, `positive`, `negative`, `warning`, `muted`, `border`, `selected`
- Parrot-like palette (example)
  - background: `#0d0d0d`
  - accent: `#00ffee`
  - positive: `#00ff88`
  - negative: `#ff0055`
  - warning: `#ff8800`
  - neutral: `#b366ff`
  - border: `#2a4040`
  - muted text: `#6688aa`

---

## 11) Build Order (milestones)

### Milestone 1 — Skeleton + determinism
- repo/crates scaffold
- core types (Bar, Order, Fill, Position, Portfolio)
- manifest + config ids + seed plumbing

### Milestone 2 — Execution MVP
- order book + stop/limit/market fills
- gap rules + deterministic intrabar policy
- trade tape output + basic metrics

### Milestone 3 — Composition + simple strategies
- alpha models (MA cross, Donchian breakout, momentum)
- order policies per family
- basic PM (fixed stop, ATR stop, chandelier)

### Milestone 4 — Runner + leaderboards
- sweeps + caching + persistence
- session/all-time leaderboards

### Milestone 5 — TUI v3
- panels + charts + theme tokens
- trade tape viewer + execution lab

### Milestone 6 — Robustness ladder
- walk-forward + regime tests + universe resampling
- execution sensitivity + optional path sampling for finalists

---

## 12) Definition of Done
- You can pick any leaderboard row and:
  1) reproduce it exactly from the manifest
  2) inspect trade tape and overlays to explain performance
  3) rerun under multiple execution presets and see stability
- You can compare:
  - same alpha across PMs
  - same alpha+PM across execution models
  - and rank them separately

---

## 13) Open Design Choices (we’ll decide early)
- Intent representation: exposure target vs typed entry/exit events
- Intrabar policy defaults: deterministic vs worst-case conservative
- Portfolio scope: single-symbol vs multi-symbol portfolio simulation in v1
- Fees model: per-share vs bps; borrow costs for shorts (optional)

---
