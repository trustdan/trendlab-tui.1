/// Position manager trait and registry
///
/// PM strategies emit order intents (not direct fills) to modify positions.
/// This ensures clean separation between PM logic and execution.
use crate::domain::{Bar, Position};
use crate::position_management::intent::OrderIntent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Position side (semantic representation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    /// Long position (positive quantity)
    Long,
    /// Short position (negative quantity)
    Short,
}

impl Side {
    /// Determine side from position quantity
    pub fn from_quantity(qty: f64) -> Option<Self> {
        if qty > 0.0 {
            Some(Side::Long)
        } else if qty < 0.0 {
            Some(Side::Short)
        } else {
            None // Flat
        }
    }
}

/// Position management strategy trait
///
/// All PM strategies implement this trait to emit order intents
/// based on the current position and market data.
///
/// **Key invariants:**
/// - PM strategies emit intents, never direct fills
/// - Intents are processed through the order book
/// - Ratchet rules are enforced within the strategy (e.g., AtrStop)
pub trait PositionManager: Send + Sync {
    /// Update position based on current bar
    ///
    /// Returns a vector of order intents (cancel/replace/new orders).
    /// Returns OrderIntent::None if no action is needed.
    fn update(&mut self, position: &Position, bar: &Bar) -> Vec<OrderIntent>;

    /// Get strategy name (for logging and manifest)
    fn name(&self) -> &str;

    /// Clone into a boxed trait object
    fn clone_box(&self) -> Box<dyn PositionManager>;
}

/// Position manager registry
///
/// Factory for creating PM strategies by name.
/// Enables dynamic strategy construction from manifests.
pub struct PmRegistry {
    constructors: HashMap<String, Box<dyn Fn() -> Box<dyn PositionManager> + Send + Sync>>,
}

impl PmRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            constructors: HashMap::new(),
        }
    }

    /// Register a PM strategy constructor
    pub fn register<F>(&mut self, name: impl Into<String>, constructor: F)
    where
        F: Fn() -> Box<dyn PositionManager> + Send + Sync + 'static,
    {
        self.constructors.insert(name.into(), Box::new(constructor));
    }

    /// Create a PM strategy by name
    pub fn create(&self, name: &str) -> Option<Box<dyn PositionManager>> {
        self.constructors.get(name).map(|ctor| ctor())
    }

    /// List all registered strategy names
    pub fn list_strategies(&self) -> Vec<&str> {
        self.constructors.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a strategy is registered
    pub fn contains(&self, name: &str) -> bool {
        self.constructors.contains_key(name)
    }
}

impl Default for PmRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock PM for testing
    #[derive(Clone)]
    struct MockPm {
        name: String,
    }

    impl PositionManager for MockPm {
        fn update(&mut self, _position: &Position, _bar: &Bar) -> Vec<OrderIntent> {
            vec![OrderIntent::None]
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn clone_box(&self) -> Box<dyn PositionManager> {
            Box::new(self.clone())
        }
    }

    #[test]
    fn test_side_from_quantity() {
        assert_eq!(Side::from_quantity(100.0), Some(Side::Long));
        assert_eq!(Side::from_quantity(-100.0), Some(Side::Short));
        assert_eq!(Side::from_quantity(0.0), None);
    }

    #[test]
    fn test_pm_registry_register_and_create() {
        let mut registry = PmRegistry::new();

        registry.register("test_pm", || {
            Box::new(MockPm {
                name: "test_pm".to_string(),
            })
        });

        let pm = registry.create("test_pm");
        assert!(pm.is_some());
        assert_eq!(pm.unwrap().name(), "test_pm");
    }

    #[test]
    fn test_pm_registry_missing_strategy() {
        let registry = PmRegistry::new();
        let pm = registry.create("nonexistent");
        assert!(pm.is_none());
    }

    #[test]
    fn test_pm_registry_contains() {
        let mut registry = PmRegistry::new();
        registry.register("test_pm", || {
            Box::new(MockPm {
                name: "test_pm".to_string(),
            })
        });

        assert!(registry.contains("test_pm"));
        assert!(!registry.contains("missing"));
    }

    #[test]
    fn test_pm_registry_list_strategies() {
        let mut registry = PmRegistry::new();
        registry.register("pm1", || {
            Box::new(MockPm {
                name: "pm1".to_string(),
            })
        });
        registry.register("pm2", || {
            Box::new(MockPm {
                name: "pm2".to_string(),
            })
        });

        let mut strategies = registry.list_strategies();
        strategies.sort();
        assert_eq!(strategies, vec!["pm1", "pm2"]);
    }
}
