# TODO — CoralOS Multi-Agent Conversion

> Status audit as of 2026-07-11 (Phase 6 completed same day).  
> Cross-references: `crates/rig-venice/ROADMAP.md` (Phase 6 L384-420, Loose ends L600+),
> `coral-agents/README.md` (deleted Python stubs, protocol preserved at `git show HEAD~N:coral-agents/README.md`).

---

## ✅ Completed (Steps 0–5)

### Step 0 — coral-server discovery & MCP contract
- [x] `coral/coral.toml` written with `[registry] localAgents` pointing at `crates/agents/proof-guard-agent/coral-agent.toml`
- [x] `docker-compose.coralos.yml` defines coral-server + proof-guard-agent services
- [x] Real (non-devmode) agent registration flow understood: coral-server reads `coral-agent.toml`, launches Docker image, injects `CORAL_CONNECTION_URL`

### Step 1 — `crates/coral-client` (connect/wait/reply loop)
- [x] `coral_client::run(specialist, max_wait_ms, max_steps)` — generic connect → `wait_for_mention` → `send_message` loop
- [x] `Specialist` trait: `name()` + `async handle(CoralMention) -> String`
- [x] `wire` module: `verb()`, `num()`, `str_val()` parsers for `VERB key=value` flat grammar
- [x] `CoralMention` struct: `thread_id`, `sender`, `text`

### Step 2 — `crates/agents/proof-guard-agent` pilot binary
- [x] `ProofGuardSpecialist` implements `Specialist`, calls `agent_core::proof_guard::verify`
- [x] Handles `WAGER_PROOF_REQUESTED` → replies `WAGER_PROOF_VERDICT`
- [x] `coral-agent.toml` manifest (edition 4, options, Docker runtime)
- [x] `Dockerfile` (multi-stage, builds from workspace root)
- [x] Unit tests: handles delegation, ignores non-delegation mentions

### Step 3 — native orchestrator delegates for real
- [x] `native/src/services/agent/runtime/wager_proof_delegation.rs` — POSTs `WAGER_PROOF_REQUESTED` to a live CoralOS thread, polls `thread_reader::wait_for_message` for `proof-guard-agent`'s `WAGER_PROOF_VERDICT`
- [x] Fails closed (`WagerStatus::ProofFailed`) on timeout, missing process, or malformed reply
- [x] `live_session` threaded through the round so `console::publish_run` reuses the same session
- [x] Wired into `runtime/mod.rs` at the real wager-proof gate point (L412-478)

### Step 4 — proof-guard-agent registered in coral-server config
- [x] `coral/coral.toml` `[registry]` references `crates/agents/proof-guard-agent/coral-agent.toml`
- [x] `docker-compose.coralos.yml` has `proof-guard-agent` service with `CORAL_CONNECTION_URL`

### Step 5 — agent-core Rust ports (prerequisite for all specialist brains)
- [x] `agent_core::proof_guard` — 5-point deterministic verification, `ProofGuardConfig`, `ProofGuardVerdict`
- [x] `agent_core::fundamentals` — softmax probability model ported from Python `coral-agents/`
- [x] Both modules tested: `cargo test -p agent-core`

---

## ✅ Phase 6 — "each specialist as its own process" (completed 2026-07-11)

The ROADMAP.md (L384-420) called for replacing `specialist_ack()` in
`runtime/handoff.rs` with each specialist as its own process publishing real
messages on the Coral bus. All sub-items are now done; deviations from the
original sketch are noted inline.

### 6a. fan-pundit-agent → CoralOS process ✅
- [x] Create `crates/agents/fan-pundit-agent/` with `Cargo.toml`, `src/main.rs`, `coral-agent.toml`
- [x] Implement `FanPunditSpecialist` using `coral_client::Specialist` trait
- [x] Wire grammar: `PUNDIT_REACT_REQUESTED wager=<json>` → `PUNDIT_REACT_VERDICT stance=<s> wager=<json>`
- [x] Venice LLM tool-calling loop (`run_venice_verdict`) with `submit_pundit_verdict` tool, confidence nudge, and fallback
- [x] Add to workspace `Cargo.toml` members
- [x] Unit tests: handles delegation, NoBet passthrough, ignores non-delegation mentions
- [x] Registered — `coral/coral.toml` discovers every manifest via `localAgents = ["/agents/*"]` (compose mounts `./crates/agents:/agents:ro`); per-agent compose services are obsolete because coral-server spawns the containers itself via the Docker socket
- [x] Dockerfile (multi-stage, builds from workspace root)

### 6b. settlement-agent → CoralOS process ✅ (on-chain settlement still simulated)
- [x] Create `crates/agents/settlement-agent/` with `Cargo.toml`, `src/main.rs`, `coral-agent.toml`
- [x] Implement `SettlementSpecialist` using `coral_client::Specialist` trait with `SettleCap` capability token (§8)
- [x] Wire grammar: `SETTLE_REQUESTED proofRef=<ref> wager=<json>` → `SETTLE_VERDICT status=<settled|rejected|deferred> reason="..." txSig=<sig>`
- [x] Devnet settlement simulation with deterministic pseudo-signatures, proof-ref validation, edge/stake guards
- [x] Add to workspace `Cargo.toml` members
- [x] Unit tests: happy-path settlement, reject no-edge/zero-stake, reject missing proof-ref, ignore non-settle verbs
- [x] Registered via the `/agents/*` glob (see 6a note)
- [x] Dockerfile (multi-stage, builds from workspace root)
- [ ] Wire real Solana on-chain settlement (currently devnet simulation only) — moved to Loose Ends

### 6c. Wire existing standalone agents with coral-client ✅
Design decision made and implemented:

- [x] **sharp-movement-detector (Trading-track delegation)**: solved by the separate `crates/agents/trading-specialist` crate, which registers under the CoralOS identity `sharp-movement-detector` (fixed by `protocol.rs`) and answers `TRADE_REQUESTED` → `TRADE_VERDICT`. The original `crates/agents/sharp-movement-detector` poll-loop binary keeps its own job (watching the market itself) and stays standalone by design — see the naming note in `trading-specialist/coral-agent.toml`
- [x] **contrarian**: hybrid (option b). The FADE poll loop stays authoritative and unchanged; when coral-server spawns the container (`CORAL_CONNECTION_URL` present) it also services `FADE_STATUS_REQUESTED` → `FADE_STATUS strategy=fade_sharp positions=<n> lastPositionAt=<ts|none>` on the bus. Manifest + Dockerfile added; standalone runs (no env var) are unaffected
- [x] **arena-coordinator**: hybrid (option b). Settlement poll loop unchanged; services `ARENA_STATUS_REQUESTED` → `ARENA_STATUS settled=<n> followWins=<n> followLosses=<n> fadeWins=<n> fadeLosses=<n> leader=<FOLLOW|FADE|TIE>`. Manifest + Dockerfile added (note: settling in Docker requires a shared volume for the position logs — documented in its manifest)
- Design note: the observed coral-server MCP surface is `wait_for_mention` → `send_message` only — an agent cannot open a thread or publish unprompted, so "publishes signals back to the bus" is not implementable on the real transport; mention-driven status is the honest hybrid

### 6d. Orchestrator handoff.rs → real delegation for all specialists ✅
- [x] `specialist_ack()` is gone — every track delegates for real, mirroring `wager_proof_delegation`'s shape
- [x] Implemented as one module per specialist (`settlement_delegation.rs`, `trading_delegation.rs`, `fan_pundit_delegation.rs`) rather than the sketched generic `specialist_delegation.rs` — each verdict has its own typed outcome (status/txSig vs stance/wager vs positionId/sizeSol), so a generic verb-in/verb-out helper would have pushed the parsing back to the callers
- [x] Each handoff is a real CoralOS publish + poll (`console::send_raw_message` + `thread_reader::wait_for_message`), failing closed on timeout

### 6e. Payload enrichment (thesis + tool-trail) ✅
- [x] Delegation wire messages now carry ` toolTrail=<json>` (the Venice loops' real tool calls + deterministic results) alongside `wager=<json>`; the wager JSON itself already carried the `thesis`/rationale. Ordering contract: `toolTrail=` precedes `wager=`, which stays the trailing key (`handoff::tool_trail_wire_suffix`)
- [x] `CoralMessage.payload` for the wager-proposal and pundit-reaction messages includes `toolTrail` (`handoff::wager_ruling_payload`)
- [x] Implemented as `agent_core::ToolTrailEntry { agent, tool, result }`, **not** the sketched `Vec<ToolCallRecord>` — the rig tool loop has no idempotency key, capability check, or pre-execution timestamp to report, and fabricating those audit fields for the transcript would violate the honest-audit rule (§24)
- [x] All `wager=`/`signal=`/`decision=` extractors unified on `coral_client::wire::json_val` — word-boundary + brace-matching + string-escape aware, replacing two greedy-to-end-of-string parsers (proof-guard, fan-pundit) and two non-string-aware brace counters (settlement, trading-specialist). Unit-tested in `wire.rs`; each specialist has a `toolTrail=`-tolerance regression test

---

## 🔲 Loose Ends (ROADMAP.md L600+)

### Infrastructure
- [ ] **CoralOS/Docker runtime verification**: Stand up the actual `solana_coralOS` sibling runtime and verify proof-guard-agent (and future agents) against it. Explicitly deferred because that infra wasn't available to build and verify in-session
- [ ] **Docker Compose integration test**: `docker compose -f docker-compose.coralos.yml up` should launch coral-server + all agent containers, and a test harness should send a `WAGER_PROOF_REQUESTED` and receive a `WAGER_PROOF_VERDICT` end-to-end
- [ ] **CI pipeline**: Add `cargo build -p proof-guard-agent` and eventually all agent binaries to CI

### agent-core gaps
- [ ] **Live fundamentals feed**: `agent_core::fundamentals` softmax model runs on neutral defaults because there's no live form/xG/injuries data source — wiring a real fundamentals feed is future work (see `wager_agent.rs` L16-25 honest limitation)
- [ ] **Live news/narrative feed for fan-pundit**: Same gap — `pundit_agent.rs` L17-21 notes it reacts to the wager's own thesis, not external news
- [ ] Port remaining 7 of 9 Python agent stubs' decision logic into `agent_core` (only `proof_guard` and `fundamentals` done so far)
- [ ] **Real Solana on-chain settlement** (moved from 6b): settlement-agent's verdicts are devnet simulations with deterministic pseudo-signatures

### Protocol / wire format
- [ ] Consider upgrading from flat `VERB key=value wager=<json>` text grammar to structured JSON payloads once coral-server supports a structured `payload` field in `send_message` MCP tool
- [ ] Idempotency keys for Coral messages (currently only signal/position logs have `IdempotencyKey`)

### Testing
- [ ] Integration tests in `tests/e2e/` that spin up coral-server + proof-guard-agent and run a full round
- [ ] Property tests for `coral_client::wire` parser (fuzz the `VERB key=value` grammar)
- [ ] Test `wager_proof_delegation.rs` timeout path with a mock CoralOS endpoint

---

## Summary — What's Done vs What's Left

| Area | Done | Remaining |
|------|------|-----------|
| coral-client library (incl. shared `wire::json_val`) | ✅ Full | — |
| proof-guard-agent (CoralOS process) | ✅ Full | Docker verification against real coral-server |
| Native orchestrator → real delegation, all tracks | ✅ Full | — |
| fan-pundit-agent (CoralOS process) | ✅ Full | — |
| settlement-agent (CoralOS process) | ✅ Full | Real on-chain settlement (devnet simulation today) |
| trading-specialist (CoralOS process, `sharp-movement-detector` identity) | ✅ Full | — |
| contrarian (hybrid coral-bus) | ✅ Full | — |
| arena-coordinator (hybrid coral-bus) | ✅ Full | Shared log volume for containerised settling |
| Payload enrichment (tool-trail) | ✅ Full | Structured payloads blocked on coral-server support |
| CoralOS/Docker runtime verification | ❌ | Infra not available in-session |
| agent-core decision logic ports | ✅ 2/9 | 7 Python stubs remain |
| Live fundamentals/news feeds | ❌ | No data source in codebase |
