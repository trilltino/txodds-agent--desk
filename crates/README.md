# crates/

Rust workspace members that provide the agent logic, shared domain types, and LLM integration.

## Members

| Crate | Path | Purpose |
|-------|------|---------|
| `txodds-types` | `crates/txodds-types/` | Shared domain types used by all crates and mirrored in `ui/types.ts` |
| `agent-core` | `crates/agent-core/` | Trait definitions and shared infrastructure for all agent implementations |
| `rig-venice` | `crates/rig-venice/` | [Rig](https://github.com/0xPlaygrounds/rig) provider adapter for the Venice.ai inference API |
| `agents/*` | `crates/agents/` | Individual agent implementations (match-intelligence, contrarian, sharp-movement-detector, arena-coordinator) |

## Building

```bash
# Build all crates
cargo build

# Run tests across all crates
cargo test

# Check for lint issues
cargo clippy -- -D warnings
```

## Dependency policy (`deny.toml`)

`cargo-deny` enforces license and duplicate-crate policies. Run `cargo deny check` before merging.
