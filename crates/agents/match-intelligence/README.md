# crates/agents/match-intelligence/

**Role:** `sharp` — the primary market-intelligence agent.

## Strategy

Analyses TxLINE odds movements, sharp-money signals, and live match data to produce high-confidence betting intelligence for the trading track.

## Tool calls

| Tool | Description |
|------|-------------|
| `get_odds_history` | Retrieves historical odds for the fixture from TxLINE |
| `get_sharp_signal` | Queries the sharp-movement-detector agent for its current signal |
| `get_proof_status` | Checks whether a TxLINE Merkle proof is available for the current stat root |

## Bid parameters (typical)

- `role`: `sharp`
- `confidence`: derived from odds-movement magnitude and sharp-money consensus (0.6–0.95)
- `priceSol`: 0.02–0.05 SOL depending on market complexity
- `etaMs`: 800–1500ms

## Running tests

```bash
cargo test -p match-intelligence
```
