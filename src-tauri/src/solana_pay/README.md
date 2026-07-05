# src-tauri/src/solana_pay

Rust-owned Solana Pay Transfer Request support.

## Files

- `mod.rs`: payment intent type, devnet guardrails, URL generation, reference creation, Triton reference verification, and receipt conversion.

## Rules

- React renders QR/URL payloads but does not create payment authority.
- Default cluster is devnet only.
- `SOLANA_PAY_RECIPIENT` is required before a real intent is created.
- Each run receives a unique 32-byte base58 reference.
- Payment status changes are persisted in SQLite and emitted through `pay://intent` / `pay://status`.
