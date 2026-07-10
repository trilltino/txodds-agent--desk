# TxOdds Agent Desk

A Tauri desktop app that turns TxLINE live World Cup scores, odds, match events, and Solana-anchored validation data into autonomous agent decisions:

```text
TxLINE event -> normalized event bus -> Match Intelligence Agent runtime
                                      -> Coral/CoralOS settlement bridge
                                      -> Verified/proof-gated delivery
```

Rust owns secrets, TxLINE ingestion, Triton RPC, Yellowstone observation, persistence, Solana Pay, LLM calls, and settlement/proof side effects. React renders the product surfaces and calls thin Tauri commands through `ui/desktop/transport.ts`. Standalone Rust agent binaries (`crates/agents/*`) can also run outside the desktop app for local testing.

## Run

Install `just` once:

```powershell
winget install --id Casey.Just -e
```

Then use the project recipes:

```powershell
just setup
just desktop
```

Common recipes (see [Justfile](Justfile) for the full list):

| Command | Purpose |
| --- | --- |
| `just desktop` | Start the native Tauri desktop app (also brings up the CoralOS Console if Docker is available). |
| `just txline-onboard` | Mint free-tier TxLINE credentials into `.env`. |
| `just check` | Run TypeScript, Rust, sidecar, and bundle-dep checks. |
| `just build` | Build webview assets and prepare sidecars. |
| `just tauri-build` | Build the packaged desktop app/installer. |
| `just run-agent-<name>` | Run one of the `crates/agents/*` binaries standalone (e.g. `just run-agent-sharp-movement-detector`). |

With `TXLINE_GUEST_JWT` and `TXLINE_API_TOKEN` set (see `.env.example`), the desktop app starts live TxLINE odds and scores SSE streams from Rust. Missing credentials surface as a visible `credentials_required` ingest status. Direct browser preview and browser-owned data paths are intentionally blocked; TxLINE, Triton, Yellowstone, and txoracle validation stay Rust/sidecar-owned.

## Repository Layout

```text
ui/                         # React/TypeScript frontend (Vite + Tauri webview)
  app/                       # webview orchestrator, global chrome, navigation
  apps/
    agent/                   # Intelligence Agent Desk — the primary product screen
    shared/                  # components/hooks reused across pages
  core/                      # pure TS domain types/clients (agent, chain, coral, markets, proof, rooms, txline, wallet)
  desktop/                   # Tauri IPC/event boundary

native/                     # Rust/Tauri desktop backend
  src/
    commands/                # thin Tauri IPC adapters
    domain/                  # deterministic Rust contracts (agent, proof, arena)
    services/
      agent/                 # Match Intelligence Agent runtime (LLM, policy, safety)
      chain/                 # Triton RPC and Yellowstone sidecar supervision
      coral/                 # Coral agent registry + CoralOS settlement bridge
      coralos/                # CoralOS protocol/console/transcript helpers
      ledger/                 # SQLite persistence
      llm/                     # Venice LLM client
      proof/                   # txoracle proof-gate logic
      txline/                  # TxLINE API, live/replay/mock ingest

crates/                     # Rust workspace members shared by native + standalone agents
  txodds-types/               # shared domain types
  agent-core/                 # safety/policy/capability primitives
  rig-venice/                  # Venice (OpenAI-compatible) client + shared LLM tools
  agents/                       # standalone agent binaries (match-intelligence, contrarian,
                                 # arena-coordinator, sharp-movement-detector)

coral-agents/                # Python CoralOS Docker participant stubs (thin; real logic is in native/crates)
runtime/sidecars/             # Node sidecar bridges (CoralOS, Yellowstone, txoracle validation)
tooling/                       # dev/build helper scripts
tests/                          # Vitest unit + e2e tests for the TS layer
vendor/tx-on-chain/              # vendored Anchor IDLs/schemas for the txoracle program
```

Module responsibilities are documented in `//!` doc comments on the Rust side and in local `README.md` files throughout. Commands stay glue-only; I/O belongs in services; deterministic business logic belongs in domain/agent modules.

## TxLINE And Chain Wiring

The desktop backend follows the current TxLINE OpenAPI source at `https://txline.txodds.com/docs/docs.yaml`.

- Auth/data credentials stay in Rust: `Authorization: Bearer <guest JWT>` and `X-Api-Token`.
- Live SSE uses `GET /api/odds/stream` and `GET /api/scores/stream` from `native/src/services/txline/ingest.rs`.
- Snapshot/proof commands cover fixtures, odds, scores, historical intervals, score history, and `/api/scores/stat-validation`.
- Yellowstone watches the configured txoracle program so proof-root transactions can surface through `chain://tx`.

Triton and Yellowstone secrets remain in Rust-managed backend processes:

```bash
TRITON_GRPC_ENDPOINT=https://your-endpoint.rpcpool.com:443
TRITON_X_TOKEN=...
WATCH_ESCROW_PROGRAM_ID=...
WATCH_MARKET_PROGRAM_ID=...
```

## Coral / CoralOS

The active intelligence path is the Match Intelligence Agent runtime (`native/src/services/agent/runtime/`), driven directly by live TxLINE events — there is no separate buyer/seller/verifier auction simulator in the product path. `native/src/services/coral/` keeps the Coral agent registry (backed by `coral-agents/*/coral-agent.toml` manifests) and the CoralOS settlement bridge available for the CoralOS integration. CoralOS settlement can be reached through `runtime/sidecars/coralos-bridge.mjs` when configured:

```bash
CORALOS_ROOT=C:\path\to\solana_coralOS
CORALOS_TXODDS_PROXY=http://localhost:8801
CORALOS_AUTOSTART_PROXY=1
CORALOS_SETTLEMENT_ENABLED=1
```

## Production readiness

See [PRODUCTION_READINESS_AUDIT.md](PRODUCTION_READINESS_AUDIT.md) for the current, dated results of the project's production-readiness checklist.
