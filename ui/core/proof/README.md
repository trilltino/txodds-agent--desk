# ui/core/proof/

Proof-receipt validation helpers — verifying TxLINE Merkle proofs before settlement is allowed.

## Key concepts

| Concept | Description |
|---------|-------------|
| `TxLineProofReceipt` | Received from Rust; contains Merkle root, stat keys, rootPda, verification status |
| `ValidationSimulationStatus` | `'not_started' \| 'passed' \| 'failed' \| 'unavailable'` |
| Proof validation | Rust simulates the on-chain instruction; the result is reflected in `simulationStatus` |

## Frontend role

The frontend does **not** perform cryptographic verification. It:

1. Receives a `TxLineProofReceipt` from Rust over IPC.
2. Renders the proof fields (Merkle root, stat keys, rootPda, explorer link).
3. Gates settlement UI on `proofReceipt.verified === true`.

Any logic that decides whether a proof is valid lives in the Rust `native/` crate.
