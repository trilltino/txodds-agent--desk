# Agentic Expansion Plan — Tool Surface, Memory, Planning, Proactivity, Reputation

> Evaluates the "Core Expansions for Greater Autonomy" proposal against this
> repo's actual code, not a generic Coral-marketplace template. None of the
> proposal's original identifiers (`runToolLoop`, `waitForMention()`,
> `llm_buyer.ts`, `maxToolCalls`, `solana-agent-tools`) exist here — that
> proposal was written for a different (TypeScript) scaffold. This plan
> re-derives each item against the real Rust architecture: `crates/rig-venice`
> (LLM tool-calling), `crates/agent-core` (deterministic decision primitives),
> `native/src/services/agent/runtime` (the in-process Match Intelligence
> round), and `crates/agents/*` (standalone CoralOS participants).

## The one constraint every item below must respect

This repo has a load-bearing, repeatedly-stated rule:

> "LLMs may explain a decision that code has already made; they never make
> one." — `native/src/services/llm/mod.rs`, `ui/core/agent/types.ts`

And per `crates/rig-venice/ROADMAP.md` ("Removing the kill switch"):

> "proof-guard-agent's verdict before settlement is now the only
> human-in-the-loop moment left in the system."

Concretely: **position sizing, settlement, and proof verdicts stay pure
deterministic Rust — no LLM in that call path, ever.** Today only two of
eleven agent crates actually call an LLM at all:

| Agent | LLM-driven? |
|---|---|
| `sharp-movement-detector` | ✅ (`venice.rs`, own tool set) |
| `fan-pundit-agent` | ✅ (`main.rs`, own tool set) |
| `proof-guard-agent`, `settlement-agent`, `arena-coordinator`, `contrarian`, `match-intelligence`, `trading-specialist`, `idle-agent`, `user-proxy` | ❌ pure deterministic Rust |

Every expansion below must preserve that split. "More autonomy" here means
*richer research and explanation* for the two LLM-driven agents (and the
in-process match-intelligence LLM narration step), not new LLM authority over
money-moving or verification decisions.

## What already exists (don't rebuild it)

- **The tool loop already exists.** `crates/rig-venice/src/loop_runner.rs`
  (`run_tool_loop`) is this repo's `runToolLoop` — a hand-rolled multi-turn
  loop (rig-core 0.9's built-in `Agent::chat` only does one tool call) that
  terminates on a designated "final" tool call, bounded by `max_rounds`, with
  an `on_step` hook already wired to `agent_core::safety::BudgetGuard`.
  `sharp-movement-detector/src/venice.rs` and `fan-pundit-agent/src/main.rs`
  both already use it.
- **A tool surface already exists.** `crates/rig-venice/src/tools.rs` has
  five `rig::tool::Tool` impls today: `FetchOddsSnapshot`,
  `FetchActiveFixtures`, `ComputeSharpMovement`, `ComputeModelProbability`,
  `ComputeFairProbability`. New tools are additions to this pattern, not a
  new mechanism.
- **Bounded reasoning is a deliberate safety feature, not an oversight.**
  `agent-core::safety::BudgetGuard` defaults to 200 tool calls / 1,000,000
  lamports / 3600s per session, and cannot be raised by the agent itself
  (Checklist §28). "Increase max rounds" is a per-agent tuning knob
  (`MAX_TOOL_ROUNDS` env var in `fan-pundit-agent`), not a removal of the cap.
- **Short-term memory already exists.** `agent-core::context::AgentContext`
  carries `recent_runs: Vec<RunSummary>` (last 20 runs: run_id, fixture_id,
  track, created_at) into every round. It's a bare summary today — no
  verdict/outcome/PnL — which is exactly the gap Priority 2 below closes.
- **Long-term storage already exists.** `native/src/services/ledger/store.rs`
  (SQLite) already persists everything a reflection pass would need:
  `list_runs`, `get_run`, `list_arena_positions`, `list_settlement_records`,
  `list_signal_records`, `get_arena_score`, `list_agent_leaderboard`,
  `list_tool_call_records`, `list_arena_sessions`. Nothing here needs a new
  store — it needs new *read* tools over the store that already exists.
- **Deterministic reputation already exists.** `agent-core::arena` computes
  `AgentLeaderboardEntry` (win rate, cumulative PnL, avg winning confidence)
  from settled `ArenaPosition`s — served today via `list_agent_leaderboard`.
  This *is* the reputation system the proposal asks for; it just isn't yet
  exposed as an LLM-callable tool.
- **Proactive behavior partially already exists.** `sharp-movement-detector`
  already runs its own poll loop against live TxLINE odds and decides for
  itself when to emit a signal — it is not purely `waitForMention()`-reactive
  (that pattern doesn't exist in this codebase; CoralOS participants use
  `coral_client::run(specialist, ...)` → `wait_for_mention` → `handle()`,
  which *is* reactive for the delegation-based agents, but
  sharp-movement-detector's own ingest loop is not).

---

## Priority 1 — Expand the tool surface (Medium)

**Where:** `crates/rig-venice/src/tools.rs` (shared tools), plus
agent-specific tools colocated with the agent that needs them
(`crates/agents/sharp-movement-detector/src/venice.rs`,
`crates/agents/fan-pundit-agent/src/main.rs`).

**Add, following the existing `Tool` impl pattern exactly** (typed
`schemars::JsonSchema` args, `wrap_untrusted` on anything from an external
source, bounded response size like `FetchOddsSnapshot`'s 32 KiB cap):

- `GetAgentLeaderboard` — wraps `LedgerStore::list_agent_leaderboard`. Read-only,
  no new logic; the deterministic reputation numbers already exist.
- `GetRecentSignals` / `GetRecentSettlements` — wrap
  `list_signal_records` / `list_settlement_records`, scoped to a fixture or
  agent ID, for "what has the market actually done recently" context.
- `GetToolCallHistory` — wraps `list_tool_call_records`, useful for a
  reflection pass (Priority 2) to see what an agent actually tried, not just
  what it concluded.
- `GetProofStatus` — thin wrapper over `native/src/services/proof` for
  on-chain/txoracle verification state (feeds Priority 5 below too).

**Explicitly do not add:** a tool that proposes or executes a stake/position
size, or a tool that overrides `proof-guard-agent`'s verdict. Those stay
where they are — deterministic, in `agent-core::policy` /
`agent-core::proof_guard` / the settlement delegation modules.

**Tuning knob, not architecture change:** raise `MAX_TOOL_ROUNDS` for
`fan-pundit-agent` and add the equivalent env-tunable `max_rounds` to
`sharp-movement-detector/src/venice.rs` if it's hardcoded today — check
before assuming it needs a code change; `BudgetGuard`'s `max_tool_calls`
already provides the hard outer bound regardless of what an agent requests.

---

## Priority 2 — Memory + basic reflection (Medium)

**Where:** `agent-core::context` (richer `RunSummary`),
`native/src/services/agent/runtime/persistence.rs` (already writes
everything needed), a new read path exposed as a tool (Priority 1's
`GetToolCallHistory`/`GetAgentLeaderboard`), and — for the two LLM agents
only — a post-round reflection prompt.

**Concrete steps:**

1. Extend `context::RunSummary` (today: run_id, fixture_id, track,
   created_at) with the fields that make "learn from the past" possible:
   `verdict_status`, `pnl_points` (if settled), `signal_confidence`. This is
   additive to an existing struct, not a new subsystem — `build_context`
   already takes `Vec<AgentRun>` and maps it; it just isn't pulling these
   fields out yet.
2. `persistence.rs::persist_run` already writes the full `AgentRun` plus its
   `LlmResponse` and `AgentDecision` to the ledger every round — reflection
   has a complete audit trail to query from day one. No new write path
   needed.
3. Reflection itself is scoped to the two LLM-driven agents
   (`sharp-movement-detector`, `fan-pundit-agent`): after N rounds, an
   additional tool-loop call (reusing `run_tool_loop`, not a new mechanism)
   with a prompt built from `GetAgentLeaderboard` + `GetToolCallHistory`,
   terminating in a `SubmitStrategyNote` final tool whose *text* output gets
   stored (e.g. a new `ledger.upsert_strategy_note(agent_id, note)`) and
   surfaced back into the next round's prompt as *additional context a human
   or the agent's own future self can read* — never as a value that changes
   `BudgetGuard` limits, policy thresholds, or capability grants
   automatically. Self-modifying safety parameters is exactly the kind of
   autonomy this repo's "no kill switch, but hard non-agent-writable budget
   caps" design explicitly guards against.

---

## Priority 3 — LLM-driven planning (Higher difficulty)

**Where:** the existing `run_tool_loop` call sites in
`sharp-movement-detector/src/venice.rs` and `fan-pundit-agent/src/main.rs`.
**Not** `crates/agent-core/src/agent.rs` or a new
`runtime/planner.rs` — neither exists, and match-intelligence's in-process
round (`native/src/services/agent/runtime/mod.rs`) is not itself an LLM
agent loop; it calls `llm::venice` once, after the deterministic decision is
already made, purely to narrate it (`explain_decision`, mod.rs:300) —
inserting a "planning phase" there would cross the line this repo has
explicitly drawn (ROADMAP.md's "Two Venice clients" resolution: that
narration call is slated for deletion once nothing needs it, not for
expansion into a decision-maker).

**What a "plan-and-execute" pattern looks like here, correctly scoped:**
`run_tool_loop`'s "forced final tool" convention already *is* a lightweight
plan-then-execute shape — the agent must converge on one structured final
call (`SubmitSignalDecision`, `SubmitPunditVerdict`) rather than free text.
A genuine planning phase means: before the existing tool-calling round,
one additional `run_tool_loop` call whose final tool is `SubmitPlan` (a
structured list of "which tools I intend to call and why"), which is then
*validated against the existing policy gates* (`agent-core::policy`) before
the real tool-calling round proceeds — the plan informs *tool call order and
justification in the trace*, not whether policy allows an action. This
keeps "the LLM decides" scoped to research strategy, while
`agent-core::policy`/`safety` keep deciding what's *allowed*.

Recommend building this only after Priorities 1–2 land and only for
`sharp-movement-detector` first (it already has the most tool variety) —
this is real net-new complexity, unlike 1/2/5 which are mostly wiring
existing data through the existing pattern.

---

## Priority 4 — Proactive behavior (Medium)

**Where:** `sharp-movement-detector`'s existing ingest/poll loop (extend it,
don't replace it) and, if buyer-side proactivity is wanted,
`native/src/services/agent/runtime` would need a new background task
alongside the existing `run_agent_round`-per-TxLINE-event trigger — there is
no existing scheduler in `native/` for this; `run_match_intelligence_round`
is called per-event today (via `run_agent_round` Tauri command), not on a
timer.

**Concrete step:** rather than a net-new proactive system, extend what
`sharp-movement-detector` already does (it already polls and decides
autonomously when to signal) with the richer tool set from Priority 1, so
its proactive decisions are better-informed — e.g. it can call
`GetAgentLeaderboard` before deciding a signal is worth emitting, so
signal-worthiness accounts for how the market has actually been trading
recently, not just this poll's odds delta.

A genuinely new proactive loop for the in-process match-intelligence round
(the desktop app spontaneously analyzing fixtures without a chat request or
a TxLINE event) is a much bigger change — it would need a scheduler in
`native/` and a decision about which fixtures to prioritize — and should be
scoped separately if wanted; it is not a small extension of existing code
the way the other four priorities are.

---

## Priority 5 — Reputation + on-chain tools (Medium)

**Where:** new tools in `crates/rig-venice/src/tools.rs` (or agent-specific,
if only one agent needs a given tool), wrapping already-existing read paths.

- **Reputation:** `GetAgentLeaderboard` (Priority 1) — this is genuinely the
  bulk of this priority; the deterministic leaderboard computation already
  exists in `agent-core::arena`, it's just not yet tool-callable.
- **On-chain/proof history:** wrap `native/src/services/proof`'s
  `ValidationBridge` (txoracle simulation/validation) and
  `LedgerStore::list_arena_positions`'s `tx_signature` /
  `outcome.settlement_tx` fields — these already carry on-chain references
  per settled position; a tool exposing "has this fixture/position been
  settled on-chain, and what was the tx" is a read wrapper, not new
  infrastructure.

**Feed into planning/bidding, not into settlement.** Per the constraint at
the top: reputation and proof-history tools inform what an LLM-driven agent
*investigates and narrates* (e.g. "this fixture's sharp-movement signal
history has been unreliable, treat with lower confidence in the
explanation") — they must not become a new input that
`agent-core::policy`'s deterministic gates don't also see and check
independently. If reputation should actually affect whether a position is
taken, that's a change to `agent-core::policy`/`arena` (deterministic), with
the LLM tool only *reading and narrating* that same deterministic value —
consistent with how `ComputeModelProbability` already works today (pure
deterministic math, callable by the LLM, not computed by it).

---

## Suggested landing order

1. **Priority 1** (tool surface) — pure additive wrapping of data that
   already exists in the ledger; lowest risk, immediate payoff for both
   LLM-driven agents.
2. **Priority 5** (reputation/on-chain tools) — same shape as Priority 1,
   bundle it in the same pass since `GetAgentLeaderboard` serves both.
3. **Priority 2** (memory + reflection) — needs Priority 1's tools to have
   something to reflect *on*; the `RunSummary` extension is small and
   independent, can start in parallel.
4. **Priority 4** (proactive) — extend sharp-movement-detector's existing
   loop with the new tools; defer the "new scheduler in native/" version
   until there's a concrete product need for it.
5. **Priority 3** (LLM planning) — highest complexity and highest risk of
   drifting into "LLM decides" territory; do last, and only once the
   planning output's role (informs tool order/trace, not policy) is nailed
   down in review before writing code.
