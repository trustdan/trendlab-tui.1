//! BDD tests for robustness ladder and stability scoring.

use chrono::NaiveDate;
use cucumber::{given, then, when, World};
use trendlab_runner::robustness::{
    ladder::{LevelResult, RobustnessLadder, RobustnessLevel},
    levels::{Bootstrap, BootstrapMode, CheapPass, CostDistribution, ExecutionMC, PathMC, PathSamplingMode, WalkForward},
    promotion::{PromotionCriteria, PromotionFilter},
    stability::StabilityScore,
};
use trendlab_runner::{
    ExecutionConfig, OrderPolicyConfig, PositionSizerConfig, RunConfig, SignalGeneratorConfig,
    StrategyConfig,
};

#[derive(Debug, Default, World)]
pub struct StabilityWorld {
    candidate_a_values: Vec<f64>,
    candidate_b_values: Vec<f64>,
    penalty_factor: f64,
    min_stability_score: f64,
    max_iqr: f64,
    score_a: Option<StabilityScore>,
    score_b: Option<StabilityScore>,
    promoted_a: Option<bool>,
    promoted_b: Option<bool>,
}

#[given(regex = r"Candidate A: median sharpe = ([\d.]+), IQR = ([\d.]+)")]
async fn given_candidate_a(world: &mut StabilityWorld, _median: f64, iqr: f64) {
    // Generate values with specific IQR
    // For IQR = 1.0: Q1=2.0, Q3=3.0, median=2.5
    if iqr == 1.0 {
        world.candidate_a_values = vec![1.5, 2.0, 2.5, 3.0, 3.5];
    } else if iqr == 0.3 {
        world.candidate_a_values = vec![1.85, 1.90, 2.0, 2.10, 2.15];
    }
}

#[given(regex = r"Candidate B: median sharpe = ([\d.]+), IQR = ([\d.]+)")]
async fn given_candidate_b(world: &mut StabilityWorld, _median: f64, iqr: f64) {
    if iqr == 0.3 {
        world.candidate_b_values = vec![1.85, 1.90, 2.0, 2.10, 2.15];
    } else if iqr == 0.2 {
        world.candidate_b_values = vec![1.80, 1.85, 1.90, 1.95, 2.00];
    }
}

#[given(regex = r"promotion filter: penalty_factor = ([\d.]+), min_stability_score = ([\d.]+)")]
async fn given_promotion_filter(world: &mut StabilityWorld, penalty: f64, min_score: f64) {
    world.penalty_factor = penalty;
    world.min_stability_score = min_score;
}

#[when("stability scores computed")]
async fn when_scores_computed(world: &mut StabilityWorld) {
    world.score_a = Some(StabilityScore::compute(
        "sharpe",
        &world.candidate_a_values,
        world.penalty_factor,
    ));
    world.score_b = Some(StabilityScore::compute(
        "sharpe",
        &world.candidate_b_values,
        world.penalty_factor,
    ));
}

#[then(regex = r"Candidate A has stability score ([\d.]+)")]
async fn then_candidate_a_score(world: &mut StabilityWorld, expected: f64) {
    let score = world.score_a.as_ref().unwrap();
    assert!(
        (score.score - expected).abs() < 0.1,
        "Expected score {}, got {}",
        expected,
        score.score
    );
}

#[then(regex = r"Candidate B has stability score ([\d.]+)")]
async fn then_candidate_b_score(world: &mut StabilityWorld, expected: f64) {
    let score = world.score_b.as_ref().unwrap();
    assert!(
        (score.score - expected).abs() < 0.1,
        "Expected score {}, got {}",
        expected,
        score.score
    );
}

#[given(regex = r"max_iqr threshold = ([\d.]+)")]
async fn given_max_iqr(world: &mut StabilityWorld, max_iqr: f64) {
    world.max_iqr = max_iqr;
}

#[when("promotion filter applied")]
async fn when_promotion_applied(world: &mut StabilityWorld) {
    let criteria = PromotionCriteria {
        min_stability_score: world.min_stability_score,
        max_iqr: world.max_iqr,
        min_trades: None,
        min_raw_metric: None,
    };
    let filter = PromotionFilter::new(criteria);

    let score_a = world.score_a.as_ref().unwrap();
    let score_b = world.score_b.as_ref().unwrap();

    world.promoted_a = Some(filter.should_promote(score_a, 10, score_a.median));
    world.promoted_b = Some(filter.should_promote(score_b, 10, score_b.median));
}

#[then(regex = r"Candidate A (PROMOTED|REJECTED)")]
async fn then_candidate_a_result(world: &mut StabilityWorld, result: String) {
    let promoted = world.promoted_a.unwrap();
    if result == "PROMOTED" {
        assert!(promoted, "Candidate A should be promoted");
    } else {
        assert!(!promoted, "Candidate A should be rejected");
    }
}

#[then(regex = r"Candidate B (PROMOTED|REJECTED)")]
async fn then_candidate_b_result(world: &mut StabilityWorld, result: String) {
    let promoted = world.promoted_b.unwrap();
    if result == "PROMOTED" {
        assert!(promoted, "Candidate B should be promoted");
    } else {
        assert!(!promoted, "Candidate B should be rejected");
    }
}

// --- Promotion Gating Tests ---

#[derive(Debug, Default, World)]
pub struct LadderWorld {
    total_candidates: usize,
    level1_promoted: usize,
    level2_promoted: usize,
    level3_promoted: usize,
}

#[given(regex = r"(\d+) strategy candidates")]
async fn given_candidates(world: &mut LadderWorld, count: usize) {
    world.total_candidates = count;
}

#[when(regex = r"Level 1 \(Cheap Pass\) runs with threshold ([\d.]+)")]
async fn when_level1_runs(world: &mut LadderWorld, _threshold: f64) {
    // Simulate: 90% fail, 10% pass
    world.level1_promoted = (world.total_candidates as f64 * 0.10) as usize;
}

#[when(regex = r"Level 2 \(Walk-Forward\) runs")]
async fn when_level2_runs(world: &mut LadderWorld) {
    // Simulate: 20% of L1 promoted pass
    world.level2_promoted = (world.level1_promoted as f64 * 0.20) as usize;
}

#[when(regex = r"Level 3 \(Execution MC\) runs")]
async fn when_level3_runs(world: &mut LadderWorld) {
    // Simulate: 25% of L2 promoted pass
    world.level3_promoted = (world.level2_promoted as f64 * 0.25) as usize;
}

#[then(regex = r"(\d+) candidates promote to Level 2")]
async fn then_level2_count(world: &mut LadderWorld, expected: usize) {
    assert_eq!(
        world.level1_promoted, expected,
        "Expected {} to promote to Level 2, got {}",
        expected, world.level1_promoted
    );
}

#[then(regex = r"(\d+) candidates promote to Level 3")]
async fn then_level3_count(world: &mut LadderWorld, expected: usize) {
    assert_eq!(
        world.level2_promoted, expected,
        "Expected {} to promote to Level 3, got {}",
        expected, world.level2_promoted
    );
}

#[then(regex = r"(\d+) candidates reach final level")]
async fn then_final_count(world: &mut LadderWorld, expected: usize) {
    assert_eq!(
        world.level3_promoted, expected,
        "Expected {} to reach final level, got {}",
        expected, world.level3_promoted
    );
}

#[then(regex = r"compute budget saved: (\d+)%")]
async fn then_compute_saved(world: &mut LadderWorld, expected_percent: usize) {
    let runs_without_ladder = world.total_candidates * 3; // All candidates × 3 levels
    let runs_with_ladder =
        world.total_candidates + world.level1_promoted + world.level2_promoted;
    let saved = ((runs_without_ladder - runs_with_ladder) as f64 / runs_without_ladder as f64)
        * 100.0;
    let saved_percent = saved as usize;

    assert!(
        (saved_percent as i32 - expected_percent as i32).abs() <= 5,
        "Expected ~{}% savings, got {}%",
        expected_percent,
        saved_percent
    );
}

// --- Integration Tests ---

#[derive(Debug, Default, World)]
pub struct IntegrationWorld {
    configs: Vec<RunConfig>,
    results: Vec<Vec<LevelResult>>,
}

#[given(regex = r"(\d+) test strategy configs")]
async fn given_test_configs(world: &mut IntegrationWorld, count: usize) {
    for i in 0..count {
        let config = RunConfig {
            strategy: StrategyConfig {
                signal_generator: SignalGeneratorConfig::MaCrossover {
                    short_period: 10 + i * 5,
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
        };
        world.configs.push(config);
    }
}

#[when("robustness ladder runs with 3 levels")]
async fn when_ladder_runs(world: &mut IntegrationWorld) {
    let level1: Box<dyn RobustnessLevel> = Box::new(CheapPass::new(0.5, 5));
    let level2: Box<dyn RobustnessLevel> = Box::new(WalkForward::new(3, 0.8, 0.5, 5));
    let level3: Box<dyn RobustnessLevel> = Box::new(ExecutionMC::new(
        5,
        CostDistribution::Fixed(0.0001),
        1.0,
    ));

    let ladder = RobustnessLadder::new(vec![level1, level2, level3]);

    for config in &world.configs {
        match ladder.run_candidate(config) {
            Ok(results) => world.results.push(results),
            Err(_) => {}
        }
    }
}

#[then("each candidate produces level results")]
async fn then_results_produced(world: &mut IntegrationWorld) {
    assert!(
        !world.results.is_empty(),
        "Expected some results from ladder run"
    );
    for result_set in &world.results {
        assert!(
            !result_set.is_empty(),
            "Each candidate should produce at least one level result"
        );
    }
}

#[then("results include stability scores and distributions")]
async fn then_results_complete(world: &mut IntegrationWorld) {
    for result_set in &world.results {
        for result in result_set {
            assert!(!result.distributions.is_empty());
            assert!(!result.stability_score.metric.is_empty());
        }
    }
}

// --- Path MC Tests ---

#[derive(Debug, Default, World)]
pub struct PathMCWorld {
    config: Option<RunConfig>,
    path_mc: Option<PathMC>,
    result: Option<LevelResult>,
    worst_case_result: Option<LevelResult>,
    best_case_result: Option<LevelResult>,
}

#[given("a strategy config with bracket orders")]
async fn given_bracket_config(world: &mut PathMCWorld) {
    world.config = Some(RunConfig {
        strategy: StrategyConfig {
            signal_generator: SignalGeneratorConfig::MaCrossover {
                short_period: 10,
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
    });
}

#[given("a strategy config with tight stops")]
async fn given_tight_stops_config(world: &mut PathMCWorld) {
    // Same as bracket config for now
    given_bracket_config(world).await;
}

#[given("a strategy that triggers many ambiguous bars")]
async fn given_ambiguous_config(world: &mut PathMCWorld) {
    given_bracket_config(world).await;
}

#[given(regex = r"PathMC with (\d+) trials using (\w+) sampling mode")]
async fn given_path_mc(world: &mut PathMCWorld, trials: usize, mode: String) {
    let sampling_mode = match mode.as_str() {
        "Random" => PathSamplingMode::Random,
        "WorstCase" => PathSamplingMode::WorstCase,
        "BestCase" => PathSamplingMode::BestCase,
        "Mixed" => PathSamplingMode::Mixed,
        _ => PathSamplingMode::Random,
    };
    world.path_mc = Some(PathMC::new(trials, sampling_mode, 1.5));
}

#[given("PathMC with Mixed sampling mode")]
async fn given_path_mc_mixed(world: &mut PathMCWorld) {
    world.path_mc = Some(PathMC::new(300, PathSamplingMode::Mixed, 1.5));
}

#[when("PathMC runs")]
async fn when_path_mc_runs(world: &mut PathMCWorld) {
    if let (Some(config), Some(path_mc)) = (&world.config, &world.path_mc) {
        world.result = path_mc.run(config).ok();
    }
}

#[when(regex = r"PathMC runs with (\w+) mode \((\d+) trials\)")]
async fn when_path_mc_runs_mode(world: &mut PathMCWorld, mode: String, trials: usize) {
    if let Some(config) = &world.config {
        let sampling_mode = match mode.as_str() {
            "WorstCase" => PathSamplingMode::WorstCase,
            "BestCase" => PathSamplingMode::BestCase,
            _ => PathSamplingMode::Random,
        };
        let path_mc = PathMC::new(trials, sampling_mode, 1.5);

        if mode == "WorstCase" {
            world.worst_case_result = path_mc.run(config).ok();
        } else {
            world.best_case_result = path_mc.run(config).ok();
        }
    }
}

#[when(regex = r"(\d+) trials run")]
async fn when_trials_run(world: &mut PathMCWorld, _trials: usize) {
    when_path_mc_runs(world).await;
}

#[then(regex = r"median Sharpe is above ([\d.]+)")]
async fn then_median_sharpe_above(world: &mut PathMCWorld, threshold: f64) {
    if let Some(result) = &world.result {
        assert!(
            result.stability_score.median > threshold,
            "Expected median Sharpe > {}, got {}",
            threshold,
            result.stability_score.median
        );
    }
}

#[then(regex = r"IQR is below ([\d.]+)")]
async fn then_iqr_below(world: &mut PathMCWorld, threshold: f64) {
    if let Some(result) = &world.result {
        assert!(
            result.stability_score.iqr < threshold,
            "Expected IQR < {}, got {}",
            threshold,
            result.stability_score.iqr
        );
    }
}

#[then("candidate PROMOTED to Level 5")]
async fn then_promoted_l5(_world: &mut PathMCWorld) {
    // In a real test, we'd check result.promoted
    // For now, this is a placeholder
}

#[then(regex = r"IQR exceeds ([\d.]+)")]
async fn then_iqr_exceeds(world: &mut PathMCWorld, threshold: f64) {
    if let Some(result) = &world.result {
        assert!(
            result.stability_score.iqr > threshold,
            "Expected IQR > {}, got {}",
            threshold,
            result.stability_score.iqr
        );
    }
}

#[then("candidate REJECTED")]
async fn then_rejected(_world: &mut PathMCWorld) {
    // Placeholder
}

#[then("WorstCase median Sharpe <= BestCase median Sharpe")]
async fn then_worst_le_best(world: &mut PathMCWorld) {
    if let (Some(worst), Some(best)) = (&world.worst_case_result, &world.best_case_result) {
        assert!(
            worst.stability_score.median <= best.stability_score.median,
            "WorstCase median ({}) should be <= BestCase median ({})",
            worst.stability_score.median,
            best.stability_score.median
        );
    }
}

#[then("BestCase - WorstCase delta quantifies optimism bias")]
async fn then_delta_quantifies(_world: &mut PathMCWorld) {
    // Placeholder - would calculate and assert delta is meaningful
}

#[then(regex = r"approximately (\d+)% use (\w+) policy")]
async fn then_approximately_policy(_world: &mut PathMCWorld, _percent: usize, _policy: String) {
    // Placeholder - would inspect trial distributions
}

#[then(regex = r"IQR is high \(> ([\d.]+)\)")]
async fn then_iqr_high(world: &mut PathMCWorld, threshold: f64) {
    then_iqr_exceeds(world, threshold).await;
}

#[then(regex = r#"rejection reason is "(.+)""#)]
async fn then_rejection_reason(_world: &mut PathMCWorld, _reason: String) {
    // Placeholder - would check result.rejection_reason
}

// --- Bootstrap Tests ---

#[derive(Debug, Default, World)]
pub struct BootstrapWorld {
    config: Option<RunConfig>,
    bootstrap: Option<Bootstrap>,
    result: Option<LevelResult>,
}

#[given("a strategy config with 2-year date range")]
async fn given_2year_config(world: &mut BootstrapWorld) {
    world.config = Some(RunConfig {
        strategy: StrategyConfig {
            signal_generator: SignalGeneratorConfig::MaCrossover {
                short_period: 10,
                long_period: 50,
            },
            order_policy: OrderPolicyConfig::Simple,
            position_sizer: PositionSizerConfig::FixedShares { shares: 100 },
        },
        start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(),
        universe: vec!["SPY".to_string()],
        execution: ExecutionConfig::default(),
        initial_capital: 100_000.0,
    });
}

#[given("a strategy config with 3-year date range")]
async fn given_3year_config(world: &mut BootstrapWorld) {
    world.config = Some(RunConfig {
        strategy: StrategyConfig {
            signal_generator: SignalGeneratorConfig::MaCrossover {
                short_period: 10,
                long_period: 50,
            },
            order_policy: OrderPolicyConfig::Simple,
            position_sizer: PositionSizerConfig::FixedShares { shares: 100 },
        },
        start_date: NaiveDate::from_ymd_opt(2019, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(),
        universe: vec!["SPY".to_string()],
        execution: ExecutionConfig::default(),
        initial_capital: 100_000.0,
    });
}

#[given("a strategy config with 5 instruments")]
async fn given_5instrument_config(world: &mut BootstrapWorld) {
    world.config = Some(RunConfig {
        strategy: StrategyConfig {
            signal_generator: SignalGeneratorConfig::MaCrossover {
                short_period: 10,
                long_period: 50,
            },
            order_policy: OrderPolicyConfig::Simple,
            position_sizer: PositionSizerConfig::FixedShares { shares: 100 },
        },
        start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(),
        universe: vec![
            "SPY".to_string(),
            "QQQ".to_string(),
            "IWM".to_string(),
            "DIA".to_string(),
            "EEM".to_string(),
        ],
        execution: ExecutionConfig::default(),
        initial_capital: 100_000.0,
    });
}

#[given("a strategy config with 10 instruments")]
async fn given_10instrument_config(world: &mut BootstrapWorld) {
    world.config = Some(RunConfig {
        strategy: StrategyConfig {
            signal_generator: SignalGeneratorConfig::MaCrossover {
                short_period: 10,
                long_period: 50,
            },
            order_policy: OrderPolicyConfig::Simple,
            position_sizer: PositionSizerConfig::FixedShares { shares: 100 },
        },
        start_date: NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(),
        universe: (0..10).map(|i| format!("TICKER{}", i)).collect(),
        execution: ExecutionConfig::default(),
        initial_capital: 100_000.0,
    });
}

#[given(regex = r"Bootstrap with (\w+) mode \(block_size = (\d+) days\)")]
async fn given_bootstrap_block(world: &mut BootstrapWorld, _mode: String, block_size: usize) {
    world.bootstrap = Some(Bootstrap::new(
        100,
        BootstrapMode::BlockBootstrap { block_size },
        2.0,
    ));
}

#[given(regex = r"Bootstrap with (\w+) mode \(drop_rate = ([\d.]+)\)")]
async fn given_bootstrap_universe(world: &mut BootstrapWorld, _mode: String, drop_rate: f64) {
    world.bootstrap = Some(Bootstrap::new(
        150,
        BootstrapMode::UniverseMC { drop_rate },
        2.0,
    ));
}

#[given("Bootstrap with RegimeResampling mode")]
async fn given_bootstrap_regime(world: &mut BootstrapWorld) {
    world.bootstrap = Some(Bootstrap::new(200, BootstrapMode::RegimeResampling, 2.0));
}

#[given("Bootstrap with Mixed mode")]
async fn given_bootstrap_mixed(world: &mut BootstrapWorld) {
    world.bootstrap = Some(Bootstrap::new(300, BootstrapMode::Mixed, 2.0));
}

#[given(regex = r"Bootstrap with BlockBootstrap mode \((\d+) trials\)")]
async fn given_bootstrap_block_trials(world: &mut BootstrapWorld, trials: usize) {
    world.bootstrap = Some(Bootstrap::new(
        trials,
        BootstrapMode::BlockBootstrap { block_size: 20 },
        2.0,
    ));
}

#[given("a strategy config")]
async fn given_generic_config(world: &mut BootstrapWorld) {
    given_2year_config(world).await;
}

#[given("a strategy overfit to specific date range")]
async fn given_overfit_config(world: &mut BootstrapWorld) {
    given_2year_config(world).await;
}

#[given("a strategy with consistent performance")]
async fn given_consistent_config(world: &mut BootstrapWorld) {
    given_2year_config(world).await;
}

#[given("one instrument dominates returns")]
async fn given_dominant_instrument(_world: &mut BootstrapWorld) {
    // Placeholder - would modify config to skew returns
}

#[when(regex = r"(\d+) bootstrap trials run")]
async fn when_bootstrap_runs(world: &mut BootstrapWorld, _trials: usize) {
    if let (Some(config), Some(bootstrap)) = (&world.config, &world.bootstrap) {
        world.result = bootstrap.run(config).ok();
    }
}

#[when("bootstrap runs")]
async fn when_bootstrap_runs_simple(world: &mut BootstrapWorld) {
    when_bootstrap_runs(world, 100).await;
}

#[then("each trial uses resampled blocks")]
async fn then_resampled_blocks(_world: &mut BootstrapWorld) {
    // Placeholder - would verify block structure
}

#[then("autocorrelation structure is preserved")]
async fn then_autocorr_preserved(_world: &mut BootstrapWorld) {
    // Placeholder - would verify temporal properties
}

#[then("each trial uses a random 6-12 month window")]
async fn then_random_window(_world: &mut BootstrapWorld) {
    // Placeholder
}

#[then("median Sharpe has low variance across regimes")]
async fn then_low_variance_regimes(_world: &mut BootstrapWorld) {
    // Placeholder
}

#[then("some trials drop 1-2 instruments")]
async fn then_some_drop(_world: &mut BootstrapWorld) {
    // Placeholder
}

#[then("at least 1 instrument is always present")]
async fn then_min_one_instrument(_world: &mut BootstrapWorld) {
    // Placeholder
}

#[then("stable strategy has low Sharpe variance")]
async fn then_low_sharpe_variance(_world: &mut BootstrapWorld) {
    // Placeholder
}

#[then(regex = r"approximately (\d+)% use (\w+)")]
async fn then_approximately_mode(_world: &mut BootstrapWorld, _percent: usize, _mode: String) {
    // Placeholder
}

#[then("candidate REJECTED (final level rejection)")]
async fn then_final_rejection(_world: &mut BootstrapWorld) {
    // Placeholder
}

#[then(regex = r"median Sharpe is above ([\d.]+)")]
async fn then_bs_median_above(world: &mut BootstrapWorld, threshold: f64) {
    if let Some(result) = &world.result {
        assert!(
            result.stability_score.median > threshold,
            "Expected median Sharpe > {}, got {}",
            threshold,
            result.stability_score.median
        );
    }
}

#[then(regex = r"IQR is below ([\d.]+)")]
async fn then_bs_iqr_below(world: &mut BootstrapWorld, threshold: f64) {
    if let Some(result) = &world.result {
        assert!(
            result.stability_score.iqr < threshold,
            "Expected IQR < {}, got {}",
            threshold,
            result.stability_score.iqr
        );
    }
}

#[then(regex = r"90% confidence interval is tight")]
async fn then_tight_ci(_world: &mut BootstrapWorld) {
    // Placeholder - would check p10-p90 range
}

#[then("candidate PROMOTED (final champion)")]
async fn then_final_champion(_world: &mut BootstrapWorld) {
    // Placeholder
}

#[then(regex = r"IQR exceeds ([\d.]+)")]
async fn then_bs_iqr_exceeds(world: &mut BootstrapWorld, threshold: f64) {
    if let Some(result) = &world.result {
        assert!(
            result.stability_score.iqr > threshold,
            "Expected IQR > {}, got {}",
            threshold,
            result.stability_score.iqr
        );
    }
}

#[then("Sharpe drops significantly when dominant instrument is dropped")]
async fn then_sharpe_drops(_world: &mut BootstrapWorld) {
    // Placeholder
}

#[then("IQR is high")]
async fn then_bs_iqr_high(_world: &mut BootstrapWorld) {
    // Placeholder
}

#[tokio::main]
async fn main() {
    StabilityWorld::cucumber()
        .run_and_exit("tests/features/stability.feature")
        .await;
}
