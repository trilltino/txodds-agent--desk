# tests/core/agent/

Unit tests for `ui/core/agent/` — arena position tracking, leaderboard, and safety types.

## Files

| File | Source module | What is covered |
|------|--------------|-----------------|
| `leaderboard.test.ts` | `ui/core/agent/types.ts` | `AgentLeaderboardEntry` shape invariants, `ArenaScore` leader derivation, `ArenaPosition` outcome maths |

## Philosophy

The frontend receives pre-computed leaderboard data from the Rust side over Tauri IPC.
These tests therefore assert **shape invariants** rather than computation logic:

- `winRate ∈ [0, 1]`
- `winRate ≈ positionsWon / positionsTaken`
- `strategy ∈ { 'FollowSharp', 'FadeSharp' }`
- PnL maths: win → `oddsAtEntry - 1`, loss → `-1`

This protects the UI from rendering nonsensical values if the Rust side ever sends unexpected data while the type system is in flux.
