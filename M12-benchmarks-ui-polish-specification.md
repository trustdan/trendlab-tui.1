# M12: Benchmarks & UI Polish — TrendLab v3

**Status:** Not Started
**Depends on:** M0–M11 (all prior milestones)
**Estimated effort:** 2–3 weeks
**Purpose:** Deliver production-grade performance benchmarks, profiling infrastructure, and a polished, responsive TUI experience.

---

## Why M12 Exists

After 11 milestones of building a research-grade backtesting engine with rigorous execution realism and robustness validation, **M12 ensures TrendLab v3 is production-ready**:

1. **Performance visibility:** Benchmark hot loops, detect regressions, prove scalability claims
2. **Optimization guidance:** Profile actual bottlenecks (not guesses), measure improvements
3. **User experience:** Polish TUI animations, themes, responsiveness, and error messaging
4. **Release readiness:** Documentation, examples, CI/CD, versioning

---

## M12 Deliverables

### A) Performance Benchmarks (`trendlab-core/benches/`)

#### 1. **Criterion Benchmark Suite** (7 benchmark modules)

```
trendlab-core/benches/
├── bench_signals.rs          # Signal generation (100-1000 bars)
├── bench_order_policy.rs     # Order generation per bar
├── bench_execution.rs        # Fill simulation (path policies)
├── bench_portfolio.rs        # Portfolio accounting updates
├── bench_position_mgmt.rs    # Trailing stop calculations
├── bench_monte_carlo.rs      # L3/L4 MC overhead
└── bench_end_to_end.rs       # Full backtest pipeline
```

#### 2. **Key Metrics per Benchmark**

| Benchmark | Input Scale | Target Throughput | Regression Threshold |
|-----------|-------------|-------------------|----------------------|
| Signal generation | 1000 bars | > 100k bars/sec | +10% |
| Order policy | 1000 bars × 20 assets | > 50k orders/sec | +15% |
| Fill simulation | 10k orders | > 20k fills/sec | +20% |
| Portfolio update | 1000 bars × 50 positions | > 10k updates/sec | +15% |
| Trailing stop calc | 1000 bars × 20 positions | > 50k calcs/sec | +10% |
| L3 Exec MC | 100 trials | < 5 sec total | +25% |
| L4 Path MC | 50 paths × 100 exec | < 30 sec total | +30% |
| End-to-end | 10 years × 10 assets | < 2 sec | +20% |

**Regression threshold:** If runtime increases by more than threshold, CI fails.

#### 3. **Benchmark Scenarios**

```rust
// bench_end_to_end.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

pub fn bench_donchian_backtest(c: &mut Criterion) {
    let data = load_test_data("SPY_10y.parquet");
    let signal = DonchianSignal::new(20, 10);
    let exec = ExecutionModel::L1WorstCase { slippage_bps: 5 };

    c.bench_function("donchian_10y_spy_l1", |b| {
        b.iter(|| {
            let bt = Backtest::new(black_box(&data), black_box(&signal), black_box(&exec));
            bt.run()
        });
    });
}

pub fn bench_l4_path_mc(c: &mut Criterion) {
    let data = load_test_data("SPY_1y.parquet");
    let signal = DonchianSignal::new(20, 10);
    let exec = ExecutionModel::L4PathMC {
        num_paths: 50,
        num_exec_trials: 100,
        slippage: SlippageDistribution::Normal { mean_bps: 5, std_bps: 2 },
    };

    c.bench_function("l4_path_mc_1y_spy", |b| {
        b.iter(|| {
            let bt = Backtest::new(black_box(&data), black_box(&signal), black_box(&exec));
            bt.run()
        });
    });
}

criterion_group!(benches, bench_donchian_backtest, bench_l4_path_mc);
criterion_main!(benches);
```

---

### B) Profiling Infrastructure (`trendlab-runner/profiling/`)

#### 1. **Profiling Harness** (3 modules)

```
trendlab-runner/profiling/
├── mod.rs                # ProfilerConfig, ProfilerOutput
├── flamegraph.rs         # Generate flamegraphs (cargo-flamegraph)
└── allocation.rs         # Track allocations (dhat-rs)
```

#### 2. **`ProfilerConfig`** — What to Profile

```rust
pub struct ProfilerConfig {
    pub mode: ProfilingMode,
    pub output_dir: PathBuf,
    pub scenario: ProfilingScenario,
}

pub enum ProfilingMode {
    Cpu,          // Flamegraph (sampling profiler)
    Allocations,  // dhat-rs (heap profiling)
}

pub enum ProfilingScenario {
    SignalGeneration { bars: usize },
    FullBacktest { years: usize, assets: usize },
    L4PathMC { paths: usize, exec_trials: usize },
    L5Bootstrap { bootstrap_trials: usize },
}
```

#### 3. **Flamegraph Generation**

```rust
// profiling/flamegraph.rs
pub fn generate_flamegraph(config: &ProfilerConfig) -> Result<PathBuf> {
    let output = config.output_dir.join("flamegraph.svg");

    // Run workload with perf/dtrace
    let scenario = build_scenario(&config.scenario)?;
    let mut profiler = CpuProfiler::new()?;

    profiler.start()?;
    scenario.run()?;
    profiler.stop()?;

    // Generate SVG
    profiler.save_flamegraph(&output)?;
    Ok(output)
}
```

#### 4. **Allocation Profiling**

```rust
// profiling/allocation.rs
use dhat::{Dhat, DhatAlloc};

#[global_allocator]
static ALLOCATOR: DhatAlloc = DhatAlloc;

pub fn profile_allocations(config: &ProfilerConfig) -> Result<AllocationReport> {
    let _dhat = Dhat::start_heap_profiling();

    let scenario = build_scenario(&config.scenario)?;
    scenario.run()?;

    // Extract stats
    Ok(AllocationReport {
        total_allocated: _dhat.total_allocated_bytes(),
        peak_memory: _dhat.peak_memory_bytes(),
        num_allocations: _dhat.num_allocations(),
    })
}
```

---

### C) TUI Polish (`trendlab-tui/`)

#### 1. **Theme System** (neon parrot theme + customization)

```rust
// trendlab-tui/theme.rs
pub struct Theme {
    pub background: Color,
    pub accent: Color,          // electric cyan
    pub positive: Color,        // neon green
    pub negative: Color,        // hot pink
    pub warning: Color,         // neon orange
    pub neutral: Color,         // cool purple
    pub muted: Color,           // steel blue
}

impl Theme {
    pub fn parrot() -> Self {
        Self {
            background: Color::Rgb(18, 18, 20),      // near-black
            accent: Color::Rgb(0, 255, 255),         // electric cyan
            positive: Color::Rgb(57, 255, 20),       // neon green
            negative: Color::Rgb(255, 16, 240),      // hot pink
            warning: Color::Rgb(255, 159, 10),       // neon orange
            neutral: Color::Rgb(155, 135, 245),      // cool purple
            muted: Color::Rgb(100, 149, 237),        // steel blue
        }
    }
}
```

#### 2. **Progress Animations** (pacman bar + spinners)

```rust
// trendlab-tui/widgets/progress.rs
pub struct PacmanBar {
    progress: f64,        // 0.0 to 1.0
    stage: String,        // e.g., "L4 Path MC"
}

impl PacmanBar {
    pub fn render(&self) -> String {
        const WIDTH: usize = 16;
        let filled = (self.progress * WIDTH as f64).floor() as usize;
        let empty = WIDTH.saturating_sub(filled);

        let bar = if filled == 0 {
            format!("[ᗧ{}]", "·".repeat(WIDTH))
        } else if filled == WIDTH {
            format!("[{}ᗧ]", ".".repeat(WIDTH))
        } else {
            format!("[{}ᗧ{}]", ".".repeat(filled), "·".repeat(empty))
        };

        format!("{} {:3}% ({})", bar, (self.progress * 100.0) as u8, self.stage)
    }
}

// Example output:
// [ᗧ··············] 0%   (initializing)
// [......ᗧ········] 40%  (L3 Exec MC)
// [..............ᗧ] 100% (complete)
```

#### 3. **Responsive Layout** (terminal resize handling)

```rust
// trendlab-tui/layout.rs
pub struct ResponsiveLayout {
    pub min_width: u16,
    pub min_height: u16,
}

impl ResponsiveLayout {
    pub fn compute(&self, area: Rect) -> LayoutConfig {
        if area.width < self.min_width || area.height < self.min_height {
            LayoutConfig::Compact  // Hide panels, show essentials only
        } else if area.width < 120 {
            LayoutConfig::Medium   // 2-column layout
        } else {
            LayoutConfig::Wide     // 3-column layout with side panels
        }
    }
}

pub enum LayoutConfig {
    Compact,   // < 80×24: single column, minimal widgets
    Medium,    // 80×24 to 120×40: 2 columns
    Wide,      // > 120×40: 3 columns + side panels
}
```

#### 4. **Error Messaging** (user-friendly, actionable)

```rust
// trendlab-tui/error_display.rs
pub fn format_error(err: &BacktestError) -> Vec<String> {
    match err {
        BacktestError::InsufficientData { required, available } => {
            vec![
                "❌ Insufficient data for backtest".to_string(),
                format!("   Required: {} bars", required),
                format!("   Available: {} bars", available),
                "   → Try reducing lookback period or using longer dataset".to_string(),
            ]
        }
        BacktestError::OrderRejected { reason } => {
            vec![
                "❌ Order rejected by execution model".to_string(),
                format!("   Reason: {}", reason),
                "   → Check position sizing and capital constraints".to_string(),
            ]
        }
        BacktestError::InvalidConfig { field, message } => {
            vec![
                format!("❌ Invalid configuration: {}", field),
                format!("   {}", message),
                "   → Review config file or use --validate flag".to_string(),
            ]
        }
        _ => vec![format!("❌ Error: {}", err)],
    }
}
```

#### 5. **Keyboard Shortcuts** (modal + vim-like navigation)

```
Global:
  q / Esc       Quit / back
  ?             Help overlay
  /             Search
  Tab           Next panel
  Shift+Tab     Previous panel

Leaderboard View:
  j / k         Navigate down/up
  g / G         Top / bottom
  Enter         View detail
  d             Delete candidate
  e             Export to CSV

Detail View:
  h / l         Previous/next metric
  c             Compare mode
  p             Export plot (PNG)
  r             Re-run backtest

Backtest Running:
  Space         Pause/resume
  s             Skip to next phase
  Ctrl+C        Abort
```

---

### D) Documentation & Examples

#### 1. **User Guide** (`docs/user-guide.md`)

- Installation (Cargo, pre-built binaries)
- Quick start (5-minute tutorial)
- Configuration (signals, execution models, position management)
- TUI walkthrough (screenshots, navigation)
- Troubleshooting (common errors, performance tips)

#### 2. **API Reference** (`docs/api-reference.md`)

- Core traits (`Signal`, `OrderPolicy`, `ExecutionModel`, `PositionManager`)
- Execution models (L1–L5)
- Monte Carlo APIs (L3, L4, L5)
- Data loading (`TimeSeriesData`, Parquet readers)

#### 3. **Example Strategies** (`examples/`)

```
examples/
├── donchian_basic.rs        # Simple Donchian breakout
├── ma_crossover.rs          # MA crossover with trailing stop
├── multi_asset_portfolio.rs # 10 assets with correlation
├── custom_signal.rs         # Implementing Signal trait
└── l5_bootstrap_sweep.rs    # Full L1→L5 promotion ladder
```

---

## File Structure (M12 additions)

```
trendlab-v3/
├── trendlab-core/
│   └── benches/
│       ├── bench_signals.rs
│       ├── bench_order_policy.rs
│       ├── bench_execution.rs
│       ├── bench_portfolio.rs
│       ├── bench_position_mgmt.rs
│       ├── bench_monte_carlo.rs
│       └── bench_end_to_end.rs
│
├── trendlab-runner/
│   └── profiling/
│       ├── mod.rs
│       ├── flamegraph.rs
│       └── allocation.rs
│
├── trendlab-tui/
│   ├── theme.rs                  # Theme system
│   ├── widgets/
│   │   ├── progress.rs           # Pacman bar, spinners
│   │   └── error_display.rs     # User-friendly errors
│   └── layout.rs                 # Responsive layout
│
├── docs/
│   ├── user-guide.md
│   ├── api-reference.md
│   └── performance.md            # Benchmark results
│
└── examples/
    ├── donchian_basic.rs
    ├── ma_crossover.rs
    ├── multi_asset_portfolio.rs
    ├── custom_signal.rs
    └── l5_bootstrap_sweep.rs
```

**Total files:** ~25
**Estimated lines of code:** ~3,500 (benchmarks ~1,500, profiling ~500, TUI polish ~800, docs ~700)

---

## BDD Scenarios (M12)

### Feature 1: Core Benchmarks (`features/benchmark_core.feature`)

```gherkin
Feature: Core Performance Benchmarks
  Scenario: Signal generation throughput
    Given a dataset with 1000 bars
    When I benchmark Donchian signal generation
    Then throughput should exceed 100k bars/sec

  Scenario: Order policy throughput
    Given 1000 bars and 20 assets
    When I benchmark order generation
    Then throughput should exceed 50k orders/sec

  Scenario: Fill simulation throughput
    Given 10000 pending orders
    When I benchmark fill simulation with WorstCase policy
    Then throughput should exceed 20k fills/sec

  Scenario: Portfolio update throughput
    Given 1000 bars and 50 active positions
    When I benchmark portfolio accounting
    Then throughput should exceed 10k updates/sec
```

### Feature 2: Monte Carlo Benchmarks (`features/benchmark_monte_carlo.feature`)

```gherkin
Feature: Monte Carlo Performance Benchmarks
  Scenario: L3 Execution MC overhead
    Given a 1-year dataset and 100 execution trials
    When I benchmark L3 Execution MC
    Then total runtime should be under 5 seconds

  Scenario: L4 Path MC overhead
    Given a 1-year dataset, 50 paths, and 100 execution trials per path
    When I benchmark L4 Path MC
    Then total runtime should be under 30 seconds

  Scenario: L5 Bootstrap overhead
    Given a 2-year dataset, 20 bootstrap trials, 10 paths, and 20 exec trials
    When I benchmark L5 Bootstrap
    Then total runtime should scale linearly with num_bootstrap_trials

  Scenario: Regression detection
    Given baseline benchmark results stored in CI
    When I run current benchmarks
    Then no benchmark should regress by more than threshold (10-30%)
```

### Feature 3: Profiling (`features/profiling.feature`)

```gherkin
Feature: Profiling Infrastructure
  Scenario: CPU flamegraph generation
    Given a full backtest workload (10 years, 10 assets)
    When I profile CPU usage with flamegraph mode
    Then a flamegraph.svg file is generated
    And the top function should be identified (e.g., fill_simulation, signal_compute)

  Scenario: Allocation profiling
    Given a full backtest workload
    When I profile memory allocations with dhat
    Then total allocated bytes should be under 500 MB
    And peak memory should be under 100 MB

  Scenario: Hot loop identification
    Given profiling results
    When I analyze function call distribution
    Then top 5 functions should account for > 80% of runtime
```

### Feature 4: TUI Theme System (`features/tui_theme.feature`)

```gherkin
Feature: TUI Theme System
  Scenario: Parrot theme colors
    Given the TUI is initialized with parrot theme
    When I render the leaderboard view
    Then background is near-black (18,18,20)
    And accent borders are electric cyan (0,255,255)
    And positive Sharpe is neon green (57,255,20)
    And negative Sharpe is hot pink (255,16,240)

  Scenario: Custom theme override
    Given a custom theme config file
    When I load the TUI with --theme custom.toml
    Then all colors match the custom config

  Scenario: High-contrast mode
    Given a user with accessibility needs
    When I enable --high-contrast flag
    Then all text has minimum 4.5:1 contrast ratio
```

### Feature 5: Progress Animations (`features/tui_progress.feature`)

```gherkin
Feature: TUI Progress Animations
  Scenario: Pacman bar rendering
    Given a backtest at 40% progress in "L3 Exec MC" stage
    When I render the pacman bar
    Then output is "[......ᗧ········] 40% (L3 Exec MC)"
    And bar width is exactly 18 characters (16 pellets + 2 brackets)

  Scenario: Stage transitions
    Given a multi-stage backtest (L1 → L2 → L3 → L4)
    When each stage completes
    Then pacman bar updates with new stage label
    And progress resets to 0% at start of each stage

  Scenario: Completion animation
    Given a backtest at 100% progress
    When I render the final frame
    Then output is "[..............ᗧ] 100% (complete)"
    And the bar displays for 0.5 seconds before disappearing
```

### Feature 6: Responsive Layout (`features/tui_layout.feature`)

```gherkin
Feature: Responsive TUI Layout
  Scenario: Compact layout (< 80×24)
    Given terminal size is 70×20
    When I render the leaderboard
    Then layout is single-column
    And only essential columns are shown (Rank, Sharpe, DD)

  Scenario: Medium layout (80×24 to 120×40)
    Given terminal size is 100×30
    When I render the leaderboard
    Then layout is 2-column
    And side panel shows summary stats

  Scenario: Wide layout (> 120×40)
    Given terminal size is 140×50
    When I render the leaderboard
    Then layout is 3-column
    And right panel shows equity curve mini-plot

  Scenario: Terminal resize handling
    Given the TUI is running in medium layout
    When terminal is resized to compact size
    Then layout immediately switches to compact mode
    And no visual artifacts appear
```

### Feature 7: Error Display (`features/tui_errors.feature`)

```gherkin
Feature: User-Friendly Error Messages
  Scenario: Insufficient data error
    Given a backtest requires 500 bars but only 300 are available
    When the error is displayed
    Then message shows "❌ Insufficient data for backtest"
    And includes "Required: 500 bars, Available: 300 bars"
    And suggests "→ Try reducing lookback period"

  Scenario: Order rejection error
    Given an order is rejected due to insufficient capital
    When the error is displayed
    Then message shows "❌ Order rejected by execution model"
    And includes reason "Insufficient capital: need $10k, have $5k"
    And suggests "→ Check position sizing and capital constraints"

  Scenario: Invalid config error
    Given a config file has invalid execution model "L6"
    When the config is loaded
    Then error shows "❌ Invalid configuration: execution_model"
    And includes message "Unknown model 'L6', expected L1-L5"
    And suggests "→ Review config file or use --validate flag"
```

### Feature 8: Keyboard Navigation (`features/tui_keyboard.feature`)

```gherkin
Feature: TUI Keyboard Navigation
  Scenario: Vim-like list navigation
    Given the leaderboard has 50 candidates
    When I press 'j' 10 times
    Then selection moves down 10 rows
    When I press 'G'
    Then selection jumps to last row (50)
    When I press 'g'
    Then selection jumps to first row (1)

  Scenario: Modal shortcuts
    Given I'm in leaderboard view
    When I press 'Enter'
    Then detail view opens for selected candidate
    When I press 'Esc'
    Then I return to leaderboard view

  Scenario: Search mode
    Given the leaderboard is visible
    When I press '/'
    Then search input appears at bottom
    When I type "donchian"
    Then only candidates matching "donchian" are shown
```

### Feature 9: Documentation Examples (`features/documentation.feature`)

```gherkin
Feature: Documentation & Examples
  Scenario: Example strategies compile
    Given all example files in examples/ directory
    When I run "cargo build --examples"
    Then all examples compile without errors

  Scenario: Example strategies run
    Given example "donchian_basic.rs"
    When I run "cargo run --example donchian_basic"
    Then backtest completes successfully
    And output shows Sharpe ratio and max drawdown

  Scenario: User guide accuracy
    Given the user guide quick start tutorial
    When I follow steps 1-5 exactly as written
    Then I successfully run my first backtest
    And TUI displays results as shown in screenshots
```

---

## Completion Criteria (M12)

### Architecture (10 items)
- [ ] 7 Criterion benchmark modules (`bench_*.rs`)
- [ ] 3 profiling modules (`flamegraph.rs`, `allocation.rs`, `mod.rs`)
- [ ] Theme system with parrot color scheme
- [ ] Responsive layout engine (compact/medium/wide)
- [ ] Pacman progress bar widget
- [ ] Error display formatter
- [ ] Keyboard navigation handler
- [ ] Help overlay system
- [ ] Search/filter functionality
- [ ] Export utilities (CSV, PNG)

### Performance (5 items)
- [ ] All benchmarks meet target throughput (see table above)
- [ ] Regression thresholds configured in CI
- [ ] Flamegraph generation working for all scenarios
- [ ] Allocation profiling confirms < 500 MB total allocated
- [ ] End-to-end backtest (10y × 10 assets) runs in < 2 seconds

### TUI Polish (7 items)
- [ ] Parrot theme applied to all views
- [ ] Pacman bar animates smoothly during backtests
- [ ] Responsive layout switches correctly on terminal resize
- [ ] Error messages are user-friendly and actionable
- [ ] Keyboard shortcuts work in all views (leaderboard, detail, help)
- [ ] Search/filter highlights matches
- [ ] Export functions generate valid CSV/PNG files

### Documentation (5 items)
- [ ] User guide complete (installation → troubleshooting)
- [ ] API reference covers all public traits and structs
- [ ] 5 example strategies compile and run
- [ ] Screenshots in user guide match actual TUI appearance
- [ ] Performance.md documents benchmark results and profiling findings

### BDD Tests (9 items)
- [ ] `benchmark_core.feature` (4 scenarios)
- [ ] `benchmark_monte_carlo.feature` (4 scenarios)
- [ ] `profiling.feature` (3 scenarios)
- [ ] `tui_theme.feature` (3 scenarios)
- [ ] `tui_progress.feature` (3 scenarios)
- [ ] `tui_layout.feature` (4 scenarios)
- [ ] `tui_errors.feature` (3 scenarios)
- [ ] `tui_keyboard.feature` (3 scenarios)
- [ ] `documentation.feature` (3 scenarios)

### Release Readiness (6 items)
- [ ] CI/CD pipeline runs benchmarks on every PR
- [ ] Performance regression check fails CI if threshold exceeded
- [ ] All examples pass in CI
- [ ] Version tags follow semver (e.g., v0.1.0)
- [ ] CHANGELOG.md updated
- [ ] README.md includes badges (build status, docs, crates.io)

**Total:** 42 completion criteria

---

## Key Insights

### 1. **Benchmark-Driven Optimization**
Don't optimize blindly — let benchmarks identify real bottlenecks:
- Criterion gives statistical confidence intervals (not noisy single runs)
- Flamegraphs show where CPU time *actually* goes (not where you guess)
- Allocation profiling reveals unnecessary heap allocations in hot loops

**Example finding:**
```
Flamegraph shows 35% of time in path_monte_carlo::sample_path()
→ Profiling reveals 10M allocations for Vec<f64> in inner loop
→ Fix: Pre-allocate buffer, reuse across samples
→ Result: 3× speedup, 95% fewer allocations
```

### 2. **Regression Prevention**
Without CI benchmarks, performance silently degrades over time:
- Add innocent-looking `clone()` in hot loop → 20% slower
- Switch from `&[f64]` to `Vec<f64>` → 15% slower
- Regression detection catches this before merge

**CI workflow:**
```yaml
- name: Run benchmarks
  run: cargo bench --workspace
- name: Compare to baseline
  run: |
    critcmp baseline current
    if [[ $(critcmp --threshold 10) ]]; then
      echo "Performance regression detected!"
      exit 1
    fi
```

### 3. **TUI Polish = Production-Readiness Perception**
Even if the engine is perfect, users judge quality by the TUI:
- ✅ Smooth animations, consistent theme → "polished"
- ❌ Janky redraws, inconsistent colors → "alpha quality"

**Key polish details:**
- Pacman bar updates at 60 fps (not jumpy)
- Errors are actionable (tell users *what to do*, not just *what failed*)
- Keyboard shortcuts are discoverable (? for help overlay)
- Terminal resize doesn't break layout

### 4. **Documentation = Adoption**
Without examples and guides, users won't adopt TrendLab v3:
- Quick start tutorial → 5 minutes to first backtest
- Example strategies → copy-paste starting point
- API reference → discoverability of advanced features

**Success metric:** New user runs first backtest in < 10 minutes.

---

## Dependencies & Integration

### M12 depends on:
- **M0–M11:** All prior milestones (benchmarks test the full pipeline)
- **Criterion:** Rust benchmarking framework
- **cargo-flamegraph:** Flamegraph generation
- **dhat-rs:** Heap profiling
- **Ratatui:** TUI framework (already in use)

### M12 enables:
- **Production deployment:** Performance validation and polish complete
- **Open-source release:** Documentation and examples ready for community
- **Future optimization:** Profiling infrastructure to guide improvements

---

## Testing Strategy

### Unit Tests
- Theme color conversion (RGB → ratatui::Color)
- PacmanBar rendering (0%, 50%, 100% progress)
- Responsive layout calculation (80×24, 120×40, 160×50)
- Error message formatting (all error types)

### Integration Tests
- Benchmark suite runs without errors
- Profiling generates valid flamegraph.svg and dhat output
- TUI renders correctly in all layout modes
- Examples compile and run

### BDD Tests
- 9 features, 30 scenarios (see above)
- Covers benchmarks, profiling, TUI polish, documentation

### Performance Regression Tests (CI)
- Compare benchmark results to baseline
- Fail if any benchmark regresses beyond threshold (10–30%)

---

## Example: End-to-End Benchmark

```rust
// trendlab-core/benches/bench_end_to_end.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use trendlab_core::*;

fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");

    // Test different dataset sizes
    for years in [1, 5, 10] {
        let data = generate_test_data(years);
        let signal = DonchianSignal::new(20, 10);
        let order_policy = SimpleOrderPolicy::new();
        let exec = ExecutionModel::L1WorstCase { slippage_bps: 5 };
        let pm = TrailingStopPM::new(0.15, 0.30);

        group.bench_with_input(
            BenchmarkId::new("donchian_l1", years),
            &years,
            |b, _| {
                b.iter(|| {
                    let mut backtest = Backtest::new(
                        black_box(&data),
                        black_box(&signal),
                        black_box(&order_policy),
                        black_box(&exec),
                        black_box(&pm),
                    );
                    backtest.run()
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_full_pipeline);
criterion_main!(benches);
```

**Expected output:**
```
end_to_end/donchian_l1/1   time: [145.2 ms 148.7 ms 152.1 ms]
end_to_end/donchian_l1/5   time: [712.3 ms 728.9 ms 746.8 ms]
end_to_end/donchian_l1/10  time: [1.421 s  1.456 s  1.492 s]
```

✅ All under 2 seconds for 10 years → **passes target**

---

## Example: Pacman Progress Bar in TUI

```rust
// trendlab-tui/widgets/progress.rs
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

pub struct PacmanProgress {
    progress: f64,        // 0.0 to 1.0
    stage: String,
    theme: Theme,
}

impl Widget for PacmanProgress {
    fn render(self, area: Rect, buf: &mut Buffer) {
        const WIDTH: usize = 16;
        let filled = (self.progress * WIDTH as f64).floor() as usize;
        let empty = WIDTH.saturating_sub(filled);

        let bar = if filled == 0 {
            format!("[ᗧ{}]", "·".repeat(WIDTH))
        } else if filled >= WIDTH {
            format!("[{}ᗧ]", ".".repeat(WIDTH))
        } else {
            format!("[{}ᗧ{}]", ".".repeat(filled), "·".repeat(empty))
        };

        let percentage = (self.progress * 100.0) as u8;
        let text = format!("{} {:3}% ({})", bar, percentage, self.stage);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(self.theme.muted))
            .block(block);

        paragraph.render(area, buf);
    }
}
```

**Usage in TUI:**
```rust
let progress = PacmanProgress {
    progress: 0.67,
    stage: "L4 Path MC".to_string(),
    theme: Theme::parrot(),
};

frame.render_widget(progress, area);
```

**Rendered output:**
```
┌──────────────────────────────────────────┐
│ [..........ᗧ····] 67% (L4 Path MC)      │
└──────────────────────────────────────────┘
```

---

## Why M12 Matters

M0–M11 delivered a **research-grade backtesting engine** with rigorous execution realism and overfitting defenses.

**M12 makes it production-ready:**
- ✅ Benchmark suite proves performance claims (not guesses)
- ✅ Profiling infrastructure guides optimization (data-driven)
- ✅ Polished TUI creates professional impression (smooth, responsive)
- ✅ Documentation enables adoption (quick start, examples, API reference)
- ✅ CI/CD prevents regressions (automated checks)

**Without M12:**
- Performance is unknown (might be 10× too slow for large sweeps)
- Bottlenecks are guessed (optimization effort wasted)
- TUI feels unfinished (janky animations, unclear errors)
- Users struggle to get started (no examples or guides)

**With M12:**
- Performance is validated (2 sec for 10y backtest, 30 sec for L4 MC)
- Optimization is targeted (flamegraph shows real hot loops)
- TUI feels polished (60fps animations, parrot theme, responsive)
- Users succeed quickly (5-minute quick start, copy-paste examples)

---

## M12 Summary

| Aspect | Deliverable | Impact |
|--------|-------------|--------|
| **Benchmarks** | 7 Criterion modules | Prove scalability, detect regressions |
| **Profiling** | Flamegraph + dhat | Identify real bottlenecks (not guesses) |
| **TUI Theme** | Parrot color scheme | Consistent, recognizable aesthetic |
| **Animations** | Pacman bar, spinners | Smooth, professional feel |
| **Responsive** | 3 layout modes | Works on all terminal sizes |
| **Errors** | Actionable messages | Users know what to do (not just what failed) |
| **Keyboard** | Vim-like shortcuts | Efficient navigation |
| **Docs** | Guide + examples | 5-minute quick start, copy-paste code |
| **CI/CD** | Regression checks | Performance never degrades silently |

---

## Next Steps (After M12)

1. **Open-source release:**
   - Publish to crates.io
   - Create GitHub repository (public)
   - Add badges (build, docs, crates.io)

2. **Community onboarding:**
   - Write blog post ("Introducing TrendLab v3")
   - Share on Reddit (r/rust, r/algotrading)
   - Create video tutorial (YouTube)

3. **Future milestones (post-v1.0):**
   - M13: Intraday data support (1-minute bars)
   - M14: Live trading integration (paper trading)
   - M15: Multi-asset correlations and portfolio optimization
   - M16: GPU-accelerated Monte Carlo (CUDA/Vulkan)

---

**M12 Status:** Not Started
**Estimated Completion:** 2–3 weeks
**Blocker:** M0–M11 must be complete

Once M12 is complete, **TrendLab v3 is ready for v1.0 release.** 🎉
