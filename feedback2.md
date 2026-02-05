# ChatGPT

I’m going to (1) sanity-check the roadmap structure and test strategy, then (2) call out the few places where the plan still has contradictions or missing “decision points,” and (3) give a tight list of concrete edits that will make it executable without ambiguity.

## How it looks now (overall)

The roadmap is much more “buildable” than before: you’ve got a tracer-bullet smoke backtest, a clear module dependency graph, and explicit checkpoints so you can stop and validate realism before you build the whole cathedral.

You also nailed the *right* realism targets for a trend engine: intrabar priority, bracket/OCO correctness, atomic cancel/replace, liquidity caps, cache invalidation, and stability scoring are all explicitly in scope now.

## The biggest thing to fix: the doc still contradicts itself in a few “load-bearing” places

Right now the roadmap contains multiple versions of the *same* file/spec (likely remnants from edits). If you implement the wrong version, you’ll break determinism and/or caching.

### 1) Deterministic IDs: pick one hashing approach and delete the other

You have **two different `RunId::hash()` implementations**:

- BLAKE3 + canonical JSON (stable across platforms/builds)
- `DefaultHasher` (not what you want for cross-build stability guarantees)

**Fix:** Keep the BLAKE3 version and remove the `DefaultHasher` version. Then add a one-line “Determinism rule: no DefaultHasher anywhere in IDs.”

### 2) Universe determinism: you have both `BTreeSet` and `HashSet`

You explicitly call out deterministic iteration and show `BTreeSet` (good)
…but elsewhere the same `Universe` is `HashSet` (bad for stable hashing/ordering)

**Fix:** Standardize on `BTreeSet` for any structure that ever participates in hashing, manifests, or deterministic iteration. Remove the `HashSet` version entirely.

### 3) Dataset hashing / cache invalidation: you have both “proxy hash” and “sampled BLAKE3”

You have a **proper** (sampled) BLAKE3-based content hash description
…but you also still have an older **proxy** approach (column names + row count + first/last timestamps) using `DefaultHasher`

**Fix:** Delete the proxy hash version. It will absolutely fail the “mutate the middle” cache invalidation scenario you say you care about.

> If you’re worried about hashing cost: keep the sampled-BLAKE3 approach, but add a *second mode*:
>
> - `DatasetHashFast` (sampled) for dev sweeps
> - `DatasetHashStrict` (full scan) for “publish / leaderboard / compare” runs
>   …and record which mode was used in the manifest.

## Next-most-important improvements (these will save you weeks later)

### A) Make bar + intrabar semantics “one page” canonical

You have solid event-loop intent (“Start-of-bar… Intrabar… End-of-bar… Post-bar”)
and good bracket activation BDD
…but you still need one canonical section that answers:

1. **When** do bracket children become active if the entry fills at the open?
2. Can children fill **in the same bar** as the parent fill? (Daily bars: this matters a lot.)
3. In WorstCase mode, if both stop and target are “reachable” intrabar, what’s the exact priority rule?

**Concrete doc edit:** Add a short “Intrabar micro-timeline” that defines:

- sub-steps (open fill step → activation step → path traversal steps → close step)
- trigger evaluation order
- whether activation can occur mid-bar and participate in remaining steps

Then add 2–3 BDD scenarios for the ambiguous cases:

- “Entry fills at open; stop touched before high later in bar”
- “Stop and target both reachable same bar; WorstCase chooses worse fill”
- “Child remains Pending until activation step; never triggers before that”

### B) Liquidity caps: specify allocation when multiple orders compete for volume

You’ve got liquidity caps as a concept, but the roadmap should declare the **allocation rule** when several fills compete for limited bar volume:

- pro-rata by remaining qty
- time-priority by submission time
- priority tiers (stops first vs limits first vs market first), etc.

Without this, your MC execution realism will be non-reproducible *even with a fixed seed*, because tiny ordering differences change fill distribution.

**Concrete doc edit:** Add one paragraph + one BDD feature:

- “Given bar volume 10,000 and max_participation 10% ⇒ max_fill 1,000 shares”
- “Two orders compete; allocation = pro-rata (or time priority)”
- “Unfilled remainder stays Active (or expires) per TIF”

### C) Turn the “TODO tests” into a minimum golden/property test set per milestone

You already state BDD vs unit vs property vs golden (good)
But a lot of the sample tests are still placeholders (“Create test DataFrame… Verify…”).

**Concrete doc edit:** For each checkpoint (A/B/C), list the *exact non-negotiable* tests that must exist and pass.
Example for Checkpoint A:

- golden: 10-bar smoke run equals known equity
- property: “no negative cash unless margin enabled”
- property: “OCO invariant: sibling cancels only after full fill” (you already defined this behavior)
- golden: “stop gapped through fills at open worse price” (matches your realism invariant)

### D) Add a tiny “Decision Log” at the top to prevent future contradictions

This doc is big, and you *will* revisit assumptions. Put a small section near the top:

- **Chosen:** BLAKE3 for IDs/hashes; BTreeSet/BTreeMap for deterministic collections
- **Chosen:** WorstCase ambiguity rule = [spell it out]
- **Chosen:** bracket children activation timing = [spell it out]
- **Open:** liquidity allocation rule (until you choose)
- **Open:** strict vs fast dataset hashing mode (until you choose)

This prevents the exact duplication drift you currently have.

## Quick wins you can apply immediately

1. Delete the `DefaultHasher` variants for `RunId` and dataset hashing; keep the BLAKE3 versions.
2. Make `Universe.symbols` a `BTreeSet` everywhere; remove the `HashSet` copy.
3. Add the 3 “same-bar ambiguity” BDD scenarios (they’re the ones that will quietly wreck realism if unspecified).

If you want, I can also propose an exact “canonical intrabar micro-timeline” (with a tiny state machine) that matches your WorstCase/Deterministic modes and plugs directly into the order book + bracket activation rules you already wrote.

---

# Gemini

This version of the **TrendLab v3 Roadmap** is highly robust. You have successfully addressed the "research-grade" requirements by formalizing the relationship between **Signal Intent** and **Execution Reality**.

The most significant improvement is the **"Deterministic Contract"** in M1 and M2, which ensures that your results aren't just accurate, but immutable across different environments.

Here is a breakdown of what works well and the final refinements needed to ensure "v3 Done" status.

### ### Strengths of the Current Plan

- **The 4-Phase Event Loop (M3):** Separating maintenance intents into the "Post-bar" phase (Phase 4) is critical. It prevents look-ahead bias and ensures that your **Position Manager** only acts on information that was actually available at the bar's close.
- **Atomic Cancel/Replace (M4):** Making this a first-class operation is essential for trend-following strategies. It eliminates the "stopless window" that frequently plagues retail backtesters during trailing stop updates.
- **Ghost Curve & Execution Lab (M10):** Tracking "Ideal" vs. "Real" equity side-by-side provides an immediate visual of **Execution Drag**. This is the fastest way to identify strategies that look great on paper but will fail in live markets due to slippage.
- **Stability Scoring (M9):** Moving away from point-estimates (like just "Sharpe") to a scoring system that penalizes variance ($StabilityScore = median - (penalty \times IQR)$) ensures you are selecting for robustness, not luck.

------

### ### Recommended Final Refinements

#### 1. M2: Multi-Symbol Time Alignment

While the roadmap mentions "align_multi_symbol_timestamps," it should explicitly define the **Missing Bar Policy**.

- **The Risk:** If SPY has a bar but QQQ is missing a minute of data, your loop might "shift" the QQQ data forward, causing a look-ahead bias where the engine thinks it's seeing QQQ's 10:01 AM price at 10:00 AM.
- **Refinement:** Ensure M2 includes a **Reindexing** step that forces all symbols to share a unified timestamp index, filling gaps with `NaN` or a Forward-Fill to maintain strict temporal alignment.

#### 2. M10: The "Rejected Intent" Diagnostic

Trend-following strategies often fail not because of bad trades, but because of **missed trades**.

- **Refinement:** In the TUI (M10), ensure the "Rejected Intents" view explicitly logs **why** an intent was blocked. Common reasons to track:
  - **Volatility Guard:** PM blocked entry because ATR was too high.
  - **Liquidity Guard:** Sizer reduced qty to 0 because participation limits were exceeded.
  - **Margin/Cash:** Portfolio had insufficient buying power.

#### 3. M8: Feature-to-Warmup Sync

You've correctly identified that **Warmup** must sync with the feature cache.

- **Refinement:** Implement a `max_lookback()` method on all indicators. If a user modifies a configuration from an MA(20) to an MA(200) in the TUI, the runner should automatically detect that the required warmup has increased and invalidate the results cache to prevent calculating signals on null data.

------

### ### Final Integration Checkpoints

To confirm you are "v3 Done," I recommend these three "Hard Fails" in M12:

| **Test**                 | **Objective**                            | **Success Metric**                                           |
| ------------------------ | ---------------------------------------- | ------------------------------------------------------------ |
| **Concurrency Torture**  | Ensure no race conditions in the engine. | Run 16-thread sweep vs 1-thread sweep; results must match bit-for-bit. |
| **The "Death Crossing"** | Identify execution-fragile strategies.   | Flag any strategy where the **Ghost Curve** (ideal) and **Real Curve** diverge by $>15\%$. |
| **Cache Mutation**       | Verify data integrity.                   | Manually delete a `.cache` file; hitting "Rerun" in TUI must reproduce the exact same equity curve. |

### ### Next Step

**Would you like me to generate the full Rust implementation for M1 (Domain Model), focusing on the `Instrument` metadata and the `TickPolicy` logic for side-aware rounding?**