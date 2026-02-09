//! TrendLab CLI — download, run, and cache management commands.
//!
//! Commands:
//! - `download` — fetch market data from Yahoo Finance and cache as Parquet
//! - `run` — execute a backtest from a TOML config file or named preset
//! - `cache status` — report cache size, symbol count, date ranges
//! - `cache clean` — remove symbols not accessed recently

use anyhow::{bail, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use trendlab_core::components::composition::StrategyPreset;
use trendlab_core::data::{
    download_symbols, CircuitBreaker, ParquetCache, StdoutProgress, YahooProvider,
};
use trendlab_runner::runner::run_single_backtest;
use trendlab_runner::{save_artifacts, BacktestConfig, BacktestResult, LoadOptions};

#[derive(Parser)]
#[command(
    name = "trendlab",
    about = "TrendLab CLI — trend-following backtesting engine"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download market data from Yahoo Finance and cache as Parquet.
    Download {
        /// Symbols to download (e.g., SPY QQQ AAPL).
        #[arg(required = true)]
        symbols: Vec<String>,

        /// Start date (YYYY-MM-DD). Defaults to 10 years ago.
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD). Defaults to today.
        #[arg(long)]
        end: Option<String>,

        /// Force re-download even if cached.
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Cache directory. Defaults to ./data.
        #[arg(long, default_value = "data")]
        cache_dir: PathBuf,
    },
    /// Execute a backtest from a TOML config file or named preset.
    Run {
        /// Path to a TOML config file.
        #[arg(long)]
        config: Option<PathBuf>,

        /// Named preset: donchian_trend, bollinger_breakout, ma_crossover, momentum_roc, supertrend.
        #[arg(long)]
        preset: Option<String>,

        /// Symbol (required with --preset).
        #[arg(long)]
        symbol: Option<String>,

        /// Start date (YYYY-MM-DD). Defaults to 5 years ago.
        #[arg(long)]
        start: Option<String>,

        /// End date (YYYY-MM-DD). Defaults to today.
        #[arg(long)]
        end: Option<String>,

        /// Offline mode: no network access.
        #[arg(long, default_value_t = false)]
        offline: bool,

        /// Use synthetic data as fallback.
        #[arg(long, default_value_t = false)]
        synthetic: bool,

        /// Cache directory. Defaults to ./data.
        #[arg(long, default_value = "data")]
        cache_dir: PathBuf,

        /// Output directory for result JSON.
        #[arg(long, default_value = "results")]
        output_dir: PathBuf,
    },
    /// Cache management commands.
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Report cache size, symbol count, and date ranges.
    Status {
        /// Cache directory. Defaults to ./data.
        #[arg(long, default_value = "data")]
        cache_dir: PathBuf,
    },
    /// Remove cached symbols not accessed within the given number of days.
    Clean {
        /// Remove symbols not accessed in this many days.
        #[arg(long)]
        unused_days: u64,

        /// Cache directory. Defaults to ./data.
        #[arg(long, default_value = "data")]
        cache_dir: PathBuf,

        /// Actually delete (without this flag, only previews what would be removed).
        #[arg(long, default_value_t = false)]
        confirm: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Download {
            symbols,
            start,
            end,
            force,
            cache_dir,
        } => run_download(symbols, start, end, force, cache_dir),
        Commands::Run {
            config,
            preset,
            symbol,
            start,
            end,
            offline,
            synthetic,
            cache_dir,
            output_dir,
        } => run_backtest_cmd(
            config, preset, symbol, start, end, offline, synthetic, cache_dir, output_dir,
        ),
        Commands::Cache { action } => match action {
            CacheAction::Status { cache_dir } => run_cache_status(&cache_dir),
            CacheAction::Clean {
                unused_days,
                cache_dir,
                confirm,
            } => run_cache_clean(&cache_dir, unused_days, confirm),
        },
    }
}

fn run_download(
    symbols: Vec<String>,
    start: Option<String>,
    end: Option<String>,
    force: bool,
    cache_dir: PathBuf,
) -> Result<()> {
    let start_date = start
        .as_deref()
        .map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .transpose()?
        .unwrap_or_else(|| chrono::Local::now().date_naive() - chrono::Duration::days(365 * 10));

    let end_date = end
        .as_deref()
        .map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .transpose()?
        .unwrap_or_else(|| chrono::Local::now().date_naive());

    let circuit_breaker = Arc::new(CircuitBreaker::default_provider());
    let provider = YahooProvider::new(circuit_breaker);
    let cache = ParquetCache::new(cache_dir);
    let progress = StdoutProgress;

    let sym_refs: Vec<&str> = symbols.iter().map(|s| s.as_str()).collect();

    let summary = download_symbols(
        &provider, &cache, &sym_refs, start_date, end_date, force, &progress,
    );

    if !summary.all_succeeded() {
        for (sym, err) in &summary.errors {
            eprintln!("Error for {sym}: {err}");
        }
        std::process::exit(1);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_backtest_cmd(
    config_path: Option<PathBuf>,
    preset_name: Option<String>,
    symbol: Option<String>,
    start: Option<String>,
    end: Option<String>,
    offline: bool,
    synthetic: bool,
    cache_dir: PathBuf,
    output_dir: PathBuf,
) -> Result<()> {
    // Validate mutually exclusive options
    if config_path.is_some() && preset_name.is_some() {
        bail!("--config and --preset are mutually exclusive");
    }
    if config_path.is_none() && preset_name.is_none() {
        bail!("one of --config or --preset is required");
    }

    // Build BacktestConfig
    let backtest_config = if let Some(path) = config_path {
        BacktestConfig::from_file(&path)?
    } else {
        let preset_name = preset_name.unwrap();
        let sym = symbol.as_deref().unwrap_or("SPY");
        build_config_from_preset(&preset_name, sym, start.as_deref(), end.as_deref())?
    };

    // Build load options
    let start_date = NaiveDate::parse_from_str(&backtest_config.backtest.start_date, "%Y-%m-%d")?;
    let end_date = NaiveDate::parse_from_str(&backtest_config.backtest.end_date, "%Y-%m-%d")?;
    let opts = LoadOptions {
        start: start_date,
        end: end_date,
        offline,
        synthetic,
        force: false,
    };

    // Set up cache + provider
    let cache = ParquetCache::new(&cache_dir);
    let circuit_breaker = Arc::new(CircuitBreaker::default_provider());
    let provider = YahooProvider::new(circuit_breaker);

    let provider_ref: Option<&dyn trendlab_core::data::provider::DataProvider> =
        if offline { None } else { Some(&provider) };

    // Run backtest
    let result = run_single_backtest(&backtest_config, &cache, provider_ref, &opts)?;

    // Print summary
    print_summary(&result);

    // Save full artifact set (manifest.json, trades.csv, equity.csv)
    let run_dir = save_artifacts(&result, &output_dir)?;
    println!("Artifacts saved to: {}", run_dir.display());

    Ok(())
}

fn build_config_from_preset(
    name: &str,
    symbol: &str,
    start: Option<&str>,
    end: Option<&str>,
) -> Result<BacktestConfig> {
    let preset = match name {
        "donchian_trend" => StrategyPreset::DonchianTrend,
        "bollinger_breakout" => StrategyPreset::BollingerBreakout,
        "ma_crossover" => StrategyPreset::MaCrossoverTrend,
        "momentum_roc" => StrategyPreset::MomentumRoc,
        "supertrend" => StrategyPreset::SupertrendSystem,
        _ => bail!(
            "unknown preset '{name}'. Valid: donchian_trend, bollinger_breakout, ma_crossover, momentum_roc, supertrend"
        ),
    };

    let strategy_config = preset.to_config();
    let start_date = start.unwrap_or("2020-01-02");
    let end_date = end.unwrap_or("2024-12-31");

    // Build a TOML string and parse it — ensures the config goes through the same path
    let toml_str = format!(
        r#"[backtest]
symbol = "{symbol}"
start_date = "{start_date}"
end_date = "{end_date}"
initial_capital = 100000.0
trading_mode = "long_only"
position_size_pct = 1.0

[signal]
type = "{sig_type}"
{sig_params}

[position_manager]
type = "{pm_type}"
{pm_params}

[execution_model]
type = "{exec_type}"
{exec_params}

[signal_filter]
type = "{filter_type}"
{filter_params}
"#,
        sig_type = strategy_config.signal.component_type,
        sig_params = format_params("signal", &strategy_config.signal.params),
        pm_type = strategy_config.position_manager.component_type,
        pm_params = format_params("position_manager", &strategy_config.position_manager.params),
        exec_type = strategy_config.execution_model.component_type,
        exec_params = format_params("execution_model", &strategy_config.execution_model.params),
        filter_type = strategy_config.signal_filter.component_type,
        filter_params = format_params("signal_filter", &strategy_config.signal_filter.params),
    );

    Ok(BacktestConfig::from_toml(&toml_str)?)
}

fn format_params(section: &str, params: &std::collections::BTreeMap<String, f64>) -> String {
    if params.is_empty() {
        return String::new();
    }
    let pairs: Vec<String> = params.iter().map(|(k, v)| format!("{k} = {v}")).collect();
    format!("[{section}.params]\n{}", pairs.join("\n"))
}

fn run_cache_status(cache_dir: &Path) -> Result<()> {
    if !cache_dir.exists() {
        println!("Cache directory does not exist: {}", cache_dir.display());
        return Ok(());
    }

    let mut total_size: u64 = 0;
    let mut symbol_count = 0;

    let entries = std::fs::read_dir(cache_dir)?;
    let mut rows: Vec<(String, String, String, u64)> = Vec::new();

    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("symbol=") {
            continue;
        }

        let symbol = name.trim_start_matches("symbol=").to_string();
        symbol_count += 1;

        // Read metadata
        let meta_path = entry.path().join("meta.json");
        let (date_range, bar_count) = if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if let Ok(meta) =
                serde_json::from_str::<trendlab_core::data::cache::CacheMeta>(&content)
            {
                (
                    format!("{} to {}", meta.start_date, meta.end_date),
                    meta.bar_count,
                )
            } else {
                ("(corrupt meta)".into(), 0)
            }
        } else {
            ("(no meta)".into(), 0)
        };

        // Calculate directory size
        let dir_size = dir_size(&entry.path());
        total_size += dir_size;

        rows.push((symbol, date_range, format!("{bar_count} bars"), dir_size));
    }

    if symbol_count == 0 {
        println!("Cache is empty: {}", cache_dir.display());
        return Ok(());
    }

    rows.sort_by(|a, b| a.0.cmp(&b.0));

    println!("Cache: {}", cache_dir.display());
    println!("Symbols: {symbol_count}");
    println!("Total size: {}", format_size(total_size));
    println!();
    println!("{:<8} {:<25} {:<12} {:>10}", "Symbol", "Date Range", "Bars", "Size");
    println!("{}", "-".repeat(58));
    for (sym, range, bars, size) in &rows {
        println!("{:<8} {:<25} {:<12} {:>10}", sym, range, bars, format_size(*size));
    }

    Ok(())
}

fn run_cache_clean(cache_dir: &Path, unused_days: u64, confirm: bool) -> Result<()> {
    if !cache_dir.exists() {
        println!("Cache directory does not exist: {}", cache_dir.display());
        return Ok(());
    }

    let cutoff = chrono::Local::now().naive_local()
        - chrono::Duration::days(unused_days as i64);

    let entries = std::fs::read_dir(cache_dir)?;
    let mut to_remove: Vec<(String, PathBuf)> = Vec::new();

    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("symbol=") {
            continue;
        }

        let symbol = name.trim_start_matches("symbol=").to_string();
        let meta_path = entry.path().join("meta.json");

        let should_remove = if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if let Ok(meta) =
                serde_json::from_str::<trendlab_core::data::cache::CacheMeta>(&content)
            {
                meta.cached_at < cutoff
            } else {
                false // don't remove if we can't parse metadata
            }
        } else {
            false
        };

        if should_remove {
            to_remove.push((symbol, entry.path()));
        }
    }

    if to_remove.is_empty() {
        println!("No symbols older than {unused_days} days to remove.");
        return Ok(());
    }

    println!(
        "Found {} symbol(s) not accessed in {unused_days} days:",
        to_remove.len()
    );
    for (sym, path) in &to_remove {
        let size = dir_size(path);
        println!("  {sym} ({})", format_size(size));
    }

    if !confirm {
        println!();
        println!("Dry run — pass --confirm to actually delete.");
        return Ok(());
    }

    for (sym, path) in &to_remove {
        std::fs::remove_dir_all(path)?;
        println!("Removed: {sym}");
    }

    println!("Done. Removed {} symbol(s).", to_remove.len());
    Ok(())
}

fn dir_size(path: &Path) -> u64 {
    let mut size = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                size += meta.len();
            }
        }
    }
    size
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn print_summary(result: &BacktestResult) {
    println!();
    println!("=== Backtest Result ===");
    println!("Symbol:         {}", result.symbol);
    println!(
        "Period:         {} to {}",
        result.start_date, result.end_date
    );
    println!(
        "Bars:           {} ({} warmup)",
        result.bar_count, result.warmup_bars
    );
    println!("Signals:        {}", result.signal_count);
    println!("Trades:         {}", result.metrics.trade_count);
    println!();
    println!("--- Performance ---");
    println!(
        "Total Return:   {:.2}%",
        result.metrics.total_return * 100.0
    );
    println!("CAGR:           {:.2}%", result.metrics.cagr * 100.0);
    println!("Sharpe:         {:.3}", result.metrics.sharpe);
    println!("Sortino:        {:.3}", result.metrics.sortino);
    println!("Calmar:         {:.3}", result.metrics.calmar);
    println!(
        "Max Drawdown:   {:.2}%",
        result.metrics.max_drawdown * 100.0
    );
    println!("Win Rate:       {:.1}%", result.metrics.win_rate * 100.0);
    println!("Profit Factor:  {:.2}", result.metrics.profit_factor);
    println!("Turnover:       {:.1}x", result.metrics.turnover);
    println!("Max Consec Win: {}", result.metrics.max_consecutive_wins);
    println!("Max Consec Loss:{}", result.metrics.max_consecutive_losses);
    println!("Avg Lose Streak:{:.1}", result.metrics.avg_losing_streak);
    if result.has_synthetic {
        println!();
        println!("WARNING: Results based on SYNTHETIC data");
    }
    for warn in &result.data_quality_warnings {
        println!("WARNING: {warn}");
    }
    println!();
}

