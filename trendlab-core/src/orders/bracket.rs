use crate::domain::{OrderId, Symbol};
use crate::orders::order_book::OrderBook;
use crate::orders::order_type::{OrderType, StopDirection};

/// Bracket order builder
pub struct BracketOrderBuilder {
    symbol: Symbol,
    entry_order: OrderType,
    qty: u32,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
}

impl BracketOrderBuilder {
    pub fn new(symbol: Symbol, entry_order: OrderType, qty: u32) -> Self {
        Self {
            symbol,
            entry_order,
            qty,
            stop_loss: None,
            take_profit: None,
        }
    }

    pub fn with_stop_loss(mut self, stop_price: f64) -> Self {
        self.stop_loss = Some(stop_price);
        self
    }

    pub fn with_take_profit(mut self, target_price: f64) -> Self {
        self.take_profit = Some(target_price);
        self
    }

    /// Submit bracket order to book
    /// Returns (entry_id, stop_id, target_id)
    pub fn submit(
        self,
        book: &mut OrderBook,
        bar: usize,
    ) -> (OrderId, Option<OrderId>, Option<OrderId>) {
        // Submit entry order
        let entry_id = book.submit(self.symbol.clone(), self.entry_order, self.qty, bar);

        // Submit stop-loss (if provided)
        let stop_id = self.stop_loss.map(|stop_price| {
            let stop_order = OrderType::StopMarket {
                direction: StopDirection::Sell,
                trigger_price: stop_price,
            };
            let id = book.submit(self.symbol.clone(), stop_order, self.qty, bar);

            // Link to parent
            if let Some(order) = book.get_mut(&id) {
                order.parent_id = Some(entry_id.clone());
            }

            id
        });

        // Submit take-profit (if provided)
        let target_id = self.take_profit.map(|target_price| {
            let target_order = OrderType::Limit {
                limit_price: target_price,
            };
            let id = book.submit(self.symbol.clone(), target_order, self.qty, bar);

            // Link to parent
            if let Some(order) = book.get_mut(&id) {
                order.parent_id = Some(entry_id.clone());
            }

            id
        });

        // Set OCO relationship between stop and target
        if let (Some(stop_id), Some(target_id)) = (stop_id.clone(), target_id.clone()) {
            book.set_oco(stop_id, target_id).ok();
        }

        (entry_id, stop_id, target_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::order::OrderState;
    use crate::orders::order_type::MarketTiming;

    #[test]
    fn test_bracket_with_stop_and_target() {
        let mut book = OrderBook::new();

        let bracket = BracketOrderBuilder::new(
            "SPY".to_string(),
            OrderType::Market(MarketTiming::MOO),
            100,
        )
        .with_stop_loss(95.0)
        .with_take_profit(105.0);

        let (entry_id, stop_id, target_id) = bracket.submit(&mut book, 10);

        // Entry should be active (market order)
        assert_eq!(book.get(&entry_id).unwrap().state, OrderState::Active);

        // Stop and target should be pending
        assert_eq!(
            book.get(stop_id.as_ref().unwrap()).unwrap().state,
            OrderState::Pending
        );
        assert_eq!(
            book.get(target_id.as_ref().unwrap()).unwrap().state,
            OrderState::Pending
        );

        // Verify parent linkage
        assert_eq!(
            book.get(stop_id.as_ref().unwrap()).unwrap().parent_id,
            Some(entry_id.clone())
        );
        assert_eq!(
            book.get(target_id.as_ref().unwrap()).unwrap().parent_id,
            Some(entry_id)
        );

        // Verify OCO linkage
        assert_eq!(
            book.get(stop_id.as_ref().unwrap())
                .unwrap()
                .oco_sibling_id,
            target_id
        );
        assert_eq!(
            book.get(target_id.as_ref().unwrap()).unwrap().oco_sibling_id,
            stop_id
        );
    }

    #[test]
    fn test_bracket_activation_on_entry_fill() {
        let mut book = OrderBook::new();

        let bracket = BracketOrderBuilder::new(
            "SPY".to_string(),
            OrderType::Market(MarketTiming::MOO),
            100,
        )
        .with_stop_loss(95.0)
        .with_take_profit(105.0);

        let (entry_id, stop_id, target_id) = bracket.submit(&mut book, 10);

        // Fill entry
        book.fill(entry_id.clone(), 100, 10).unwrap();

        // Now activate children (this would be done by engine)
        book.activate(stop_id.clone().unwrap()).unwrap();
        book.activate(target_id.clone().unwrap()).unwrap();

        assert_eq!(
            book.get(stop_id.as_ref().unwrap()).unwrap().state,
            OrderState::Active
        );
        assert_eq!(
            book.get(target_id.as_ref().unwrap()).unwrap().state,
            OrderState::Active
        );
    }

    #[test]
    fn test_bracket_oco_behavior() {
        let mut book = OrderBook::new();

        let bracket = BracketOrderBuilder::new(
            "SPY".to_string(),
            OrderType::Market(MarketTiming::MOO),
            100,
        )
        .with_stop_loss(95.0)
        .with_take_profit(105.0);

        let (entry_id, stop_id, target_id) = bracket.submit(&mut book, 10);

        // Fill entry and activate children
        book.fill(entry_id, 100, 10).unwrap();
        book.activate(stop_id.clone().unwrap()).unwrap();
        book.activate(target_id.clone().unwrap()).unwrap();

        // Fill target
        book.fill(target_id.clone().unwrap(), 100, 12).unwrap();

        // Verify target filled
        assert_eq!(
            book.get(target_id.as_ref().unwrap()).unwrap().state,
            OrderState::Filled
        );

        // Verify stop cancelled (OCO behavior)
        assert_eq!(
            book.get(stop_id.as_ref().unwrap()).unwrap().state,
            OrderState::Cancelled
        );
    }

    #[test]
    fn test_bracket_with_only_stop() {
        let mut book = OrderBook::new();

        let bracket = BracketOrderBuilder::new(
            "SPY".to_string(),
            OrderType::Market(MarketTiming::MOO),
            100,
        )
        .with_stop_loss(95.0);

        let (entry_id, stop_id, target_id) = bracket.submit(&mut book, 10);

        assert_eq!(book.get(&entry_id).unwrap().state, OrderState::Active);
        assert!(stop_id.is_some());
        assert!(target_id.is_none());
    }

    #[test]
    fn test_bracket_with_only_target() {
        let mut book = OrderBook::new();

        let bracket = BracketOrderBuilder::new(
            "SPY".to_string(),
            OrderType::Market(MarketTiming::MOO),
            100,
        )
        .with_take_profit(105.0);

        let (entry_id, stop_id, target_id) = bracket.submit(&mut book, 10);

        assert_eq!(book.get(&entry_id).unwrap().state, OrderState::Active);
        assert!(stop_id.is_none());
        assert!(target_id.is_some());
    }
}
