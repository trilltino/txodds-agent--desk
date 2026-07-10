# settlement-agent

CoralOS idle participant for the TxODDS on-chain settlement track.

This agent receives the `DELEGATE` handoff from `match-intelligence-agent`
when `TrackMode::Settlement` is active and the proof gate has passed.
The Rust desktop runtime publishes messages as this participant via the puppet API:

- `TOOL_RESULT` — settlement acknowledgement after match-intelligence-agent delegates
- `SETTLED` — final settlement confirmation (emitted by Rust chain service)

**Policy:** `allow_settlement_release = false`. No funds are moved by this
participant. Transaction signing and release remain exclusively in the Rust
chain service (`native/src/services/chain`), gated by user wallet approval.
