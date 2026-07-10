# ui/apps/agent/

The Intelligence Agent Desk page — the primary product screen of TxOdds Agent Desk.

## Responsibilities

- Renders the live fixture list sourced from TxLINE.
- Displays the active agent round: bids received, winner selected, delivery, verification verdict, and settlement status.
- Shows the agent leaderboard and arena position history.

## Data sources

| Data | Origin |
|------|--------|
| Fixture list | `useAgentDesk` → `desktop/transport.txlineFixturesSnapshotNative` |
| TxLINE events | `useAgentDesk` → `desktop/transport.listenTxLineEvent` |
| Agent run state | `useAgentDesk` → Tauri commands via `desktop/transport` |
| Leaderboard | `useAgentDesk` → `desktop/transport.agentLeaderboardNative` |

## Key interactions

1. User selects a fixture → app triggers a market round.
2. Agent bids arrive over IPC → `chooseWinner` selects the best agent.
3. Delivery is received → verifier runs, verdict is displayed.
4. User approves settlement → Solana Pay or escrow flow initiates.
