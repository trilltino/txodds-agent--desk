# crates/txodds-types/

Shared domain types for the TxOdds Agent Desk — the single source of truth for data shapes used by the Rust backend, Tauri IPC, and TypeScript frontend.

## Modules

| Module | Purpose |
|--------|---------|
| `agent` | `AgentBid`, `AgentDelivery`, `VerificationVerdict`, `AgentRun` |
| `chain` | On-chain account types, PDA seeds, cluster config |
| `cluster` | Solana cluster constants and RPC endpoint helpers |
| `coral` | `CoralSession`, `CoralMessage`, `CoralVerb`, market-round types |
| `oracle` | `TxOracleRootEvent`, Merkle-root instruction types |
| `trace` | `AgentTraceEvent`, `AgentTracePhase` for LLM reasoning traces |
| `txline` | `TxLineEvent`, `OddsQuote`, `Fixture`, `TxLineProofReceipt` |
| `wallet` | `WalletContext`, `SolanaPayIntent`, `SettlementReceipt` |

## Mirror contract

Every type here has a corresponding TypeScript interface or type in `ui/types.ts`.  When changing a type in either place, update both. The Tauri IPC layer performs JSON serialisation — no explicit conversion code is needed as long as field names match.

## Running tests

```bash
cargo test -p txodds-types
```
