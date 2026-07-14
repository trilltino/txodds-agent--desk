# crates/rig-venice/

Thin factory layer that points `rig-core`'s OpenAI-compatible provider at the Venice AI inference endpoint, plus the shared LLM tool implementations used by every agent in the workspace.

## What it is

It is **not** a full provider implementation. `rig-core` already knows how to speak to any OpenAI-compatible API. This crate does two things:

1. **`client()` / `model_name()` / `prose_model_name()`** — reads the active provider's API key (and optional base URL / model overrides) from the environment and hands back a ready-to-use `rig::providers::openai::Client`. Every agent binary calls `client()` exactly once at startup. API keys are never stored in structs that could leak into a prompt or log.

   Two providers are supported, both through the same OpenAI-compatible client — Groq is "mostly compatible with OpenAI's client libraries" by their own design, so this needed no new abstraction:

   - **Venice** (default) — the original provider.
   - **Groq** — a genuinely free fallback (`GROQ_API_KEY`, no paid tier required) for when Venice credits run out.

   `active_provider()` picks Venice or Groq: `LLM_PROVIDER=venice|groq` wins if set; otherwise it auto-detects from whichever `*_API_KEY` is present, preferring Venice. Auto-detection sees *presence* of a key, not its remaining quota — if your Venice key still exists but is out of credits, set `LLM_PROVIDER=groq` explicitly (or unset `VENICE_API_KEY`) rather than relying on auto-detection.

2. **`tools` module** — `rig::tool::Tool` implementations that agents share:

| Tool | Kind | What it does |
|------|------|-------------|
| `FetchOddsSnapshot` | HTTP | Fetches current decimal odds for a fixture from TxLINE (`/fixtures/{id}/odds`). Enforces a 32 KiB response-size safety limit before returning to the LLM. |
| `ComputeSharpMovement` | Pure | Given current and previous decimal odds, computes `pct_change`, `is_sharp_move`, `direction`, and a `confidence` heuristic. No I/O — fully deterministic. |
| `FetchActiveFixtures` | HTTP | Lists World Cup fixtures from TxLINE, optionally filtered to `status=live`. |
| `ComputeModelProbability` | Pure | Fundamentals softmax model (ported from `coral-agents/match-intelligence-agent/agent.py`): per-side form/xG/rank/injuries + h2h → fair `{home, draw, away}` distribution summing to 1.0. |
| `ComputeFairProbability` | Pure | Strips the bookmaker overround from up to three decimal odds so the implied probabilities sum to 1.0. Reuses `txodds_types::implied_probability`. |

There is no kill-switch tool — this system has none; see
`crates/rig-venice/ROADMAP.md`, "Removing the kill switch".

## Environment variables

| Variable | Required | Default |
|----------|----------|---------|
| `LLM_PROVIDER` | no | auto-detected (`venice` if `VENICE_API_KEY` set, else `groq`, else `venice`) |
| `VENICE_API_KEY` | required if provider is Venice | — |
| `VENICE_MODEL` | no | `kimi-k2-7-code` |
| `VENICE_PROSE_MODEL` | no | `llama-3.3-70b` |
| `VENICE_BASE_URL` | no | `https://api.venice.ai/api/v1` |
| `GROQ_API_KEY` | required if provider is Groq | — |
| `GROQ_MODEL` | no | `llama-3.1-8b-instant` (free tier: 14,400 req/day) |
| `GROQ_PROSE_MODEL` | no | `llama-3.3-70b-versatile` (free tier: 1,000 req/day) |
| `GROQ_BASE_URL` | no | `https://api.groq.com/openai/v1` |

Get a free Groq key at https://console.groq.com/keys — no card required.

## Usage (from an agent crate)

```rust
use rig_venice::{client, model_name, tools::{FetchOddsSnapshot, ComputeSharpMovement}};

let rig_client = client()?;
let agent = rig_client
    .agent(model_name())
    .tool(FetchOddsSnapshot::new(api_base, api_key))
    .tool(ComputeSharpMovement::default())
    .build();
```

## Design rules

- `#![forbid(unsafe_code)]`
- `#![deny(clippy::unwrap_used, clippy::expect_used)]` — all fallible paths return `Result`.
- Tool input schemas are derived via `schemars::JsonSchema` — no hand-written JSON blobs.
- HTTP tools validate response size before handing data to the LLM (checklist §20 / §28).

## Running tests

```bash
cargo test -p rig-venice
```
