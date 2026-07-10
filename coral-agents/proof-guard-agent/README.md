# proof-guard-agent

CoralOS idle participant for the TxODDS txoracle proof gate.

This agent occupies a named seat in every CoralOS session. The Rust
desktop runtime (`native/src/services/proof`) does the actual work and
publishes messages as this participant via the puppet API:

- `PROOF_REQUESTED` — match-intelligence-agent asks for proof validation
- `PROOF_RECEIVED` — receipt from the txoracle bridge
- `VALIDATION_SIMULATED` — deterministic pass/fail verdict
- `VERIFIED` — final proof clearance (Settlement track only)

**Policy:** `allow_proof_verdicts = true`, `allow_settlement_release = false`.
Proof verdicts are code-owned; no LLM can flip the gate.
