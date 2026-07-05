# Compartments

This repo follows the `solana_coralOS` split, adapted for a Tauri desktop app.

## Agent Identity

`coral-agents/` is the visible CoralOS agent layer:

- `worldcup-buyer-agent`: creates WANTs from TxLINE triggers.
- `seller-worldcup-edge`: bids on `txline.edge` work.
- `seller-risk-policy`: bids on `risk.policy` work.
- `seller-fan-card`: bids on `fan.card` work.
- `verifier-agent`: checks delivery/proof/policy before release.
- `settlement-arbiter-agent`: bridges verified runs to settlement.

Each agent has a `coral-agent.toml` manifest. The desktop app exposes the same list through the `list_coral_agents` Tauri command.

## Native Runtime

`src-tauri/src/coral/` owns Coral market behavior:

- `agents.rs`: built-in manifest registry exposed to the webview.
- `market.rs`: WANT -> BID -> AWARD -> DELIVERED -> VERIFIED state machine.
- `settlement.rs`: NDJSON bridge to CoralOS settlement sidecar.

Other native compartments:

- `src-tauri/src/triton/`: JSON-RPC and Yellowstone gRPC observation.
- `src-tauri/src/txline/`: live/mock/replay TxLINE ingestion.
- `src-tauri/src/ledger/`: SQLite run persistence.

## Frontend Domains

The React app keeps browser-dev fallbacks out of generic `lib` folders:

- `src/domain/coral/`: local Coral round simulation, bidding, scoring, agent registry.
- `src/domain/triton/`: browser-dev Triton proxy fallback.
- `src/domain/txline/`: browser-dev TxLINE fallback and mock fixtures.
- `src/desktop/transport.ts`: the only Tauri IPC boundary.

Production desktop mode routes privileged work through Rust. Browser-dev mode stays useful for UI iteration.
