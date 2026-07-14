# native/src/services/

Async service layer — orchestrates agents, manages I/O, and owns long-running background tasks.

## Services

| Module | Purpose |
|--------|---------|
| `txline` | TxLINE HTTP client — fetches fixture snapshots and subscribes to the live event stream |
| `agent_runner` | Multi-agent round lifecycle: opens rounds, collects bids, selects winner, triggers delivery |
| `verifier` | Routes agent deliveries to the proof-verification pipeline |
| `settlement` | Executes the settlement flow — constructs Solana Pay intents or CoralOS escrow transactions |
| `sidecar` | Manages the lifecycle of CoralOS sidecar processes (start, restart, health-check) |

## Design rules

- Services are `async` and use `tokio`. They hold a reference to `AppState` and emit events via `EventBus`.
- Business logic is split out into `src/domain/` so it can be unit-tested without a runtime.
- Services call `txodds_types` types for all public interfaces; internal implementation types stay in each service module.
- Each service is initialised once in `src/lib.rs` during app setup and stored in `AppState`.

## Testing

Services that make real network calls are integration-tested with `#[cfg(feature = "integration-tests")]`. Unit-testable slices are extracted to `src/domain/`.

```bash
# Unit tests only (no network)
cargo test -p txodds-native -- services::

# With integration tests (requires TXLINE_API_KEY and VENICE_API_KEY or GROQ_API_KEY)
cargo test -p txodds-native --features integration-tests -- services::
```
