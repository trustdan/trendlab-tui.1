use serde::{Deserialize, Serialize};
use std::fmt;

/// Deterministic configuration ID (hash of strategy + params + execution config)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConfigId(pub String);

impl ConfigId {
    pub fn from_hash(hash: &str) -> Self {
        Self(hash.to_string())
    }
}

impl fmt::Display for ConfigId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Deterministic dataset hash (content hash of canonicalized data)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DatasetHash(pub String);

impl DatasetHash {
    pub fn from_hash(hash: &str) -> Self {
        Self(hash.to_string())
    }
}

impl fmt::Display for DatasetHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Deterministic run ID (config + dataset + seed)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId {
    pub config_id: ConfigId,
    pub dataset_hash: DatasetHash,
    pub seed: u64,
}

impl RunId {
    pub fn new(config_id: ConfigId, dataset_hash: DatasetHash, seed: u64) -> Self {
        Self { config_id, dataset_hash, seed }
    }

    /// Generate deterministic run hash
    /// Uses BLAKE3 for stable, collision-resistant hashing across builds/platforms
    pub fn hash(&self) -> String {
        use serde_json::json;

        // Canonical serialization (sorted keys)
        let canonical = json!({
            "config_id": &self.config_id.0,
            "dataset_hash": &self.dataset_hash.0,
            "seed": self.seed,
        });

        // Use BLAKE3 for stable deterministic hash
        // Alternative: xxhash64 if BLAKE3 dep is too heavy
        let hash_bytes = blake3::hash(canonical.to_string().as_bytes());
        hash_bytes.to_hex().to_string()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.config_id, self.dataset_hash, self.seed)
    }
}

/// Order ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(pub String);

impl OrderId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<u64> for OrderId {
    fn from(id: u64) -> Self {
        Self(id.to_string())
    }
}

/// Fill ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FillId(pub String);

impl FillId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

/// Trade ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TradeId(pub String);

impl TradeId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_id_deterministic() {
        let run1 = RunId::new(ConfigId::from_hash("abc123"), DatasetHash::from_hash("def456"), 42);
        let run2 = RunId::new(ConfigId::from_hash("abc123"), DatasetHash::from_hash("def456"), 42);
        assert_eq!(run1.hash(), run2.hash());
    }

    #[test]
    fn test_run_id_different_seed_different_hash() {
        let run1 = RunId::new(ConfigId::from_hash("abc123"), DatasetHash::from_hash("def456"), 42);
        let run2 = RunId::new(ConfigId::from_hash("abc123"), DatasetHash::from_hash("def456"), 43);
        assert_ne!(run1.hash(), run2.hash());
    }
}
