//! TrendLab Runner — backtest orchestration, YOLO mode, leaderboards, metrics.
//!
//! This crate builds on `trendlab-core` to provide:
//! - Single-backtest runner with trade extraction and metrics
//! - YOLO mode (continuous auto-discovery engine)
//! - Per-symbol and cross-symbol leaderboards
//! - Risk profile ranking system
//! - Run fingerprinting and JSONL history
//! - Promotion ladder (walk-forward, execution MC, bootstrap)

#[cfg(test)]
mod tests {
    #[test]
    fn it_links() {
        assert_eq!(2 + 2, 4);
    }
}
