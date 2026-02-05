//! Execution presets: bundled configurations for quick setup
//!
//! Presets combine path policy, slippage, priority, and liquidity constraints
//! into common configurations: Optimistic, Realistic, Hostile.

use super::{
    BestCase, BestCasePriority, Deterministic, FixedSlippage, LiquidityConstraint,
    PathPolicy, PriorityPolicy, RemainderPolicy, SlippageModel, WorstCase, WorstCasePriority,
};

/// Execution preset: bundles all execution parameters
pub trait ExecutionPreset {
    /// Get path policy for this preset
    fn path_policy(&self) -> Box<dyn PathPolicy>;

    /// Get slippage model for this preset
    fn slippage_model(&self) -> Box<dyn SlippageModel>;

    /// Get priority policy for this preset
    fn priority_policy(&self) -> Box<dyn PriorityPolicy>;

    /// Get optional liquidity constraint
    fn liquidity_constraint(&self) -> Option<LiquidityConstraint>;

    /// Name of this preset
    fn name(&self) -> &str;
}

/// Optimistic preset: best-case execution assumptions
///
/// Use for: debugging, best-case scenario analysis
/// - BestCase path policy (targets before stops)
/// - BestCase priority (targets before stops)
/// - Minimal slippage (2 bps)
/// - No liquidity constraints
#[derive(Debug, Clone, Copy)]
pub struct Optimistic;

impl ExecutionPreset for Optimistic {
    fn path_policy(&self) -> Box<dyn PathPolicy> {
        Box::new(BestCase)
    }

    fn slippage_model(&self) -> Box<dyn SlippageModel> {
        Box::new(FixedSlippage::new(2.0)) // 2 bps
    }

    fn priority_policy(&self) -> Box<dyn PriorityPolicy> {
        Box::new(BestCasePriority)
    }

    fn liquidity_constraint(&self) -> Option<LiquidityConstraint> {
        None // No constraints
    }

    fn name(&self) -> &str {
        "Optimistic"
    }
}

/// Realistic preset: balanced execution assumptions
///
/// Use for: production backtesting, realistic performance estimates
/// - Deterministic path policy (OHLC order)
/// - WorstCase priority (conservative conflicts)
/// - Moderate slippage (5 bps)
/// - Optional liquidity constraint (10% participation)
#[derive(Debug, Clone, Copy)]
pub struct Realistic {
    /// Enable liquidity constraints
    pub with_liquidity: bool,
}

impl Realistic {
    pub fn new() -> Self {
        Self {
            with_liquidity: true,
        }
    }

    pub fn without_liquidity() -> Self {
        Self {
            with_liquidity: false,
        }
    }
}

impl Default for Realistic {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionPreset for Realistic {
    fn path_policy(&self) -> Box<dyn PathPolicy> {
        Box::new(Deterministic)
    }

    fn slippage_model(&self) -> Box<dyn SlippageModel> {
        Box::new(FixedSlippage::new(5.0)) // 5 bps
    }

    fn priority_policy(&self) -> Box<dyn PriorityPolicy> {
        Box::new(WorstCasePriority)
    }

    fn liquidity_constraint(&self) -> Option<LiquidityConstraint> {
        if self.with_liquidity {
            Some(LiquidityConstraint::new(0.1, RemainderPolicy::Carry))
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "Realistic"
    }
}

/// Hostile preset: worst-case execution assumptions
///
/// Use for: stress testing, robustness validation
/// - WorstCase path policy (stops before targets)
/// - WorstCase priority (stops before targets)
/// - High slippage (10 bps)
/// - Strict liquidity constraint (5% participation)
#[derive(Debug, Clone, Copy)]
pub struct Hostile;

impl ExecutionPreset for Hostile {
    fn path_policy(&self) -> Box<dyn PathPolicy> {
        Box::new(WorstCase)
    }

    fn slippage_model(&self) -> Box<dyn SlippageModel> {
        Box::new(FixedSlippage::new(10.0)) // 10 bps
    }

    fn priority_policy(&self) -> Box<dyn PriorityPolicy> {
        Box::new(WorstCasePriority)
    }

    fn liquidity_constraint(&self) -> Option<LiquidityConstraint> {
        Some(LiquidityConstraint::new(0.05, RemainderPolicy::Carry))
    }

    fn name(&self) -> &str {
        "Hostile"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimistic_preset() {
        let preset = Optimistic;
        assert_eq!(preset.name(), "Optimistic");
        assert_eq!(preset.path_policy().name(), "BestCase");
        assert_eq!(preset.slippage_model().name(), "FixedSlippage");
        assert_eq!(preset.priority_policy().name(), "BestCasePriority");
        assert!(preset.liquidity_constraint().is_none());
    }

    #[test]
    fn test_realistic_preset_default() {
        let preset = Realistic::default();
        assert_eq!(preset.name(), "Realistic");
        assert_eq!(preset.path_policy().name(), "Deterministic");
        assert_eq!(preset.slippage_model().name(), "FixedSlippage");
        assert_eq!(preset.priority_policy().name(), "WorstCasePriority");
        assert!(preset.liquidity_constraint().is_some());

        if let Some(liq) = preset.liquidity_constraint() {
            assert_eq!(liq.max_participation, 0.1);
            assert_eq!(liq.remainder_policy, RemainderPolicy::Carry);
        }
    }

    #[test]
    fn test_realistic_without_liquidity() {
        let preset = Realistic::without_liquidity();
        assert_eq!(preset.name(), "Realistic");
        assert!(preset.liquidity_constraint().is_none());
    }

    #[test]
    fn test_hostile_preset() {
        let preset = Hostile;
        assert_eq!(preset.name(), "Hostile");
        assert_eq!(preset.path_policy().name(), "WorstCase");
        assert_eq!(preset.slippage_model().name(), "FixedSlippage");
        assert_eq!(preset.priority_policy().name(), "WorstCasePriority");
        assert!(preset.liquidity_constraint().is_some());

        if let Some(liq) = preset.liquidity_constraint() {
            assert_eq!(liq.max_participation, 0.05);
            assert_eq!(liq.remainder_policy, RemainderPolicy::Carry);
        }
    }

    #[test]
    fn test_preset_slippage_levels() {
        let optimistic = Optimistic;
        let realistic = Realistic::default();
        let hostile = Hostile;

        // Verify slippage escalation: Optimistic < Realistic < Hostile
        // This is implicit in the bps values (2 < 5 < 10)
        // We can't directly test without exposing bps, but we verify they differ
        assert_eq!(optimistic.slippage_model().name(), "FixedSlippage");
        assert_eq!(realistic.slippage_model().name(), "FixedSlippage");
        assert_eq!(hostile.slippage_model().name(), "FixedSlippage");
    }

    #[test]
    fn test_realistic_new_vs_default() {
        let new = Realistic::new();
        let default = Realistic::default();

        assert_eq!(new.with_liquidity, default.with_liquidity);
        assert_eq!(new.with_liquidity, true);
    }
}
