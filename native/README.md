# native

Rust/Tauri desktop backend. This is the `native` workspace member (crate name `txodds-agent-desk`).

## Contents

- `src/`: Rust backend modules and Tauri command/event wiring — see [src/README.md](src/README.md).
- `capabilities/`: Tauri permission surface for webview access.
- `branding/`, `icons/`: app icons and bundle branding assets.
- `gen/`: Tauri-generated schemas (capabilities/ACL manifests) — regenerated, not hand-edited.
- `tauri.conf.json`: desktop window, bundle, resource, and CSP configuration.
- `build.rs`: Tauri build script.
- `Cargo.toml`: Rust dependency and package definition; depends on the `crates/` workspace members (`txodds-types`, `agent-core`, `rig-venice`).

## Rules

- Rust owns secrets, native APIs, network integrations, persistence, settlement, and sidecar supervision.
- React receives typed results/events through IPC; it does not receive credentials or signing authority.
- Generated directories such as `target/` and `gen/schemas/` should not be hand-edited.
