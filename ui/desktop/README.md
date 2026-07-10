# src/desktop

The desktop transport layer is the frontend boundary to Tauri IPC.

## Files

- `transport.ts`: typed wrapper around Tauri `invoke`/`listen`.

## Rules

- This should be the only frontend code that imports `@tauri-apps/api`.
- Native mode should route privileged operations to Rust.
- Browser rendering is blocked; live TxLINE data is desktop-only.
