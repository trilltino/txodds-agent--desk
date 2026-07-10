# fan-pundit-agent

CoralOS idle participant for the TxODDS fan-track narrator.

This agent receives the `DELEGATE` handoff from `match-intelligence-agent`
when `TrackMode::Fan` is active. The Rust runtime publishes the Venice LLM
explanation as this participant's message via the puppet API:

- `TOOL_RESULT` — fan narrative acknowledgement containing the LLM-generated
  two-sentence explanation of the match intelligence decision

**Policy:** `allow_money_decisions = false`. This agent produces human-readable
commentary only. No financial data, proof verdicts, or settlement actions are
accessible to or publishable by this participant.
