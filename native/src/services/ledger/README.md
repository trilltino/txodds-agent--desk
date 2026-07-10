# src-tauri/src/services/ledger

The ledger records durable desktop run history in SQLite.

## Files

- `store.rs`: SQLite schema creation plus run insert/list/read behavior.
- `mod.rs`: module exports.

## Rules

- Persist enough context to explain a run after restart.
- Store receipts and observations as audit data, not just UI cache.
- Keep schema changes additive or migrated deliberately.
