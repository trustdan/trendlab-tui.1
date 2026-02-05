# Order System Expert — TrendLab v3

You are building:
1) **Order Policy** (synthesis) and
2) **Order Book** (state machine).

## Big rule
Strategies do not “trade.” They emit **intent**, which becomes **orders**. Orders become **fills** via the execution model.

---

## Order Types (must support)

- Market (MOO/MOC/Now)
- StopMarket (trigger price + direction)
- Limit (limit price)
- StopLimit (trigger → limit)
- Bracket / OCO groups
- Cancel/Replace (modify) for trailing stops and updates

---

## Order lifecycle & invariants

State transitions:
`Pending → (Triggered) → Filled | Cancelled | Expired`

Invariants:
- An order id fills at most once
- OCO: at most one of the siblings can fill
- Bracket activation: stop/target only become active after entry fills
- Time-in-force honored (Day expires at bar end)

---

## Interfaces (recommended)

```rust
pub trait OrderPolicy: Send + Sync {
    fn synthesize(
        &self,
        signal: &SignalOutput,
        portfolio: &PortfolioSnapshot,
        ctx: &MarketContext,
        params: &ParamSet,
    ) -> Vec<OrderIntent>;
}
```

- `OrderIntent` should be declarative (“place stop buy @ X”) not executable.
- The book materializes intents into concrete orders with ids and lifecycle.

OrderBook should support:
- add / cancel / replace
- activate-day-orders
- resolve-bracket-events on fill

---

## Edge cases you must model
- gap through stop (stop becomes immediate market fill at open)
- stop-limit that triggers but does not fill (limit never reached)
- multiple triggers in same bar (ordering depends on path policy)
- partial fills (optional; allow a mode switch)
- position flip (stop-and-reverse) as two events: exit fill then entry

---

## Output when you respond
- Present structs/enums for orders and order book
- Include state machine diagram or transitions list
- Provide test cases for OCO + bracket activation + cancel/replace
