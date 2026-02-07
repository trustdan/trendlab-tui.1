# Architecture Invariants

These rules are non-negotiable. Every module, every phase, every PR must respect them. If a change violates an invariant, the change is wrong — not the invariant.

---

## 1. Separation of Concerns

**Signals must never see portfolio state.**
A signal generator receives bar history and produces signal events. It must not know whether a position is open, what the current equity is, or what orders are pending. Signal output must be identical regardless of portfolio state.

**Position management must never influence signal generation.**
The PM operates after fills, on existing positions. It emits order intents (cancel/replace) for the next bar. It never feeds back into the signal pipeline.

**Execution must be configurable and realistic, not hardcoded.**
Execution parameters (slippage, spread, path policy, gap rules) are explicit configuration, not baked-in assumptions. Named presets bundle these for convenience but every parameter is inspectable and overridable.

### Canonical pipeline flow

```
Signals → Signal Filter → Order Policy → Order Book → Execution Model → Portfolio → Position Manager
```

Each stage has a single responsibility and a clean interface boundary.

---

## 2. Four-Component Composition Model

Every strategy is composed of exactly four independent components:

| Component | Responsibility | Cannot access |
|---|---|---|
| **Signal generator** | Detects market events, emits directional intent | Portfolio state, position state |
| **Position manager** | Manages open positions, emits exit/adjustment intents | Signal pipeline, other symbols |
| **Execution model** | Determines order type and fill simulation rules | Signal logic, PM logic |
| **Signal filter** | Gates entry signals based on market conditions | Portfolio state |

Components are independently swappable. Any signal can pair with any PM, execution model, and filter (subject to compatibility constraints documented in the factory system).

---

## 3. Bar Event Loop Contract

The engine runs a deterministic bar-by-bar event loop with four phases per bar:

1. **Start-of-bar:** Activate day orders, fill market-on-open (MOO) orders at the open price.
2. **Intrabar:** Simulate trigger checks for stop and limit orders using the configured path policy (Deterministic, WorstCase, or BestCase). Fill triggered orders.
3. **End-of-bar:** Fill market-on-close (MOC) orders and close-on-signal orders at the close price.
4. **Post-bar:** Mark all positions to market at the close price. Compute equity. Then let the position manager emit maintenance orders for the **next** bar (never the current bar).

**Critical timing:** Post-bar PM processing must see all fills from all prior phases of the current bar, including end-of-bar fills. The order book must be updated with end-of-bar fills *before* the PM runs. If a close-on-signal fill exits a position at end-of-bar, the PM must NOT emit a maintenance order for that now-closed position.

---

## 4. Decision / Placement / Fill Timeline

This is the canonical timing contract for the entire project:

- Signals evaluated at bar T's close may only use data **up to and including bar T**.
- Orders generated from those signals execute on **bar T+1** (next-bar-open by default).
- No order may execute on the same bar whose data generated the signal, unless using explicit intrabar logic (deferred).

Violations of this timeline constitute look-ahead bias and are bugs.

---

## 5. Look-Ahead Contamination Guard

No indicator value at bar t may depend on price data from bar t+1 or later.

**Canonical test:** Compute the indicator on a truncated series (bars 1–100) and on the full series (bars 1–200). Assert that bars 1–100 produce identical values in both cases. If they differ, there is look-ahead contamination.

This test is mandatory for every indicator and every signal. It must pass before any phase gate.

---

## 6. NaN Propagation Guard

Invalid or NaN input must never generate a trade.

- If a bar contains NaN in any OHLCV field, indicators computed from that bar produce NaN.
- Signals produce no signal event for that bar.
- The event loop skips order checks for that symbol on that bar.
- The invalid-bar rate (fraction of bars with NaN for each symbol) is tracked per-run.
- If the invalid-bar rate exceeds 10% for any symbol, the result is flagged with a data quality warning.

---

## 7. Void Bar Policy

When the event loop encounters a NaN/missing bar for a symbol (all OHLCV fields NaN):

- Market status is `MarketStatus::Closed` for that symbol on that bar.
- Equity is marked-to-market using the previous bar's close (carry forward). No PnL change recorded.
- Pending orders (stops, limits, brackets) are NOT checked against NaN prices. They remain pending.
- The PM receives `MarketStatus::Closed` and may increment time-based counters but must NOT emit price-dependent order intents.
- Time-based exits (e.g., max holding period) that expire during void bars emit on the **next valid bar**, not on the void bar itself.
- Indicators produce NaN. Signals are not evaluated.

---

## 8. Ratchet Invariant

A stop may tighten (move closer to current price on winning trades) but may **never** loosen (move further away), even if volatility expands.

- For long positions: stop level is monotonically non-decreasing.
- For short positions: stop level is monotonically non-increasing.
- A gap through the stop does not cause the stop to widen — the position exits at the gap-open price per gap rules.
- Volatility expansion after a gap does not loosen the ratchet.

---

## 9. Deterministic RNG Hierarchy

A master seed generates deterministic sub-seeds for each `(run_id, symbol, iteration)` tuple. Sub-seeds are derived independently of thread scheduling order, so results are identical regardless of thread count or execution order.

**Canonical test:** Run the same YOLO sweep with a fixed seed and fixed iteration count, once with 1 thread and once with 8 threads. Assert identical config hashes, metric values, and leaderboard ordering.

---

## 10. Synthetic Data Policy

Synthetic bars are a developer-only debug mode requiring an explicit `--synthetic` flag. Results produced on synthetic data are tagged as synthetic and cannot enter the all-time leaderboard. The system must never silently substitute synthetic data for real data. If data is unavailable and `--synthetic` is not set, the operation fails with a clear error message.
