# crates/agents/arena-coordinator/

**Role:** `settlement` — orchestrates the multi-agent market round.

## Responsibilities

1. Opens a market round when a qualifying `TxLineEvent` arrives.
2. Broadcasts the bid request to all registered agents.
3. Collects bids, runs `choose_winner` to select the winning agent.
4. Triggers the winner's `deliver()` call and waits for the `AgentDelivery`.
5. Routes the delivery to the verifier agent.
6. On a passing verdict, initiates the settlement flow (Solana Pay or escrow).

## Settlement rails

| Rail | Condition |
|------|-----------|
| Solana Pay | `priceSol > 0` and wallet is connected |
| CoralOS escrow | Escrow PDA is pre-funded |
| Off-chain | Fallback when no on-chain rail is available |

## Running tests

```bash
cargo test -p arena-coordinator
```
