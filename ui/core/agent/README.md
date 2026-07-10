# ui/core/agent/

Domain types and derivation helpers for agent arena tracking.

## Concepts

| Concept | Description |
|---------|-------------|
| `AgentLeaderboardEntry` | Aggregated agent performance: win rate, total PnL points, avg confidence |
| `ArenaPosition` | One position taken by the app in a market round: direction, odds, confidence, outcome |
| `ArenaScore` | Season-level FollowSharp vs FadeSharp PnL comparison |

## Source of truth

The leaderboard and arena positions are computed by Rust (`native/`) and received over Tauri IPC. The TypeScript layer stores and displays the data — it does not recompute it.

## Tests

See `tests/core/agent/leaderboard.test.ts` for shape invariant assertions.
