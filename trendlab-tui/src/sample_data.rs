//! Sample data generator for the TUI.
//!
//! Produces realistic-looking sample results with:
//! - Noisy equity curves (not perfectly smooth) so Sharpe/drawdown are realistic
//! - Signal trace fields on trades (signal_intent, order_type, fill_context)
//! - Slippage and gap data for diagnostics
//! - Rejected intents across all 4 guard types
//! - A RunConfig in metadata so the manifest viewer works

use chrono::{Duration, NaiveDate, Utc};
use std::collections::HashMap;
use trendlab_runner::config::*;
use trendlab_runner::result::{
    BacktestResult, EquityPoint, PerformanceStats, ResultMetadata, TradeDirection, TradeRecord,
};

pub fn sample_results() -> Vec<BacktestResult> {
    vec![
        build_sample_result(
            "sample_run_alpha",
            NaiveDate::from_ymd_opt(2022, 1, 3).unwrap(),
            NaiveDate::from_ymd_opt(2022, 7, 1).unwrap(),
            180,
            0.0008, // drift: ~20% annual
            0.012,  // daily vol: ~19% annual
            42,     // noise seed
            100_000.0,
        ),
        build_sample_result(
            "sample_run_beta",
            NaiveDate::from_ymd_opt(2022, 3, 1).unwrap(),
            NaiveDate::from_ymd_opt(2022, 9, 1).unwrap(),
            180,
            0.0005, // drift: ~12% annual
            0.015,  // daily vol: ~24% annual
            99,     // noise seed
            100_000.0,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn build_sample_result(
    run_id: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
    days: i64,
    drift: f64,
    volatility: f64,
    seed: u64,
    initial_capital: f64,
) -> BacktestResult {
    let equity_curve =
        build_equity_curve(start_date, days, initial_capital, drift, volatility, seed);
    let trades = build_trades(start_date);
    let stats = PerformanceStats::from_results(&equity_curve, &trades, initial_capital);

    // Build enriched metadata
    let mut custom = HashMap::new();

    // Ideal equity curve (smoother, higher drift — for ghost curve demo)
    let ideal_equity = build_equity_curve(
        start_date,
        days,
        initial_capital,
        drift * 1.3,      // 30% higher drift
        volatility * 0.4,  // much less noise
        seed + 1,
    );
    let ideal_json: Vec<serde_json::Value> = ideal_equity
        .iter()
        .map(|ep| {
            serde_json::json!({
                "date": ep.date.to_string(),
                "equity": ep.equity,
            })
        })
        .collect();
    custom.insert(
        "ideal_equity_curve".to_string(),
        serde_json::Value::Array(ideal_json),
    );

    // Rejected intents covering all 4 guard types
    let rejected_intents = build_sample_rejections(start_date);
    custom.insert(
        "rejected_intents".to_string(),
        serde_json::Value::Array(rejected_intents),
    );
    custom.insert("total_signals".to_string(), serde_json::json!(days));

    // RunConfig so the manifest viewer has something to show
    let config = RunConfig {
        strategy: StrategyConfig {
            signal_generator: SignalGeneratorConfig::MaCrossover {
                short_period: 10,
                long_period: 50,
            },
            order_policy: OrderPolicyConfig::Simple,
            position_sizer: PositionSizerConfig::FixedDollar { amount: 10_000.0 },
        },
        start_date,
        end_date,
        universe: vec!["SPY".to_string(), "QQQ".to_string(), "IWM".to_string()],
        execution: ExecutionConfig::default(),
        initial_capital,
    };

    BacktestResult {
        run_id: run_id.to_string(),
        equity_curve,
        trades,
        stats,
        metadata: ResultMetadata {
            timestamp: Utc::now(),
            duration_secs: 1.2,
            custom,
            config: Some(config),
        },
    }
}

/// Build a noisy equity curve with realistic daily returns.
///
/// Uses a simple deterministic pseudo-random based on a linear congruential
/// generator so that sample data is reproducible without pulling in `rand`.
fn build_equity_curve(
    start_date: NaiveDate,
    days: i64,
    initial_capital: f64,
    drift: f64,
    volatility: f64,
    seed: u64,
) -> Vec<EquityPoint> {
    let mut equity_curve = Vec::new();
    let mut equity = initial_capital;
    let mut rng_state = seed;

    equity_curve.push(EquityPoint {
        date: start_date,
        equity,
    });

    for offset in 1..=days {
        let date = start_date + Duration::days(offset);

        // Deterministic pseudo-random: LCG producing values in [-1, 1]
        rng_state = rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let u = ((rng_state >> 33) as f64) / (u32::MAX as f64) * 2.0 - 1.0;

        // Daily return = drift + volatility * noise
        let daily_return = drift + volatility * u;
        equity *= 1.0 + daily_return;

        // Floor at 1% of initial to avoid negative equity
        equity = equity.max(initial_capital * 0.01);

        equity_curve.push(EquityPoint { date, equity });
    }

    equity_curve
}

fn build_trades(start_date: NaiveDate) -> Vec<TradeRecord> {
    vec![
        TradeRecord {
            symbol: "SPY".to_string(),
            entry_date: start_date + Duration::days(5),
            exit_date: start_date + Duration::days(35),
            direction: TradeDirection::Long,
            entry_price: 448.50,
            exit_price: 462.30,
            quantity: 22,
            pnl: 303.60,
            return_pct: 3.08,
            signal_intent: Some("Long".to_string()),
            order_type: Some("Market(MOO)".to_string()),
            fill_context: Some("Filled at open $448.65".to_string()),
            entry_slippage: Some(0.15),
            exit_slippage: Some(0.08),
            entry_was_gapped: Some(false),
            exit_was_gapped: Some(false),
        },
        TradeRecord {
            symbol: "QQQ".to_string(),
            entry_date: start_date + Duration::days(45),
            exit_date: start_date + Duration::days(80),
            direction: TradeDirection::Short,
            entry_price: 372.00,
            exit_price: 358.40,
            quantity: 27,
            pnl: 367.20,
            return_pct: 3.66,
            signal_intent: Some("Short".to_string()),
            order_type: Some("Market(MOO)".to_string()),
            fill_context: Some("Filled at open $371.84".to_string()),
            entry_slippage: Some(0.16),
            exit_slippage: Some(0.12),
            entry_was_gapped: Some(false),
            exit_was_gapped: Some(false),
        },
        TradeRecord {
            symbol: "IWM".to_string(),
            entry_date: start_date + Duration::days(90),
            exit_date: start_date + Duration::days(110),
            direction: TradeDirection::Long,
            entry_price: 198.30,
            exit_price: 191.75,
            quantity: 50,
            pnl: -327.50,
            return_pct: -3.30,
            signal_intent: Some("Long".to_string()),
            order_type: Some("StopMarket(195.00)".to_string()),
            fill_context: Some("Filled at open $198.53 (gap fill)".to_string()),
            entry_slippage: Some(0.23),
            exit_slippage: Some(0.45),
            entry_was_gapped: Some(true),
            exit_was_gapped: Some(false),
        },
        TradeRecord {
            symbol: "SPY".to_string(),
            entry_date: start_date + Duration::days(120),
            exit_date: start_date + Duration::days(155),
            direction: TradeDirection::Long,
            entry_price: 455.20,
            exit_price: 470.85,
            quantity: 22,
            pnl: 344.30,
            return_pct: 3.43,
            signal_intent: Some("Long".to_string()),
            order_type: Some("Limit(454.00)".to_string()),
            fill_context: Some("Filled at limit $454.00".to_string()),
            entry_slippage: Some(0.0),
            exit_slippage: Some(0.10),
            entry_was_gapped: Some(false),
            exit_was_gapped: Some(false),
        },
        TradeRecord {
            symbol: "QQQ".to_string(),
            entry_date: start_date + Duration::days(160),
            exit_date: start_date + Duration::days(175),
            direction: TradeDirection::Long,
            entry_price: 365.10,
            exit_price: 362.40,
            quantity: 27,
            pnl: -72.90,
            return_pct: -0.74,
            signal_intent: Some("Long".to_string()),
            order_type: Some("Market(MOO)".to_string()),
            fill_context: Some("Filled at open $365.25".to_string()),
            entry_slippage: Some(0.15),
            exit_slippage: Some(0.20),
            entry_was_gapped: Some(false),
            exit_was_gapped: Some(true),
        },
    ]
}

fn build_sample_rejections(start_date: NaiveDate) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "bar_index": 15,
            "date": (start_date + Duration::days(15)).to_string(),
            "signal": "Long",
            "reason": "VolatilityGuard",
            "context": "range=0.08, threshold=0.05",
        }),
        serde_json::json!({
            "bar_index": 32,
            "date": (start_date + Duration::days(32)).to_string(),
            "signal": "Short",
            "reason": "LiquidityGuard",
            "context": "volume=45000, min_volume=100000",
        }),
        serde_json::json!({
            "bar_index": 55,
            "date": (start_date + Duration::days(55)).to_string(),
            "signal": "Long",
            "reason": "MarginGuard",
            "context": "cash=800.00, min_cash=1000.00",
        }),
        serde_json::json!({
            "bar_index": 78,
            "date": (start_date + Duration::days(78)).to_string(),
            "signal": "Long",
            "reason": "RiskGuard",
            "context": "open_positions=10, max_positions=10",
        }),
        serde_json::json!({
            "bar_index": 95,
            "date": (start_date + Duration::days(95)).to_string(),
            "signal": "Short",
            "reason": "VolatilityGuard",
            "context": "range=0.12, threshold=0.05",
        }),
    ]
}
