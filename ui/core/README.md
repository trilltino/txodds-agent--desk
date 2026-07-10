# src/core

Pure, network-free domain logic and shared contracts. Nothing here may import
React or talk to Tauri/TxLINE/Solana directly - I/O lives behind
`src/desktop/transport.ts` and feature hooks.

- `txline/`: normalized event helpers and live fixture/odds parsing.
- `rooms/`, `markets/`, `proof/`, `agent/`: lean-track contracts mirrored from
  `src-tauri/src/domain/*` (see docs/architecture/01-lean-e2e-architecture.md section 6).
- `chain/`: chain status and observation types.
- `coral/`: deterministic scoring helpers mirrored by the Rust round engine.
