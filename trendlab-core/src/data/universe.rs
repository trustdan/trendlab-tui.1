use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Universe of symbols
/// Uses BTreeSet for deterministic iteration order (required for stable hashing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Universe {
    pub name: String,
    pub symbols: BTreeSet<String>,
}

impl Universe {
    pub fn new(name: String, symbols: Vec<String>) -> Self {
        Self {
            name,
            symbols: symbols.into_iter().collect(),
        }
    }

    pub fn contains(&self, symbol: &str) -> bool {
        self.symbols.contains(symbol)
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// Collection of named universes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseSet {
    pub universes: Vec<Universe>,
}

impl UniverseSet {
    pub fn new() -> Self {
        Self {
            universes: Vec::new(),
        }
    }

    pub fn add_universe(&mut self, universe: Universe) {
        self.universes.push(universe);
    }

    pub fn get(&self, name: &str) -> Option<&Universe> {
        self.universes.iter().find(|u| u.name == name)
    }
}

impl Default for UniverseSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universe_contains() {
        let universe = Universe::new(
            "sp500".into(),
            vec!["AAPL".into(), "MSFT".into(), "GOOGL".into()],
        );
        assert!(universe.contains("AAPL"));
        assert!(!universe.contains("TSLA"));
    }

    #[test]
    fn test_universe_deterministic_order() {
        // BTreeSet maintains sorted order
        let universe = Universe::new(
            "test".into(),
            vec!["ZZZ".into(), "AAA".into(), "MMM".into()],
        );
        let symbols: Vec<_> = universe.symbols.iter().collect();
        assert_eq!(symbols, vec![&"AAA".to_string(), &"MMM".to_string(), &"ZZZ".to_string()]);
    }

    #[test]
    fn test_universe_len() {
        let universe = Universe::new(
            "test".into(),
            vec!["AAPL".into(), "MSFT".into()],
        );
        assert_eq!(universe.len(), 2);
        assert!(!universe.is_empty());
    }

    #[test]
    fn test_universe_set_get() {
        let mut set = UniverseSet::new();
        let universe = Universe::new(
            "sp500".into(),
            vec!["AAPL".into(), "MSFT".into()],
        );
        set.add_universe(universe);

        assert!(set.get("sp500").is_some());
        assert!(set.get("nasdaq100").is_none());
    }
}
