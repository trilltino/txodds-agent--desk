# Closing the E2E-Agentic Gaps

> Plan for the four honest gaps identified in the "is this genuinely agentic"
> review: no autonomous trigger, a silently-dead LLM credential path, an
> unlabeled simulation dressed as a contest, and a version-control hygiene
> miss. Ordered by how concrete/low-risk the fix is, not by how the earlier
> review presented them.

## 1. fan-pundit-agent's dead Venice key (do this first — smallest, highest-confidence fix)

**The bug, precisely:** `crates/agents/fan-pundit-agent/coral-agent.toml` declares
`VENICE_API_KEY = { type = "string", default = "" }`, and
`native/src/services/coralos/console.rs::agent_graph_entry` — the function
that builds every Docker-spawned participant's options — never sets it. So
the container gets `VENICE_API_KEY=""`. That's not even a clean failure:
`std::env::var("VENICE_API_KEY")` returns `Ok("")` for an empty value, not an
`Err`, so `rig_venice::client()` happily builds an `openai::Client` with an
empty key. The HTTP call to Venice fails downstream (401), `run_tool_loop`
returns `Err`, and `run_venice_verdict` catches it with its own designed
fallback: `"Venice reasoning failed; defaulting to endorse"`. The failure
mode was built deliberately (fail-open to a neutral stance, not fail-closed)
— it just means nobody would ever see this from the outside. The pundit has
plausibly been narrating nothing this whole time.

**The fix** — mirror exactly how `DESK_API_BASE`/`DESK_API_TOKEN` already get
injected for this same agent, four lines above where those are set:

```rust
// native/src/services/coralos/console.rs, inside agent_graph_entry
if name == FAN_PUNDIT_AGENT {
    if let Some(venice_key) = &config.venice_api_key {
        options.insert(
            "VENICE_API_KEY".to_string(),
            json!({ "type": "string", "value": venice_key }),
        );
    }
    if config.axum_enabled {
        // ... existing DESK_API_BASE / DESK_API_TOKEN block, unchanged
    }
}
```

`config.venice_api_key: Option<String>` already exists (`native/src/config.rs`,
read from `VENICE_API_KEY`/keyring exactly like every other secret in this
file) — this is wiring, not new plumbing.

**Verification, not just compilation:** the existing coral-server log
technique from this session works here directly — start a round, `docker
logs` (or watch `coralos-server`'s logs) for the `fan-pundit-agent`
container's own stdout. Its `tracing::warn!("fan-pundit-agent: Venice not
configured; defaulting to endorse")` line either appears (still broken) or
doesn't (fixed) — that's an observable, not an inferred, confirmation.
Better: temporarily add a one-line `tracing::info!` right after
`rig_venice::client()` succeeds, confirming a real client was built, then
remove it once confirmed once.

## 2. No autonomous trigger (Priority A from ARENA-AUTONOMY-PLAN.md — build second)

Already designed in ARENA-AUTONOMY-PLAN.md; restating the shape here because
it's the literal answer to "not agentic yet":

- New background task in `native/src/lib.rs`'s `app.setup()`, polling live
  fixtures every `AUTONOMOUS_POLL_SECS` (config, default 60s) and calling
  the *same* `run_match_intelligence_round` the chat's Analyze button calls
  — no forked decision path.
- Diff each fixture's odds against a `HashMap<fixture_id, LastSeenOdds>` kept
  in `DesktopState`; only trigger a round when the diff crosses
  `odds_move_trigger_pct`.
- Per-fixture rate limit (e.g. one triggered round per fixture per hour) so
  a choppy market doesn't spawn a round every poll cycle.
- A `set_autonomous_loop_enabled(bool)` Tauri command + chat-header toggle,
  **defaulting on** — the whole point is a judge (or you) can open the app
  and watch it act without touching anything.
- Reuses the CoralOS session-persistence work already shipped this session,
  so autonomous rounds show up in the same long-lived Console session as
  everything else.

**What "done" looks like, concretely:** open the app, do nothing, and watch
a `round` card appear in chat on its own within a poll cycle after a real
fixture's odds move — no click, no typed message. That observable is the
actual bar for "Autonomous Operation," not the presence of the code.

## 3. The arena is a scored simulation — decide the label, don't add real stakes

This isn't a bug to fix; it's a claim to stop making by accident. Two honest
options, and only one of them is compatible with the hackathon's own rules
(it explicitly disclaims "illegal betting, wagering, or financial activity"
and pushes compliance responsibility onto participants):

- **Keep it simulated, say so everywhere it's visible (recommended).** The
  backtest card already does this (dashed border, "backtest" badge,
  explicit disclaimer line). Do the same for the *live* arena positions in
  the UI — a small, persistent "simulated position — no funds at risk" note
  wherever `ArenaPosition`/`ArenaScore` render, matching
  `trading-specialist`'s own code comment ("advisory only — no real funds
  move here"). Cheap, removes any ambiguity, and is the only option that
  doesn't touch the money-movement/regulatory surface the hackathon brief
  flags.
- **Add real stakes.** Would mean real Solana positions settling for real
  value based on match outcomes — i.e. actual wagering. Not recommending
  this; flagging only so it's clear it was considered and deliberately
  rejected, not overlooked. If ever revisited, it's a legal/compliance
  decision before it's an engineering one, not something to back into via a
  feature request.

Net effect either way: don't let "Agent vs Agent Arena" copy (chat welcome
message, README, submission writeup) describe this as a contest with stakes
without the UI itself saying otherwise in the same breath.

## 4. Version-control hygiene (quick, do alongside #1)

`AppPicker.tsx` existed on disk, functioning, for an unknown span of this
repo's history without ever being `git add`-ed — confirmed via
`git log --all --follow` returning nothing for it. Current tree is clean
(`git status` shows no untracked files right now), so this isn't an active
problem, but the fact it happened once means it's worth a cheap, permanent
guard rather than trusting it won't happen again:

- Add a `just check` step (or extend the existing one) that runs
  `git status --porcelain --untracked-files=all` and fails/warns if it's
  non-empty on a build/release path — catches "file exists, works, never
  committed" before it becomes a silent gap discovered months later.
- No code archaeology needed beyond what's already been done — the working
  tree is clean as of this plan.

## Suggested order

1. **#1 (Venice key)** — smallest diff, directly restores a component that's
   plausibly been silently non-functional; verify via container logs before
   moving on.
2. **#4 (git hygiene guard)** — five minutes, prevents a repeat of exactly
   what just happened with `AppPicker.tsx`.
3. **#3 (label the simulation)** — small UI/copy change, closes the
   overclaiming risk cheaply.
4. **#2 (autonomous loop)** — the real build; do it last because it's the
   only one that's genuinely new infrastructure rather than a fix, and its
   design (rate limiting, toggle, background task) deserves the focus the
   first three shouldn't be competing with.
