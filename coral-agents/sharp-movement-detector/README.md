# sharp-movement-detector

CoralOS idle participant for the TxODDS sharp odds movement detector.

This agent receives the `DELEGATE` handoff from `match-intelligence-agent`
when `TrackMode::Trading` is active and a `SharpOddsMove` signal fires.
The Rust sidecar (`crates/agents/sharp-movement-detector`) and desktop
runtime publish messages as this participant via the puppet API:

- `TOOL_RESULT` — sharp signal acknowledgement after handoff
- Further signal qualification messages on subsequent TxLINE odds updates

**Policy:** `allow_money_decisions = false`. Position simulation only —
no real bets or orders are placed. The simulate-only constraint is
enforced by the Rust policy layer, not this participant.
