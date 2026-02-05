/// Warmup state tracker
#[derive(Debug, Clone)]
pub struct WarmupState {
    warmup_bars: usize,
    bars_processed: usize,
}

impl WarmupState {
    pub fn new(warmup_bars: usize) -> Self {
        Self {
            warmup_bars,
            bars_processed: 0,
        }
    }

    /// Compute warmup from feature requirements (max lookback across all indicators)
    pub fn from_features(features: &[impl Indicator]) -> Self {
        let max_lookback = features
            .iter()
            .map(|f| f.max_lookback())
            .max()
            .unwrap_or(0);
        Self::new(max_lookback)
    }

    pub fn process_bar(&mut self) {
        self.bars_processed += 1;
    }

    pub fn is_warm(&self) -> bool {
        self.bars_processed >= self.warmup_bars
    }

    pub fn bars_until_warm(&self) -> usize {
        if self.is_warm() {
            0
        } else {
            self.warmup_bars - self.bars_processed
        }
    }
}

/// Trait for indicators to expose their lookback requirements
pub trait Indicator {
    fn max_lookback(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warmup_state() {
        let mut warmup = WarmupState::new(20);
        assert!(!warmup.is_warm());
        assert_eq!(warmup.bars_until_warm(), 20);

        for _ in 0..19 {
            warmup.process_bar();
        }
        assert!(!warmup.is_warm());
        assert_eq!(warmup.bars_until_warm(), 1);

        warmup.process_bar();
        assert!(warmup.is_warm());
        assert_eq!(warmup.bars_until_warm(), 0);
    }

    #[test]
    fn test_zero_warmup() {
        let warmup = WarmupState::new(0);
        assert!(warmup.is_warm());
        assert_eq!(warmup.bars_until_warm(), 0);
    }

    #[test]
    fn test_warmup_progress() {
        let mut warmup = WarmupState::new(10);

        for i in 1..=10 {
            warmup.process_bar();
            if i < 10 {
                assert!(!warmup.is_warm());
                assert_eq!(warmup.bars_until_warm(), 10 - i);
            } else {
                assert!(warmup.is_warm());
                assert_eq!(warmup.bars_until_warm(), 0);
            }
        }
    }

    struct MockIndicator {
        lookback: usize,
    }

    impl Indicator for MockIndicator {
        fn max_lookback(&self) -> usize {
            self.lookback
        }
    }

    #[test]
    fn test_from_features() {
        let features = vec![
            MockIndicator { lookback: 20 },
            MockIndicator { lookback: 50 },
            MockIndicator { lookback: 10 },
        ];

        let warmup = WarmupState::from_features(&features);
        assert_eq!(warmup.bars_until_warm(), 50); // max lookback
    }

    #[test]
    fn test_from_empty_features() {
        let features: Vec<MockIndicator> = vec![];
        let warmup = WarmupState::from_features(&features);
        assert_eq!(warmup.bars_until_warm(), 0);
        assert!(warmup.is_warm());
    }
}
