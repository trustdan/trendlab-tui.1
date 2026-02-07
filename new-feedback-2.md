# Gemini

This v4 plan is significantly stronger. You have successfully closed the "merge cliff" gap, addressed the concurrency architecture early, and added necessary integration checkpoints. The timeline is tight but far more realistic.

However, because the easy problems are solved, the remaining risks are subtler and more dangerous. You are now facing **complexity risks** rather than structural gaps.

Here is the harsh review of the v4 plan.

### 1. Architectural Blind Spots

**Issue: The "Missing Bar" Event Loop Ambiguity**

- **Where:** Phase 4 (Step 5 - Alignment) vs. Phase 5b (Step 1 - Event Loop).
- **Why:** Phase 4 dictates "NaN for missing bars" (aligned timeline). Phase 5b describes a 4-phase event loop. If `SPY` has data for today but `TSLA` does not (e.g., specific halt or data hole), what happens to the `TSLA` Position Manager?
  - If the PM *runs* on a `NaN` bar, it might compute invalid indicators or trigger false exits.
  - If the PM *skips* the bar, how do you handle multi-bar logic (e.g., "exit after 5 bars")? Does a missing bar count as time passing?
- **Suggestion:** In **Phase 5b**, explicitly define the **"Void Bar" Policy**. When the Event Loop encounters a `None/NaN` bar for a symbol:
  1. Equity is marked-to-market using the *previous* close (carry forward value).
  2. Pending orders (Stops/Limits) are **NOT** checked (market is closed for that symbol).
  3. The PM `on_bar` method is called with a distinct `MarketStatus::Closed` variant, allowing it to increment time counters without seeing a price update.

**Issue: Polars vs. Rayon Thread Contention**

- **Where:** Phase 10b (YOLO Mode), Step 3.
- **Why:** You mention a "Polars thread cap" and an "Outer thread cap." Polars uses its own thread pool; Rayon uses another. If you nest them (Rayon for symbols, Polars for indicators inside the symbol loop), you will oversaturate the CPU, causing excessive context switching and thrashing. A `cores * 2` thread load will actually be *slower* than single-threaded execution for this specific workload.
- **Suggestion:** In **Phase 10b**, enforce a **Mutual Exclusion Rule**: If `Outer Thread Cap > 1`, force `Polars Thread Cap = 1`. You should only use Polars parallelism when running a single backtest. When YOLO-ing 500 symbols, Polars must run sequentially per symbol.

### 2. Scope Realism

**Issue: The TUI "Chart Panel" Time Sink**

- **Where:** Phase 12 (TUI), Step 19 & 20.
- **Why:** You have allotted 3 weeks for the entire TUI. Building a robust caching system, a worker thread architecture, and 5 data-heavy panels is already a full load. Adding a terminal-based chart with "ghost curve overlays," "execution drag," and "side-by-side comparison" using `ratatui` primitives is a massive scope creep. Terminal graphing is finicky; getting axes, scaling, and resolution right across different terminal sizes will eat a week alone.
- **Suggestion:** Mark Step 19 & 20 as **Phase 12b (Optional)**. Define a "MVP Chart" that is simply a sparkline in the Leaderboard row. Do not block the release of the TUI on a complex charting widget.

**Issue: Math in the Critical Path (Again)**

- **Where:** Phase 10a (Runner), Step 3.
- **Why:** You are still implementing the entire library of performance metrics (Sortino, Calmar, Downside Deviation) inside the "Runner" phase. This creates a mental context-switching tax. You are debugging the file loader one minute and debugging the variance formula the next.
- **Suggestion:** Move **Step 3 (Metrics Implementation)** into **Phase 3 (Domain Model)** or **Phase 9 (Strategy Composition)**. Metrics are pure functions (`Vec<f64> -> f64`). They are library code. Implement and test them in isolation early so Phase 10a is purely about *wiring* them up.

### 3. Capability Risks

**Issue: Disk Fill during "Infinite" YOLO**

- **Where:** Phase 10c (History), Step 6.
- **Why:** You are appending every YOLO iteration to a JSONL file. If the engine is efficient, it might run 50,000 iterations overnight. A 500MB JSONL file is manageable, but if you include "full metrics" and "trade counts" for every failure and noisy random guess, this file will become a bloat liability and slow down the TUI history query.
- **Suggestion:** Implement a **Write Filter** in Phase 10c. Only persist runs that meet a minimum criteria (e.g., Positive CAGR *or* >5 trades). Do not log junk iterations to disk, only to the transient session memory.

**Issue: The "Yahoo Ban" Circuit Breaker**

- **Where:** Phase 4 (Data), Step 2.
- **Why:** The plan handles "network failure" and "rate limiting." It does not explicitly handle an **IP Ban** (HTTP 403 Forbidden). If Yahoo bans you during a massive 500-symbol fetch, the `retry` logic might just hammer them harder, extending the ban.
- **Suggestion:** In **Phase 4**, Step 2, add a **Circuit Breaker**. If the provider returns `403 Forbidden` or `429 Too Many Requests` (after retries), the entire `DataFetcher` system must **trip** and refuse all subsequent requests for the duration of the session (or a long cooldown), presenting a "Hard Stop" error to the user immediately.

### 4. Testing Gaps

**Issue: "Golden Test" Fragility**

- **Where:** Phase 11 (Robustness), Step 13.
- **Why:** You are using a "Real Data" golden test for walk-forward validation. Real data changes (Yahoo adjusts history for splits/dividends retroactively). If you download SPY data today, and Yahoo retroactively adjusts a dividend from 2021 next week, your "Golden Test" breaks, and you won't know if it's the code or the data.
- **Suggestion:** Phase 11 Golden Tests must use the **Frozen Parquet Fixture** created in Phase 4, NEVER a fresh download. Explicitly ban network access during the Phase 11 test suite.

### 5. Integration Risks

**Issue: The TOML Config "Stringly Typed" Trap**

- **Where:** Phase 9 (Strategy Composition), Step 4 & 7.
- **Why:** You are building a Factory that converts config strings ("RSI", "Aroon") to Rust types. If the TOML schema drifts from the Rust struct definitions, or if the Factory misses a match arm, the app panics at runtime.
- **Suggestion:** In **Phase 9**, add a **Iterative Factory Test**. Iterate over every single variant of the `SignalType`, `PMType`, etc., enums, generate a default config for it, serialize it to TOML, deserialize it back, and pass it through the Factory. This ensures 100% coverage of the config-to-runtime mapping.

### Summary of Critical Actions

1. **Phase 5b:** Define `MarketStatus::Closed` handling for aligned-but-missing bars.
2. **Phase 10b:** Enforce `Polars Threads = 1` when `Outer Threads > 1`.
3. **Phase 12:** Downgrade the "Chart Panel" to a stretch goal; it is the most likely feature to delay v3.
4. **Phase 3/9:** Pull "Metrics Math" forward out of the Runner phase.
5. **Phase 4:** Add a "Circuit Breaker" for HTTP 403s.

---

# Claude

Dan, this is a strong revision. You addressed all 18 issues from the review, and in several cases went beyond the suggestions with additions I didn't ask for but that are clearly correct. Let me do a systematic pass.

------

**All 18 original issues — status:**

Every P0, P1, P2, and P3 item from the review is resolved. Position sizing is explicitly deferred with a documented default (line 69). Phase 10 is split into 10a/10b/10c across three weeks. Look-ahead contamination is a project-wide invariant with a mandatory test. The FDR null hypothesis is specified. The track interface contract is explicit with column names and types. YOLO error recovery, TUI error display, configuration persistence, first-run UX, signal rejection traces, partial-fill accounting, bootstrap block length, walk-forward guardrails, hash stability, event loop timing, thread contention benchmarks, and survivorship bias documentation are all addressed in the right places.

------

**Improvements you added that I didn't suggest (all good):**

The RNG hierarchy with scheduling-order-independent sub-seeds (Phase 3, Step 7) and the corresponding YOLO determinism regression test (Phase 10b, Step 7) are excellent. Parallel backtesting with reproducible results is a hard problem and this nails the design constraint upfront. The `Send + Sync` compile-time bounds check in the Phase 3 gate (line 190) prevents a painful retrofit when the TUI worker arrives. The atomic cache writes with temp-file-then-rename (Phase 4, Step 6) prevent the "500-symbol batch download crashes at symbol 247" scenario. The Donchian signal clarification (Phase 9, line 428) correctly separates entry signals from exit logic. The signal metadata payload (Phase 3, Step 9) gives PMs context without breaking portfolio-agnosticism. The schema versioning on persisted artifacts (Phase 13, Step 1) prevents silent misreads across versions. The cooperative cancellation semantics for YOLO (Phase 12, Step 14) are well-specified. The Phase 4 scope note with priority ordering (line 206) is the kind of pragmatic planning that prevents a 2-week phase from blocking everything.

------

**Three remaining items worth flagging — none are blockers, all are "nice to catch now":**

**1. The degradation ratio edge case handling (Phase 11, Step 3) introduces a branch that could confuse the FDR pipeline.** When IS Sharpe < 0.1, you switch to a difference-based metric (OOS − IS) instead of the ratio. But the FDR correction in Step 4 uses a t-test on fold-level OOS Sharpe values, not on the degradation ratio. So the degradation ratio edge case and the FDR pipeline are actually independent — the ratio is a diagnostic, the t-test is the hypothesis test. This is fine architecturally, but the plan reads as though they're coupled. A one-sentence clarification in Step 4 noting that FDR operates on the raw OOS Sharpe values (not the degradation ratio) would prevent an implementer from accidentally feeding the ratio into the t-test.

**2. The cross-symbol leaderboard (Phase 10c, Step 1) now has a lot of conditional logic.** Trimmed geometric mean, catastrophic threshold flags, log-return aggregation for negative returns, minimum sample sizes for tail metrics. This is all correct, but it's pushing Phase 10c toward a scope that's tight for one week alongside fingerprinting, JSONL history, dual-scope, and risk profiles. Not a schedule bomb — more like a "this is the phase where you'll be tempted to cut corners on edge case handling, don't."

**3. The welcome overlay (Phase 12, Step 4) says it's "saved as dismissed in the state file."** But the state file is built in Step 5, which comes after Step 4. Minor ordering issue — just make sure the state persistence infrastructure exists before the overlay tries to write to it. In practice you'd build Steps 4 and 5 together, but if someone reads the steps linearly they might build the overlay before the state file exists.

------

**Overall verdict:** This plan is ready to build from. The 20–22 week estimate is honest. The invariants are testable. The escape hatches are explicit about what's deferred and why. The Phase 10 split removes the biggest schedule risk. The statistical validation pipeline is now well-specified enough that an implementer can build it without having to make research decisions at code time. Ship it.

---

# ChatGPT

I’ll review your build plan like a code/design review: what’s strong, what’s missing or risky, and the top changes that would make execution (and later YOLO results) more reliable and faster to ship.

Overall: **much better on the substance** (checkpoint A.5, stricter contracts, and the Phase 11 ladder are the right direction)… but the document as-is still contains **contradictory copies of whole phases** and **a couple of “silent fake data” landmines**. I’d fix those *before* anyone starts building.

Below are the issues I’d flag in a harsh senior-architect review. For each one: what / where / why / what to change (without adding code).

------

## Blocking doc integrity problems

### Issue 1 — Duplicate merge point + duplicate Phase 10/11/12/13/14 sections

- **Where:** Two-track structure appears twice with different merge points (Phase 10 vs Phase 10a).
  Phase 11 appears twice (Week 12 vs Weeks 13–14).
  Phase 12 appears twice (Weeks 15–17 vs Weeks 13–15).
  Phase 13/14 also duplicated with conflicting weeks.
- **Why it matters:** Implementers will pick different “truths,” and you’ll get **unmergeable work** and **missed dependencies** that look like “people problems” but are actually spec ambiguity.
- **Suggestion:** Pick **one canonical timeline + one canonical phase list**. Delete the obsolete copies. Add a single “Phase → Week(s) → Gate” index at the top so the plan is self-consistent.

### Issue 2 — Version naming mismatch (“v3 done” inside a v4 plan)

- **Where:** “TrendLab v3 — Phased Development Plan” + “You are ‘v3 done’ when…”
- **Why it matters:** This creates immediate stakeholder confusion (“are we building v3 or v4?”) and makes acceptance criteria slippery.
- **Suggestion:** Rename all “v3” references to “v4” (or remove version labels entirely) and ensure DoD uses the same version as the file.

------

## Sequencing risks

### Issue 3 — You still have an *auto-synthetic fallback* in some copies

- **Where:** One Phase 10 copy says “if download fails, use synthetic bars with warning.”
  One Phase 5a copy says “If no network access, generate synthetic bars and warn.”
  But elsewhere you require explicit `--synthetic` (good).
- **Why it matters:** This violates your own “no fake data by default” stance and will **poison trust in every result**. A warning in a fast TUI is not enough—people will miss it.
- **Suggestion:** Make the rule absolute: **no synthetic unless explicitly requested**. On failure: show an empty state + actionable error, and stop. Then remove the contradictory copies.

### Issue 4 — Missing-bar policy is not pinned (contract vs “choose forward-fill”)

- **Where:** Track contract says strict NaN / no forward-fill.
  Phase 4 step still says “Define policy… forward-fill vs strict NaN.”
- **Why it matters:** Forward-fill can create **fake continuity**, distort indicators, and create stealth look-ahead-like artifacts. Strict NaN is safer—but only if downstream behavior is defined.
- **Suggestion:** Remove the choice. Declare one policy (strict NaN) and add one global downstream rule: “invalid bar → invalid indicators → no signal/trade for that symbol at that time.”

### Issue 5 — Corporate actions are underspecified (“adj_close exists” isn’t enough)

- **Where:** Schema includes `adj_close`.
  Phase 4 says “Use adjusted close for split/dividend adjustment; store adjustment metadata.”
- **Why it matters:** If indicators use high/low/close but only close is adjusted, you’ll get **nonsense ATR/volatility**, broken breakouts, and PnL inconsistencies around splits/dividends.
- **Suggestion:** State explicitly which price series drives **all** calculations and fills (raw vs adjusted), and require a consistency test around a known split date.

------

## Scope realism

### Issue 6 — The plan simultaneously claims ~17 weeks and admits ~20–22 weeks solo

- **Where:** Timeline estimate: “20–22 weeks solo… 17–18 weeks with 2-person team.”
- **Why it matters:** A schedule that pretends to be shorter will cause rushed merges and “we’ll test later” debt—exactly what makes backtest engines untrustworthy.
- **Suggestion:** Make the canonical schedule match the estimate. If you still want 17 weeks, you need an explicit **cut list** (not “hope”).

### Issue 7 — DoD requires a massive component surface area early

- **Where:** DoD requires “ten signals, nine PMs, four execution models, four filters.”
- **Why it matters:** This is where solo schedules go to die—because every added component multiplies test matrix size and integration complexity.
- **Suggestion:** Keep the “must exist” stance, but introduce *gated tiers*:
  - Tier 0: minimal subset required to validate pipeline+engine+YOLO correctness
  - Tier 1: full catalog required for “done”
    This keeps scope without lying about time.

### Issue 8 — “Clone → first real YOLO run in under 5 minutes” is not aligned with real-data download

- **Where:** DoD #1 requires real downloads from Yahoo Finance; DoD #12 says under 5 minutes.
- **Why it matters:** First-run experience will feel broken or misleading, especially on slow networks or throttled data sources.
- **Suggestion:** Define Quick Start as “tiny default universe + short date range” (or pre-packaged minimal cache), with a clear path to “full universe” that can take longer.

### Issue 9 — Phase 11 stats is still likely under-budgeted

- **Where:** Phase 11 includes walk-forward, BH/FDR, execution Monte Carlo, bootstrap confidence grades, and golden tests.
- **Why it matters:** This is the “make it real” layer; it will take longer than you think, and if it slips it drags TUI and reporting.
- **Suggestion:** Split Phase 11 into two gates:
  1. Walk-forward + multiple-comparison correction + guardrails
  2. Bootstrap confidence + stability scoring + tests/goldens

------

## Missing capabilities and edge cases

### Issue 10 — Cache integrity is tested, but “atomic + quarantine” isn’t mandated

- **Where:** You test partial download corruption scenarios.
- **Why it matters:** Partial/corrupt Parquet can create cascading NaNs and “valid-looking but wrong” results.
- **Suggestion:** Add a hard requirement: writes must be atomic, loads must validate integrity, and corrupt cache files must be quarantined (not repeatedly re-used).

### Issue 11 — NaN propagation rules are implied, not specified

- **Where:** Strict NaN missing-bar policy is stated, but downstream behavior isn’t.
- **Why it matters:** One NaN can explode across indicator columns and create silent “zero-trade” backtests that look like strategy failure (or worse, look like success if mishandled).
- **Suggestion:** Add a global invariant: “invalid input never generates a trade; invalid-rate is tracked and surfaced in results.”

### Issue 12 — Component compatibility constraints are implied but not enforced

- **Where:** “Execution models determine order type… breakout pairs with stop entries…”
  YOLO structural exploration explicitly mixes components.
- **Why it matters:** YOLO will generate junk combos that dominate leaderboards due to artifacts, not genuine edge.
- **Suggestion:** Add a “compatibility rules” section: either constrain sampling or mark combos invalid and exclude them from leaderboards/robustness ladder.

### Issue 13 — Threading model can collapse under nested parallelism

- **Where:** TUI worker uses Rayon + Polars threading; YOLO config exposes both caps.
- **Why it matters:** Oversubscription causes performance cliffs, nondeterminism, and makes “reproducible runs” harder in practice.
- **Suggestion:** Add a “threading invariants + stress test” deliverable before Phase 10c: demonstrate stable throughput and determinism across cap settings.

------

## Testing gaps and testability risks

### Issue 14 — Exact-value golden tests for statistical layers are brittle

- **Where:** Phase 11 demands exact degradation ratio, fold Sharpe values, and BH-adjusted p-values.
- **Why it matters:** Tiny numeric differences across platforms/changes will create constant “fix tests vs fix math” churn, and people will start ignoring failures.
- **Suggestion:** Keep exact-value goldens only for small deterministic fixtures; for stats, assert *semantic invariants* (pass/fail on known overfit vs generalizing cases, monotonicity, threshold behavior).

### Issue 15 — “YOLO produces distinct entries after 10 iterations” can be flaky

- **Where:** Phase 10c integration verification expects distinct leaderboard entries quickly.
- **Why it matters:** Valid implementations can still hit low-trade regimes or dedup collisions depending on parameters; you’ll get nondeterministic CI failures.
- **Suggestion:** Define a deterministic “test universe + test sweep ranges” designed to guarantee variation and trades, separate from exploratory defaults.

------

## Integration risks at the track merge

### Issue 16 — The merge boundary is still too underspecified

- **Where:** Track interface contract defines DataFrame/Parquet invariants.
  But the plan’s merge point is inconsistent (10 vs 10a), and you don’t explicitly define the adapter behavior (ordering guarantees, per-symbol vs per-date iteration, NaN handling in the engine).
- **Why it matters:** This is the classic “both sides finished, but nothing works together” moment. You’ll lose a week here even on a good team.
- **Suggestion:** Add a pre-merge deliverable: a written “data adapter contract” (ordering, gaps, types, and performance expectations) and force it to pass an end-to-end test before building YOLO/TUI polish.

------

## Architectural blind spots

### Issue 17 — “Signal event includes filter accept/reject trace” violates separation

- **Where:** Phase 3 adds a status field on the signal event type to store filter trace.
- **Why it matters:** You’re pushing downstream concerns back upstream, which creates awkward mutability/ownership pressure and undermines the clean separation you’re trying to enforce.
- **Suggestion:** Keep signals pure; store accept/reject reasoning in a separate evaluation record attached to trades and leaderboard entries.

### Issue 18 — Composite “risk profile” scoring lacks normalization semantics

- **Where:** Risk profiles weight metrics differently for a composite score.
- **Why it matters:** Weighting raw metrics without normalization produces unstable, often meaningless rankings (scale differences dominate).
- **Suggestion:** Require a written normalization policy (rank-based or bounded transforms) and a test that rankings are stable under unit/scale changes.

------

## UX completeness problems

### Issue 19 — Error visibility is promised, but UX is “status bar only”

- **Where:** DoD says errors are displayed in the TUI status bar.
- **Why it matters:** Users will miss transient errors during long runs, and you’ll have no post-mortem trail.
- **Suggestion:** Without adding panels, add an “error history overlay” reachable from Help, showing recent warnings/errors with timestamps and suggested actions.

### Issue 20 — Settings persistence is demanded; make it a named deliverable, not just DoD text

- **Where:** DoD item 13 requires restart persistence of multiple UI settings.
- **Why it matters:** This is the difference between “toy” and “tool.” If it slips, the TUI will feel unfinished even if the engine is great.
- **Suggestion:** Make “UI state persistence” a concrete Phase 10c/12 deliverable with explicit scope (what’s persisted) and safety (crash-safe writes).

------

## Statistical validity pitfalls

### Issue 21 — Walk-forward inference uses a t-test on fold-level Sharpe; that’s shaky

- **Where:** Phase 11 specifies fold-level OOS Sharpe values → one-sided t-test → BH correction.
- **Why it matters:** Sharpe estimates are noisy and non-normal; folds aren’t independent; K is small. This tends to create **overconfident p-values**, and BH will then “correct” garbage.
- **Suggestion:** Either (a) clearly label this as a heuristic ranking score (not a literal p-value), or (b) base inference on something more statistically defensible (returns-level resampling/permutation). Add a null-simulation sanity check to validate false-positive behavior.

### Issue 22 — Minimum OOS fold length (one quarter) is too short for stable Sharpe-based decisions

- **Where:** Guardrails allow 63-bar OOS folds.
- **Why it matters:** You’ll promote/kill strategies due to randomness. Overfit vs generalize becomes indistinguishable.
- **Suggestion:** Increase minimum OOS fold length or require more folds; when insufficient, label as “insufficient evidence” instead of producing a pass/fail.

### Issue 23 — Cross-symbol confidence bootstraps “mean per-symbol Sharpe,” ignoring dependence

- **Where:** Phase 11 step 10.
- **Why it matters:** If symbols co-move (they do), your CI will be too narrow and confidence grades will lie.
- **Suggestion:** Prefer bootstrap on the aggregated equity curve (portfolio-level returns) as the primary confidence estimate; keep per-symbol as secondary diagnostic.

------

## Bottom line

You’ve fixed a lot of the “missing seriousness” (promotion ladder, FDR, stickiness integration, explicit thread caps, real-data ethos). The **two biggest remaining hazards** are:

1. **Document inconsistency / duplicated sections** (will derail execution), and
2. **Any remaining path that can silently fall back to synthetic data** (will destroy trust).

If you clean those up and lock down missing-bar + corporate-action semantics, this plan becomes something I’d actually let a team execute without constant architect babysitting.