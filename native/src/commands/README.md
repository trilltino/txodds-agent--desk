# native/src/commands/

Tauri command handlers — the public IPC surface that the TypeScript frontend calls via `invoke`.

Every function here is decorated with `#[tauri::command]` and registered in `lib.rs`.

## Command inventory

| Command | TypeScript caller | Purpose |
|---------|------------------|---------|
| `txline_fixtures_snapshot` | `transport.txlineFixturesSnapshotNative` | Returns a snapshot of today's TxLINE fixture list |
| `start_agent_run` | `transport.startAgentRunNative` | Opens a new multi-agent market round for a given fixture + track |
| `cancel_agent_run` | `transport.cancelAgentRunNative` | Cancels the current run and frees resources |
| `agent_leaderboard` | `transport.agentLeaderboardNative` | Returns the current agent leaderboard and arena score |
| `settle_run` | `transport.settleRunNative` | Initiates the settlement flow for a completed run |
| `wallet_connect` | `transport.walletConnectNative` | Triggers wallet-connection logic in the backend |

## Rules

- Commands **return** serialisable types from `txodds-types` or `src/types.rs` — never raw Rust types.
- Commands **emit events** via `src/event_bus.rs` for progress/streaming updates; they do not stream via their return value.
- Business logic lives in `src/services/`; commands are thin dispatch layers only.
- Errors are returned as `Result<T, AppError>` where `AppError` serialises to a typed JSON error the frontend can match.

## Adding a command

1. Write the handler function in the appropriate file in this folder.
2. Add `#[tauri::command]` and register it in the `.invoke_handler(…)` builder in `src/lib.rs`.
3. Add a matching typed wrapper in `ui/desktop/transport.ts`.
4. Update this README.
