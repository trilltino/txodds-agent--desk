# AutoAgents + Ractor — Scope Assessment

> Question: is there scope in this repo for [AutoAgents](https://github.com/liquidos-ai/AutoAgents)
> (Rust multi-agent framework built on the [Ractor](https://github.com/slawlor/ractor)
> Erlang/OTP-style actor model)?
> Assessed 2026-07-11 against the stack as of `TODO.md` (Steps 0–5 complete, Phase 6 in flight).

---

## TL;DR

**No scope for AutoAgents as a framework.** It duplicates the two layers this repo has
already built and tested (Rig for the agent/LLM layer, coral-server for multi-agent
coordination), and its in-process actor coordination is architecturally opposed to the
repo's core design decision: each specialist is a **genuinely independent OS process**
registered with coral-server, so it can be launched, monitored, and marketplace-listed
by CoralOS.

**Narrow, optional scope for Ractor alone** in two places (hybrid poll+mention agents,
orchestrator fan-out) — but plain tokio tasks/channels cover both at current scale.
Concrete revisit triggers are listed at the bottom.

---

## 1. What we run today

The stack the question describes is already the stack we have:

```
Rig agent loop (crates/rig-venice, rig-core 0.9 → Venice AI, OpenAI-compat)
        ↓
rmcp 0.9 (crates/coral-client — MCP client, Specialist trait)
        ↓
coral-server (MCP orchestration: threads, mentions, wait_for_mention)
        ↓
CoralOS (marketplace, monitoring, docker-compose.coralos.yml runtime)
```

| Layer | Where | Status |
|---|---|---|
| LLM client + tool-calling loop | `crates/rig-venice` (`client()`, `loop_runner`, `tools`) | Built, used by 6 agent crates |
| Agent decision logic (deterministic) | `crates/agent-core` (`proof_guard`, `fundamentals`) | Ported from Python, tested |
| MCP participant runtime | `crates/coral-client` (`Specialist` trait, `run()` connect → `wait_for_mention` → `handle` → `send_message`) | Verified against a live coral-server |
| Specialist processes | `crates/agents/*` — proof-guard, fan-pundit, settlement (Specialist-based); sharp-movement-detector, contrarian, arena-coordinator (standalone poll loops) | 3 converted, 3 pending (TODO 6c) |
| Native orchestrator | `native/src/services/agent/runtime/` (`run_match_intelligence_round`, per-specialist delegation modules) | proof-guard delegation real; rest still `specialist_ack()` (TODO 6d) |

Two properties of this stack matter for the assessment:

1. **The actor model is already here — at the process level.** Each specialist is a
   single-threaded mailbox: it blocks on `wait_for_mention`, handles one message, replies.
   Coral threads are the mailboxes; coral-server is the router; Docker restart policies
   are the supervisor. That *is* Erlang/OTP semantics, with the process boundary where
   OTP puts it (isolated heap, crash = restart, no shared state).
2. **Process isolation is load-bearing, not incidental.** `crates/coral-client/src/lib.rs`
   states it directly: what makes a participant real is that "coral-server launches it,
   it blocks on its own network call waiting for work, and only the thread carries state
   between it and everyone else." CoralOS registration (`coral-agent.toml`, edition 4),
   marketplace listing, and per-agent monitoring all attach to a *process/container*,
   not to an in-process actor.

## 2. What AutoAgents provides, mapped against that

AutoAgents (LiquidOS AI) is a full-stack agent framework: typed agent model, ReAct-style
executors, derive-macro tools, configurable memory, pluggable LLM backends, and
multi-agent coordination built on Ractor actors — with cloud/edge/WASM deployment targets.

| AutoAgents feature | Already covered by | Adopting it would mean |
|---|---|---|
| LLM backends (OpenAI-compat, etc.) | `rig-venice::client()` → Venice | Migrating 6 crates off rig-core for zero new capability |
| Tool calling / executors | `rig-venice::loop_runner` + `tools` (incl. `wrap_untrusted` prompt-injection defence, deterministic tool results gating side effects) | Re-implementing the safety patterns (§28) inside a new framework |
| Memory | `agent-core` state + JSONL signal logs + Coral thread history | Duplicate state stores |
| Multi-agent coordination (Ractor, in-process) | coral-server threads/mentions (cross-process) | **The conflict — see below** |
| Supervision/restart | Docker restart policies + coral-server agent lifecycle | Second, weaker supervisor inside the process |

The coordination row is the disqualifier, not just redundancy. AutoAgents coordinates
agents as Ractor actors **inside one process**. Fold the specialists back into one
process and they stop being CoralOS participants: coral-server can't launch them,
CoralOS can't list or monitor them individually, and the Coral transcript (the product's
audit trail — TODO 6e wants it *richer*, with tool trails) loses its authors. That would
undo Phase 6, which exists precisely to break agents *out* into their own processes —
`TODO.md` calls the remaining work "each specialist as its own process."

AutoAgents competes with the Coral stack; it doesn't slot into it. The one place a
framework could slot in — the per-agent brain between `Specialist::handle()` and the
LLM — is exactly where Rig already sits.

## 3. Where Ractor *alone* could genuinely fit

Dropping the framework question and keeping just the actor library, there are two real
candidates, both already flagged as open design questions in `TODO.md`:

### 3a. Hybrid agents (TODO 6c) — the strongest case

sharp-movement-detector, contrarian, and arena-coordinator are self-contained poll loops
(TxLINE polling + Venice tool-calling + JSONL logs, with `prev_odds` state carried across
polls). TODO 6c's option (b) — "runs its poll loop but *publishes* signals back to the
Coral bus" — means one process running **two long-lived concurrent loops sharing state**:

- a poll-loop actor (owns `prev_odds`, `BudgetGuard`/`StepCounter`, signal log), and
- a Coral mention-servicing actor (the existing `coral_client::run` loop),

plus restart-with-state-reset if one side dies. That is a textbook two-actor supervision
tree, and it's the one spot where Ractor buys something tokio doesn't give for free:
named actors, supervised restarts, and a mailbox instead of hand-rolled
`mpsc` + `select!` + reconnect logic.

Counterpoint: two tokio tasks and a `watch`/`mpsc` channel also solve it, the crash story
is already handled one level up (process dies → Docker restarts it → idempotency keys
make crash-and-restart safe, §14), and it would introduce a second concurrency idiom into
a codebase that is uniformly plain-tokio today.

### 3b. Orchestrator fan-out (TODO 6d) — weaker case

When `specialist_ack()` is replaced by real delegation for all specialists
(`native/src/services/agent/runtime/`), delegations could go concurrent: fan out
`VERB_REQUESTED` to several specialists, gather `VERB_VERDICT`s with per-specialist
timeouts, fail closed on stragglers. An actor per in-flight delegation models this — but
`tokio::join!`/`select!` with the existing `wager_proof_delegation` timeout pattern does
too, and the round is fundamentally a *sequence* (proof-gate before settlement), not a
free fan-out. Not worth a dependency on its own.

### What Ractor does **not** buy here

OTP-grade fault tolerance. A Rust panic with `panic = "abort"` (set in the workspace
`[profile.release]`) kills the whole process, actors and supervisor included. The only
supervision that survives that is the one we already have: Docker + coral-server.

## 4. Recommendation

1. **Do not adopt AutoAgents.** The Rig → rmcp → coral-server → CoralOS stack is the
   deliberate architecture; AutoAgents is an alternative to it, not an addition. The
   migration cost lands on working, tested code and the destination removes the
   process-level properties CoralOS needs.
2. **Do not add Ractor yet.** Finish Phase 6 on plain tokio. The supervision story is
   already correct at the process level, and every remaining TODO item (6a–6e) is
   expressible with the existing patterns.
3. **Revisit Ractor if any of these become true:**
   - A hybrid agent (6c) ends up with **3+ independent long-lived loops** in one process
     (poll + mention-service + prediction-tracker + …) and hand-rolled channel wiring is
     visibly buying bugs;
   - The orchestrator moves to genuinely concurrent multi-specialist rounds with
     per-delegation lifecycles (spawn/cancel/retry) that outgrow `select!`;
   - A future in-process subsystem needs *partial* restart (one loop restarts while its
     siblings keep state) — the one thing process-level supervision can't do.

   If a trigger fires, adopt `ractor` the library (small, no framework lock-in), not
   AutoAgents the framework, and confine it to the binary that needs it.

---

## Sources

- [AutoAgents — GitHub (liquidos-ai)](https://github.com/liquidos-ai/AutoAgents)
- [AutoAgents documentation](https://liquidos-ai.github.io/AutoAgents/)
- [autoagents-core on lib.rs](https://lib.rs/crates/autoagents-core)
- [LiquidOS — Open-Source Agent SDK in Rust](https://liquidos.ai/)
- [Rust-Native AI Agent Frameworks in 2026 — Zylos Research](https://zylos.ai/research/2026-04-01-rust-native-ai-agent-frameworks-ecosystem-2026/)

Internal cross-references: `TODO.md` (Phase 6, 6c/6d/6e), `crates/rig-venice/ROADMAP.md`
(Phase 6 L384-420), `crates/coral-client/src/lib.rs` (module docs on process isolation),
`native/src/services/agent/runtime/handoff.rs` (`specialist_ack` replacement target).
