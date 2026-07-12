# Loose Ends — Plan & Status

> Plan for the remaining `TODO.md` Loose Ends, written 2026-07-11 after Phase 6
> completed. Each item is classified **executable now** (done in this pass) or
> **blocked** (needs infrastructure, keys, or a data-source decision that code
> alone can't supply — documented with a concrete design instead of a fake).

---

## Executable now

### 1. CI pipeline ✅
`.github/workflows/ci.yml`, two jobs:

- **workspace** (fast, no system deps): `cargo build --bins` + `cargo test`
  for every crate except the Tauri app — this covers all nine agent binaries
  (`proof-guard-agent`, `settlement-agent`, `trading-specialist`,
  `fan-pundit-agent`, `contrarian`, `arena-coordinator`,
  `sharp-movement-detector`, `match-intelligence`, `idle-agent`) plus
  `coral-client`, `agent-core`, `rig-venice`, `txodds-types`, `coral-e2e`.
- **native** (Tauri): installs the Linux WebKit/GTK build deps, then
  `cargo test -p txodds-agent-desk`.

The e2e harness (below) is *not* run in CI yet — it needs Docker +
the coral-server image; wire it into a separate workflow with a Docker
runner when CI hardware is decided.

### 2. Docker Compose integration test ✅ (harness) / runtime verification ⚠ (see result note)
New crate **`crates/coral-e2e`** — a standalone binary, not a `cargo test`
target, so builds stay hermetic. It drives the exact HTTP surface `native/`
uses (mirroring `console.rs` / `thread_reader.rs`, verified against the same
live-server behavior):

1. `POST /api/v1/local/session` — agent graph with all six CoralOS
   participants (`runtime: docker`), which makes coral-server spawn the real
   agent containers;
2. `POST /api/v1/puppet/{ns}/{session}/match-intelligence-agent/thread`;
3. puppet-POST `WAGER_PROOF_REQUESTED round=1 wagerId=w-e2e-1 wager=<json>`
   mentioning `proof-guard-agent`;
4. poll `GET /api/v1/local/session/{ns}/{session}/extended` until a
   `WAGER_PROOF_VERDICT … wagerId=w-e2e-1` arrives from `proof-guard-agent`
   (fail-closed timeout, default 180 s to allow first-time container spawns);
5. assert `passed=true` and that the round-tripped wager parses; exit 0/1.

Run book:
```
docker build -f crates/agents/proof-guard-agent/Dockerfile  -t proof-guard-agent:0.1.0  .
docker build -f crates/agents/settlement-agent/Dockerfile   -t settlement-agent:0.1.0   .
docker build -f crates/agents/trading-specialist/Dockerfile -t trading-specialist:0.1.0 .
docker build -f crates/agents/fan-pundit-agent/Dockerfile   -t fan-pundit-agent:0.1.0   .
docker build -f crates/agents/idle-agent/Dockerfile         -t idle-agent:0.1.0         .
docker compose -f docker-compose.coralos.yml up -d
cargo run -p coral-e2e     # CORALOS_SERVER_URL / CORAL_TOKEN / CORALOS_NAMESPACE / E2E_TIMEOUT_SECS to override
```

This doubles as the **CoralOS/Docker runtime verification** item: a green run
is proof-guard-agent verified against a real coral-server. Result of the
in-session run is recorded at the bottom of this file.

### 3. Remaining Python decision-logic ports ✅ (audited; one real gap found and ported)
Audit of the deleted Python (`git show HEAD:coral-agents/...`) against the
current Rust:

| Python | Rust home | Status |
|---|---|---|
| `coral_agent/wager.py` (Wager, Kelly, implied prob) | `txodds_types::wager` | ported earlier, field-for-field |
| `coral_agent/framework.py` (Specialist ABC, serve loop) | `crates/coral-client` | ported earlier |
| `proof-guard-agent/agent.py` (5-point verify) | `agent_core::proof_guard` | ported earlier |
| `sharp-movement-detector/agent.py` (fundamentals softmax) | `agent_core::fundamentals` | ported earlier |
| `fan-pundit-agent/agent.py` (stance + nudge) | `pundit_agent.rs` + fan-pundit binary | ported earlier |
| `settlement-agent/agent.py` (proof-gated ack) | settlement-agent binary | ported earlier |
| `match-intelligence-agent/agent.py` (orchestration) | `native/.../runtime/` | superseded (Rust exceeds it) |
| **Debate transcript** — every specialist appended a `DebateContribution` to `wager.debate` | **nowhere** — `debate: None` at every construction site | **the one real gap → ported in this pass** |

The TODO's "7 of 9 stubs remain" was stale bookkeeping: the stubs' decision
logic had already been ported or superseded piecemeal; only the debate
transcript was genuinely missing.

---

## Blocked (design documented, no fake implementations)

### 4. Live fundamentals feed — blocked on a data source
`agent_core::fundamentals` runs on neutral defaults because nothing in this
codebase supplies form/xG/rank/injuries/h2h. TxLINE (the only wired feed)
carries odds and score events. Options, in order of preference:

1. **TxLINE stats expansion** — if the TxODDS account gains access to a
   pre-match stats product, add a `txline::fundamentals` client next to the
   existing odds client; `wager_agent.rs` then passes real inputs instead of
   the all-neutral flag in its prompt.
2. **Third-party API** (football-data.org, API-Football, Understat scrape) —
   needs a key decision and licensing review; wire as a new
   `crates/fundamentals-feed` with the same shape.
3. Keep the honest baseline (current state).

Wiring is mechanical once a source exists: `ComputeModelProbability` already
accepts the full input vector; only the fetch is missing. **Decision needed
from the repo owner — not codeable today.**

### 5. Live news/narrative feed for fan-pundit — blocked, same shape
Same as (4): the pundit reasons only over the wager's own thesis
(`pundit_agent.rs` honest-limitation note). Candidate sources (news API,
X/Twitter search, club RSS) all need account/key decisions. Once chosen, feed
snippets into the existing prompt behind `wrap_untrusted()` — the loop and
tool contract don't change.

### 6. Real Solana on-chain settlement — blocked on keys/program decisions
Current: settlement-agent emits deterministic `devnet:settle:` pseudo-sigs.
A real implementation needs three decisions before code:

1. **What moves** — real escrow program (the `escrow_pda`/`deposit_tx`/
   `release_tx` fields in `SettlementReceipt` anticipate one) vs. a plain
   devnet `SystemProgram::transfer` as a first honest step.
2. **Who signs** — settlement-agent holds `SettleCap` in-process, but the
   design docs say the *user's* Phantom signature advances
   `ProofPassed → Signed`; an agent-held keypair would change the trust
   model. Likely split: user signs deposit in the desktop app (wallet flow
   already exists in `ui/core/wallet`), agent only *verifies* and records.
3. **Key management** — if the agent signs anything, the key must come from
   env/secret-store injection via `coral-agent.toml` options, never the repo.

Suggested first increment (small, honest): settlement-agent gains an
env-gated `SETTLE_MODE=devnet-transfer` path that submits a self-transfer
memo transaction recording the wager id, returning the *real* signature —
proves the RPC path end-to-end without inventing an escrow program. Defer
until (1)–(3) are decided.

---

## In-session verification result

Filled in after the compose run below — see the end-of-pass summary in the
conversation / commit message.
