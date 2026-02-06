# TrendLab v3 - Implementation vs Roadmap Discrepancy Report

**Date:** 2026-02-06
**Status:** Post-M12 Analysis
**Author:** Implementation Review

---

## Executive Summary

The TrendLab v3 project completed M0-M12 milestones with **significant gaps** between the roadmap specification and actual implementation, particularly in:

1. **Data Integration** - NO Yahoo Finance integration (roadmap implied)
2. **TUI Features** - Missing tabbed interface, charts, session management
3. **Data Loading** - TUI launches with empty state (no data loaded)
4. **Milestone Completion** - M9-M11 only partially implemented

**Overall Completion:** ~60% of planned functionality

---

## Milestone-by-Milestone Analysis

### M0-M8: Backend Engine ✅ COMPLETE

**Status:** Fully implemented as specified

**Deliverables Met:**
- ✅ Workspace scaffold
- ✅ Domain model (Bar, Order, Fill, Portfolio, etc.)
- ✅ Data ingest infrastructure (Parquet)
- ✅ Event loop and execution engine
- ✅ Order system with OCO/brackets
- ✅ Position management
- ✅ Runner with cache
- ✅ Leaderboard ranking

**Deliverables NOT Met:**
- ❌ Yahoo Finance integration (roadmap line 95: "start with Parquet ingest + local lists only")
  - **Interpretation:** This was an "escape hatch" but user expected Yahoo Finance
  - **Gap:** NO data source integration - relies on pre-existing Parquet files

---

### M9: Robustness Ladder & Stability Scoring

**Roadmap Specification (lines 82-83):**
- "M9 (robustness ladder + stability scoring)"
- "M9 Robustness: ship Walk-Forward + Execution MC first; add Path MC and bootstrap later." (line 98)

**What Was Planned:**
- Walk-Forward validation
- Execution MC (slippage sensitivity)
- Path MC (intrabar ambiguity) - stretch goal
- Bootstrap resampling - stretch goal
- Stability scoring (IQR-based)

**What Was Actually Built:**
- ✅ Walk-Forward (Level 2)
- ✅ Execution MC (Level 3)
- ✅ Path MC (Level 4) - **SCAFFOLDED ONLY**
- ✅ Bootstrap (Level 5) - **SCAFFOLDED ONLY**
- ✅ CheapPass (Level 1)
- ✅ Stability scoring (IQR, promotion criteria)
- ✅ 5-level ladder orchestration
- ✅ 32 unit tests + 22 integration tests

**Discrepancies:**
- ❌ **PathMC is scaffolded** - runs same config instead of varying path policies
  - Code comment (path_mc.rs): "TODO: hook into ExecutionModel path_policy"
  - Currently just runs baseline backtest N times (not actual path MC)
- ❌ **Bootstrap is scaffolded** - block resampling stubbed
  - Code comment (bootstrap.rs): "TODO: implement date block permutation"
  - Currently just runs baseline backtest N times (not actual bootstrap)

**Completion:** 70% (core structure done, but Levels 4-5 need real implementation)

---

### M10: TUI + Drill-Down + Ghost Curve

**Roadmap Specification (lines 84, 99):**
- "M10 (TUI + drill-down + ghost curve)"
- "M10 TUI: ship 4 core panels first; expand to full suite after runner is solid."

**What Was Planned (from Checkpoint C, lines 142-152):**
1. **Ghost Curve Analysis** - Flag strategies where ideal vs real diverge >15%
2. **Rejected Intent Coverage** - Display all 4 rejection types
3. **Drill-Down Completeness** - Trace signal → intent → order → fill

**Additional TUI Requirements (inferred from user expectations):**
- Tabbed interface (Charts | Current Session | All-Time)
- Live data loading from sweeps
- Visual equity curves
- Session management
- Data source integration (Yahoo Finance?)

**What Was Actually Built:**

**✅ Implemented:**
- Basic TUI shell (main.rs, app.rs)
- DrillDownState enum (Leaderboard, SummaryCard, TradeTape, ChartWithTrade, Diagnostics, etc.)
- LeaderboardPanel (renders strategy table)
- Navigation system (up/down, drill down, back)
- Theme system (Parrot/neon colors)
- 49 TUI tests

**❌ NOT Implemented:**
- ❌ **NO TABBED INTERFACE** - User expected Charts | Current | All-Time tabs
- ❌ **NO DATA LOADING** - App starts with empty results Vec
- ❌ **NO GHOST CURVE** - Not implemented despite roadmap mention
- ❌ **ALL DRILL-DOWN VIEWS ARE PLACEHOLDERS** - Code shows "TODO" for:
  - SummaryCard (line 98-99 in main.rs)
  - TradeTape (line 102-111)
  - ChartWithTrade (line 113-122)
  - Diagnostics (line 124-133)
  - RejectedIntents (line 135-144)
  - ExecutionLab (line 146-155)
- ❌ **NO CHARTS** - No equity curve rendering
- ❌ **NO SESSION MANAGEMENT** - No current vs all-time toggle
- ❌ **NO YAHOO FINANCE INTEGRATION** - No live data fetching

**Code Evidence:**

From main.rs:
```rust
DrillDownState::SummaryCard(_run_id) => {
    // Render leaderboard as background
    let panel = LeaderboardPanel::new(...);
    f.render_widget(panel, area);

    // TODO: Render summary card overlay
    // For MVP, we'll just show the leaderboard  ← PLACEHOLDER
}
```

All 6 drill-down states just render the leaderboard placeholder.

**App Initialization:**

From app.rs:
```rust
pub fn new() -> Self {
    Self {
        theme: Theme::default(),
        drill_down: DrillDownState::Leaderboard,
        results: Vec::new(),  // ← EMPTY! No data loaded
        results_by_id: HashMap::new(),
        selected_index: 0,
        should_quit: false,
        error_message: None,
    }
}
```

**Completion:** 30% (skeleton exists, but NO actual features implemented beyond navigation shell)

---

### M11: Reporting/Artifacts

**Roadmap Specification (line 85):**
- "M11 (reporting/artifacts)"

**What Was Planned:**
- Unknown (roadmap doesn't detail M11 in first 2000 lines)

**What Was Actually Built:**
- ❌ NOTHING - M11 was conflated with M12

**Completion:** 0% (skipped/merged into M12?)

---

### M12: Hardening & Performance

**Roadmap Specification (line 88):**
- "M12 (hardening: perf/regression/docs)"

**What Was Planned:**
- Performance profiling
- Regression test suite
- Production documentation

**What Was Actually Built:**
- ✅ Criterion benchmarks (5 groups)
- ✅ Profiling infrastructure (ProfileScope, flame graphs)
- ✅ Regression tests (7 golden tests)
- ✅ PERFORMANCE.md (348 lines)
- ✅ PRODUCTION.md (413 lines)

**Completion:** 100% (exceeded expectations on docs)

---

## Critical Missing Features

### 1. Data Integration - Yahoo Finance

**User Expectation:**
> "We're supposed to be pulling in data from yahoo finance"

**Roadmap Guidance (line 95):**
> "M2 Data: start with Parquet ingest + local lists only (no vendor APIs)."

**Current State:**
- NO Yahoo Finance integration
- NO live data fetching
- System expects pre-existing Parquet files
- No data ingestion from external sources

**Gap Analysis:**
- Roadmap said "start with Parquet" (implying Yahoo Finance would come later)
- User expected Yahoo Finance to be integrated by M12
- This was NEVER implemented in any milestone

**To Implement:**
1. Add `yfinance` crate or HTTP client
2. Create `YahooFinanceIngestor` in trendlab-runner
3. Add download command: `trendlab-cli download --symbols SPY,QQQ --start 2020-01-01`
4. Cache downloaded data as Parquet
5. TUI integration: "Load from Yahoo Finance" button

---

### 2. TUI Tabbed Interface

**User Expectation:**
> "There should be a tab for charts and a tab for leaderboards (current session and all-time)"

**Roadmap Guidance:**
- M10: "ship 4 core panels first"
- Checkpoint C: "Ghost Curve", "Drill-Down Completeness"

**Current State:**
- NO tabs
- NO charts panel
- NO current vs all-time session toggle
- Just one static leaderboard view

**Gap Analysis:**
- Roadmap focused on "drill-down" (vertical navigation)
- User expected "tabs" (horizontal navigation between views)
- This mismatch suggests roadmap didn't capture full UI requirements

**To Implement:**
1. Add `TabState` enum: `Charts | CurrentSession | AllTime`
2. Tab switcher UI (Tab key to switch)
3. Charts panel with ratatui-charts
4. Session persistence (save current session results)
5. All-time leaderboard (load from database)

---

### 3. Data Loading in TUI

**User Expectation:**
> "I can't tell if it's pulling in data for the 500+ securities tickers or if it's running some other analysis"

**Current State:**
- App starts with `results: Vec::new()` (empty)
- NO loading indicator
- NO data fetch on startup
- NO background processing

**Gap Analysis:**
- Roadmap didn't specify data loading UX
- TUI is a "viewer" with no data pipeline connection
- Missing integration layer between runner and TUI

**To Implement:**
1. Add `load_results()` method to App
2. Load sweep results from Parquet on startup
3. Show loading indicator while fetching
4. Background data refresh (watch for new results)
5. Command-line arg: `trendlab-tui --results ./results/sweep_001.parquet`

---

### 4. Ghost Curve (Roadmap M10 Feature)

**Roadmap Specification (lines 145-148):**
> "Death Crossing Analysis: Flag strategies where Ghost Curve (ideal) vs Real Curve diverge >15%"
> "Marks execution-fragile strategies"
> "Visible in TUI drill-down view"

**Current State:**
- ❌ NOT implemented
- NO ghost curve calculation
- NO ideal vs real comparison
- drill_down/ghost_curve.rs exists (176 lines) but is NOT integrated into TUI

**Gap Analysis:**
- Roadmap explicitly required this for M10 Checkpoint C
- Code was stubbed but never wired into the TUI render loop
- This is a CORE feature for execution sensitivity analysis

**To Implement:**
1. Wire `ghost_curve::GhostCurveAnalyzer` into App
2. Compute ghost curve for selected strategy on drill-down
3. Render dual equity curves (real vs ideal)
4. Flag >15% divergence with warning color
5. Add to drill-down panel (not just placeholder)

---

## Roadmap Interpretation Issues

### Issue 1: "Escape Hatches" vs User Expectations

**Roadmap Line 95:**
> "M2 Data: start with Parquet ingest + local lists only (no vendor APIs)."

**Interpretation:**
- **Roadmap intent:** Ship MVP without Yahoo Finance complexity
- **User expectation:** Yahoo Finance would be added post-MVP
- **Reality:** Never implemented, not even mentioned in M9-M12

**Resolution:**
- Clarify "escape hatches" means "deferred indefinitely" vs "coming later"
- User expected production-ready data pipeline, got test harness

---

### Issue 2: "Ship 4 Core Panels First" Ambiguity

**Roadmap Line 99:**
> "M10 TUI: ship 4 core panels first; expand to full suite after runner is solid."

**Interpretation:**
- **Roadmap intent:** Build basic panels (leaderboard, summary, trade tape, chart)
- **Reality:** Built 1 panel (leaderboard), all others are placeholders with "TODO"

**Resolution:**
- "Ship" was interpreted as "stub with TODO comments"
- Should have meant "fully functional panels"

---

### Issue 3: M11 Missing

**Roadmap Line 85:**
> "M11 (reporting/artifacts)"

**Issue:**
- NO M11 deliverables in first 2000 lines of roadmap
- M11 appears to have been skipped or merged into M12
- User has no visibility into what M11 was supposed to deliver

**Resolution:**
- Either M11 is defined later in the 10,000+ line roadmap, OR
- M11 was intended but never specified

---

## Functionality Matrix

| Feature | Roadmap | Implemented | Usable | Notes |
|---------|---------|-------------|--------|-------|
| **Backend Engine** | ✅ | ✅ | ✅ | Fully functional |
| Domain model | ✅ | ✅ | ✅ | Bar, Order, Fill, etc. |
| Event loop | ✅ | ✅ | ✅ | Bar-by-bar execution |
| Order system | ✅ | ✅ | ✅ | OCO, brackets, stop/limit |
| Position management | ✅ | ✅ | ✅ | Trailing stops, scaling |
| Cache + runner | ✅ | ✅ | ✅ | Parquet cache working |
| **Data Integration** | ⚠️ | ❌ | ❌ | No Yahoo Finance |
| Parquet ingest | ✅ | ✅ | ✅ | Manual Parquet only |
| Yahoo Finance | ⚠️ | ❌ | ❌ | "Escape hatch" deferred |
| Live data fetch | ❌ | ❌ | ❌ | Not in roadmap |
| **Robustness Ladder** | ✅ | ⚠️ | ⚠️ | Partially functional |
| CheapPass (L1) | ✅ | ✅ | ✅ | Working |
| WalkForward (L2) | ✅ | ✅ | ✅ | Working |
| ExecutionMC (L3) | ✅ | ✅ | ✅ | Working |
| PathMC (L4) | ✅ | ⚠️ | ❌ | Scaffolded only |
| Bootstrap (L5) | ✅ | ⚠️ | ❌ | Scaffolded only |
| Stability scoring | ✅ | ✅ | ✅ | IQR, promotion |
| **TUI Features** | ✅ | ⚠️ | ❌ | Skeleton only |
| Leaderboard panel | ✅ | ✅ | ✅ | Working |
| Navigation | ✅ | ✅ | ✅ | Up/down/drill-down |
| Theme | ✅ | ✅ | ✅ | Parrot/neon colors |
| Tabbed interface | ⚠️ | ❌ | ❌ | User expected, not in roadmap |
| Charts panel | ⚠️ | ❌ | ❌ | Expected but not implemented |
| Current/All-time toggle | ⚠️ | ❌ | ❌ | User expected, not in roadmap |
| Ghost curve | ✅ | ⚠️ | ❌ | Code exists, not integrated |
| Summary card | ✅ | ❌ | ❌ | TODO placeholder |
| Trade tape | ✅ | ❌ | ❌ | TODO placeholder |
| Chart drill-down | ✅ | ❌ | ❌ | TODO placeholder |
| Diagnostics | ✅ | ❌ | ❌ | TODO placeholder |
| Rejected intents | ✅ | ❌ | ❌ | TODO placeholder |
| Execution lab | ✅ | ❌ | ❌ | TODO placeholder |
| Data loading | ❌ | ❌ | ❌ | App starts empty |
| Loading indicator | ❌ | ❌ | ❌ | No feedback during load |
| **Performance** | ✅ | ✅ | ✅ | Exceeded expectations |
| Benchmarks | ✅ | ✅ | ✅ | 5 Criterion groups |
| Profiling | ✅ | ✅ | ✅ | ProfileScope + flame |
| Regression tests | ✅ | ✅ | ✅ | 7 golden tests |
| Docs | ✅ | ✅ | ✅ | 750+ lines |

**Legend:**
- ✅ = Fully implemented and working
- ⚠️ = Partially implemented or scaffolded
- ❌ = Not implemented
- Empty = Not applicable

---

## Test Coverage Analysis

| Component | Unit Tests | Integration Tests | BDD Tests | Total | Coverage |
|-----------|------------|-------------------|-----------|-------|----------|
| trendlab-core | 224 | - | - | 224 | High |
| trendlab-runner (lib) | 81 | 22 | 9 | 112 | High |
| trendlab-runner (regression) | 7 | - | - | 7 | Medium |
| trendlab-runner (robustness) | 32 | 22 | 9 | 63 | High |
| trendlab-tui | 49 | - | - | 49 | **Low** |
| **Total** | **393** | **44** | **18** | **455** | - |

**TUI Test Gap:**
- 49 tests cover basic structs and navigation
- ZERO tests for:
  - Data loading
  - Drill-down rendering
  - Ghost curve display
  - Tab switching (doesn't exist)
  - Charts (doesn't exist)

---

## What Actually Works (As of M12)

### ✅ Fully Functional
1. **Backend engine** - Can run backtests with real execution simulation
2. **Robustness Levels 1-3** - CheapPass, WalkForward, ExecutionMC work correctly
3. **Leaderboard ranking** - Sorts strategies by fitness
4. **Cache system** - Parquet-based result caching
5. **Benchmarks** - Criterion performance measurement
6. **Profiling** - Flame graphs and manual instrumentation
7. **Docs** - PERFORMANCE.md and PRODUCTION.md are comprehensive

### ⚠️ Partially Functional
1. **Robustness Levels 4-5** - PathMC and Bootstrap are scaffolds (need real implementation)
2. **TUI navigation** - Works but only shows leaderboard
3. **Ghost curve** - Code exists but not integrated
4. **Drill-down** - State machine works but views are placeholders

### ❌ Not Functional
1. **Yahoo Finance integration** - Completely missing
2. **Data loading in TUI** - App starts empty, no load mechanism
3. **TUI tabs** - No tabbed interface
4. **Charts** - No equity curve visualization
5. **Session management** - No current vs all-time distinction
6. **All drill-down views** - SummaryCard, TradeTape, ChartWithTrade, etc. are TODOs

---

## Priority Gaps (Recommended Fix Order)

### P0: Critical - Blocks User Experience
1. **Data loading in TUI** - App is unusable without data
   - Effort: 2 hours
   - Impact: High (unblocks everything else)

2. **Yahoo Finance integration** - User explicitly expects this
   - Effort: 4 hours
   - Impact: High (core feature)

### P1: High - Major Missing Features
3. **Implement PathMC hooks** - Currently just a placeholder
   - Effort: 6 hours
   - Impact: Medium (robustness ladder incomplete)

4. **Implement Bootstrap resampling** - Currently just a placeholder
   - Effort: 6 hours
   - Impact: Medium (robustness ladder incomplete)

5. **Tabbed interface** - Charts | Current | All-Time
   - Effort: 4 hours
   - Impact: High (user expectation)

6. **Charts panel** - Equity curve rendering
   - Effort: 8 hours
   - Impact: High (user expectation)

### P2: Medium - Polish and Completeness
7. **Integrate Ghost Curve** - Code exists, wire it up
   - Effort: 3 hours
   - Impact: Medium (roadmap feature)

8. **Implement drill-down views** - SummaryCard, TradeTape, etc.
   - Effort: 12 hours
   - Impact: Medium (roadmap Checkpoint C)

9. **Loading indicator** - Visual feedback during data load
   - Effort: 1 hour
   - Impact: Low (UX polish)

### P3: Low - Nice to Have
10. **Session management** - Current vs all-time persistence
    - Effort: 4 hours
    - Impact: Low (user mentioned but not critical)

---

## Recommendations

### Immediate Actions (Ship Usable MVP)
1. **Add mock data loading** (30 min)
   - Generate 10-20 sample BacktestResults in App::new()
   - User can at least SEE the TUI working

2. **Document "what works"** (1 hour)
   - Update README with clear status
   - Set expectations: "TUI is viewer-only, no data pipeline yet"

### Short-Term (Next 2 weeks)
3. **Implement Yahoo Finance downloader** (1 day)
   - Add `yfinance-rs` or HTTP client
   - Create CLI command: `trendlab-cli fetch --symbols SPY,QQQ`
   - Save to Parquet cache

4. **Wire data loading into TUI** (1 day)
   - Add `--results <path>` CLI arg
   - Load BacktestResults from Parquet on startup
   - Show loading spinner

5. **Add tabbed interface** (2 days)
   - Implement TabState enum
   - Add tab switcher UI (Tab key)
   - Create Charts panel with equity curves

### Medium-Term (Next month)
6. **Complete PathMC and Bootstrap** (1 week)
   - Implement real path policy switching
   - Implement real block bootstrap resampling
   - Add comprehensive tests

7. **Complete all drill-down views** (1-2 weeks)
   - SummaryCard with metrics
   - TradeTape with scrollable trade list
   - ChartWithTrade with entry/exit markers
   - Diagnostics with execution details

8. **Integrate Ghost Curve** (3 days)
   - Wire into drill-down state machine
   - Render dual equity curves
   - Flag execution-fragile strategies

### Long-Term (Next quarter)
9. **Live data integration** (2 weeks)
   - Background data refresh
   - Watch file system for new results
   - Auto-reload leaderboard

10. **Session management** (1 week)
    - Save current session to SQLite
    - Load all-time results from database
    - Toggle between current and historical

---

## Conclusion

**What the roadmap promised:**
- Full backend engine (M0-M8) ✅
- Robustness ladder with 5 levels (M9) ⚠️ (Levels 4-5 scaffolded)
- TUI with drill-down and ghost curve (M10) ❌ (30% complete)
- Reporting/artifacts (M11) ❌ (skipped?)
- Performance hardening (M12) ✅

**What was actually delivered:**
- Full backend engine ✅
- Robustness ladder (Levels 1-3 work, 4-5 scaffolded) ⚠️
- TUI navigation shell (no data, no tabs, no charts, no drill-downs) ❌
- Excellent docs and benchmarks ✅✅

**Overall Assessment:**
- **Backend:** Production-ready (90% complete)
- **TUI:** Prototype only (30% complete)
- **Data Integration:** Missing (0% complete)

**User Frustration Justified:**
- Empty TUI on launch (no data)
- No Yahoo Finance (expected feature)
- No tabs/charts (expected features)
- Drill-downs are placeholders (not mentioned until user drilled down)

**Path Forward:**
- P0: Load sample data so TUI is visible
- P1: Add Yahoo Finance integration
- P1: Add tabs + charts panel
- P2: Complete PathMC/Bootstrap implementation
- P2: Implement all drill-down views
- P3: Session management

**Estimated Effort to "Done-Done":**
- P0 work: 2-4 days
- P1 work: 1-2 weeks
- P2 work: 2-3 weeks
- **Total: 4-6 weeks to fully complete roadmap vision**

---

## Appendix: Code Evidence

### Empty App Initialization
```rust
// trendlab-tui/src/app.rs:34
pub fn new() -> Self {
    Self {
        theme: Theme::default(),
        drill_down: DrillDownState::Leaderboard,
        results: Vec::new(),  // ← EMPTY
        results_by_id: HashMap::new(),
        selected_index: 0,
        should_quit: false,
        error_message: None,
    }
}
```

### Placeholder Drill-Down Views
```rust
// trendlab-tui/src/main.rs:89-99
DrillDownState::SummaryCard(_run_id) => {
    let panel = LeaderboardPanel::new(...);
    f.render_widget(panel, area);

    // TODO: Render summary card overlay
    // For MVP, we'll just show the leaderboard
}
```

### Scaffolded PathMC
```rust
// trendlab-runner/src/robustness/levels/path_mc.rs:180-185
// TODO: In production, this would:
// 1. Get path_policy from config
// 2. Override with sampled mode (WorstCase, BestCase, Random, etc.)
// 3. Run backtest with varied path policy
// 4. Collect results for each trial
```

### Scaffolded Bootstrap
```rust
// trendlab-runner/src/robustness/levels/bootstrap.rs:295-300
// TODO: In production, this would:
// 1. Sample blocks from date range
// 2. Permute block order
// 3. Run backtest on resampled data
// 4. Collect results for each trial
```

---

**End of Report**