# Rust Agent Runtime Plan — LLM Discussion with Venice Kimi K2.7

Deep plan for replacing the deterministic template engine in
`src-tauri/src/coral/market.rs` with **real Rust agentic flows**: per-role agent
tasks that hold an LLM-mediated market discussion (WANT → BID → AWARD →
DELIVER → VERIFY → SETTLE) over a typed protocol, grounded in live TxLINE data,
with Venice AI (`kimi-k2-7-code`) as the reasoning engine.

The pattern is ported from [`trilltino/solana_coralOS`](https://github.com/trilltino/solana_coralOS),
which already runs this loop in TypeScript. This document maps that pattern to
Rust inside the Tauri backend, phase by phase.

---

## 1. Where we are vs. where the pattern is

### Current state (this repo)

| Layer | Today | Problem |
| --- | --- | --- |
| `coral-agents/*/coral-agent.toml` | 6 manifests (buyer, 3 sellers, verifier, arbiter) | **Documentation only** — nothing reads them at runtime |
| `src-tauri/src/coral/market.rs` | `run_round()` synchronously fabricates bids, winner, delivery, verdict | No agents exist: one function role-plays all six; every payload is a template string; confidence numbers are hardcoded |
| `src-tauri/src/coral/agents.rs` | `built_in_agents()` mirrors the TOML in Rust constants | Two sources of truth |
| LLM usage | None. `LLM_PROVIDER` / `VENICE_API_KEY` sit unused in `.env` | The "agents" cannot reason about the live TxLINE data we now stream |

### The source pattern (solana_coralOS)

Four pillars, each with a direct Rust translation:

1. **LLM pillar** — `packages/agent-runtime/src/llm/complete.ts`: one SDK-free
   `complete({system, user, model, maxTokens})` over `fetch`. Provider chosen by
   env (`LLM_PROVIDER=venice` → `https://api.venice.ai/api/v1/chat/completions`,
   OpenAI-compatible). Venice Kimi models (`kimi-k2-7-code`) get a floor of
   **1024 max_tokens** because they spend budget on internal reasoning before
   emitting content. `TRACE=1` logs provider/model/raw reply.
   Principle: **the model proposes, code disposes** — every LLM answer is parsed
   as JSON and clamped by code-enforced guards.

2. **Market protocol pillar** — `packages/agent-runtime/src/market/protocol.ts`:
   pure, network-free formatters/parsers for the wire messages
   (`WANT round=1 service=txline.edge budget=0.001`, `BID`, `AWARD`,
   `VERIFY`/`VERIFIED`, `LLM_USED`, settlement messages). Every message carries
   a `round` tag to correlate replies in a shared thread. Fully unit-tested.

3. **Transport pillar** — `CoralMcpAgent`: `waitForMention`, `sendMessage`,
   `createThread`. Agents are `while true { msg = waitForMention(); reply() }`
   loops. Coral moves opaque strings; the protocol layer gives them meaning.

4. **Guard pillar** — budgets and trust enforced **in code, not prompts**:
   bounded tool-use turns (`maxTurns`), spend caps in lamports the model cannot
   exceed, recipients/references only accepted from real challenges (never from
   model output), deterministic fallbacks everywhere so a missing API key or a
   malformed reply degrades to the current deterministic behavior instead of
   failing the round. Every LLM decision emits an `LLM_USED` audit record with
   `status=used|fallback|skipped|error`.

---

## 2. Target architecture

Agents run as **tokio tasks inside the Tauri backend** (phase 2), talking over
an in-process bus shaped like Coral threads, so the same role code can later be
lifted onto real CoralOS MCP transport (phase 4) without rewriting strategy
logic.

```
src-tauri/src/agents/
├── mod.rs          // module exports; spawn_market() entrypoint
├── llm.rs          // Venice/OpenAI/Anthropic complete() — port of complete.ts
├── protocol.rs     // typed MarketMessage enum + format/parse + round tags
├── bus.rs          // in-process "thread": tokio broadcast + mention filter
├── manifest.rs     // parse coral-agents/*/coral-agent.toml (source of truth)
├── guard.rs        // spend caps, turn bounds, JSON-clamp helpers
├── runtime.rs      // AgentTask trait + round orchestration + timeout
└── roles/
    ├── buyer.rs        // worldcup-buyer-agent: WANT, award reasoning
    ├── seller_edge.rs  // seller-worldcup-edge: fair-line read from live odds
    ├── seller_risk.rs  // seller-risk-policy: no-action/observe/simulate
    ├── seller_fan.rs   // seller-fan-card: plain-English fan card
    ├── verifier.rs     // verifier-agent: deterministic checks + LLM explanation
    └── arbiter.rs      // settlement-arbiter-agent: packages verified run
```

### Data flow for one round

```
TxLINE SSE event (live)                    Yellowstone chain://tx (txoracle)
        │                                             │
        ▼                                             ▼
 run_agent_round(trigger, track)          proof context attached to round
        │
        ▼
 buyer task ── WANT round=N service=… budget=… ──► bus
        │
        ├─ seller-edge  ─ LLM: fair-line read from trigger.odds ─ BID + draft
        ├─ seller-risk  ─ LLM: bounded action recommendation    ─ BID + draft
        └─ seller-fan   ─ LLM: fan explainer                    ─ BID + draft
        │
 buyer task ── LLM: award reasoning over bids (clamped by score_bid) ── AWARD
        │
 winner ── DELIVERED payload (LLM content, hash-bound by code) ──► bus
        │
 verifier ── deterministic checks (hash, fixture binding, policy) ──► VERIFIED
        │            └─ LLM used ONLY for the human-readable reason text
        ▼
 existing settlement path unchanged: Solana Pay intent → Triton observation
```

Every bus message is (a) appended to the run timeline (SQLite ledger, exactly
like today), and (b) emitted to the webview as a new `market://message` Tauri
event so **AgentArena renders the discussion live** — judges watch agents argue
instead of seeing a pre-baked timeline appear at once.

### What the LLM decides vs. what code decides

| Decision | Owner | Guard |
| --- | --- | --- |
| Bid **note** + proposed price | LLM (per-seller persona from TOML) | price clamped to `[FLOOR_SOL, BUYER_MAX_SOL]`; malformed JSON → deterministic bid (today's values) |
| Delivery **payload content** (fair-line read, risk memo, fan card) | LLM, grounded in the trigger's real `odds[]`/`score` + fixture snapshot | schema validated with serde; missing fields → template fallback; payload hashed by code |
| **Award** choice | LLM proposes with reasoning | code recomputes `score_bid()`; LLM may only pick among the top-2 scored bids, else fallback to argmax |
| **Verification verdict** | **Code only** (hash, fixture binding, policy, proof shape) | LLM writes the explanation string, never the pass/fail |
| **Settlement release** | **Code only** (existing `verifier_passed()` gate) | LLM never touches money paths; `MAX_DEVNET_SPEND_SOL` cap stays |
| Spend / turns | Code | per-round token budget, `maxTurns`, 30s round timeout → fallback |

This is the exact trust split from solana_coralOS's `llm_buyer.ts` ("the
tool-use loop is BOUNDED; the budget is enforced in CODE; the model can only
pay values from a REAL challenge").

---

## 3. The LLM pillar in Rust (`agents/llm.rs`)

Direct port of `complete.ts` onto the existing `reqwest::Client` (already in
`DesktopState`; no new dependencies — `serde_json` and `reqwest` are in the
tree).

```rust
pub enum LlmProvider { Venice, OpenAi, Anthropic }

pub struct CompleteOpts<'a> {
    pub system: &'a str,
    pub user: &'a str,
    pub model: Option<&'a str>,   // per-call override
    pub max_tokens: u32,
}

const VENICE_URL: &str = "https://api.venice.ai/api/v1/chat/completions";
const DEFAULT_VENICE_MODEL: &str = "kimi-k2-7-code";
const KIMI_MIN_COMPLETION_TOKENS: u32 = 1024;

/// Explicit LLM_PROVIDER wins; else key-presence detection (same order as
/// solana_coralOS pickProvider()).
pub fn pick_provider(config: &AppConfig) -> LlmProvider { /* env-driven */ }

/// Venice-hosted Kimi spends budget on internal reasoning before emitting
/// content: raise small requests to 1024, keep caller budgets otherwise.
fn effective_max_tokens(provider: &LlmProvider, model: &str, requested: u32) -> u32 {
    if matches!(provider, LlmProvider::Venice)
        && model.to_ascii_lowercase().contains("kimi")
        && requested < KIMI_MIN_COMPLETION_TOKENS
    { KIMI_MIN_COMPLETION_TOKENS } else { requested }
}

pub async fn complete(client: &Client, config: &AppConfig, opts: CompleteOpts<'_>)
    -> Result<String, AppError> { /* OpenAI-compatible POST, bearer key */ }
```

Config additions in `config.rs` (env names already exist in `.env.example`):
`llm_provider`, `venice_api_key` (via the `secret()` helper → keyring-capable),
`llm_model`, `trace`. `PublicConfig` gains one boolean `llm_configured` — the
key never crosses IPC, same rule as TxLINE/Triton tokens.

**JSON discipline:** every prompt ends with "Reply with ONLY a JSON object:
`{...schema...}`". The caller `serde_json::from_str`s into a typed struct; on
failure it strips code fences and retries the parse once; on second failure the
deterministic fallback fires and an `LLM_USED status=fallback reason=...`
message is posted. A round can therefore **never** be broken by model output.

---

## 4. Protocol pillar (`agents/protocol.rs`)

Typed enum instead of string parsing (we own both ends in-process), but with
`Display`/`FromStr` that produce exactly the solana_coralOS wire strings so the
UI can show them and phase 4 can put them on real Coral threads unchanged:

```rust
pub enum MarketMessage {
    Want     { round: u32, service: String, fixture_id: u64, budget_sol: f64 },
    Bid      { round: u32, price_sol: f64, by: String, confidence: f64, note: String },
    Award    { round: u32, to: String, reasoning: String },
    Delivered{ round: u32, sha256: String, by: String },
    LlmUsed  { round: u32, agent: String, purpose: String, status: LlmUseStatus,
               provider: Option<String>, model: Option<String>, reason: Option<String> },
    Verify   { round: u32, sha256: String, payload: String },
    Verified { round: u32, verdict: VerdictStatus, by: String, reason: String },
    Settle   { round: u32, reference: String, rail: String },
}
```

Unit tests mirror `protocol.ts`'s: round-trip format/parse for every variant,
plus malformed-input rejection. This module is pure — no tokio, no network —
so `cargo test` covers it completely.

---

## 5. Transport pillar (`agents/bus.rs`)

Phase 2 in-process stand-in for Coral threads:

```rust
pub struct MarketBus { tx: broadcast::Sender<Envelope> }
pub struct Envelope { pub thread: String, pub from: String,
                      pub mentions: Vec<String>, pub message: MarketMessage }

impl MarketBus {
    pub fn subscribe(&self, agent: &str) -> BusRx;   // filtered on mentions/broadcast
    pub fn send(&self, envelope: Envelope);
}
```

`BusRx::wait_for_mention(timeout)` gives the same blocking primitive as
`CoralMcpAgent.waitForMention(maxWaitMs)`. Agent role code is written against a
small `AgentTransport` trait implemented by `MarketBus` now and by a Coral MCP
client later — that trait boundary is what makes phase 4 a transport swap, not
a rewrite.

---

## 6. Role tasks (`agents/roles/*`)

Each role is a tokio task started by `spawn_market()` during Tauri setup,
parameterized by its parsed `coral-agent.toml` (which finally becomes the
source of truth; `agents.rs` built-ins become the fallback when a manifest is
missing). The shape mirrors `startCoralAgent()`:

```rust
pub async fn run_seller_edge(ctx: AgentCtx) {
    loop {
        let Some(want) = ctx.wait_for::<Want>().await else { continue };
        // 1. Ground: pull live odds/scores snapshot for want.fixture_id
        //    (reuse txline::api::authenticated_get — agents read the same
        //    live TxLINE data the UI sees; nothing invented).
        // 2. LLM: persona prompt (from TOML `PERSONA`) + trigger + snapshot →
        //    JSON {priceSol, confidence, note, draftPayload}.
        // 3. Guard: clamp price to FLOOR_SOL..=budget, confidence to 0..=0.95.
        // 4. ctx.send(Bid {...}) + LlmUsed audit.
        // 5. If awarded: finalize delivery payload, return it hash-bound.
    }
}
```

Seller prompt grounding is the key quality difference from today: the
`seller-worldcup-edge` prompt receives the **actual quotes** (`decimal`,
`impliedProbability` per outcome) from the trigger event plus the previous
snapshot, and is asked for a fair-line read with explicit numbers. Kimi K2.7
(code-tuned) is a good fit for strict-JSON numeric analysis.

`run_agent_round` keeps its exact Tauri command signature: it posts a `Want`
to the bus, awaits round completion (or 30s timeout → today's deterministic
`market::run_round` as the fallback path), and returns the same `AgentRun`.
**Nothing downstream changes** — ledger, Solana Pay, Triton observation,
export, UI contracts all stay as-is.

---

## 7. Phases, deliverables, verification

### Phase 0 — groundwork (½ day)
- `agents/manifest.rs`: parse the 6 TOML files (add `toml = "0.8"` to
  `src-tauri/Cargo.toml` — the only new dependency in the whole plan).
- `config.rs`: `llm_provider`, `venice_api_key`, `llm_model`, `trace`,
  `llm_configured` in `PublicConfig`.
- **Verify:** unit test parses all 6 manifests; `get_config` shows
  `llmConfigured: true` with a key set.

### Phase 1 — LLM + protocol pillars, LLM-enhanced round (1–2 days)
- `agents/llm.rs` + `agents/protocol.rs` with unit tests.
- Wire LLM into the **existing** `market.rs` flow (no bus yet): winner's
  delivery payload + bid notes + award reasoning become LLM products with the
  current templates as fallback; add `LLM_USED` timeline entries.
- **Verify:** `TRACE=1 just desktop` — run a round off a live fixture; timeline
  shows `LLM_USED status=used provider=venice model=kimi-k2-7-code`; then unset
  `VENICE_API_KEY` and confirm the round still completes with
  `status=fallback`. `cargo test` green.

### Phase 2 — real agent tasks + live discussion UI (2–3 days)
- `agents/bus.rs`, `agents/runtime.rs`, `agents/roles/*`; `spawn_market()` in
  setup; `run_agent_round` posts WANT and awaits the round.
- New `market://message` Tauri event; AgentArena renders the discussion feed
  (message list with agent avatars from the manifest registry).
- **Verify:** trigger a round from a live World Cup fixture; watch WANT → three
  BIDs (distinct LLM notes) → AWARD with reasoning → DELIVERED → VERIFIED
  appear incrementally in the UI; SQLite run timeline matches the bus log;
  pull the network cable mid-round → timeout fallback completes the run.

### Phase 3 — buyer tool-use loop (1–2 days, optional pre-deadline)
- Port `LLMBuyerStrategy`'s bounded tool-use to Rust: tools =
  `fetch_odds_snapshot`, `fetch_scores_snapshot`, `fetch_stat_validation`
  (all already exist as allowlisted `txline::api` paths). Max 6 turns.
- Buyer can *investigate* before awarding: e.g. verify a seller's claimed
  implied-probability move against the actual snapshot.
- **Verify:** transcript shows a tool call round-trip; turn bound enforced by
  test with a mock client that never converges.

### Phase 4 — CoralOS MCP transport (post-hackathon)
- Implement `AgentTransport` over Coral's MCP (`CORAL_CONNECTION_URL`), reuse
  the existing `coralos_server_url`/`coralos_token` config; agents become
  launchable both in-process and as separate processes registered with the
  same manifests — full compatibility with the solana_coralOS server.

---

## 8. Venice / Kimi specifics

| Item | Value |
| --- | --- |
| Endpoint | `POST https://api.venice.ai/api/v1/chat/completions` (OpenAI-compatible) |
| Auth | `Authorization: Bearer $VENICE_API_KEY` |
| Model | `LLM_MODEL=kimi-k2-7-code` (Kimi K2.7 code model; strict-JSON friendly). Check `GET /api/v1/models` for the current id list before phase 1 lands. |
| Kimi quirk | Floor `max_tokens` at **1024** (reasoning eats small budgets → empty `content`) |
| Fallbacks | Missing key / HTTP error / bad JSON → deterministic templates + `LLM_USED status=fallback` |
| Cost/latency | Kimi K2.7 is large: budget ~2–6s per completion. Run the 3 seller completions **concurrently** (`tokio::join!`); prefetch the fixture snapshot before prompting. Keep award reasoning ≤512 tokens. |
| Env | Already present: `LLM_PROVIDER=venice`, `VENICE_API_KEY`; added by this plan: `LLM_MODEL`, `TRACE` |

## 9. Risks & mitigations

- **Live-demo latency** (3 sellers × Kimi): concurrent completions + 30s round
  timeout + deterministic fallback means the demo can never hang.
- **Prompt injection via feed data:** TxLINE strings go into prompts, so treat
  every model reply as untrusted: typed-JSON parse, price/confidence clamps,
  verifier and settlement gates are code-only. (Same posture as
  solana_coralOS's guard.ts.)
- **Model id drift on Venice:** `LLM_MODEL` env override + provider default
  chain; `llmRuntimeInfo`-style helper logs the resolved model under `TRACE=1`.
- **Scope vs. July 19 deadline:** Phase 1 alone is demo-visible (real LLM
  reasoning in deliveries/awards with audit trail). Phase 2 is the wow (live
  discussion). Phase 3+ are stretch. Each phase ships independently.

## 10. File-touch summary

| File | Change |
| --- | --- |
| `src-tauri/Cargo.toml` | + `toml` crate (only new dep) |
| `src-tauri/src/config.rs` | + LLM env fields, `llm_configured` |
| `src-tauri/src/agents/*` | new module (7 files + roles) |
| `src-tauri/src/coral/market.rs` | becomes the deterministic **fallback** engine; `score_bid` reused as the award clamp |
| `src-tauri/src/coral/agents.rs` | registry backed by `agents/manifest.rs`, built-ins as fallback |
| `src-tauri/src/lib.rs` | `spawn_market()` in setup; `run_agent_round` delegates to runtime |
| `src/components/AgentArena.tsx` | + live discussion feed from `market://message` |
| `src/desktop/transport.ts` | + `onMarketMessage` listener |
| `coral-agents/*/coral-agent.toml` | + `[prompt]` section per agent (persona/system text) — manifests finally drive runtime |
