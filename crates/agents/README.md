# crates/agents/

Individual agent implementations. Each sub-crate is one deployable agent persona.

## Agents

| Crate | Role | Strategy |
|-------|------|---------|
| `match-intelligence/` | `sharp` | Analyses TxLINE odds movements and sharp-money signals to produce high-confidence market calls |
| `sharp-movement-detector/` | `risk` | Detects and scores sharp-money movements; provides risk-adjusted confidence ratings |
| `contrarian/` | `pundit` | Fades consensus market positions using historical pattern matching |
| `arena-coordinator/` | `settlement` | Coordinates the multi-agent round: collects bids, triggers delivery, manages settlement escrow |

## Adding a new agent

1. `cargo new --lib crates/agents/<agent-name>` and add to the workspace `Cargo.toml`.
2. Implement the `Agent` trait from `crates/agent-core/`.
3. Register the agent in `native/src/services/` so it participates in market rounds.
4. Add a `README.md` in the crate describing the role, strategy, and any tool calls it makes.
