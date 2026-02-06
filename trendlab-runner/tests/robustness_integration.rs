//! Integration tests for robustness ladder.

use chrono::NaiveDate;
use trendlab_runner::robustness::{
    ladder::{RobustnessLadder, RobustnessLevel},
    levels::{Bootstrap, BootstrapMode, CheapPass, CostDistribution, ExecutionMC, PathMC, PathSamplingMode, WalkForward},
    stability::StabilityScore,
};
use trendlab_runner::{
    ExecutionConfig, OrderPolicyConfig, PositionSizerConfig, RunConfig, SignalGeneratorConfig,
    StrategyConfig,
};

fn make_test_config(short_period: usize) -> RunConfig {
    RunConfig {
        strategy: StrategyConfig {
            signal_generator: SignalGeneratorConfig::MaCrossover {
                short_period,
                long_period: 50,
            },
            order_policy: OrderPolicyConfig::Simple,
            position_sizer: PositionSizerConfig::FixedShares { shares: 100 },
        },
        start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2021, 1, 1).unwrap(),
        universe: vec!["SPY".to_string()],
        execution: ExecutionConfig::default(),
        initial_capital: 100_000.0,
    }
}

#[test]
fn test_stability_score_penalizes_variance() {
    // Stable strategy: low IQR
    let stable_values = vec![1.8, 1.9, 2.0, 2.1, 2.0];
    let stable = StabilityScore::compute("sharpe", &stable_values, 0.5);

    // Unstable strategy: high IQR
    let unstable_values = vec![0.5, 2.5, 1.0, 3.0, 1.5];
    let unstable = StabilityScore::compute("sharpe", &unstable_values, 0.5);

    // Stable should have higher stability score despite similar medians
    assert!(stable.iqr < unstable.iqr);
    assert!(stable.score > unstable.score);
}

#[test]
fn test_promotion_filter_gates_unstable_strategies() {
    // Stable strategy should have good stability metrics
    let stable = StabilityScore::compute("sharpe", &[1.9, 2.0, 2.1, 1.95, 2.05], 0.5);
    assert!(stable.score >= 1.5);
    assert!(stable.iqr <= 0.5);

    // Unstable strategy should have high IQR
    let unstable = StabilityScore::compute("sharpe", &[0.5, 3.0, 1.0, 2.5, 1.5], 0.5);
    assert!(unstable.iqr > 0.5);
}

#[test]
fn test_cheap_pass_level_executes() {
    let cheap_pass = CheapPass::new(0.5, 5);
    let config = make_test_config(10);

    let result = cheap_pass.run(&config).unwrap();

    assert_eq!(result.level_name, "CheapPass");
    assert_eq!(result.distributions.len(), 1);
    assert_eq!(result.distributions[0].all_values.len(), 1); // Single run
    assert!(!result.stability_score.metric.is_empty());
}

#[test]
fn test_walk_forward_level_executes() {
    let walk_forward = WalkForward::new(3, 0.8, 0.5, 5);
    let config = make_test_config(10);

    let result = walk_forward.run(&config).unwrap();

    assert_eq!(result.level_name, "WalkForward");
    assert_eq!(result.distributions.len(), 1);
    assert_eq!(result.distributions[0].all_values.len(), 3); // 3 splits
}

#[test]
fn test_execution_mc_level_executes() {
    let execution_mc = ExecutionMC::new(5, CostDistribution::Fixed(0.0001), 1.0);
    let config = make_test_config(10);

    let result = execution_mc.run(&config).unwrap();

    assert_eq!(result.level_name, "ExecutionMC");
    assert_eq!(result.distributions.len(), 1);
    assert_eq!(result.distributions[0].all_values.len(), 5); // 5 trials
}

#[test]
fn test_ladder_progressive_filtering() {
    let level1 = Box::new(CheapPass::new(0.5, 5));
    let level2 = Box::new(WalkForward::new(3, 0.8, 0.5, 5));
    let level3 = Box::new(ExecutionMC::new(5, CostDistribution::Fixed(0.0001), 1.0));

    let ladder = RobustnessLadder::new(vec![level1, level2, level3]);

    let config = make_test_config(10);
    let results = ladder.run_candidate(&config).unwrap();

    // Should produce at least 1 level result (CheapPass always runs)
    assert!(!results.is_empty());

    // Each result should have complete data
    for result in &results {
        assert!(!result.distributions.is_empty());
        assert!(!result.stability_score.metric.is_empty());
    }
}

#[test]
fn test_ladder_batch_execution() {
    let configs = vec![make_test_config(10), make_test_config(15), make_test_config(20)];

    let level1 = Box::new(CheapPass::new(0.5, 5));
    let ladder = RobustnessLadder::new(vec![level1]);

    let results = ladder.run_batch(&configs).unwrap();

    assert_eq!(results.len(), 3); // One result set per config
    for result_set in results {
        assert!(!result_set.is_empty());
    }
}

#[test]
fn test_ladder_summary_statistics() {
    let level1 = Box::new(CheapPass::new(0.5, 5));
    let level2 = Box::new(WalkForward::new(2, 0.8, 0.5, 5));

    let ladder = RobustnessLadder::new(vec![level1, level2]);

    let configs = vec![make_test_config(10), make_test_config(15)];
    let results = ladder.run_batch(&configs).unwrap();

    let summary = ladder.batch_summary(&results);

    assert_eq!(summary.total_candidates, 2);
    assert!(!summary.level_stats.is_empty());

    // First level should have all candidates enter
    assert!(summary.level_stats[0].entered > 0);
}

// --- Level 4 (PathMC) Tests ---

#[test]
fn test_path_mc_level_executes() {
    let path_mc = PathMC::new(5, PathSamplingMode::Random, 1.5);
    let config = make_test_config(10);

    let result = path_mc.run(&config).unwrap();

    assert_eq!(result.level_name, "PathMC");
    assert_eq!(result.distributions.len(), 1);
    assert_eq!(result.distributions[0].all_values.len(), 5); // 5 trials
}

#[test]
fn test_path_mc_worst_case_mode() {
    let path_mc = PathMC::new(10, PathSamplingMode::WorstCase, 1.5);
    let config = make_test_config(10);

    let result = path_mc.run(&config).unwrap();

    assert_eq!(result.level_name, "PathMC");
    assert!(!result.distributions.is_empty());
}

#[test]
fn test_path_mc_best_case_mode() {
    let path_mc = PathMC::new(10, PathSamplingMode::BestCase, 1.5);
    let config = make_test_config(10);

    let result = path_mc.run(&config).unwrap();

    assert_eq!(result.level_name, "PathMC");
    assert!(!result.distributions.is_empty());
}

#[test]
fn test_path_mc_mixed_mode() {
    let path_mc = PathMC::new(15, PathSamplingMode::Mixed, 1.5);
    let config = make_test_config(10);

    let result = path_mc.run(&config).unwrap();

    assert_eq!(result.level_name, "PathMC");
    assert_eq!(result.distributions[0].all_values.len(), 15); // 15 trials
}

#[test]
fn test_path_mc_criteria() {
    let path_mc = PathMC::new(100, PathSamplingMode::Random, 2.0);
    let criteria = path_mc.promotion_criteria();

    assert_eq!(criteria.min_stability_score, 2.0);
    assert_eq!(criteria.max_iqr, 0.3); // Stricter than ExecutionMC
    assert_eq!(criteria.min_trades, Some(20));
}

// --- Level 5 (Bootstrap) Tests ---

#[test]
fn test_bootstrap_block_mode_executes() {
    let bootstrap = Bootstrap::new(5, BootstrapMode::BlockBootstrap { block_size: 20 }, 2.0);

    let mut config = make_test_config(10);
    config.end_date = NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(); // Extend to 2 years

    let result = bootstrap.run(&config).unwrap();

    assert_eq!(result.level_name, "Bootstrap");
    assert_eq!(result.distributions.len(), 1);
    assert_eq!(result.distributions[0].all_values.len(), 5); // 5 trials
}

#[test]
fn test_bootstrap_regime_mode_executes() {
    let bootstrap = Bootstrap::new(5, BootstrapMode::RegimeResampling, 2.0);

    let mut config = make_test_config(10);
    config.end_date = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap(); // Extend to 3 years

    let result = bootstrap.run(&config).unwrap();

    assert_eq!(result.level_name, "Bootstrap");
    assert!(!result.distributions.is_empty());
}

#[test]
fn test_bootstrap_universe_mc_mode() {
    let bootstrap = Bootstrap::new(5, BootstrapMode::UniverseMC { drop_rate: 0.3 }, 2.0);

    let mut config = make_test_config(10);
    config.universe = vec![
        "SPY".to_string(),
        "QQQ".to_string(),
        "IWM".to_string(),
    ];

    let result = bootstrap.run(&config).unwrap();

    assert_eq!(result.level_name, "Bootstrap");
    assert!(!result.distributions.is_empty());
}

#[test]
fn test_bootstrap_mixed_mode() {
    let bootstrap = Bootstrap::new(9, BootstrapMode::Mixed, 2.0);

    let mut config = make_test_config(10);
    config.end_date = NaiveDate::from_ymd_opt(2022, 1, 1).unwrap();
    config.universe = vec!["SPY".to_string(), "QQQ".to_string()];

    let result = bootstrap.run(&config).unwrap();

    assert_eq!(result.level_name, "Bootstrap");
    assert_eq!(result.distributions[0].all_values.len(), 9); // 9 trials
}

#[test]
fn test_bootstrap_criteria() {
    let bootstrap = Bootstrap::new(500, BootstrapMode::BlockBootstrap { block_size: 20 }, 2.5);
    let criteria = bootstrap.promotion_criteria();

    assert_eq!(criteria.min_stability_score, 2.5);
    assert_eq!(criteria.max_iqr, 0.2); // Strictest threshold
    assert_eq!(criteria.min_trades, Some(30));
}

// --- Full 5-Level Ladder Tests ---

#[test]
fn test_full_5_level_ladder_executes() {
    let level1: Box<dyn RobustnessLevel> = Box::new(CheapPass::new(0.5, 5));
    let level2: Box<dyn RobustnessLevel> = Box::new(WalkForward::new(2, 0.8, 0.5, 5));
    let level3: Box<dyn RobustnessLevel> = Box::new(ExecutionMC::new(3, CostDistribution::Fixed(0.0001), 1.0));
    let level4: Box<dyn RobustnessLevel> = Box::new(PathMC::new(3, PathSamplingMode::Random, 1.5));
    let level5: Box<dyn RobustnessLevel> = Box::new(Bootstrap::new(3, BootstrapMode::BlockBootstrap { block_size: 10 }, 2.0));

    let ladder = RobustnessLadder::new(vec![level1, level2, level3, level4, level5]);

    let mut config = make_test_config(10);
    config.end_date = NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(); // Extend for bootstrap

    let results = ladder.run_candidate(&config).unwrap();

    // Should produce at least 1 level result (CheapPass always runs)
    assert!(!results.is_empty());

    // First result should be CheapPass
    assert_eq!(results[0].level_name, "CheapPass");

    // Each result should have complete data
    for result in &results {
        assert!(!result.distributions.is_empty());
        assert!(!result.stability_score.metric.is_empty());
        assert!(result.trade_count > 0 || result.level_name == "CheapPass");
    }
}

#[test]
fn test_full_ladder_batch_execution() {
    let configs = vec![
        make_test_config(10),
        make_test_config(15),
        make_test_config(20),
    ];

    let level1: Box<dyn RobustnessLevel> = Box::new(CheapPass::new(0.5, 5));
    let level2: Box<dyn RobustnessLevel> = Box::new(WalkForward::new(2, 0.8, 0.5, 5));
    let level3: Box<dyn RobustnessLevel> = Box::new(ExecutionMC::new(2, CostDistribution::Fixed(0.0001), 1.0));

    let ladder = RobustnessLadder::new(vec![level1, level2, level3]);

    let results = ladder.run_batch(&configs).unwrap();

    assert_eq!(results.len(), 3); // One result set per config
    for result_set in results {
        assert!(!result_set.is_empty());
        // First result should always be CheapPass
        assert_eq!(result_set[0].level_name, "CheapPass");
    }
}

#[test]
fn test_ladder_progressive_filtering_all_levels() {
    // Create a simple 5-level ladder with low trial counts for speed
    let level1: Box<dyn RobustnessLevel> = Box::new(CheapPass::new(0.3, 3));
    let level2: Box<dyn RobustnessLevel> = Box::new(WalkForward::new(2, 0.6, 0.3, 3));
    let level3: Box<dyn RobustnessLevel> = Box::new(ExecutionMC::new(2, CostDistribution::Fixed(0.0001), 0.5));
    let level4: Box<dyn RobustnessLevel> = Box::new(PathMC::new(2, PathSamplingMode::WorstCase, 0.8));
    let level5: Box<dyn RobustnessLevel> = Box::new(Bootstrap::new(2, BootstrapMode::RegimeResampling, 1.0));

    let ladder = RobustnessLadder::new(vec![level1, level2, level3, level4, level5]);

    let mut config = make_test_config(10);
    config.end_date = NaiveDate::from_ymd_opt(2022, 1, 1).unwrap();

    let results = ladder.run_candidate(&config).unwrap();

    // Should have at least Level 1 result
    assert!(!results.is_empty());

    // Verify level names are in order (only for levels that executed)
    let expected_order = ["CheapPass", "WalkForward", "ExecutionMC", "PathMC", "Bootstrap"];
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result.level_name, expected_order[i]);
    }
}

#[test]
fn test_ladder_summary_all_5_levels() {
    let level1: Box<dyn RobustnessLevel> = Box::new(CheapPass::new(0.5, 5));
    let level2: Box<dyn RobustnessLevel> = Box::new(WalkForward::new(2, 0.8, 0.5, 5));
    let level3: Box<dyn RobustnessLevel> = Box::new(ExecutionMC::new(2, CostDistribution::Fixed(0.0001), 1.0));
    let level4: Box<dyn RobustnessLevel> = Box::new(PathMC::new(2, PathSamplingMode::Random, 1.5));
    let level5: Box<dyn RobustnessLevel> = Box::new(Bootstrap::new(2, BootstrapMode::BlockBootstrap { block_size: 10 }, 2.0));

    let ladder = RobustnessLadder::new(vec![level1, level2, level3, level4, level5]);

    let mut configs = vec![make_test_config(10), make_test_config(15)];
    for config in &mut configs {
        config.end_date = NaiveDate::from_ymd_opt(2022, 1, 1).unwrap();
    }

    let results = ladder.run_batch(&configs).unwrap();

    let summary = ladder.batch_summary(&results);

    assert_eq!(summary.total_candidates, 2);
    assert_eq!(summary.level_stats.len(), 5); // All 5 levels tracked

    // First level should have all candidates enter
    assert_eq!(summary.level_stats[0].entered, 2);

    // Each subsequent level should have <= previous level's promoted count
    for i in 1..summary.level_stats.len() {
        assert!(summary.level_stats[i].entered <= summary.level_stats[i-1].promoted);
    }
}
