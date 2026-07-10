# src-tauri/src/services/chain

Triton/Solana chain integration lives here.

## Files

- `rpc.rs`: allowlisted JSON-RPC client over `reqwest`.
- `yellowstone.rs`: Rust-managed Yellowstone sidecar supervision and subscription updates.
- `mod.rs`: module exports.

## Rules

- The webview must not call Triton directly in production.
- Keep RPC methods allowlisted.
- Emit chain observations as typed events so the UI can render live state without owning credentials.
