# rig-venice: path to a real tool-calling agent

This document is the working plan for turning the specialists in
`crates/agents/*` and `coral-agents/*` from deterministic-logic-plus-narration
into actual LLM agents: an LLM that decides which tools to call, in what
order, and stops when it has enough to act — rather than Rust code computing
an answer and asking Venice to describe it afterward.

## Where we actually are today (read this before touching anything)

Grounding this in the real code, not the aspiration. **Updated post-Phase-1**
— see the Phase 1 section below for what changed and why; this section now
describes the state after that landed.

- **`crates/agents/sharp-movement-detector/src/main.rs`** is the only agent
  binary in the workspace that runs a real tool-calling loop today (Phase 1,
  done). It has real safety infra (`BudgetGuard`, `StepCounter`, idempotency
  keys, capability tokens) plus a hand-rolled multi-turn agent loop
  (`run_agent_loop()`) built on `rig_venice::client()` — `rig-core` 0.9
  doesn't provide multi-turn tool-calling out of the box, see the Phase 1
  notes for why. The sharpness threshold decision now lives in the
  `compute_sharp_movement` tool the LLM chooses to call, not in inline Rust
  arithmetic; the model's free-text narrative remains advisory-only and
  never gates whether a signal is logged.
- **`crates/rig-venice/src/tools.rs`** has real `rig::tool::Tool`
  implementations (`FetchOddsSnapshot`, `ComputeSharpMovement`,
  `FetchActiveFixtures`) with schemars-derived schemas, now exercised by
  `sharp-movement-detector`. `MovementResult` also derives `Deserialize` now
  (needed so an agent loop can read a tool's JSON output back out of a
  `ToolCall` response, not just construct it directly as Rust used to).
- **`coral-agents/*/agent.py`** (match-intelligence, fan-pundit, etc.) are
  still idle Coral MCP stubs — untouched by Phase 1, which only targeted
  `sharp-movement-detector`. Real logic for these still lives in
  `native/src/services/agent/runtime.rs` on the Rust/Tauri side, and per
  `coral-agents/README.md` it is 100% deterministic formulas (softmax over
  weighted features, narrative-nudge arithmetic) — no LLM call in the
  decision path at all, anywhere. This is Phase 4/5/6 territory, not done.
- **`native/src/services/llm/venice.rs`** is still a second, separate Venice
  client (plain reqwest, not `rig`) used from the Tauri backend for the
  `AGENT_THOUGHT` narration step in the Coral message flow — untouched by
  Phase 1. So there are still **two independent Venice integrations** in
  this repo; see "Loose ends" below for the resolution plan, which still
  hasn't executed (it depends on Phase 6, not Phase 1).

Net: Phase 1's tool-calling pattern is proven in one binary. Phases 2–7 are
still open — see their sections below, several of which now have sharper
detail from what Phase 1 actually required in practice.

## Target architecture: multiple agents that argue, not one agent run N times

It's worth being explicit about this because it's easy to drift toward the
wrong end-state: promoting `sharp-movement-detector`, then
`match-intelligence-agent`, then `fan-pundit-agent` one at a time (Phases
1–5 below) gets you **three independently tool-calling LLM agents that are
each still invoked one at a time by a scripted Rust orchestrator** — that is
not multi-agent, it's single-agent-times-three. The docstrings in the
existing Python stubs already describe the right shape and it's a debate,
not a pipeline:

> "In the debate it plays the adversary that either endorses a proposed
> wager... or challenges it" — `fan-pundit-agent`
> "sharp-movement-detector compares it against the overround-stripped
> market... fan-pundit-agent nudges it with narrative conviction... the
> Authority ultimately re-derives Kelly sizing" — `match-intelligence-agent`

For this to be real multi-agent, three things have to be true that aren't
today:

1. **Specialists see each other's reasoning, not just each other's final
   number.** Right now `fan-pundit-agent` receives a serialized `Wager` dict
   (`_proposed_wager`) — a number, no rationale. A real debate means it
   receives (or can ask for) `match-intelligence-agent`'s tool-call trail and
   thesis text, and can rebut specific claims in it ("your xG differential
   assumes fit squad, but injuries tool shows 3 key absentees").
2. **The handoff is peer-to-peer on the Coral bus, not orchestrator-scripted.**
   `coral-agents/README.md` already has the transport for this (`TOOL_CALL`
   / `TOOL_RESULT` messages, `DELEGATE`) — what's missing is any specialist
   *initiating* a message to another specialist or back to the orchestrator
   with a counter-argument, rather than only ever replying once to a single
   inbound `DELEGATE`.
3. **Someone (or something) resolves disagreement.** A debate that never
   converges is useless. The Rust Authority already owns final Kelly sizing
   and the proof gate — that's correct and should stay deterministic — but
   there also needs to be an explicit round limit and a tie-break rule for
   "sharp says back HOME, pundit challenges to NO_BET, do we listen to the
   challenge or not" that isn't just "pundit's nudge is capped at ±3%" (the
   current `CONF_NUDGE` mechanism, which is really a soft veto disguised as
   a probability adjustment).

Phases 1–5 build the *ingredients* (each specialist reasoning with tools
instead of fixed formulas). Phase 6 is where they actually become
multi-agent — don't treat it as a protocol-plumbing afterthought, it's the
actual goal. Restated:

## Non-negotiable invariant (do not relax this while doing any of the below)

LLM output stays **advisory-to-proposal**, never execution. Per
`coral-agents/README.md`: `allow_money_decisions = false`,
`allow_settlement_release = false`, and the txoracle proof gate
(`proof-guard-agent` / `services::proof`) stays deterministic Rust forever.
Everything in this roadmap increases how much an LLM gets to *decide among
options*; nothing in it should give an LLM the ability to *execute* a trade,
move funds, or bypass the proof gate. If a phase below seems to require that,
stop and redesign the phase instead.

## Phase 0 — pick one crate as the pilot, don't touch all of them

Target: **`sharp-movement-detector`**. Reasons: it's the most complete agent
skeleton already (safety gates, idempotency, audit log all exist), it's
already importing nothing from `rig-venice` so there's no migration risk to
existing behavior, and its job (decide if a signal is worth logging) is
low-stakes to get wrong compared to `match-intelligence-agent`, which
proposes actual stakes.

Do not start on `match-intelligence-agent` or the Python `coral-agents/*`
stubs until Phase 0–2 are proven here.

## Phase 1 — wire `rig-venice`'s tools into a real agent loop — DONE

Landed in `crates/agents/sharp-movement-detector/src/main.rs`. Notes for
whoever does Phase 4/5 next, since this surfaced two things the earlier plan
didn't anticipate:

- **`rig-core` 0.9's `Agent::chat`/`prompt` is not a multi-turn loop.** Its
  `Chat::chat()` impl executes at most *one* tool call and returns that
  tool's raw output as the "final answer" — it never feeds the result back
  to the model for a second turn. Building a real tool-calling agent on this
  version of `rig` means hand-rolling the loop yourself via the
  `Completion` trait (`agent.completion(prompt, history).send()`), inspecting
  `response.choice` for `AssistantContent::ToolCall`s, executing them
  through `agent.tools.call(name, args)`, appending both the assistant's
  tool-call message and a `UserContent::ToolResult` message to history, and
  looping until the model returns `AssistantContent::Text` or a round cap is
  hit. See `run_agent_loop()` in `sharp-movement-detector/src/main.rs` for
  the reference implementation — reuse it (or extract it into `rig-venice`
  as a shared helper once a second binary needs it) rather than
  re-deriving this per agent.
- **`MovementResult` needed a `Deserialize` derive.** It only had
  `Serialize` (fine when Rust was constructing it directly); once an agent
  loop reads a tool's JSON output back out of a `ToolCall` response, it
  needs to deserialize too. Added in `rig-venice/src/tools.rs`.
- **The threshold decision moved into the tool, not the agent's prose.**
  `detector_step()` now only pre-filters on "did the odds change at all"
  (not a sharpness threshold — that's still the LLM's call) and then gates
  strictly on the `compute_sharp_movement` tool's own `is_sharp_move` /
  `confidence` fields pulled out of the trace. The model's free-text answer
  is stored as `narrative` and never influences whether a signal is logged
  — same invariant as before, now actually enforced structurally instead of
  by convention, because the boolean literally comes from deterministic Rust
  tool code, not text-parsing.
- **Model default divergence is resolved.** `sharp-movement-detector` no
  longer has its own `VENICE_MODEL` default (was `llama-3.3-70b`); it now
  goes through `rig_venice::model_name()` like everything else, so the
  workspace has one default (`kimi-k2-7-code`) instead of two.

Acceptance (revised from the original wording, which assumed `rig` looped
for you): a transcript where the LLM's tool-call sequence is sane, it
reaches a stop condition (plain-text final answer) within
`MAX_TOOL_ROUNDS`, and `compute_sharp_movement`'s own output — not the
model's prose — is what the calling code reads for the sharp/not-sharp
decision. `cargo test -p sharp-movement-detector` and
`cargo clippy -p sharp-movement-detector --all-targets -- -D warnings` both
pass; the actual multi-fixture live transcript read-through (item 4 in the
original plan) still needs a real `VENICE_API_KEY` and live TxLINE data and
hasn't been done — do that before trusting this on a real poll loop.

## Phase 2 — structured final output, not free text — DONE

Landed as `SignalDecision` + a `submit_signal_decision` tool in
`sharp-movement-detector/src/main.rs` (there's no built-in structured-
extraction helper in `rig-core` 0.9, so this uses the "force termination via
a tool call" approach the original plan named as the fallback). One
deliberate scope narrowing versus the original wording:

- **`SignalDecision` carries only `rationale`, not `direction`/`confidence`
  too.** The original plan said it should "mirror `SignalRecord`" with those
  fields included. Doing that would mean the model self-reports
  `is_sharp_move`/`confidence`/`direction` in the same structured call —
  which quietly reintroduces exactly what Phase 1 was built to prevent:
  trusting the model's own claim about sharpness instead of the
  deterministic `compute_sharp_movement` tool result. So `SignalDecision`
  stays narrow (just the human-readable rationale) and
  `AgentTrace::compute_sharp_movement_result()` remains the only source of
  truth for the gating fields. If Phase 3's eval work later wants to compare
  "what the model would have self-assessed" against the tool's answer for
  calibration purposes, add those fields back deliberately then, with that
  comparison as the explicit reason — not preemptively now.
- The loop now treats a plain-text response (no tool call at all) as "agent
  gave up without an answer" rather than attempting to salvage a decision
  out of prose, since the system preamble instructs it to always terminate
  via `submit_signal_decision`.

Acceptance (revised): `cargo test -p sharp-movement-detector` and
`cargo clippy -p sharp-movement-detector --all-targets -- -D warnings` pass
with the new tool wired in and covered by unit tests. The original
acceptance wording ("~20 replayed fixture-poll cycles") still needs a real
`VENICE_API_KEY` and hasn't been run — same live-data caveat as Phase 1.

## Phase 3 — eval harness before anything touches a real track — PARTIALLY DONE

The original plan (diff the old deterministic `detector_step()` against the
new agent-loop path over historical data) turned out to not be executable as
written: Phase 1 deleted the old deterministic path outright (per its own
instructions — "delete the Rust-side pre-computation"), so there is no
second implementation left to diff against, and no live TxLINE/Venice
credentials are available in this environment to pull real historical
sequences or run live Venice calls anyway.

What actually landed instead, as the offline-inspectable substitute:
`sharp-movement-detector/src/main.rs`'s test module now has
`phase3_agent_loop_replay_against_mock_venice` — a minimal hand-rolled mock
HTTP server (no crate dependency needed, no live API key) that scripts a
two-round OpenAI-compatible transcript (agent calls `compute_sharp_movement`,
then `submit_signal_decision`) and drives the *real*
`build_reasoning_agent` / `assess_movement` / `run_agent_loop` code path
against it, asserting on the resulting `MovementResult` and rationale. This
is a real eval harness in the sense that matters most right now: it
exercises the actual tool-calling loop deterministically and repeatably,
without hitting a live model.

What it is **not**: it does not replay real historical odds sequences (there
aren't any available here), and it only scripts one scenario. Before trusting
this on a live poll loop, someone with TxLINE/Venice access should:

1. Extend `spawn_mock_venice` with more scripted transcripts covering edge
   cases (agent skips `compute_sharp_movement` entirely, agent never calls
   `submit_signal_decision` and exhausts `max_tool_rounds`, tool call with
   malformed arguments, etc.) — the harness scaffold supports this today,
   it just has one scenario scripted.
2. Once real historical odds sequences and a live `VENICE_API_KEY` are
   available, replay real data through the loop and read the actual
   transcripts — not just assert shapes, but read whether the model's
   reasoning is *sound*, which no offline mock can substitute for.

Do not proceed to trusting this on a real track until that live read-through
happens. This is a betting system — passing a scripted mock is necessary but
not sufficient evidence.

## Phase 4 — promote `match-intelligence-agent` (harder, do it after 1–3 are boring) — DONE (additive, not a replacement)

**Naming trap worth flagging before anyone continues this.** There are two
unrelated things in this repo both called "match-intelligence":

1. `coral-agents/match-intelligence-agent/agent.py` — the fundamentals/
   softmax specialist this phase actually targets (converts form/xG/injuries/
   rank/h2h into a 1X2 probability, proposes `Wager`s, gated by
   `proof-guard-agent`).
2. `crates/agents/match-intelligence` — a completely different Rust binary,
   the **FollowSharp** side of the two-agent Arena game (`crates/agent-core/
   src/arena.rs`: `match-intelligence-agent` (FOLLOW) vs `contrarian-agent`
   (FADE) betting on sharp-odds-movement direction). It has nothing to do
   with `Wager`/Kelly sizing/proof-guard — it records `ArenaPosition`s in a
   toy competitive-agent game. Its kill-switch removal earlier in this
   session was legitimate and unrelated to this phase.

Do not confuse the two when picking up this phase — #1 is the target, #2 is
a different system that happens to share a name.

**What's landed**: the deterministic math pieces #1 needs, ported faithfully
from the Python `_model_distribution`/`_side_score`/`_fair_probabilities` to
real `rig::tool::Tool` implementations in `crates/rig-venice/src/tools.rs`:

- `ComputeModelProbability` — the weighted-feature-score-through-softmax
  fundamentals model. Straight port, same weights/defaults as the Python
  `MI_*` env vars. Verified against the Python math by hand (a neutral
  fixture yields draw ≈ 0.109, not the raw 0.26 prior, because
  `HOME_ADVANTAGE` alone creates enough home/away divergence to pull the
  draw logit down — this surprised the first version of the test I wrote,
  which assumed a range closer to the raw prior; fixed once verified against
  the actual formula rather than intuition).
- `ComputeFairProbability` — strips the bookmaker overround from decimal
  odds, reusing `txodds_types::implied_probability` (already existed in
  Rust) rather than re-deriving it.
- Kelly sizing was **not** added as a tool. `txodds_types::kelly_fraction`
  already exists and is exactly what the Python agent uses for its
  "advisory" stake suggestion — but per the non-negotiable invariant above,
  Kelly sizing is the Rust Authority's job to re-derive and clamp, not
  something to hand the LLM as a tool call it can lean on. If a future pass
  wants the agent to see an advisory Kelly number the way the Python version
  does, that's a deliberate, separate decision — don't wire it in by
  default just because the function is sitting right there.

Both new tools are covered by unit tests (`cargo test -p rig-venice`) and
pass `cargo clippy -p rig-venice --all-targets -- -D warnings` (once fixed
for a `needless_question_mark` lint on the new code).

**What landed on top of that, after reading `native/src/services/agent/
runtime.rs` in full**:

- **A real disconnect was found before writing any code.** The Wager /
  `proof-guard-agent` / Kelly-sizing flow this whole roadmap describes is
  not what the live orchestrator (`run_match_intelligence_round`) actually
  runs. It uses a completely separate, simpler model —
  `AgentSignal`/`AgentDecision`/`policy::choose_action` driven by
  severity/actionability scores — and **never constructs a `Wager` at all**.
  Meanwhile `native/src/services/agent/authority.rs` — a fully built, fully
  tested Rust "Authority" (`adjudicate()`: recomputes edge, sizes with
  Kelly, clamps to the devnet cap, gates on proof) already existed, with
  zero callers anywhere in the live path. It was dead code, exactly like
  the tool-calling scaffolding in Phase 1 was dead code before it got wired
  up.
- **`crates/rig-venice/src/loop_runner.rs`** — the hand-rolled multi-turn
  tool loop from Phase 1 was extracted out of `sharp-movement-detector` into
  a shared `run_tool_loop()` helper once this phase needed the exact same
  pattern a second time. `sharp-movement-detector` was refactored to call
  the shared version (still passes all 7 of its own tests) instead of
  keeping a second copy.
- **`native/src/services/agent/wager_agent.rs`** (new) — a Venice reasoning
  agent using `ComputeModelProbability` + `ComputeFairProbability` +
  a forced `submit_wager_assessment` tool (same "terminate via tool call,
  not prose" pattern as Phase 2). When it proposes a wager, its
  self-reported `model_prob` is discarded in favour of pulling the number
  straight out of the `compute_model_probability` tool result — same
  "trust the tool, not the model's narration" invariant as Phase 1.
- **Wired into `run_match_intelligence_round`** (`runtime.rs`) as an
  **additive** step, not a replacement: it runs alongside the existing
  severity/actionability signal pipeline, which is completely untouched.
  This is the first time `authority::adjudicate` is called from the live
  path — confirmed by the dead-code warnings for `AuthorityPolicy`,
  `AuthorityRuling`, and `adjudicate` disappearing once this landed.

**Honest limitation, stated up front rather than discovered later**:
`ComputeModelProbability`'s inputs (form, xG, rank, injuries, h2h) have **no
live data source anywhere in this codebase** — TxLINE supplies odds and
score events, not pre-match team-fundamentals stats. So in production this
always runs the softmax model on neutral defaults, which only ever produces
the home-advantage-only baseline distribution. That's still a real (if
simple) comparison against the market's fair-stripped probability — not a
fabricated data source — but it is not the fully-informed fundamentals model
the Python `coral-agents` design describes. A real fundamentals feed is
future work; nothing here pretends otherwise. See the doc comment at the top
of `wager_agent.rs` for the same caveat in the code itself.

**Also not done, scoped out for a reason**:
- **No ledger persistence for wager rulings.** They're emitted on the Coral
  message bus and into the run trace/timeline (visible in existing
  debug panels) but not written to a new SQLite table — adding that means a
  schema migration, which is its own reviewed change, not a tail-end
  addition here.
- **No live Venice test for this specific path.** `wager_agent.rs`'s unit
  tests cover odds extraction and the two skip-paths (incomplete market,
  Venice unconfigured) deterministically; there's no mock-Venice replay
  test for the full reasoning loop here the way `sharp-movement-detector`
  has one. Worth adding using the same `spawn_mock_venice` pattern before
  trusting this on live data.

Verified: `cargo build`/`test` clean for the full workspace including
`native/` (16 tests passing in that crate, up from 12 — the new
`wager_agent` and previously-dead `authority` tests). One pre-existing,
unrelated doctest failure in `native/src/services/user_store.rs` (a
markdown code fence rustdoc tries to compile as Rust) — confirmed via `git
diff` that file was never touched in this session, not something introduced
here.

## Phase 5 — promote `fan-pundit-agent` — DONE (same data-feed caveat as Phase 4)

Landed as `native/src/services/agent/pundit_agent.rs`, wired into
`run_match_intelligence_round` right after the Phase 4 wager step, only
when a wager was actually proposed (a ruling already `NoBet` has nothing to
react to). Same shape as Phase 4: a `submit_pundit_verdict` forced-tool
termination, and the model's stance (`endorse`/`challenge`/`no_bet`) drives
a **fixed**, non-model-controlled nudge magnitude (`CONFIDENCE_NUDGE =
0.03`, mirroring the Python `PUNDIT_CONF_NUDGE`) — the LLM picks the
direction, never the size. The nudged wager goes back through
`authority::adjudicate`, so the Authority re-derives edge/stake from the
nudged probability exactly as it did for the original proposal.

**Same honest limitation as Phase 4, worth repeating rather than
rediscovering**: "give it tools to fetch actual narrative sources (news,
injury reports)" was the original plan, but there is no live news/injury
feed anywhere in this codebase to build that against (verified — grepped
the whole `native/src` tree). So this agent reacts to the *wager's own
thesis and edge*, not external narrative — a real independent second
opinion (a fresh Venice call reasoning adversarially about a sibling
agent's proposal), but not the "reads actual injury reports" ideal. This is
stated in the module's doc comment, not discovered later.

Verified: 2 new unit tests (skip when ruling is already `NoBet`, skip when
Venice unconfigured) pass; full workspace build/test/clippy clean (18 tests
passing in `native/`, up from 16).

## Phase 6 — the actual multi-agent debate — PARTIALLY DONE

**What's actually true today, in the live in-process path**: this is no
longer a one-shot pipeline. `wager_agent.rs` proposes → `authority::
adjudicate` rules → `pundit_agent.rs` receives the *full thesis text*
(reasoning, not just a bare number) and reacts → `authority::adjudicate`
re-rules on the nudged probability. That satisfies criterion 1 from the
"Target architecture" section above (specialists see each other's
reasoning) and criterion 3 (a designated resolver — the Authority — with an
explicit rule: pundit's `no_bet` verdict forces `WagerStatus::NoBet`
regardless of the recomputed edge, logged in `updated.reason` with an
explicit `[pundit override]` tag, never silently).

**What's still missing, and why it wasn't attempted here**: this is still
two Rust async function calls in sequence within one process
(`run_match_intelligence_round`), not two independent agent *processes*
initiating peer-to-peer Coral messages. `fan-pundit-agent` cannot yet talk
*back* to `match-intelligence-agent` for reconsideration — it's a single
one-way reaction, not a bounded back-and-forth. Building the real version
(separate processes, actual `TOOL_CALL` messages on the Coral bus per
`coral-agents/README.md`'s protocol) requires standing up the Docker/
CoralOS runtime described there — infrastructure not available in this
environment to build *and verify* against. Writing that wiring without a
way to run it would be exactly the kind of unverified, speculative change
this roadmap has been avoiding throughout — better to land the honest
in-process version and flag the rest than to fake the harder part.

Original plan, preserved for whoever picks up the remaining piece:

1. **Promote the transport first.** Replace `specialist_ack()` in
   `native/src/services/agent/runtime.rs` with each specialist running as its
   own process (finally giving the Python stubs in `coral-agents/*` real
   logic instead of idling), publishing its own `TOOL_CALL` / `TOOL_RESULT`
   messages on the Coral bus per `coral-agents/README.md`'s existing message
   protocol. This is necessary plumbing but not sufficient — it's still just
   a relay unless step 2 also happens.
2. **Carry rationale, not just numbers, in the payload.** Extend whatever
   struct crosses the wire (today: a bare `Wager`) to include the
   originating agent's tool-call trail summary and thesis text, so the
   receiving specialist has something to actually argue with instead of a
   number to nudge.
3. **Let a specialist talk back.** Give `fan-pundit-agent` (and eventually
   `sharp-movement-detector`) the ability to emit a `TOOL_CALL` *back* to
   `match-intelligence-agent` — "your model assumes X, I have narrative
   evidence against X, reconsider" — instead of only ever replying once to
   an inbound `DELEGATE` with an endorse/challenge verdict. This is the
   actual multi-agent bit: an agent choosing to initiate contact based on
   its own reasoning, not just responding when spoken to.
4. **Bound the debate explicitly.** Add a hard round cap (reuse the
   `StepCounter` / `MAX_STEPS` pattern already in
   `sharp-movement-detector`) and a designated resolver. The Rust Authority
   remains the resolver — it owns Kelly sizing and the proof gate — but give
   it an explicit, inspectable rule for what happens when specialists still
   disagree at the round cap (e.g. "unresolved disagreement above threshold
   X defaults to NO_BET", not silently picking whichever agent replied
   last). Log the full transcript either way; an unresolved debate is a
   findable audit record, not a swallowed error.
5. **Eval this the same way as Phase 3.** Replay historical fixtures through
   the debate and check: does allowing back-and-forth actually change
   outcomes versus the one-shot endorse/challenge version, and are the
   changed outcomes justified by the transcript or is this just adding
   latency and API spend for the same answer? If debate never changes the
   outcome, that's a real finding — it may mean the round cap of 1 (today's
   behavior) was fine and the added complexity isn't earning its keep.

`proof-guard-agent` is excluded from all of this forever — it stays
deterministic Rust, never promoted to LLM reasoning, never a debate
participant, only the final gate.

## Phase 7 — consumer-facing UI/UX overhaul — DONE at the code level (item 3's live wallet round-trip unverified)

Everything above fixes the reasoning layer. Items 1-2 below are now visible
to a user; item 3 is not, for a reason stated up front rather than
discovered mid-implementation.

1. **Wire fixture selection to an actual agent run — DONE.**
   `TrackMode` in `ui/types.ts` used to declare only `'trading'`, a
   pre-existing type-drift bug — the Rust `txodds_types::TrackMode` enum
   always had `Settlement`/`Trading`/`Fan`, so the UI could never have
   requested the other two tracks even if it called `run_agent_round`.
   Fixed the type, added an `AnalyzeControl` (track-mode `<select>` +
   button) to `ui/app/App.tsx`, and gave `useAgentDesk.startRound()` a
   `track` parameter instead of hardcoding `'trading'`. Verified with
   `npm run lint:types` and a full `npm run build` (tsc + vite bundle).
2. **Turn wager reasoning into something legible — DONE, in a narrower form
   than originally planned.** Added `ui/apps/agent/components/WagerPanel.tsx`
   rendering each wager ruling (from Phases 4-5) as a card: selection,
   model vs market probability, edge, stake, thesis, and the Authority's
   reasoning — instead of raw trace JSON. It is **read-only**: no
   Approve/Reject actions, because there is nothing yet for an approval to
   *do* (see item 3). Also added the `Wager`/`WagerRuling` TypeScript types
   (`ui/core/agent/types.ts`) that didn't exist before. Since there's no
   dedicated Tauri command/event for wagers yet, the panel parses them out
   of the existing run-trace payload (`wagerRuling`, added in
   `wager_ruling_payload()` in `runtime.rs` — also fixed there to emit a
   named `{wager, reason}` object instead of a bare 2-element tuple, which
   `serde_json` would have serialized as an ambiguous JSON array).
3. **The wallet-approval-before-settlement flow — built on explicit
   request, code-level verification only.** Initially not attempted for the
   reason above; built after that caveat was read back and the call made to
   proceed anyway. What actually landed, and what "verified" does and
   doesn't mean here:

   - **`create_solana_pay_intent` / `verify_solana_pay_intent` /
     `list_payment_intents` did not exist at all before this** — not merely
     unwired, genuinely missing `#[tauri::command]` functions. The frontend
     called into commands with no backend implementation; that would have
     failed at runtime with "command not found" the moment anyone tried it.
   - Added real logic in `native/src/services/solana_pay.rs`:
     `generate_reference()` (32 CSPRNG bytes, base58-encoded — Solana Pay's
     `reference` account is deliberately off-curve like a PDA, so no ed25519
     keypair needs generating) and `SolanaPayIntent::payment_url()` (builds
     the `solana:` Transfer Request URL per the
     [Solana Pay spec](https://docs.solanapay.com/spec#transfer-request)).
   - Added `native/src/commands/payments.rs` wiring those to the
     already-built (but previously uncalled) `LedgerStore::
     upsert_payment_intent` / `list_payment_intents` /
     `get_payment_intent_by_reference`, plus `chain::rpc::solana_rpc`'s
     already-allowlisted `getSignaturesForAddress` for verification — a
     landed transaction includes the reference as a read-only account, so it
     is discoverable there without needing a new RPC method.
   - Found and fixed two more instances of the same "type declares something
     the backend never produced" bug this session already caught once for
     `TrackMode`: `SolanaPayIntent` in `native/src/services/solana_pay.rs`
     had no `#[serde(rename_all = "camelCase")]` (inconsistent with every
     other frontend contract in this codebase), and `ui/types.ts`'s
     `SolanaPayIntent` declared a `status: 'observed'` value and `url`/
     `message` fields that never matched the Rust enum at all — pure
     invention, presumably written before the backend existed and never
     reconciled.
   - Frontend: `ui/apps/agent/components/WagerPaymentApproval.tsx` — a
     "Request payment" button, the resulting `solana:` URL as an openable
     link, a manual "Check payment status" button, and a *bounded* auto-poll
     (5s interval, capped at 24 attempts / ~2 minutes, never indefinite).
     Wired into `WagerPanel`'s cards, shown only for wagers with a real
     stake (`status !== 'no_bet' && stakeSol > 0`).
   - **What "verified" means here, precisely**: `cargo test` covers
     `generate_reference`/`payment_url` (3 new Rust tests, 21 total in
     `native/` now), `cargo clippy -D warnings` is clean on every new file,
     and `tsc`/`vite build` are clean. **None of that exercises a real
     wallet.** Nobody has opened the generated `solana:` URL in an actual
     Phantom session, signed a transaction, and confirmed
     `verify_solana_pay_intent` correctly flips to `Confirmed` against a
     real devnet signature. The code is real and internally consistent; the
     live round-trip is not proven. Do that before trusting this with real
     stakes.

Still open from the original plan, unattempted for the same reasons as
above or simply not reached:

4. **Make the agent reasoning legible to a non-developer.** The Tool Call
   Audit Log and Agent Trace are currently raw Coral message dumps behind
   `<details>` drawers — fine for debugging, unusable as the "why is this
   wager being proposed" explanation a consumer needs. Once Phase 6 gives
   agents rationale-carrying payloads, render a plain-language summary
   ("Sharp money moved toward Home. Fundamentals model backs Home 54%.
   Fan-Pundit challenged on injury news, model held.") as the primary view,
   with the raw trace demoted to an "advanced" toggle for people who want
   it.
5. **Simplify onboarding proportionally.** The wallet-connect flow (Chrome
   popup fallback, manual pubkey paste, Ed25519 challenge/sign/verify) is
   solid infrastructure but exposes a lot of crypto-native detail up front.
   Once there's an actual product loop (analyze → review → approve →
   settle) worth onboarding someone into, revisit whether every user needs
   to hit that flow immediately or whether a read-only/demo mode can defer
   it.

Acceptance for Phase 7 (revised): a person who is not a developer can open
the app, pick a live fixture, trigger analysis, see an agent-produced wager
proposal with a legible rationale, and request a wallet payment for it —
**true today**, all the way through the code. What's not proven is that a
real wallet completes the round-trip and `verify_solana_pay_intent` sees it
land — that needs a live devnet session with an actual wallet before this
touches real stakes.

## Removing the kill switch — DONE

Per explicit product decision, the kill-switch mechanism has been removed
from the system entirely — not hidden, not relocated to an admin surface,
gone. Landed across the whole stack in one pass:

- **`crates/agent-core/src/safety.rs`**: deleted the `KillSwitch` struct
  entirely; `safety_check()` now takes only `&BudgetGuard`. `BudgetGuard` and
  `StepCounter` are untouched — separate rate/step-limit mechanisms, not the
  kill switch.
- **`crates/agent-core/src/error.rs`** / **`lib.rs`**: removed
  `AgentError::KillSwitchTripped` and the `KillSwitch` re-export.
- **`crates/rig-venice/src/tools.rs`**: deleted the `CheckKillSwitch` tool
  and its tests.
- **All four agent binaries** (`sharp-movement-detector`, `match-intelligence`,
  `contrarian`, `arena-coordinator`) shared the identical pattern — each had
  its own `KillSwitch::new()`, `install_kill_switch_signal()` SIGTERM/SIGINT
  handler, and `safety_check(&kill_switch, &budget)` call. All four stripped
  identically; `install_kill_switch_signal()` deleted from each.
- **`native/src/`**: removed the `trip_kill_switch` command,
  `DesktopState.killed_agents`, and `kill_switch_tripped` from
  `AgentSafetyStatusRow`. (`safety_gate_tripped` turned out to already be
  dead on the backend — nothing ever emitted it; only the frontend listener
  existed.)
- **UI**: removed the Kill button and all confirm/cancel flow from
  `SafetyGateMonitor` and `AgentRosterPanel`, the `'killed'` status variant
  from `AgentRunStatus`, and the now-dead `handleKillSwitch` /
  `tripKillSwitchNative` / `onSafetyGateTripped` plumbing from
  `useAgentDesk.ts` and `transport.ts`.

Verified: `cargo build`/`test` clean across the full workspace *including*
`native/` (the Tauri crate — the heaviest build in the repo), and
`npm run lint:types` (`tsc --noEmit`) clean across the frontend. Left
behind on purpose: a few now-unused CSS classes (`killBtn`, `killConfirm`,
etc. in `ui/styles/agent-track.css`) — cosmetic dead code, not wired to
anything, low priority to chase down.

Note this changes what "stop a runaway agent" means going forward: with no
kill switch, the safety story rests on `BudgetGuard`/`StepCounter` limits,
the deterministic proof gate, and `allow_money_decisions = false` /
`allow_settlement_release = false` — not on a human being able to
interrupt a running agent mid-loop. Worth having eyes open about that
trade-off as Phase 7's approval flow gets built, since "the user approves
before settlement" is now the only human-in-the-loop moment left in the
system.

## Loose ends to resolve along the way

- **Two Venice clients — this one has a decided answer, not just a question.**
  `native/src/services/llm/venice.rs` exists only to produce the
  `AGENT_THOUGHT` narration in round 6 of the Coral transcript
  (`coral-agents/README.md`), and it's invoked by Rust *after* Rust has
  already computed the decision — narrating something the code, not an
  agent, decided. Once Phase 6 lands, `match-intelligence-agent` is its own
  process reasoning through `rig_venice::client()` with tools, and its
  `AGENT_THOUGHT` is naturally the tail end of its own tool-call trace, not
  a second narration pass over Rust's output. At that point
  `native/src/services/llm/venice.rs` has no remaining caller and should be
  deleted, not kept "just in case" — same for `venice_complete()` /
  `narrate_signal()` in `sharp-movement-detector/src/main.rs` once Phase 1
  lands there. Every Venice call in the finished system should go through
  `rig_venice::client()`; if `cargo grep`-ing for `api.venice.ai` after
  Phase 6 finds more than one call site, something didn't get migrated.
- **Model choice — resolved.** `sharp-movement-detector` no longer has its
  own `VENICE_MODEL` default; it goes through `rig_venice::model_name()` like
  the rest of the workspace, so `kimi-k2-7-code` is now the one default in
  play instead of two disagreeing ones.
- **Budget/step accounting.** `BudgetGuard::record_tool_call()` is currently
  called manually at call sites in `detector_step()`. A real tool-calling
  loop will call tools an LLM-decided, possibly-variable number of times per
  cycle — make sure `BudgetGuard` is wired into the `rig` tool `call()`
  implementations themselves (or a wrapping layer), not left as a manual
  bookkeeping step that's easy to forget once the LLM controls call count.
