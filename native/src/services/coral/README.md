# native/src/services/coral

Coral compatibility and settlement bridge.

The active intelligence path lives in `services::agent::runtime`; this module keeps the older Coral-facing helpers (agent registry, settlement) available for the CoralOS integration without duplicating logic in the agent modules.

## Files

- `agents.rs`: built-in Coral agent registry exposed through Tauri IPC, backed by the `coral-agents/*/coral-agent.toml` manifests.
- `settlement.rs`: CoralOS settlement sidecar bridge.
- `mod.rs`: module exports.

## Rules

- Keep decisions deterministic.
- Settlement must remain policy-gated and backend-only.
- New product logic grows in `services::agent`, not here.
