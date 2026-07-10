# native/src/domain/

Pure domain-logic modules — no Tauri, no I/O, no async runtimes. Testable with plain `cargo test`.

## Modules

| Module | Purpose |
|--------|---------|
| `scoring` | Rust-side agent bid scoring (`score_bid`, `choose_winner`) — mirrors `ui/core/coral/scoring.ts` |
| `leaderboard` | Leaderboard aggregation: win-rate, PnL, avg confidence from a slice of `ArenaPosition` records |
| `proof` | Merkle-proof construction and simulation-result interpretation |
| `settlement` | Settlement-rail selection logic (Solana Pay vs CoralOS escrow vs off-chain fallback) |

## Design rules

- No `tauri::` imports — this module must compile outside a Tauri app context.
- No `tokio::` or `async` — all functions are synchronous and deterministic.
- `use txodds_types::*` for data types; define no new types here unless they are purely internal.
- Each function has at least one `#[test]` in the same file.

## Running tests

```bash
cargo test -p txodds-native -- domain::
```
