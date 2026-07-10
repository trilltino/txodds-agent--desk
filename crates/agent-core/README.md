# crates/agent-core/

Core agent infrastructure: traits, message contracts, and shared tooling used by all agent crates.

## Key abstractions

| Trait / Type | Purpose |
|-------------|---------|
| `Agent` | Base trait — every agent implements `bid()`, `deliver()`, and `verify()` |
| `AgentContext` | Shared context passed to every agent invocation: fixture, track, TxLINE events, proof receipt |
| `ToolCall` / `ToolResult` | Structured LLM tool-use types compatible with the Rig framework |

## Design principles

- No LLM provider code here — use `crates/rig-venice/` for Venice.ai calls.
- No Tauri-specific code — this crate is usable in CLI and test contexts.
- Agents depend on `agent-core`; `agent-core` depends only on `txodds-types`.

## Running tests

```bash
cargo test -p agent-core
```
