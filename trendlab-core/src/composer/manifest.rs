//! Strategy Manifest — immutable configuration record
//!
//! Manifests provide:
//! - Deterministic hashing for cache invalidation
//! - Reproducible strategy identification
//! - Audit trail for backtest results

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Immutable strategy configuration manifest
///
/// # Purpose
/// - Cache key for result storage
/// - Reproducibility audit trail
/// - Strategy comparison identity
///
/// # Determinism Requirements
/// - All fields must serialize deterministically
/// - Hash must be stable across platforms/builds
/// - Uses BLAKE3 for collision resistance
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrategyManifest {
    pub signal_name: String,
    pub signal_params: BTreeMap<String, String>, // Future: parameterized signals
    pub order_policy_name: String,
    pub pm_name: String,
    pub pm_params: BTreeMap<String, String>, // Future: parameterized PMs
    pub sizer_name: String,
    pub execution_preset: String,
    pub config_hash: String, // Deterministic hash of all above
}

impl StrategyManifest {
    /// Create new manifest with deterministic hash
    pub fn new(
        signal_name: String,
        order_policy_name: String,
        pm_name: String,
        sizer_name: String,
        execution_preset: String,
    ) -> Self {
        let signal_params = BTreeMap::new();
        let pm_params = BTreeMap::new();

        let config_hash = Self::compute_hash(
            &signal_name,
            &signal_params,
            &order_policy_name,
            &pm_name,
            &pm_params,
            &sizer_name,
            &execution_preset,
        );

        Self {
            signal_name,
            signal_params,
            order_policy_name,
            pm_name,
            pm_params,
            sizer_name,
            execution_preset,
            config_hash,
        }
    }

    /// Create manifest with custom parameters
    pub fn with_params(
        signal_name: String,
        signal_params: BTreeMap<String, String>,
        order_policy_name: String,
        pm_name: String,
        pm_params: BTreeMap<String, String>,
        sizer_name: String,
        execution_preset: String,
    ) -> Self {
        let config_hash = Self::compute_hash(
            &signal_name,
            &signal_params,
            &order_policy_name,
            &pm_name,
            &pm_params,
            &sizer_name,
            &execution_preset,
        );

        Self {
            signal_name,
            signal_params,
            order_policy_name,
            pm_name,
            pm_params,
            sizer_name,
            execution_preset,
            config_hash,
        }
    }

    /// Compute deterministic BLAKE3 hash
    fn compute_hash(
        signal_name: &str,
        signal_params: &BTreeMap<String, String>,
        order_policy_name: &str,
        pm_name: &str,
        pm_params: &BTreeMap<String, String>,
        sizer_name: &str,
        execution_preset: &str,
    ) -> String {
        use serde_json::json;

        // Canonical JSON serialization (BTreeMap ensures sorted keys)
        let canonical = json!({
            "signal": signal_name,
            "signal_params": signal_params,
            "order_policy": order_policy_name,
            "pm": pm_name,
            "pm_params": pm_params,
            "sizer": sizer_name,
            "execution": execution_preset,
        });

        // BLAKE3 hash (collision-resistant, deterministic)
        let hash_bytes = blake3::hash(canonical.to_string().as_bytes());
        hash_bytes.to_hex().to_string()
    }

    /// Verify hash matches current configuration
    pub fn verify_hash(&self) -> bool {
        let expected = Self::compute_hash(
            &self.signal_name,
            &self.signal_params,
            &self.order_policy_name,
            &self.pm_name,
            &self.pm_params,
            &self.sizer_name,
            &self.execution_preset,
        );
        self.config_hash == expected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_hash_deterministic() {
        let m1 = StrategyManifest::new(
            "MA_Cross".into(),
            "Natural".into(),
            "FixedStop".into(),
            "FixedShares".into(),
            "Deterministic".into(),
        );

        let m2 = StrategyManifest::new(
            "MA_Cross".into(),
            "Natural".into(),
            "FixedStop".into(),
            "FixedShares".into(),
            "Deterministic".into(),
        );

        assert_eq!(m1.config_hash, m2.config_hash);
        assert!(m1.verify_hash());
        assert!(m2.verify_hash());
    }

    #[test]
    fn test_manifest_hash_changes_with_signal() {
        let m1 = StrategyManifest::new(
            "MA_Cross".into(),
            "Natural".into(),
            "FixedStop".into(),
            "FixedShares".into(),
            "Deterministic".into(),
        );

        let m2 = StrategyManifest::new(
            "Donchian".into(), // Different signal
            "Natural".into(),
            "FixedStop".into(),
            "FixedShares".into(),
            "Deterministic".into(),
        );

        assert_ne!(m1.config_hash, m2.config_hash);
    }

    #[test]
    fn test_manifest_with_params() {
        let mut signal_params = BTreeMap::new();
        signal_params.insert("fast".into(), "20".into());
        signal_params.insert("slow".into(), "50".into());

        let mut pm_params = BTreeMap::new();
        pm_params.insert("stop_pct".into(), "0.02".into());

        let m = StrategyManifest::with_params(
            "MA_Cross".into(),
            signal_params,
            "Natural".into(),
            "FixedStop".into(),
            pm_params,
            "FixedShares".into(),
            "Deterministic".into(),
        );

        assert!(m.verify_hash());
        assert_eq!(m.signal_params.get("fast"), Some(&"20".to_string()));
        assert_eq!(m.pm_params.get("stop_pct"), Some(&"0.02".to_string()));
    }
}
