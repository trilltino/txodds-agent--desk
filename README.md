# World Cup Agent Desk - Tauri desktop app for TxODDS / TxLINE / CoralOS

A desktop command center that combines all three Superteam World Cup tracks in one coherent product:

```text
Live TxLINE event -> Coral market round -> verifier/proof -> Solana Pay intent
  -> Triton-observed chain state -> fan/trader/market UI
```

This repo is a standalone Tauri app that can call the existing `trilltino/solana_coralOS` settlement rails while keeping the desktop UI, secrets, chain observation, run ledger, and native app packaging in this project.

## Run

Install `just` once:

```powershell
winget install --id Casey.Just -e
```

Then use the project terminal recipes:

```powershell
just setup
just desktop
```

With `TXLINE_GUEST_JWT` and `TXLINE_API_TOKEN`, the native app starts live TxLINE odds and scores SSE streams from Rust. Without them, mock data is a visible fallback. Add `SOLANA_PAY_RECIPIENT` to generate devnet Solana Pay QR/payment intents after verifier pass.

Common recipes:

| Command | Purpose |
|---|---|
| `just desktop` | Start the native Tauri desktop app. |
| `just web` | Start browser-only Vite dev mode. |
| `just check` | Run TypeScript, Rust, and sidecar syntax checks. |
| `just build` | Build webview assets and prepare sidecars. |
| `just tauri-build` | Build the packaged desktop app/installer. |

## Desktop Architecture

This is an end-to-end Tauri desktop app, not a browser-only web app:

- Tauri owns the native window, app identity, installer bundle, icons, capabilities, notifications, and app-data storage.
- Rust owns secrets, TxLINE ingestion, Triton RPC, Yellowstone observation, run persistence, market-round execution, settlement bridging, and native export.
- React/HTML/CSS/JavaScript remains the frontend renderer inside the Tauri webview.
- In packaged/native mode, production network calls go through Rust commands and events. The webview does not receive Triton, TxLINE, CoralOS, or keypair secrets.
- Plain `just web` remains useful for browser UI iteration, but `just desktop` and `just tauri-build` are the desktop product paths.

## Compartments

The repo is organized around CoralOS-style boundaries:

| Compartment | Owns |
|---|---|
| `coral-agents/` | Coral agent manifests: buyer, sellers, verifier, settlement arbiter. |
| `src-tauri/src/coral/` | Native Coral market, agent registry, and settlement sidecar bridge. |
| `src-tauri/src/triton/` | Triton JSON-RPC and Yellowstone gRPC observation. |
| `src-tauri/src/solana_pay/` | Devnet Solana Pay transfer requests, references, and payment intent verification. |
| `src-tauri/src/txline/` | Native TxLINE live/mock/replay ingestion. |
| `src-tauri/src/ledger/` | SQLite run ledger. |
| `src/domain/coral/` | Browser-dev Coral market fallback, bidding, scoring, and agent registry. |
| `src/domain/triton/` | Browser-dev Triton fallback client. |
| `src/domain/txline/` | Browser-dev TxLINE fallback client and mock fixtures. |
| `src/desktop/transport.ts` | The Tauri IPC boundary. |
| `runtime/sidecars/` | Node sidecars for CoralOS settlement and Yellowstone gRPC. |

## Coral Agents

The app now exposes explicit Coral agent identities instead of hiding them inside generic UI labels:

| Agent | Role | Service |
|---|---|---|
| `worldcup-buyer-agent` | buyer | Converts TxLINE triggers into WANTs and awards sellers. |
| `seller-worldcup-edge` | seller | Sells fixture-bound TxLINE fair-line reads. |
| `seller-risk-policy` | seller | Sells risk policy and no-action/observe/simulate guidance. |
| `seller-fan-card` | seller | Sells shareable fan-card output. |
| `verifier-agent` | verifier | Checks hash, fixture binding, proof shape, and policy gates. |
| `settlement-arbiter-agent` | settlement | Bridges verified runs to CoralOS settlement and Triton observation. |

Each lives under `coral-agents/<agent>/coral-agent.toml`. The desktop UI loads the same registry through `list_coral_agents`.

## Live Yellowstone + CoralOS Wiring

Yellowstone gRPC runs as a Rust-managed backend sidecar using Triton's official `@triton-one/yellowstone-grpc` SDK pinned to the Windows-compatible `4.x` line.

```bash
TRITON_GRPC_ENDPOINT=https://your-endpoint.rpcpool.com:443
TRITON_X_TOKEN=...
WATCH_ESCROW_PROGRAM_ID=...
WATCH_MARKET_PROGRAM_ID=...
```

CoralOS settlement runs through `runtime/sidecars/coralos-bridge.mjs`. It prefers `CORALOS_BRIDGE_URL` if you provide a custom bridge matching `docs/integrations/coralos-settlement.md`; otherwise it calls the existing TxODDS proxy `/api/settle` from `trilltino/solana_coralOS`.

```bash
CORALOS_ROOT=C:\path\to\solana_coralOS
CORALOS_TXODDS_PROXY=http://localhost:8801
CORALOS_AUTOSTART_PROXY=1
CORALOS_SETTLEMENT_ENABLED=1
```

The sidecars run outside the webview. Tokens, keypairs, proxy credentials, and settlement operations stay in Rust/Node backend processes.

## E2E Flow

- `src-tauri/src/triton/yellowstone.rs` starts `runtime/sidecars/yellowstone-bridge.mjs` when `TRITON_GRPC_ENDPOINT` and `TRITON_X_TOKEN` are configured.
- `watch_account`, `watch_program`, and `watch_reference` commands update live Yellowstone subscription filters from the desktop app.
- `src-tauri/src/coral/market.rs` runs WANT -> BID -> AWARD -> DELIVERED -> VERIFIED.
- `src-tauri/src/coral/settlement.rs` sends completed runs to `runtime/sidecars/coralos-bridge.mjs` over newline-delimited JSON.
- `run_agent_round` creates a Solana Pay devnet Transfer Request after verifier pass, emits `pay://intent`, registers Yellowstone/Triton watches for the payment reference, optionally enriches through CoralOS, and persists the run/payment intent to SQLite.
- The Windows installer bundles sidecar scripts, their Node module runtime dependencies, and `runtime/sidecars/bin/node.exe`; `NODE_BIN` can still override the bundled runtime.

## CoralOS Reference Map

| `solana_coralOS` piece | Used here as |
|---|---|
| `coral-agents/*/coral-agent.toml` | Agent identity and persona pattern mirrored in this repo. |
| `packages/agent-runtime/src/coral` | `src-tauri/src/coral/agents.rs` and sidecar bridge boundaries. |
| `packages/agent-runtime/src/market` | `src-tauri/src/coral/market.rs` and `src/domain/coral/*`. |
| `packages/agent-runtime/src/ledger` | `src-tauri/src/ledger/store.rs`. |
| `packages/agent-runtime/src/policy` | Policy notes in verifier/settlement agents; settlement release remains gated. |
| `examples/txodds/agent/txline.ts` | `src-tauri/src/txline/ingest.rs` plus browser fallback in `src/domain/txline`. |
| `examples/txodds/escrow` | CoralOS settlement sidecar/proxy integration. |

## App Screens

- `LiveFeed`: real-time TxLINE event stream.
- `AgentArena`: Coral buyer/seller/verifier/settlement roster plus live bids.
- `SettlementLab`: market/proof/escrow state.
- `FanMode`: mainstream fan-facing product.
- `ProofPanel`: judge-facing chain of evidence.
- `TrackScorecard`: why the same product fits all three tracks.
