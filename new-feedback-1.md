# Gemini

Here is a critical review of the TrendLab v3 development plan.

### 1. Sequencing & Integration Risks

**Issue: Late Introduction of Concurrency Constraints**

- **Where:** Phase 12 (TUI), Step 1 (Background worker thread).
- **Why:** You are introducing a threaded architecture (UI thread + Worker thread) in Week 13. If the core domain types (Phase 3) and the Event Loop (Phase 5b) are not designed to be `Send` + `Sync` and thread-safe from the start, retrofitting the engine to run on a background thread while communicating via channels will require a massive refactor of the core engine. You cannot assume a single-threaded mutable borrow model works inside a Rayon/Polars environment without explicit design.
- **Suggestion:** Move the requirement for `Send` + `Sync` compliance and thread-safe design patterns into **Phase 3 (Domain Model)**. Explicitly test that the engine state can be moved across thread boundaries before building the single-threaded event loop.

**Issue: The Phase 10 "Merge Cliff"**

- **Where:** Phase 10 (Runner + CLI + YOLO + Leaderboards).
- **Why:** This phase is a death march. You are attempting to integrate the data pipeline, implement complex trade extraction, write the entire library of performance metrics (Sharpe, Sortino, etc.), build the CLI, build the YOLO auto-discovery logic, *and* build the persistence layer for leaderboards all in 2 weeks. This is the highest risk point in the plan and will likely stall progress for a month.
- **Suggestion:** Split Phase 10 into two distinct phases:
  - **Phase 10a (The Runner & Metrics):** Focus strictly on loading real data, running a backtest, and computing/validating the math for performance metrics.
  - **Phase 10b (The YOLO Engine):** Build the loop, the sliders, and the leaderboard aggregation on top of a working runner.

**Issue: Integration Checkpoint B is Too Late**

- **Where:** Checkpoint B (after Phase 10).
- **Why:** You don't verify that the engine produces realistic trades on *real* multi-symbol data until Week 11. If there is a fundamental mismatch between the Yahoo Finance data shape (Phase 4) and the Engine's consumption expectations (Phase 5b), you won't find it until the tracks merge.
- **Suggestion:** Add a **Checkpoint A.5 (after Phase 7)**. Create a "mini-runner" integration test that forces the Phase 7 execution engine to consume the Phase 4 Parquet data. Do not wait for the full Runner in Phase 10 to prove the data types are compatible.

### 2. Scope Realism

**Issue: Underestimated TUI Complexity**

- **Where:** Phase 12 (TUI).
- **Why:** Building a six-panel TUI with vim navigation, custom tree widgets (Data panel), robust state management, chart rendering with drawing primitives, and a responsive background worker thread is not a 3-week task for a solo developer. The "Chart" panel alone, with ghost curves and overlays, is a significant graphical challenge in a terminal environment.
- **Suggestion:** Defer the "Chart" panel (Panel 5) to a post-v1 update or simplify it to a basic ASCII sparkline. Focus the 3 weeks on the complex state management required for the Data and Results panels.

**Issue: Performance Metrics "From First Principles"**

- **Where:** Phase 10, Step 3.
- **Why:** Implementing CAGR, Sortino, and Max Drawdown "from first principles" with proper edge-case handling (div by zero, all-negative returns, short histories) is tedious and prone to subtle bugs. Doing this alongside building the YOLO engine is unrealistic.
- **Suggestion:** Pull the "Performance Metrics" implementation forward into **Phase 9 (Strategy Composition)** or make it a standalone mini-phase. These are pure math functions and do not need the full runner to be tested.

### 3. Missing Capabilities

**Issue: No App State Persistence**

- **Where:** Phase 12 (TUI) / General.
- **Why:** The plan mentions persisting leaderboards, but ignores user preferences. If a user spends 5 minutes selecting specific tickers in the Data panel and tuning YOLO sliders, does that state survive a restart? If not, the tool is frustrating to use.
- **Suggestion:** Add a specific step in **Phase 12** for "App State Persistence." Serialize the current TUI state (selected tickers, active panel, YOLO settings, current risk profile) to a local JSON file on exit and restore it on launch.

**Issue: Metadata Context for Position Managers**

- **Where:** Phase 3 (Domain Model) / Phase 8 (Position Management).
- **Why:** Strict separation of concerns is good, but Position Managers often need context from the Signal. Example: A "Breakout" signal fires. The Position Manager needs to know the "breakout level" (the price that triggered the signal) to set an initial stop loss below it. If the Signal only emits `Direction::Long`, the PM is flying blind and cannot implement logic like "stop loss at signal bar low."
- **Suggestion:** In **Phase 3**, the `Signal` event must carry a generic `metadata` payload (e.g., a HashMap or struct of key levels) that is passed to the `PositionManager`. This must be architected before the components are built.

**Issue: Error Recovery in Data Fetching**

- **Where:** Phase 4 (Data Ingest), Step 7.
- **Why:** The plan mentions "simulating network failure," but doesn't specify how the system recovers during a bulk fetch. If I select 500 tickers and ticker #250 crashes the connection, do I lose the progress on the first 249? Does the cache state update atomically per file or only after the batch?
- **Suggestion:** Explicitly require **per-symbol commit** in Phase 4. If the process dies halfway through a batch download, the next run should detect the existing cache files and only fetch the remainder.

### 4. Testing Gaps

**Issue: Deterministic Concurrency Testing**

- **Where:** Phase 10 (YOLO Mode) / Phase 3 (Domain Model).
- **Why:** You claim "reproducible" results as a core requirement. However, YOLO mode uses Rayon (parallel iterator) and Polars threads. Floating point operations are not associative; the order of summation matters. If thread execution order varies, results might drift slightly. Furthermore, sharing a single RNG across threads without proper forking/seeding per-thread will break reproducibility.
- **Suggestion:** In **Phase 3**, explicitly design the RNG hierarchy: A master seed generates sub-seeds for every symbol-level task. In **Phase 10**, add a strict regression test: Run the same YOLO sweep twice with the same seed on different thread counts (e.g., 1 thread vs 8 threads) and assert identical results.

**Issue: Memory Leaks in Long-Running YOLO**

- **Where:** Phase 10 (YOLO Mode).
- **Why:** The YOLO mode runs "indefinitely." Rust prevents memory leaks in safe code, but logical leaks (growing vectors of results, un-flushed history buffers) are common in backtesting loops.
- **Suggestion:** Add a "Soak Test" in **Phase 10**: Run the YOLO loop for 10,000 iterations in a constrained memory environment (e.g., using `ulimit` or docker) to verify memory usage stabilizes and doesn't grow linearly.

### 5. Statistical Validity

**Issue: Insufficient Data for Walk-Forward/Bootstrap**

- **Where:** Phase 11 (Robustness), Steps 3 & 9.
- **Why:** You cannot perform meaningful walk-forward analysis or block bootstrapping on short timeframes. If a user downloads 1 year of data, a 5-fold walk-forward split leaves statistically insignificant sample sizes for testing. The plan doesn't guard against this.
- **Suggestion:** Add a **Data Quantity Gate** in Phase 11. The robustness engine must reject or warn on datasets shorter than a minimum threshold (e.g., 3 years for walk-forward, 252 bars for bootstrap). Do not calculate junk stats on insufficient data.

**Issue: Multiple Comparison Bias in "Best of" Selection**

- **Where:** Phase 10 (Leaderboards).
- **Why:** The leaderboard ranks strategies by Sharpe. The "Winner's Curse" implies the top strategy is likely overfit, even with walk-forward validation, simply because you tested thousands of variations. While Phase 11 adds FDR, the UI in Phase 10 presents the "best" raw results immediately. This encourages users to trust optimization bias before the robustness checks in Phase 11 run.
- **Suggestion:** In **Phase 12 (TUI)**, visually differentiate "Raw" results from "Validated" results. The Leaderboard should default to showing a "Validation Pending" state or clearly mark raw high scores as "Unverified" until Phase 11's checks have passed.

### 6. Architectural Blind Spots

**Issue: Tick Rounding and "Price Magnets"**

- **Where:** Phase 7 (Execution Engine).
- **Why:** The plan mentions tick rounding, but doesn't account for psychological pricing or "magnets" in execution. In real markets, limit orders at round numbers ($100.00) execute differently than at $100.03. While complex to model, the architecture should allow for "Execution Adjusters" that can modify fill probabilities based on price levels.
- **Suggestion:** This is likely out of scope for v3, but ensure the `ExecutionModel` trait (Phase 3) accepts the *full* order context so this logic can be added later without breaking the API.

**Issue: The "Start-of-Bar" vs "End-of-Bar" Ambiguity**

- **Where:** Phase 5b (Event Loop).
- **Why:** The loop has four phases. However, some indicators (like moving averages) are calculated on the *Close*. If a signal fires based on the Close of Bar T, and the execution happens at Open of Bar T+1, that is correct. But if the signal logic (Phase 9) isn't strictly careful, it might peek at Bar T's Close to try and enter *during* Bar T.
- **Suggestion:** Enforce **Lookahead Prevention** in the `Signal` trait in Phase 3. The `Signal` trait should strictly accept `History` up to T-1 (or T if calculating strictly at close for next-open execution). The API must make it impossible to execute on the same bar that generates the signal unless utilizing explicit "Intrabar" logic (which is deferred).

---

# ChatGPT

Got it — I’m going to read your plan end-to-end and then give you a ruthless review organized by sequencing, scope, missing capabilities, testability, merge risks, architectural blind spots, UX gaps, and statistical validity, with concrete fixes tied to phase+step references.

Below is a deliberately harsh review. I’m going to focus on things that will *actually* break you around Weeks 6–12 if you don’t correct them now.

------

## Sequencing risks (dependencies, split/merge recommendations)

### 1) Multi-symbol alignment is premature and a hidden design fork

- **Issue:** You commit to a “multi-symbol time alignment system” early, before the engine’s bar semantics and per-symbol execution realities are fully proven. This will force you to choose between (a) imputing bars (forward-fill) or (b) introducing NaNs that poison indicators and fill logic.
- **Where:** Phase 4, Step 5.
- **Why it matters:** This isn’t a “data pipeline” decision; it’s an engine semantics decision. If you get it wrong, you’ll debug “weird fills / weird indicators” for weeks and you won’t know whether it’s data or execution.
- **Suggestion:** Re-scope Phase 4 Step 5 into two sub-decisions **without implementing both**:
  - **Default**: per-symbol independent series (no alignment), since your engine runs per symbol anyway.
  - **Later**: cross-symbol alignment only for *reporting aggregation*, with an explicit “missing bar behavior” contract (and a clear rule that alignment must not fabricate tradable OHLC bars).

### 2) Synthetic fallback undermines your own “no fake data” doctrine

- **Issue:** You repeatedly allow synthetic bars as a fallback, even though your “done” description explicitly says “No synthetic data.”
- **Where:** Phase 5a, Step 2. + “No synthetic data” definition in “What done looks like”.
- **Why it matters:** This will **pollute leaderboards**, **invalidate metrics**, and make it extremely easy for implementers to accidentally “pass tests” on synthetic and feel done. Your own plan then has to defend against your own escape hatch everywhere (runner, YOLO, TUI).
- **Suggestion:** Make synthetic bars a **developer-only explicit mode** that cannot run unless the user opts in via config/flag *and* results are tagged “synthetic” so they cannot enter all-time persistence.

### 3) Signal timing / decision semantics aren’t nailed down early enough

- **Issue:** The event loop is specified (orders apply to NEXT bar), but you never explicitly define *when* signals are evaluated and *what data they are allowed to use* relative to order placement.
- **Where:** Signal contract in Phase 3, Step 8. and event loop timing in Phase 5b, Step 1.
- **Why it matters:** Without an explicit “decision timestamp,” implementers will accidentally introduce **look-ahead bias** (especially with “close-on-signal” and breakout-style entries), or they’ll shove responsibilities into the wrong component to “make it work.”
- **Suggestion:** Expand the Phase 1 “architecture invariants” doc to include a *single, rigid* “decision/placement/fill timeline” for daily bars (what’s known at close, what can be placed for next open, what is intrabar-simulated). You don’t need code—just non-negotiable rules.

### 4) “Vectorized indicator precompute” will break on path-dependent indicators

- **Issue:** You require Polars precompute for all indicators, but some listed indicators are inherently sequential/stateful (and easy to get subtly wrong if forced into vectorized form).
- **Where:** Phase 5b, Steps 4–5.
- **Why it matters:** You’ll either (a) implement incorrect versions that “look plausible,” or (b) burn a week fighting the framework instead of building the engine.
- **Suggestion:** Clarify in Phase 5b that “precompute” is **a performance optimization**, not a religious rule: allow a small subset of indicators to be sequentially computed in a controlled way while preserving the “no recompute per bar” goal.

------

## Scope realism (what’s actually 2–4 weeks disguised as 1)

### 5) Phase 4 is not a one-week phase

- **Issue:** Rate limiting + retries + validation + canonicalization + anomaly detection + alignment + Parquet cache + incremental updates + CLI UX + integration tests + frozen fixtures is **several subsystems**.
- **Where:** Phase 4, Steps 2–11.
- **Why it matters:** If Phase 4 overruns (it will), Track B either stalls or continues with placeholders that create merge debt at Phase 10.
- **Suggestion:** Split Phase 4 into “Download + Cache + Fixture” and “Validation + Alignment + Universe UX,” or explicitly allow the first week to ship a narrower pipeline (download → parquet → fixture) and defer the fancy ingest pipeline until after Checkpoint 0 is real.

### 6) Phase 5b is too big for one week (especially with *hand-verified* indicator tests)

- **Issue:** Full event loop + warmup + equity accounting + **13 indicators** + precompute + correctness tests is aggressive; “hand-calculated expected values” for complex indicators is a time sink.
- **Where:** Phase 5b, Steps 1–8.
- **Why it matters:** You’ll cut corners on tests to hit the gate, and then later phases will become “debug the foundations while building the roof.”
- **Suggestion:** Keep the indicator list, but revise the testing expectation: for complex indicators, test against **external reference outputs** (not hand math). Still no code in the plan—just change the acceptance criteria.

### 7) Phase 10 is a multi-week monster disguised as “Weeks 10–11”

- **Issue:** You combine: runner orchestration, trade extraction, full metrics suite, CLI, YOLO engine, dual sliders, progress reporting, two leaderboards, risk profiles, history persistence, and integration tests.
- **Where:** Phase 10, Steps 1–18.
- **Why it matters:** This is where plans go to die. You’ll ship a half-working YOLO + half-working metrics + half-working persistence and spend Weeks 12–15 stabilizing instead of building robustness/TUI.
- **Suggestion:** Split Phase 10 into three phases:
  1. Runner + trade extraction + minimal metrics + CLI run (no YOLO)
  2. YOLO engine + progress + per-symbol leaderboard (no cross-symbol/risk profiles/history analytics yet)
  3. Cross-symbol leaderboard + risk profiles + persistence + history querying

### 8) Phase 11 (“Robustness”) is not a one-week add-on

- **Issue:** Walk-forward + FDR correction + execution Monte Carlo + stability scoring + stationary bootstrap CI grading + tests is not Week 12; it’s Weeks 12–14 by itself.
- **Where:** Phase 11, Steps 1–12.
- **Why it matters:** If you try to do this fast, you’ll implement “statistical theater” that produces confidence grades that aren’t defensible.
- **Suggestion:** Defer FDR *or* bootstrap grading initially—ship one strong robustness layer first, then add the second once you trust the first.

**Bottom line on schedule realism:** 17 weeks is *not* realistic for a solo developer unless you accept major scope cuts in Phase 10–12. For a small team, it’s plausible only if you explicitly staff: (A) data pipeline owner, (B) engine owner, (C) TUI owner—otherwise it collapses into serial work.

------

## Missing capabilities (things you will need but aren’t actually planned)

### 9) Position sizing exists in prose but not in the architecture

- **Issue:** You reference “position sizers determine quantity,” but there is no sizing component in the four-part composition model and no phase that designs/implements sizing rules.
- **Where:** Phase 9, Step 8. and component list in Phase 3, Step 8.
- **Why it matters:** Implementers will smuggle sizing into execution or PM, destroying separation of concerns and making YOLO comparisons inconsistent.
- **Suggestion:** Decide *now* whether sizing is:
  - embedded into PM as part of “order intent,” **or**
  - a fifth component (“Sizer”) that sits between signal/filter and PM
    (You can keep the plan code-free; just clarify responsibility and add it to the invariants.)

### 10) Corporate actions are hand-waved; OHLC adjustment consistency is not guaranteed

- **Issue:** “Use adjusted close” is not enough to guarantee adjusted OHLCV consistency.
- **Where:** Phase 4, Step 3.
- **Why it matters:** You’ll get phantom gaps and broken indicators around splits/dividends. Those bugs are *nasty* because they look like “strategy volatility,” not “data wrong.”
- **Suggestion:** Add an explicit requirement: store adjustment factors and ensure the cached OHLCV series is internally consistent (and test with at least one split-heavy symbol fixture).

### 11) Persistence/versioning strategy for saved artifacts is missing

- **Issue:** You persist JSON results, JSONL histories, and an all-time leaderboard, but you never define schema versioning, migration, or corruption resistance.
- **Where:** Phase 10, Steps 12 and 16. and Phase 13 persistence expectations.
- **Why it matters:** One format change mid-project will brick old results, or worse, silently misread them and corrupt leaderboards.
- **Suggestion:** Require version tags on persisted artifacts and a “refuse to load unknown version” behavior (with a clear user message) until migration support exists.

### 12) Determinism under parallelism is not guaranteed (seeds ≠ determinism)

- **Issue:** You seed stochastic components, but later you explicitly parallelize symbol runs and rely on threading caps. Parallel scheduling can change RNG consumption order and produce irreproducible outputs.
- **Where:** Phase 3, Step 7. and TUI worker parallelism. and YOLO thread caps.
- **Why it matters:** “Leaderboard entry is reproducible from its manifest” becomes false in practice. That kills trust in the tool.
- **Suggestion:** Add a requirement that each symbol/backtest gets an RNG stream derived from (run ID, symbol, iteration), independent of scheduling order.

### 13) TUI state persistence is missing (first-run UX will feel incomplete)

- **Issue:** You scan cache and load universe config, but you don’t persist user selections (tickers, last YOLO settings, last preset, last risk profile).
- **Where:** Phase 12 startup flow. and YOLO settings breadth in Phase 10.
- **Why it matters:** Every restart feels like “lost work.” Users will bounce.
- **Suggestion:** Add a “user preferences persistence” requirement (small config file) and ensure it loads before the six-panel UI is rendered.

------

## Testing gaps (things you claim, but don’t actually make testable early enough)

### 14) Look-ahead bias tests are not explicitly required (they should be)

- **Issue:** Vectorized indicator precompute is a prime source of accidental look-ahead (window alignment mistakes). You never mandate a test that would catch it.
- **Where:** Phase 5b, Step 5.
- **Why it matters:** You can “pass all unit tests” and still have a backtester that cheats. That’s catastrophic for anything involving YOLO discovery.
- **Suggestion:** Require a “future contamination” test: if you perturb future bars, earlier indicator values must not change.

### 15) Execution ambiguity tests ignore the nastiest case: entry + exit in the same bar

- **Issue:** You test ambiguity around stop-loss vs take-profit, but not entry-stop + exit-stop/limit interactions in the same bar (common in breakout systems on OHLC bars).
- **Where:** Phase 7, Steps 2–4 and tests in Step 10.
- **Why it matters:** This is where daily-bar simulation gets “gameable.” If this is wrong, your leaderboard will reward artifacts.
- **Suggestion:** Add explicit scenarios to Phase 7 tests for “entry triggers and stop triggers in same bar” across each path policy.

### 16) YOLO integration tests are likely to become flaky

- **Issue:** “YOLO runs for at least 10 iterations and populates the leaderboard with distinct entries” is not stable unless you lock down determinism, dedup rules, and iteration behavior tightly.
- **Where:** Phase 10 integration verification.
- **Why it matters:** Flaky tests will get ignored. Then regressions slip in.
- **Suggestion:** Require that the YOLO test runs with a fixed seed, fixed universe, fixed iteration count, and asserts deterministic *hashes* rather than “distinctness” by chance.

------

## Integration risks at the Phase 10 merge (two-track interface problems)

### 17) Track interface contract is implied, not enforced

- **Issue:** Track A returns “aligned bar data,” Track B expects a bar event loop. You never define the exact boundary contract early (shape, ownership, missing-bar semantics, determinism guarantees).
- **Where:** Phase 5a, Step 1 and Phase 5b, Step 1.
- **Why it matters:** Merge failures won’t look like “interface mismatch.” They’ll look like “wrong trades,” which is worse.
- **Suggestion:** Add a “data-to-engine contract” section to Phase 3 invariants: what the engine consumes (per-symbol ordered bars), what metadata must accompany it, and what happens on missing/invalid data.

### 18) Universe config vs backtest config duplication will create drift

- **Issue:** You have a sectorized universe TOML and a separate per-backtest TOML that also defines universe selection. That’s two sources of truth.
- **Where:** Universe config system in Phase 4, Step 8. and backtest TOML in Phase 9, Step 7.
- **Why it matters:** Users will select tickers in TUI, run via CLI, and get different universes depending on which file they touched last.
- **Suggestion:** Require a single canonical universe definition with references (e.g., “use universe X, with optional overrides”), not two independent formats.

### 19) “Signal trace fields” requirement has no explicit propagation plan

- **Issue:** You demand trade extraction includes signal traceability, but you don’t explicitly require that order intents/fills carry the trace across order book + execution.
- **Where:** Trade record includes “signal traceability fields” in Phase 3, Step 4. and runner trade extraction requirements in Phase 10, Step 2.
- **Why it matters:** This will get “added later” and become invasive plumbing refactors across multiple crates.
- **Suggestion:** Add a Phase 6 requirement: every order state transition/audit entry must preserve origin metadata so Phase 10 trade extraction is mechanical, not forensic.

------

## Architectural blind spots (separation of concerns that will fight you)

### 20) Some “signals” you list include exit logic that conflicts with your PM separation

- **Issue:** Donchian breakout is described as “exit when price breaks the lower channel,” which is inherently position-aware behavior. But your signal generator must be portfolio-agnostic.
- **Where:** Phase 9, Donchian breakout description. and signal contract forbidding portfolio state.
- **Why it matters:** Implementers will either violate the signal contract or they’ll silently drop the exit behavior and claim it’s implemented.
- **Suggestion:** Decide whether exits are *always PM-owned* (then rewrite the Donchian signal definition as entry-only), or allow “exit signal events” that remain portfolio-agnostic but are only acted upon if a position exists (still PM-controlled).

### 21) Execution “models” blur with order “types”

- **Issue:** You define order types in the domain model and order book, then later you define execution models that look like order types (“Stop order,” “Limit order”).
- **Where:** Order types in Phase 3, Step 2. and execution model types in Phase 7, Step 8.
- **Why it matters:** This will produce awkward workarounds (“execution decides order type, but order type already existed”), and it will leak responsibilities across boundaries.
- **Suggestion:** Clarify the conceptual split: order types are *instructions*, execution models are *fill semantics + timing policy*. Don’t let execution “be” an order type in disguise.

------

## UX completeness (six panels + vim nav isn’t enough)

### 22) Error-state UX is under-specified (especially for data fetch failures)

- **Issue:** You say the worker will send errors back, but you don’t specify how errors are surfaced per ticker, whether they’re retryable, or whether the user can see a persistent error log/history.
- **Where:** Worker communication includes “error” messages in Phase 12, Step 1. and Data panel responsibilities.
- **Why it matters:** Without a clear UX pattern, you’ll end up with “silent failures” or transient toast messages users miss—especially during multi-symbol downloads.
- **Suggestion:** Add one explicit UX requirement: a persistent status area (or per-ticker status) that records last error + retry action.

### 23) Cancellation is promised, but not realistically enforceable across heavy work

- **Issue:** “Atomic cancellation flag” is good, but long-running data scans / heavy computations may not respond quickly, and you don’t define acceptable behavior when immediate stop isn’t possible.
- **Where:** Phase 12, Step 1 cancellation.
- **Why it matters:** Users will hit Escape, nothing happens, and they’ll think the app is frozen.
- **Suggestion:** Require “cooperative cancellation semantics”: stop scheduling new tasks immediately, show “stopping…” state, and ensure partial results are either safely discarded or clearly labeled.

------

## Statistical validity pitfalls (misapplication risks)

### 24) Walk-forward “degradation ratio” is unstable and can be nonsensical

- **Issue:** OOS Sharpe / IS Sharpe blows up when IS Sharpe is near zero, and it behaves weirdly for negative Sharpe regimes (where “better” is less negative).
- **Where:** Phase 11, Step 3.
- **Why it matters:** You will misclassify overfit vs generalizing strategies, which defeats the entire point of the promotion ladder.
- **Suggestion:** Replace the ratio requirement with a sign-aware, bounded comparison rule (difference-based or rank-based) and explicitly define behavior for near-zero/negative IS Sharpe.

### 25) FDR correction is not well-defined without a p-value definition

- **Issue:** Benjamini–Hochberg requires p-values. You don’t specify the hypothesis test used to produce them.
- **Where:** Phase 11, Step 4.
- **Why it matters:** Without a defensible p-value, FDR becomes performative complexity that gives false confidence.
- **Suggestion:** Add a requirement that the implementation must define and document the p-value source (e.g., derived from bootstrap/permutation on OOS performance) before BH is applied.

### 26) Geometric mean CAGR across symbols can break on negative returns

- **Issue:** You explicitly call for geometric mean CAGR in cross-symbol aggregation, but CAGR can be negative and geometric aggregation is not always well-defined or meaningful in that case.
- **Where:** Phase 10, Step 11.
- **Why it matters:** You can end up with undefined values or rankings that reward weird edge cases.
- **Suggestion:** Require a sign-safe aggregation strategy (e.g., log-return-based when valid, otherwise median-based fallback), and mandate explicit guardrails.

### 27) Tail metrics (CVaR/skew/kurtosis) need minimum-sample guardrails

- **Issue:** You include CVaR 95%, skewness, kurtosis, downside deviation ratio as leaderboard metrics, but don’t require minimum observations or stability checks.
- **Where:** Cross-symbol leaderboard metrics.
- **Why it matters:** These metrics will be noisy and can dominate rankings if not constrained, especially on short backtests.
- **Suggestion:** Add a rule: tail metrics are only computed/used once a minimum return sample count is met; otherwise they are marked “insufficient” and excluded from composite ranking.

------

## One “meta” correction I strongly recommend

### 28) Your gates are good, but your “merge-point scope” is not

- **Issue:** You treat Phase 10 as “merge + full feature set,” which is the classic trap.
- **Where:** Two-track merge statement and Phase 10 scope.
- **Why it matters:** Merge is when you want *minimum surface area*, not maximum ambition.
- **Suggestion:** Redefine Phase 10 as: “merge + one clean CLI backtest + one clean per-symbol leaderboard.” Push YOLO + cross-symbol + risk profiles into subsequent phases.

------

## Quick scorecard (where you’re most likely to blow time)

- **Highest overrun risk:** Phase 10 (runner+YOLO+leaderboards+history+metrics), Phase 4 (data ingest/cache), Phase 11 (statistics done right).
- **Highest “subtle wrongness” risk:** indicator precompute + look-ahead bias, OHLC corporate actions, intrabar ambiguity.
- **Highest UX risk:** error-state handling + cancellation + state persistence.

------

If you want the most leverage with the least churn: **fix the Phase 10 scope**, **define decision-time semantics**, and **eliminate synthetic fallback from normal user paths**. Those three changes alone prevent the most expensive mid-project rewrites.

---

# Claude

# TrendLab v4 Build Plan — Architectural Review

**Reviewer:** Senior Software Architect **Date:** 2026-02-06 **Verdict:** The plan is well-structured and shows strong lessons learned from v2. The two-track design, the statistical validation pipeline, and the anti-stickiness system are all well-conceived. What follows are the problems that will hurt if not addressed before implementation begins.

------

## 1. Sequencing Risks

### 1.1 Phase 10 is a five-phase monster wearing a two-week disguise

**Where:** Phase 10 (Weeks 10–11)

**What:** Phase 10 contains: the runner, trade extraction, a full performance metrics library (Sharpe, Sortino, Calmar, CAGR, max drawdown, win rate, profit factor, turnover, streak analysis, CVaR, skewness, kurtosis, downside deviation), the CLI run command, the entire YOLO engine with dual-slider randomization, per-symbol and cross-symbol leaderboards with dual-scope persistence, a risk profile system with four named profiles, a composite ranking system, run fingerprinting with BLAKE3, a JSONL history system with indexing and statistical summaries, and integration tests that verify all of this. That is not two weeks of work. That is the most complex phase in the entire plan by a large margin.

**Why it matters:** This is the merge point of both tracks. If it slips, everything downstream (robustness, TUI, reporting, hardening) shifts. A two-week estimate here creates a false sense of schedule safety — the plan looks like it has three weeks of slack (17 total minus ~14 weeks of content), but that slack evaporates the moment Phase 10 takes its real duration.

**Suggestion:** Split Phase 10 into three sub-phases:

- **10a (Week 10):** Runner, trade extraction, performance metrics, CLI run command. Gate: a single backtest runs on real data and produces correct metrics.
- **10b (Week 11):** YOLO engine, dual sliders, progress reporting. Gate: YOLO runs for 100 iterations and produces distinct entries.
- **10c (Week 12):** Leaderboards (per-symbol, cross-symbol, dual-scope), risk profiles, run fingerprinting, JSONL history. Gate: Checkpoint B passes.

This pushes the total timeline to ~18–19 weeks but makes the estimate honest.

### 1.2 Phase 9 depends on Phase 8 but claims a Week 8–9 window that overlaps nothing

**Where:** Phase 9 (Weeks 8–9)

**What:** Phase 9 implements ten signals, four filters, the factory system, presets, and TOML config. It is scheduled for two weeks, which is reasonable for the scope. However, the factory system must instantiate all four component types, which means all nine PMs from Phase 8 must be done. Phase 8 is a single week (Week 7). If Phase 8 slips at all, Phase 9's start is delayed and its two-week window compresses.

**Why it matters:** Phase 8 contains nine PM implementations plus the ratchet invariant plus stickiness diagnostics plus anti-stickiness regression tests. That is aggressive for a single week, especially since the ratchet invariant has subtle edge cases (what happens to the ratchet on a partial fill? on a gap through the stop?). Any slip here cascades directly into Phase 9.

**Suggestion:** Add a buffer day between Phase 8 and Phase 9 explicitly in the schedule. Alternatively, acknowledge that Phase 8 might bleed into Week 8, making Phase 9 a Week 8.5–9.5 window.

### 1.3 Phase 5b indicator library is load-bearing for three later phases

**Where:** Phase 5b (Week 4, Track B), Steps 4–5

**What:** Phase 5b implements thirteen indicators. Phases 8, 9, and 11 all depend on these indicators being correct and performant. But the indicator tests in Phase 5b (Step 6) are unit tests against known price series. There is no integration test verifying that the Polars-precomputed indicator values match what a bar-by-bar calculation would produce — i.e., no test that the hybrid vectorized/sequential handoff is correct.

**Why it matters:** If the precompute step produces off-by-one errors (common with lookback windows), every signal, PM, and filter downstream will produce subtly wrong results. These bugs won't surface until Phase 9 at earliest, and they'll be hard to diagnose because the signals will *mostly* work.

**Suggestion:** Add an explicit integration test to Phase 5b: for each indicator, compute the value both via Polars precompute and via a naive bar-by-bar loop, and assert they match exactly for every bar. This is cheap to write and will save days of debugging later.

------

## 2. Scope Realism

### 2.1 Phase 12 (TUI) is three weeks for a full terminal application

**Where:** Phase 12 (Weeks 13–15)

**What:** Phase 12 builds: a background worker thread with channel communication, a six-panel layout, vim navigation, a startup sequence, a theme system, a two-level tree view with toggle/expand/collapse, a fetch command with progress reporting, symbol search, a four-component strategy composer with parameter tuning, the entire YOLO config panel with sliders, a leaderboard with session/all-time toggle and risk profile cycling, a drill-down detail view, an equity curve chart with ghost curves and execution drag overlay, a help panel, single-backtest mode, and tests for all of the above.

**Why it matters:** ratatui is powerful but not trivial. Building a responsive six-panel TUI with background workers, channel-based state management, and multiple interactive widgets (tree views, sliders, tables with drill-down, line charts) is a non-trivial application. Three weeks is plausible for someone who has built ratatui apps before, but tight for someone learning it. The chart panel alone — equity curves with ghost overlays — could take 3–4 days.

**Suggestion:** Identify the critical path within Phase 12. The worker thread architecture (Step 1) and panel navigation (Steps 2–3) should be done in Week 13. Panels 1 (Data) and 4 (Results) are the most complex and should get Week 14. Panels 2, 3, 5, 6, and single-backtest mode fill Week 15. If any panel runs long, the Help panel (Step 19) is the one to defer — it's text content that can be added in Phase 14.

### 2.2 Phase 11 (Robustness) is one week for three statistically complex systems

**Where:** Phase 11 (Week 12)

**What:** Walk-forward validation with time-series cross-validation, Benjamini-Hochberg FDR correction, execution Monte Carlo with distribution sampling, stability scoring, block bootstrap with autocorrelation preservation, confidence interval construction for Sharpe ratios, cross-symbol bootstrap, and stickiness integration. In one week.

**Why it matters:** Block bootstrap alone requires choosing a block length estimator (there are several, none trivial), implementing the stationary bootstrap resampling scheme, computing Sharpe ratio confidence intervals from the bootstrap distribution, and calibrating grade thresholds. FDR correction requires computing p-values for each configuration, which means defining a null hypothesis and test statistic for walk-forward results — the plan doesn't specify what the p-value is or how it's computed. These are research tasks masquerading as implementation tasks.

**Suggestion:** Budget two weeks for Phase 11 (Weeks 12–13), and push Phase 12 to Weeks 14–16. Alternatively, stub the block bootstrap initially (implement the framework but use a simpler bootstrap first) and layer in the stationary block bootstrap during Phase 14 hardening.

### 2.3 The 17-week total is realistic only if nothing slips

**Overall assessment:** Phases 1–5 (Weeks 1–4) are well-scoped. Phases 6–7 (Weeks 5–6) are tight but feasible. Phase 8 (Week 7) is slightly underscoped. Phase 9 (Weeks 8–9) is correctly scoped. Phase 10 (Weeks 10–11) is significantly underscoped. Phase 11 (Week 12) is underscoped. Phases 12–14 (Weeks 13–17) are plausible if everything before them is on time.

**Realistic estimate for a solo developer:** 20–22 weeks. For a two-person team with one on engine and one on TUI (starting Week 10), 17–18 weeks is achievable.

------

## 3. Missing Capabilities

### 3.1 No configuration persistence or state management across restarts

**Where:** Entire plan

**What:** The plan describes session vs all-time leaderboards (Phase 10, Step 12) and JSONL history (Phase 10, Step 16), but never addresses: where the YOLO configuration is saved between sessions, whether the TUI remembers the last-used panel/settings, how the universe selection (which tickers are toggled) persists across restarts, or how the user's preferred risk profile is saved.

**Why it matters:** A user who spends time configuring a YOLO sweep and then closes the TUI expects their settings to be there next time. Without persistence, every TUI launch starts from defaults. The "all-time leaderboard" implies long-lived state, but the surrounding configuration state is ephemeral.

**Suggestion:** Add a step in Phase 12 (or Phase 10) for a lightweight state file (TOML or JSON) that persists: last YOLO configuration, universe selection, active risk profile, last active panel, and window layout preferences. Load on startup, save on exit and on configuration change.

### 3.2 No error recovery for YOLO mode

**Where:** Phase 10, Steps 6–9

**What:** YOLO mode runs "indefinitely" and "each iteration" selects strategies, runs backtests, computes metrics, and updates leaderboards. The plan never addresses: what happens when a single iteration panics (bad parameter combination, NaN in metrics, divide by zero), whether the iteration is retried or skipped, whether the error is logged, or whether one bad iteration poisons the leaderboard.

**Why it matters:** Over hundreds of iterations with random parameter sampling, encountering edge cases that produce NaN metrics, zero-trade backtests, or arithmetic panics is guaranteed. If any of these crashes the YOLO loop, the user loses all unsaved progress.

**Suggestion:** Add explicit error handling policy to Phase 10: each YOLO iteration runs in a catch_unwind boundary (or equivalent). Failed iterations are logged with full context (the composition that failed, the error, the symbol), skipped, and counted separately. The YOLO progress display should show both success and error counts. NaN/Inf results must be filtered before leaderboard insertion.

### 3.3 No graceful degradation for Yahoo Finance API changes or outages

**Where:** Phase 4, Step 2

**What:** The plan builds a Yahoo Finance provider with "rate limiting, exponential backoff retries, response parsing, error handling for network failures, invalid symbols, and empty responses." But Yahoo Finance has no official API — every Rust crate for it is an unofficial scraper. Yahoo has historically changed response formats, added CAPTCHAs, and throttled IPs without warning.

**Why it matters:** If Yahoo changes something during development (or after deployment), the entire data pipeline breaks. The plan has a provider trait (good), but no fallback provider and no mechanism for the user to know *why* the download failed beyond a generic network error.

**Suggestion:** Add structured error types to the data provider trait in Phase 4 that distinguish between: network unreachable, rate limited (with retry-after hint), response format changed (parsing failed on previously-working symbol), authentication required, and symbol not found. The CLI and TUI should display these distinctly. Consider noting in the escape hatches section that a CSV import path (already mentioned in Phase 4, Step 4) should be prioritized as the primary fallback when Yahoo is unavailable.

### 3.4 No disk space management for the Parquet cache

**Where:** Phase 4, Steps 6–7

**What:** The cache grows monotonically. With Hive-style partitioning for hundreds of symbols across years of data, the cache directory can grow to several gigabytes. The plan never mentions cache eviction, size limits, or a command to clean the cache.

**Why it matters:** Mostly a quality-of-life issue, but it becomes a real problem if users fetch full S&P 500 history and then wonder why their disk is full.

**Suggestion:** Add a `trendlab cache status` CLI command (or equivalent) that reports total cache size, number of symbols, and date range. Add a `trendlab cache clean` command that removes symbols not used in the last N days. This can go in Phase 14 as a hardening task.

### 3.5 No handling of symbol delistings, ticker changes, or survivorship bias

**Where:** Phase 4, Steps 3–4

**What:** The plan handles corporate actions (splits, dividends) via adjusted close, but doesn't address: tickers that have been delisted (fetching them returns empty or error), tickers that have changed symbols (META was FB), or the survivorship bias introduced by only testing symbols that currently exist in the S&P 500.

**Why it matters:** Survivorship bias is one of the most common sources of false positive in backtesting. A plan this focused on statistical rigor should at least acknowledge it. Ticker changes cause silent data gaps.

**Suggestion:** Add a note in the Phase 4 escape hatches or in the architecture invariants document that survivorship bias is a known limitation of the Yahoo Finance data source. Add symbol alias support (FB → META) to the universe configuration. Document the limitation prominently.

------

## 4. Testing Gaps

### 4.1 No property test for the equity accounting identity under partial fills

**Where:** Phase 5b, Step 8; Phase 7, Step 9

**What:** Phase 5b defines the property test "equity == cash + position values, every bar, no exceptions." Phase 7 introduces liquidity constraints that produce partial fills (Step 9). A partial fill means part of an order fills at one price and the remainder carries to the next bar (or cancels). The equity accounting test from Phase 5b does not cover this case because partial fills don't exist yet when the test is written.

**Why it matters:** Partial fills are the most common source of equity accounting bugs. The unfilled remainder is not a position, not cash, and not an order — it's a pending carry. If the accounting identity test doesn't cover this state, you can have a "passing" test suite with broken accounting.

**Suggestion:** Add an explicit step in Phase 7 to extend the equity accounting property test to cover partial fills: equity == cash + position market value + pending order carry value (if carry policy is used). Also test the cancel policy: equity == cash + position market value (remainder is simply cancelled, no phantom value).

### 4.2 No test that the ratchet invariant holds across bar gaps

**Where:** Phase 8, Step 6

**What:** The ratchet property test says "across any price path, the stop level must be monotonically non-decreasing for long positions." But price paths with gaps (where the open is significantly different from the prior close) can cause the ATR to spike, which could cause a naive implementation to try to widen the stop. The test needs to specifically generate gapped price paths.

**Why it matters:** The ratchet invariant is the core anti-stickiness mechanism. If it fails on gaps, it fails in exactly the market conditions where it matters most (earnings gaps, weekend gaps).

**Suggestion:** Add a specific generator to the ratchet property test that produces price paths with large gaps (5%+ overnight moves). Also test the scenario where a gap moves *through* the stop — the stop should not be moved to accommodate the gap; the position should be exited.

### 4.3 No golden test for the walk-forward system

**Where:** Phase 11, Step 12

**What:** Phase 11 tests that walk-forward "detects overfit configs" and that "FDR correction reduces false positive rate," but there is no golden test that locks the exact walk-forward results for a known configuration on known data. If someone refactors the fold-splitting logic or the degradation ratio calculation, the test suite will still pass even if the numbers change.

**Why it matters:** Walk-forward validation is the primary overfit detection mechanism. If it silently changes behavior during a refactor, the entire robustness pipeline becomes unreliable.

**Suggestion:** Add a golden test in Phase 11: run walk-forward on a known strategy with known data and known fold parameters, and assert the exact degradation ratio, fold-level Sharpe values, and FDR-adjusted p-values. This locks the behavior.

### 4.4 Signal portfolio-agnosticism test may not catch all leakage paths

**Where:** Phase 9, Step 3

**What:** The plan says "verify this with a test that shows the same signal output regardless of current position state." This tests one specific leakage path (signals reading position state). But signals could also leak through: indicator state that was mutated by the execution engine, shared mutable references to the order book, or global state via thread-local storage.

**Why it matters:** The signal/PM separation is a core architectural invariant. A test that only checks one leak vector provides false confidence.

**Suggestion:** The test should verify signal outputs by running the same signal on the same bar data in two completely isolated contexts: one with an active position and pending orders, and one with a clean slate. If the signal trait is implemented correctly (taking only bar history and indicator values as input), this should be trivially true, but the test should construct the worst-case scenario of shared state to verify it.

------

## 5. Integration Risks

### 5.1 The Track A / Track B interface is implicitly defined, never explicitly tested

**Where:** Phase 5a (Track A terminus), Phase 5b (Track B start), Phase 10 (merge)

**What:** Track A produces Parquet files on disk with a specific schema and Hive-style partitioning. Track B consumes bar data via the event loop. The interface between them — the exact schema of the Parquet files, the column names, the timestamp format, the sort order, the handling of missing bars — is defined implicitly by whatever Phase 4 produces and whatever Phase 5b expects. There is no explicit interface contract between the tracks.

**Why it matters:** If Track A developer (or same developer, different week) makes a decision about column naming, timestamp timezone, or NaN handling that differs from Track B's expectations, Phase 10 is where you find out. This is a classic integration failure mode.

**Suggestion:** Add an explicit step to Phase 3 (Domain Model) that defines the Parquet schema contract: exact column names, data types, sort order, timezone convention, and NaN/missing-bar policy. Both Phase 4 and Phase 5b should reference this schema, and Phase 5a should include a test that loads a Parquet file and validates it against the schema contract. This way the interface is tested before the merge point.

### 5.2 The Polars threading model creates a resource contention risk at merge

**Where:** Phase 5b (Step 5: indicator precompute), Phase 10 (Steps 6–8: YOLO with Rayon + Polars)

**What:** Phase 5b uses "Polars lazy expressions" for indicator precompute. Phase 10 uses "Rayon for symbol-level parallelism and Polars threading for per-backtest parallelism." Both Rayon and Polars have their own thread pools. When YOLO mode runs N symbols in parallel via Rayon, and each symbol's backtest uses Polars for indicator precompute, you have nested parallelism: Rayon outer threads × Polars inner threads. The plan acknowledges this with "Polars thread cap" and "outer thread cap" settings, but there's no test or verification that these caps actually prevent thread explosion.

**Why it matters:** Unconstrained nested parallelism causes thread oversubscription, cache thrashing, and paradoxically *slower* execution. On a machine with 8 cores, running 8 Rayon tasks each spawning 8 Polars threads means 64 threads competing for 8 cores. This is a performance cliff that won't show up in single-symbol testing.

**Suggestion:** Add a specific performance test to Phase 10: run YOLO on 50 symbols with default thread settings and measure throughput. Then run with constrained settings (outer=4, inner=2 on an 8-core machine) and verify throughput is equal or better. Include this in the Phase 14 Criterion benchmarks. Also consider setting Polars to single-threaded by default within the YOLO loop, since the outer parallelism is usually more efficient than inner parallelism for this workload.

### 5.3 YOLO mode's leaderboard deduplication depends on hash stability across phases

**Where:** Phase 3 (Step 6: deterministic IDs), Phase 9 (Step 4: factory system), Phase 10 (Step 15: run fingerprinting)

**What:** The deduplication logic in Phase 10 uses config hash to detect "same structure + config + symbol." This hash is defined in Phase 3 and depends on canonical serialization with sorted keys. But the factory system in Phase 9 creates runtime objects from config — if the config structure changes between Phase 3 and Phase 9 (e.g., a field is added, renamed, or reordered), the hash changes silently. The JSONL history from previous runs becomes unqueryable by hash.

**Why it matters:** Hash instability breaks the "any leaderboard row can be reproduced from its manifest" guarantee (Definition of Done #4). It also breaks the all-time leaderboard across sessions.

**Suggestion:** Add a hash stability test to Phase 9: define a known configuration, compute its hash, and assert it matches a hardcoded expected value. This golden test breaks loudly if anyone changes the serialization format. Also document in the architecture invariants that config hashing requires additive-only schema changes — new fields must have defaults and must be appended, not inserted.

------

## 6. Architectural Blind Spots

### 6.1 The signal → filter → execution → PM pipeline has no feedback path for rejected signals

**Where:** Phase 3 (Step 8), Phase 9 (Steps 1–2, 8)

**What:** The pipeline is: signal produces intent → filter gates it → execution creates order → PM manages exit. But when a filter rejects a signal, what happens to the signal event? Is it discarded silently? Is it logged? Can the PM see that a signal was generated but filtered? The plan doesn't address this.

**Why it matters:** For diagnostics and stickiness analysis, knowing that a strategy *would have* traded but was filtered is valuable. If filtered signals are silently discarded, the trade tape and stickiness diagnostics are incomplete — they can't distinguish "the strategy didn't generate a signal" from "the strategy generated a signal that was filtered."

**Suggestion:** Add a "signal trace" concept to Phase 3 or Phase 9: every signal event is logged regardless of filter outcome, with a status field (passed, filtered_by_adx, filtered_by_regime, etc.). The trade record's signal traceability fields (Phase 10, Step 2) should reference this trace. This is also necessary for the "isolatable" Definition of Done criterion (#6).

### 6.2 Position sizing is mentioned once and never defined

**Where:** Phase 9, Step 8 ("position sizers determine quantity")

**What:** Step 8 of Phase 9 says "position sizers determine quantity" in the composition rules, but position sizing is not one of the four component traits (signal, PM, execution, filter). It's not defined in Phase 3, it has no trait, it has no implementations, and it has no tests. Yet the system must determine how many shares to buy on every entry signal.

**Why it matters:** Without position sizing, every trade is either 100% of equity (unrealistic and dangerous for backtesting) or some hardcoded amount. The leaderboard results become incomparable across strategies with different natural position sizes. This is a missing fifth component that the plan assumes exists but never builds.

**Suggestion:** Either add position sizing as a fifth component trait (with implementations like: fixed-dollar, fixed-share, percent-of-equity, volatility-targeted, Kelly criterion) or explicitly document in Phase 3 that position sizing is fixed at percent-of-equity with a configurable percentage, and that richer sizing is deferred. The current plan's silence on this is a gap that will force ad-hoc decisions during implementation.

### 6.3 No mechanism for strategy-level capital allocation in multi-symbol YOLO

**Where:** Phase 10, Steps 6–11

**What:** When YOLO mode runs a strategy across 50 symbols, each symbol gets its own backtest. But the plan never specifies whether each symbol gets the full initial capital (making results incomparable to a real portfolio) or a fraction (requiring an allocation model). The cross-symbol leaderboard aggregates per-symbol results, but the aggregation assumes independent capital pools.

**Why it matters:** A strategy that works on 50 symbols individually with $100K each is very different from a strategy that works with $2K per symbol from a $100K pool. The geometric mean CAGR and hit rate metrics in the cross-symbol leaderboard are only meaningful if the capital assumption is explicit.

**Suggestion:** Document in Phase 10 that each symbol backtest uses the same fixed initial capital (e.g., $100,000) and that results are per-symbol, not portfolio-level. Add a note in the escape hatches that portfolio-level capital allocation (risk parity, equal weight, etc.) is deferred. This makes the limitation explicit rather than implicit.

### 6.4 The four-phase bar event loop doesn't account for market-on-close orders interacting with PM exit orders

**Where:** Phase 5b (Step 1), Phase 7 (Step 1), Phase 8 (Step 1)

**What:** The event loop is: start-of-bar → intrabar → end-of-bar → post-bar. Market-on-close orders fill at end-of-bar. PMs emit maintenance orders at post-bar "for the NEXT bar." But what if a PM's exit is a close-on-signal execution model (Phase 7, Step 8)? The close-on-signal fill happens at end-of-bar, but the PM's decision happens at post-bar *after* the fill. There's a timing ambiguity: does the PM see the fill from the current bar's close before deciding on next bar's orders?

**Why it matters:** If the PM doesn't see the close-on-signal fill from the current bar, it will emit a maintenance order (e.g., cancel/replace a stop) for a position that no longer exists. This creates phantom orders in the order book.

**Suggestion:** Add an explicit rule to the event loop contract in Phase 5b: post-bar PM processing must see all fills from all prior phases of the current bar. The order book must be updated with end-of-bar fills *before* the PM runs at post-bar. Test this interaction explicitly in Phase 8.

------

## 7. UX Completeness

### 7.1 No first-run experience beyond an empty state

**Where:** Phase 12, Step 4

**What:** "If this is a fresh install with no cache, the Data panel shows all tickers as unfetched. The TUI does NOT show fake data. If there are no results, the Results panel shows an empty state with instructions." The "instructions" are unspecified. The user is dropped into a six-panel TUI with no cache, no results, and no guidance beyond the Help panel (which they have to know exists and navigate to).

**Why it matters:** The Definition of Done says "clone to first real YOLO run takes under 5 minutes." A cold-start user staring at six panels with empty states and no affordances will not achieve this in 5 minutes without the Quick Start guide (which is a separate document, not in-app guidance).

**Suggestion:** Add an in-app first-run flow to Phase 12: when the TUI detects no cached data and no results, display a prominent one-time overlay or status bar message: "Welcome. Press 1 to go to Data, select tickers with Space, press f to fetch. Press 3 for YOLO mode once you have data." This can be dismissed with any key and never shown again (persisted in the state file).

### 7.2 No feedback during long-running single backtests

**Where:** Phase 12, Step 20

**What:** "The user can launch a single backtest from the Strategy panel. The worker runs it in the background, the TUI shows a progress indicator, and the result appears in the leaderboard when complete." The "progress indicator" is unspecified. A single backtest on 20 years of data with a complex PM could take several seconds. The user has no indication of what's happening.

**Why it matters:** Without progress feedback, users will either: assume it's broken and launch another one, or mash keys and cause state corruption in the channel system.

**Suggestion:** Specify the progress indicator: at minimum, show the current bar number out of total bars, or a percentage complete, or a spinner with estimated time remaining. The worker should send periodic progress messages through the channel, not just a final result.

### 7.3 No undo or cancel for data fetches

**Where:** Phase 12, Step 7

**What:** "When the user presses f, send a fetch command to the worker for all selected tickers." But there's no mention of cancellation. If the user selects 500 tickers and presses f, they're committed to a long download with no escape.

**Why it matters:** The YOLO escape key is documented (Phase 12, Step 12: "Escape stops the run"), but the data fetch has no equivalent.

**Suggestion:** Add Escape-to-cancel behavior for data fetches. The atomic cancellation flag from the worker architecture (Step 1) should apply to fetches as well. Already-downloaded symbols should be preserved in cache; only the in-progress download should be aborted.

### 7.4 No visual indication of which panel is active

**Where:** Phase 12, Steps 2–3

**What:** The plan describes six panels accessible by number keys and Tab cycling, but doesn't mention how the user knows which panel is currently active.

**Why it matters:** With vim-style navigation, the user needs to know which panel owns j/k/h/l. Without a visual indicator (highlighted tab, border color change, header emphasis), keystrokes go to an ambiguous destination.

**Suggestion:** Add to the theme system (Step 5): the active panel's border or header should use the accent color (electric cyan). Inactive panels should use the muted color. This is a one-line addition to the plan but critical for usability.

### 7.5 No error display mechanism in the TUI

**Where:** Phase 12, entire phase

**What:** The worker thread can produce errors (network failures during fetch, panics during backtest, invalid configurations). The plan describes success paths (progress bars, leaderboard updates) but never describes how errors are displayed to the user.

**Why it matters:** Errors will happen. If they're silently dropped by the worker thread, the user sees nothing — the fetch just "doesn't work" or the YOLO iteration count doesn't advance, with no explanation.

**Suggestion:** Add an error display mechanism: either a persistent status bar at the bottom of the screen that shows the last error (with a key to dismiss), or a toast/notification system that displays errors temporarily. The worker channel should have an explicit error message type alongside the success message types.

------

## 8. Statistical Validity

### 8.1 FDR correction requires p-values, but the plan doesn't define the null hypothesis

**Where:** Phase 11, Step 4

**What:** "Apply FDR correction (Benjamini-Hochberg) across all configs that undergo walk-forward testing." Benjamini-Hochberg requires a p-value for each configuration being tested. A p-value requires a null hypothesis and a test statistic. The plan doesn't specify: what is the null hypothesis? (Probably "the strategy's OOS Sharpe is zero.") What is the test statistic? (Probably a t-statistic on fold-level OOS Sharpe values.) What distribution is assumed under the null? (Probably t-distribution with degrees of freedom equal to number of folds minus one.)

**Why it matters:** Without a specified null hypothesis, two implementers could make different choices and get different FDR results. Worse, a naive implementation might use the walk-forward degradation ratio as the test statistic, which doesn't have a known distribution under the null — making the p-value meaningless.

**Suggestion:** Add to Phase 11, Step 4: specify that the null hypothesis is "mean OOS Sharpe equals zero," the test statistic is the t-statistic computed from the K fold-level OOS Sharpe values (where K is the number of walk-forward folds), and the p-value comes from a one-sided t-test. This is enough to make the FDR correction well-defined without adding formulas to the plan.

### 8.2 Block bootstrap block length selection is unspecified

**Where:** Phase 11, Step 9

**What:** "Run stationary block bootstrap (preserving autocorrelation structure)." The stationary bootstrap requires a mean block length parameter. This parameter controls the tradeoff between preserving autocorrelation structure (longer blocks) and generating diverse resamples (shorter blocks). The plan says to "calibrate during implementation," but this is a research decision that significantly affects the confidence intervals.

**Why it matters:** If the mean block length is too short (e.g., 5 days), the bootstrap destroys the autocorrelation structure it's supposed to preserve, and the confidence intervals are too narrow (false confidence). If it's too long (e.g., 60 days), there aren't enough distinct blocks to generate meaningful variation, and the confidence intervals are too wide (useless).

**Suggestion:** Add to Phase 11, Step 9: the mean block length should be estimated using an automatic selection method (the Politis-White or Politis-Romano methods are standard for this). If automatic selection is too complex to implement in the first pass, use a conservative default of 20 trading days (approximately one month) with a configuration override. Document the sensitivity of the confidence intervals to this parameter.

### 8.3 Walk-forward fold construction needs guardrails for short data

**Where:** Phase 11, Step 3

**What:** "Split data into multiple time folds, train on in-sample, test on out-of-sample." The plan doesn't specify: minimum data length required for walk-forward, minimum in-sample or out-of-sample fold size, or what happens if the data is too short for meaningful folds.

**Why it matters:** With 252 bars (one year) and, say, 5 folds, each fold has ~50 bars of in-sample and ~50 bars of out-of-sample. That's too little data for any signal to be statistically meaningful. The walk-forward will produce high-variance degradation ratios that are more noise than signal, and the FDR correction will either reject everything or accept everything depending on the noise.

**Suggestion:** Add minimum data length requirements to Phase 11: walk-forward should require at least 3 years (756 bars) of data, with a minimum of 252 bars per in-sample fold and 63 bars (one quarter) per out-of-sample fold. If data is insufficient, the walk-forward level should be skipped (not run with meaningless folds), and the strategy should receive a "not enough data for validation" flag rather than a passing or failing grade.

### 8.4 Geometric mean CAGR across symbols can mask catastrophic failure

**Where:** Phase 10, Step 11

**What:** "Geometric mean CAGR (rewards consistency over outlier performance)" is used in the cross-symbol leaderboard. The geometric mean is indeed more conservative than the arithmetic mean, but it still masks catastrophic failures if even one symbol produces a large positive return.

**Why it matters:** Consider a strategy that produces +50% CAGR on 9 symbols and -90% on 1 symbol. The geometric mean is still strongly positive, but a real trader would consider this strategy broken. The plan includes "worst max drawdown" as a separate column, but it's not part of the geometric mean calculation.

**Suggestion:** Add a "conditional geometric mean" that excludes the best and worst performing symbols (trimmed geometric mean) as an additional ranking option. Alternatively, add a minimum per-symbol return threshold: if any symbol produces a return below -X%, the strategy is flagged regardless of its aggregate score. The hit rate metric partially addresses this, but it's a blunt instrument (profitable vs unprofitable) that doesn't capture the magnitude of losses.

### 8.5 No correction for look-ahead bias in indicator precomputation

**Where:** Phase 5b, Step 5

**What:** "Before the bar loop begins, compute all indicators using Polars lazy expressions." If indicators are computed over the entire price series at once, the implementation must ensure that each bar only sees indicator values computed from data up to and including that bar — not future data. A Polars rolling window computation handles this correctly, but a naive column-level computation might not (e.g., computing z-scores using the full-series mean and standard deviation instead of an expanding or rolling window).

**Why it matters:** Look-ahead bias is the most dangerous bug in a backtester because it produces strategies that look incredible in-sample and fail catastrophically in production. The plan doesn't explicitly guard against this.

**Suggestion:** Add an explicit invariant to Phase 5b: "No indicator value at bar t may depend on price data from bar t+1 or later." Add a test that verifies this by computing indicators on a truncated series (bars 1–100) and on the full series (bars 1–200), and asserting that bars 1–100 produce identical values in both cases. If they differ, there's look-ahead contamination.

------

## Summary of Critical Issues (prioritized by blast radius)

| Priority | Issue                                                      | Phase        | Impact                                                       |
| -------- | ---------------------------------------------------------- | ------------ | ------------------------------------------------------------ |
| **P0**   | Position sizing is undefined                               | 3, 9, 10     | Every trade quantity is arbitrary; leaderboard results are meaningless |
| **P0**   | Phase 10 is 4+ weeks disguised as 2                        | 10           | Schedule collapse cascades to all downstream phases          |
| **P0**   | Look-ahead bias guard is missing                           | 5b           | Silent corruption of all backtest results                    |
| **P1**   | FDR null hypothesis unspecified                            | 11           | Statistical validation produces meaningless p-values         |
| **P1**   | Track A/B interface contract missing                       | 3, 4, 5a, 5b | Integration failure at merge point                           |
| **P1**   | YOLO error recovery missing                                | 10           | Single bad iteration crashes entire run                      |
| **P1**   | No TUI error display mechanism                             | 12           | Errors are silently swallowed; user sees nothing             |
| **P2**   | Block bootstrap block length unspecified                   | 11           | Confidence intervals are miscalibrated                       |
| **P2**   | Walk-forward minimum data guardrails missing               | 11           | Validation on short data produces noise                      |
| **P2**   | Signal rejection trace missing                             | 3, 9         | "Isolatable" DoD criterion (#6) is unverifiable              |
| **P2**   | Equity accounting under partial fills untested             | 5b, 7        | Accounting identity can silently break                       |
| **P2**   | Configuration persistence missing                          | 12           | Every TUI restart loses all settings                         |
| **P3**   | Phase 11 is underscoped (1 week for 3 statistical systems) | 11           | Quality suffers or schedule slips                            |
| **P3**   | Bar event loop close-on-signal/PM timing ambiguity         | 5b, 7, 8     | Phantom orders in order book                                 |
| **P3**   | Nested Rayon/Polars thread contention                      | 5b, 10       | Performance cliff under multi-symbol YOLO                    |
| **P3**   | Hash stability across schema changes                       | 3, 9, 10     | All-time leaderboard corrupts silently                       |
| **P3**   | First-run UX is blank screens                              | 12           | 5-minute onboarding goal fails                               |
| **P3**   | Survivorship bias undocumented                             | 4            | Users draw wrong conclusions from results                    |