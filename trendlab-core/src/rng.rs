//! Deterministic RNG hierarchy.
//!
//! A master seed generates deterministic sub-seeds for each `(run_id, symbol, iteration)`
//! tuple. Sub-seeds are derived via BLAKE3 hashing, independently of thread scheduling
//! order, so results are identical regardless of thread count.

use crate::domain::RunId;
use rand::rngs::StdRng;
use rand::SeedableRng;

/// Deterministic RNG hierarchy.
///
/// The master seed is expanded into per-(symbol, iteration) sub-seeds using
/// BLAKE3. Because derivation is hash-based (not order-dependent), the same
/// master seed produces identical sub-seeds regardless of the order in which
/// symbols or iterations are processed.
#[derive(Debug, Clone)]
pub struct RngHierarchy {
    master_seed: u64,
}

impl RngHierarchy {
    pub fn new(master_seed: u64) -> Self {
        Self { master_seed }
    }

    pub fn master_seed(&self) -> u64 {
        self.master_seed
    }

    /// Derive a deterministic sub-seed for a specific (run_id, symbol, iteration).
    ///
    /// The sub-seed is independent of derivation order: calling
    /// `sub_seed(run, "SPY", 0)` then `sub_seed(run, "QQQ", 0)` produces the
    /// same results as calling them in reverse order.
    pub fn sub_seed(&self, run_id: &RunId, symbol: &str, iteration: u64) -> u64 {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.master_seed.to_le_bytes());
        hasher.update(&run_id.0);
        hasher.update(symbol.as_bytes());
        hasher.update(&iteration.to_le_bytes());
        let hash = hasher.finalize();
        u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap())
    }

    /// Create a seeded StdRng from a sub-seed.
    pub fn rng_for(&self, run_id: &RunId, symbol: &str, iteration: u64) -> StdRng {
        let seed = self.sub_seed(run_id, symbol, iteration);
        StdRng::seed_from_u64(seed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_seeds_are_deterministic() {
        let hierarchy = RngHierarchy::new(42);
        let run_id = RunId::from_bytes(b"test-run-1");

        let s1 = hierarchy.sub_seed(&run_id, "SPY", 0);
        let s2 = hierarchy.sub_seed(&run_id, "SPY", 0);
        assert_eq!(s1, s2);
    }

    #[test]
    fn different_symbols_different_seeds() {
        let hierarchy = RngHierarchy::new(42);
        let run_id = RunId::from_bytes(b"test-run-1");

        let spy = hierarchy.sub_seed(&run_id, "SPY", 0);
        let qqq = hierarchy.sub_seed(&run_id, "QQQ", 0);
        assert_ne!(spy, qqq);
    }

    #[test]
    fn different_iterations_different_seeds() {
        let hierarchy = RngHierarchy::new(42);
        let run_id = RunId::from_bytes(b"test-run-1");

        let i0 = hierarchy.sub_seed(&run_id, "SPY", 0);
        let i1 = hierarchy.sub_seed(&run_id, "SPY", 1);
        assert_ne!(i0, i1);
    }

    #[test]
    fn derivation_order_independent() {
        let hierarchy = RngHierarchy::new(42);
        let run_id = RunId::from_bytes(b"test-run-1");

        // Derive SPY then QQQ
        let spy_first = hierarchy.sub_seed(&run_id, "SPY", 0);
        let qqq_second = hierarchy.sub_seed(&run_id, "QQQ", 0);

        // Derive QQQ then SPY (reversed order)
        let qqq_first = hierarchy.sub_seed(&run_id, "QQQ", 0);
        let spy_second = hierarchy.sub_seed(&run_id, "SPY", 0);

        // Same seeds regardless of derivation order
        assert_eq!(spy_first, spy_second);
        assert_eq!(qqq_first, qqq_second);
    }

    #[test]
    fn different_master_seeds_different_output() {
        let h1 = RngHierarchy::new(42);
        let h2 = RngHierarchy::new(43);
        let run_id = RunId::from_bytes(b"test-run-1");

        assert_ne!(
            h1.sub_seed(&run_id, "SPY", 0),
            h2.sub_seed(&run_id, "SPY", 0)
        );
    }
}
