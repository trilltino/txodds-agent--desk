# native/src

Rust backend modules implement the desktop app core.

## Files and Directories

- `main.rs`: Tauri binary entrypoint.
- `lib.rs`: app builder, managed state, Tauri commands, and event wiring (composition root only).
- `config.rs`: environment/config loading.
- `error.rs`: IPC-safe error types.
- `types.rs`: shared backend data structures serialized to the webview.
- `state.rs`: `DesktopState` — managed Tauri state (ledger, HTTP client, config, etc).
- `event_bus.rs`: typed event-name constants emitted to the webview.
- `web.rs`: optional loopback diagnostics/API service.
- `commands/`: thin Tauri IPC adapters — see [commands/README.md](commands/README.md).
- `domain/`: deterministic Rust contracts (agent, proof, arena) — see [domain/README.md](domain/README.md).
- `services/`: I/O and business logic, one subfolder per concern — see [services/README.md](services/README.md):
  - `agent/`: Match Intelligence Agent runtime (LLM calls, policy, safety gates).
  - `chain/`: Triton JSON-RPC and Yellowstone observation.
  - `coral/`: Coral agent registry and CoralOS settlement bridge.
  - `coralos/`: CoralOS protocol/console/transcript helpers.
  - `ledger/`: SQLite persistence.
  - `llm/`: Venice LLM client.
  - `proof/`: txoracle proof-gate logic.
  - `txline/`: live TxLINE ingestion and documented data/proof API helpers.

## Rules

- Treat this as the production backend.
- Keep blocking work off the main Tauri thread.
- Emit typed events instead of making the webview poll privileged services.
- Commands stay glue-only; I/O belongs in `services/`; deterministic business logic belongs in `domain/`.
