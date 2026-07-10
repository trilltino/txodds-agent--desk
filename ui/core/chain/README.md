# ui/core/chain/

On-chain integration helpers for Solana — cluster config, RPC utilities, and explorer links.

## Responsibilities

- Cluster selection (`devnet` / `mainnet-beta`) matching the `WalletContext.cluster` field.
- Explorer URL construction for transactions, programs, and PDAs.
- Thin wrappers used by settlement and proof screens to deep-link to on-chain state.

## Design notes

- No wallet-signing logic lives here — signing is delegated to the wallet adapter layer in `ui/core/wallet/`.
- RPC calls that require a Tauri backend (e.g. slot polling) go through `ui/desktop/transport`.
- Pure URL/string helpers are tested directly; IPC-bound helpers are covered by integration tests.
