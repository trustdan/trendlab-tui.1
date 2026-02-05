use crate::orders::order_type::{MarketTiming, OrderType, StopDirection};

/// OrderPolicy: translates signal intent into concrete order types
///
/// Different signal families prefer different order types:
/// - Breakout signals → stop entries (enter above/below level)
/// - Mean-reversion signals → limit entries (enter at level)
/// - Trend-following → market entries (enter now)
pub trait OrderPolicy {
    fn entry_order(&self, signal_price: f64, is_long: bool) -> OrderType;
    fn exit_order(&self) -> OrderType;
}

/// Breakout order policy: use stop entries
pub struct BreakoutPolicy;

impl OrderPolicy for BreakoutPolicy {
    fn entry_order(&self, signal_price: f64, is_long: bool) -> OrderType {
        OrderType::StopMarket {
            direction: if is_long {
                StopDirection::Buy
            } else {
                StopDirection::Sell
            },
            trigger_price: signal_price,
        }
    }

    fn exit_order(&self) -> OrderType {
        OrderType::Market(MarketTiming::MOC)
    }
}

/// Mean-reversion order policy: use limit entries
pub struct MeanReversionPolicy;

impl OrderPolicy for MeanReversionPolicy {
    fn entry_order(&self, signal_price: f64, _is_long: bool) -> OrderType {
        OrderType::Limit {
            limit_price: signal_price,
        }
    }

    fn exit_order(&self) -> OrderType {
        OrderType::Market(MarketTiming::MOC)
    }
}

/// Immediate entry policy: use market orders
pub struct ImmediatePolicy;

impl OrderPolicy for ImmediatePolicy {
    fn entry_order(&self, _signal_price: f64, _is_long: bool) -> OrderType {
        OrderType::Market(MarketTiming::MOO)
    }

    fn exit_order(&self) -> OrderType {
        OrderType::Market(MarketTiming::MOC)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breakout_policy_long() {
        let policy = BreakoutPolicy;
        let order = policy.entry_order(100.0, true);

        match order {
            OrderType::StopMarket {
                direction,
                trigger_price,
            } => {
                assert_eq!(direction, StopDirection::Buy);
                assert_eq!(trigger_price, 100.0);
            }
            _ => panic!("Expected StopMarket"),
        }
    }

    #[test]
    fn test_breakout_policy_short() {
        let policy = BreakoutPolicy;
        let order = policy.entry_order(100.0, false);

        match order {
            OrderType::StopMarket {
                direction,
                trigger_price,
            } => {
                assert_eq!(direction, StopDirection::Sell);
                assert_eq!(trigger_price, 100.0);
            }
            _ => panic!("Expected StopMarket"),
        }
    }

    #[test]
    fn test_mean_reversion_policy() {
        let policy = MeanReversionPolicy;
        let order = policy.entry_order(95.5, true);

        match order {
            OrderType::Limit { limit_price } => {
                assert_eq!(limit_price, 95.5);
            }
            _ => panic!("Expected Limit"),
        }
    }

    #[test]
    fn test_immediate_policy() {
        let policy = ImmediatePolicy;
        let entry = policy.entry_order(100.0, true);
        let exit = policy.exit_order();

        assert_eq!(entry, OrderType::Market(MarketTiming::MOO));
        assert_eq!(exit, OrderType::Market(MarketTiming::MOC));
    }

    #[test]
    fn test_policy_exit_orders() {
        let breakout = BreakoutPolicy;
        let mean_rev = MeanReversionPolicy;
        let immediate = ImmediatePolicy;

        // All policies use MOC for exits
        assert_eq!(breakout.exit_order(), OrderType::Market(MarketTiming::MOC));
        assert_eq!(
            mean_rev.exit_order(),
            OrderType::Market(MarketTiming::MOC)
        );
        assert_eq!(
            immediate.exit_order(),
            OrderType::Market(MarketTiming::MOC)
        );
    }
}
