# Arena Autonomy Plan — Backtest Engine + Autonomous Live Loop

> Written for the TxLINE "Agent vs Agent Arena" hackathon track. Closes the
> two real gaps identified against that track's judging criteria: no
> autonomous trigger (fails "Autonomous Operation") and no full-match replay
> (the brief warns matches will be over by review — a backtest across
> completed matches may be the only track record the demo video can show).
> Everything below is designed to reuse existing deterministic decision code,
> not fork it — see "Shared core" below.

## TxLINE data verification (done — this is not a guess)

Tested directly against the live API with this repo's real `.env` credentials
(fixture 18213979, Norway vs England):

| Endpoint | Purpose | Measured size | Verdict |
|---|---|---|---|
| `GET /api/odds/updates/{fixtureId}` | Full odds tick history, one fixture | **75,540 rows, 27 MB** | Real, complete, tick-by-tick — but too heavy to fetch whole per fixture |
| `GET /api/odds/updates/{epochDay}/{hourOfDay}/1` | Odds ticks, one bounded hour | 2,387 rows, 0.87 MB | The practical fetch shape — walk hour-by-hour |
| `GET /api/scores/updates/{fixtureId}` | Full score/action history, one fixture | 1.25 MB | Fine to fetch whole — score events are far sparser than odds ticks |
| `GET /api/scores/updates/{epochDay}/{hourOfDay}/1` | Scores, one bounded hour | 24 KB, 48 rows | Also fine bounded, not required |

**Conclusion:** TxLINE has exactly the data a backtest needs — a genuine
tick-by-tick odds time series spanning pre-kickoff (`InRunning:false`) through
full time and extra time (`InRunning:true`, `MarketPeriod:"et"`) — but the
whole-fixture odds endpoint is 27 MB for one match. Backtesting must walk
`txline_odds_interval` hour-by-hour, not call `txline_odds_updates` per
fixture. Across 104 World Cup matches this is the difference between ~30 MB
and several GB of pulls.

## Shared core: one decision path, three callers

Today `run_match_intelligence_round` (`native/src/services/agent/runtime/mod.rs`)
is the only entrypoint for a round, triggered per-event by the chat's
"Analyze" action or a manual command. Both new systems below call the *same*
underlying deterministic pipeline instead of forking it:

```
                    ┌─────────────────────────────┐
   TxLineEvent  ──► │ signal::build_signal          │
   (odds+score)     │ features:: / policy::         │  ← agent-core, deterministic,
                    │ arena position + settlement    │    already exists, untouched
                    └─────────────────────────────┘
                              ▲        ▲        ▲
                              │        │        │
                    chat "Analyze"  autonomous   backtest
                    (existing)       poll loop    replay engine
                                     (new)        (new)
```

- **Chat-triggered** (existing): one `TxLineEvent` in, human clicked.
- **Autonomous loop** (new, Priority A below): one `TxLineEvent` in per
  detected live movement, no human involved.
- **Backtest replay** (new, Priority B below): many `TxLineEvent`s in,
  constructed from historical odds/score ticks instead of a live snapshot.

None of these forks `agent-core`'s signal/policy/arena logic — they just feed
it different `TxLineEvent`s. This matters for "Logic & Code Architecture"
(one decision path, not three) and for correctness (a backtest that used
different logic than the live system wouldn't actually validate the live
system's strategy).

## Priority A — Autonomous live loop

**What's missing today:** nothing calls `run_match_intelligence_round`
without a human clicking something. That's a straight fail on "Autonomous
Operation" as a judging criterion, independent of how good the decisions are.

**Design:**

1. A new background Tokio task, spawned in `native/src/lib.rs`'s
   `app.setup()` alongside the existing `spawn_loopback` pattern — call it
   `autonomous_loop::spawn(app, state)`.
2. Every `AUTONOMOUS_POLL_SECS` (default 60s, matching the brief's own
   "Sharp Movement Detector... every 60 seconds" framing — config-driven, not
   hardcoded):
   - Fetch today's live fixtures (`txline_fixtures_snapshot`, already
     wrapped by `loadLiveFixtures`'s Rust-side twin).
   - For each fixture with `status` indicating live/in-play, fetch its
     current odds snapshot and diff against the last-seen snapshot (kept in
     a small in-memory `HashMap<fixture_id, LastSeenOdds>` in `DesktopState`,
     mirroring what `sharp-movement-detector`'s own poll loop already does
     for its standalone binary — same pattern, now inside the desktop app).
   - If the diff crosses `odds_move_trigger_pct` (existing config, already
     used to gate signals), build a `TxLineEvent` and call
     `run_match_intelligence_round` — exactly the function chat already
     calls, so this produces the same coral messages, trace events, and
     round result the chat UI already knows how to render. Nothing new to
     build in the frontend for this to be visible.
3. **Safety, not a new invention — reuse what exists:** `BudgetGuard`'s
   `max_tool_calls`/`max_spend_lamports`/`max_duration_secs` already bound a
   session; the autonomous loop must construct each round's context the same
   way the chat path does so those caps apply identically. Additionally cap
   *rounds per fixture per hour* (a new, simple in-memory rate limit) so a
   choppy market doesn't spawn a round every poll cycle — once per
   detected-and-actioned movement is the intent, not once per tick.
4. **Start/stop control:** a `set_autonomous_loop_enabled(bool)` Tauri
   command + a toggle in the chat header, defaulting **on** — a judge should
   be able to open the app and watch it work without touching anything, per
   "Autonomous Operation" and the demo video requirement ("show it working").
5. **CoralOS visibility:** reuses the persistent-session work already
   built this session (one CoralOS session per app launch) — autonomous
   rounds accumulate into the same session/thread history a judge can watch
   live in the Console, not a separate invisible process.

**What this does NOT change:** still no LLM decision authority — the
autonomous trigger only decides *when* to run the existing deterministic
pipeline, exactly like a human clicking Analyze decides *when* today.

## Priority B — Backtest replay engine

**Why this one might matter more for the actual submission:** the brief
states matches will have ended by review and "there may not be live activity
on the project during review." If judges can't watch it live, a backtest
across completed matches — with the same on-chain-anchored TxLINE data,
producing the same ArenaScore/leaderboard the live system would — is the
demonstrable track record the demo video needs.

**Design:**

1. New module `native/src/services/backtest.rs`, one function:
   `replay_fixture(fixture_id: u64) -> Result<BacktestSummary, AppError>`.
2. **Data fetch** (per the verified sizes above):
   - `txline_scores_updates(fixture_id)` once — 1.25 MB, gives the full score
     timeline including final result.
   - `txline_odds_interval(epoch_day, hour, 1)` looped from
     `kickoff_hour - 1` through `kickoff_hour + 3` (covers pre-match line
     movement through full time + stoppage; extend for matches that go to
     extra time/penalties, detectable from the score timeline's `GameState`).
   - Merge, sort by `Ts`, filter to the fixture's own `FixtureId` and the
     1X2 market (reuse `parseOddsSnapshot`'s market-row parsing logic,
     ported to Rust or called via the same normalization rules — don't
     re-derive the milli-odds/`part1`→`home` aliasing a second time).
3. **Replay:** walk the merged, sorted tick list. At each *change* in the 1X2
   market (not every raw tick — most of the 75K rows are other markets or
   duplicate re-sends), construct a `TxLineEvent` exactly like a live
   `odds_update` event and feed it through the same `agent-core` signal/
   policy path used everywhere else. When a sharp move is detected, both
   `FollowSharp` and `FadeSharp` take a simulated position at that point in
   time, same as the live arena does.
4. **Settlement:** once the replay reaches the fixture's final whistle (from
   the score timeline, already fetched), settle every open simulated
   position against the real final score — reuse `arena-coordinator`'s
   existing settlement/PnL computation, don't reimplement it.
5. **No LLM calls during replay.** The core decision path is already pure
   deterministic Rust; only the narration step calls Venice, and narrating
   possibly thousands of historical ticks per match would be slow and
   expensive for zero decision value. Backtest output gets a short *single*
   post-hoc narration ("how did Follow vs Fade do on this match") if you
   want prose for the demo, generated once per fixture, not once per tick.
6. **Integrity — tag backtest output distinctly.** This is the one point
   worth being deliberate about: backtest-derived `ArenaPosition`/
   `SettlementRecord` rows must carry a `source: "backtest"` field (new
   column) so the leaderboard and chat can either merge or clearly separate
   "replayed against N completed matches" from "live tournament record."
   Presenting backtest PnL as if it were live-tournament PnL would be exactly
   the kind of overclaim the brief's "clear logic and working system beats
   polish" standard is warning against — and it's an easy, one-column fix to
   avoid it entirely.
7. **Chat surface:** a "Backtest {home} vs {away}" quick action / typed
   command, reusing the existing round-card rendering
   (`ui/app/components/ChatMessage.tsx`'s `RoundCard`) with a `(backtest)`
   badge — no new UI component needed, one prop threaded through.
8. **Scale:** running this across all 104 fixtures for the demo is a batch
   job (`just backtest-all-fixtures` or similar), not something the chat UI
   needs to trigger one-by-one — but the single-fixture path is the same
   code either way.

## How this maps to the judging criteria

| Criterion | Addressed by |
|---|---|
| Core Functionality & Data Ingestion | Both already-verified against live TxLINE data (Priority A live, Priority B historical) |
| Autonomous Operation | Priority A — the actual gap this closes |
| Logic & Code Architecture | Shared core: one deterministic decision path, three callers, not three implementations |
| Innovation & Novelty | Follow-vs-Fade cumulative tournament scoring is already a real research question; backtesting it across the whole World Cup is the novel evidence for it |
| Production Readiness | Rate-limited autonomous loop + bounded/interval-based data fetching (not naive 27 MB pulls) — both are the kind of detail a "professional trading team" would actually require |

## Suggested build order

1. **Backtest replay (Priority B)** first — it's what makes the demo video
   credible once matches are over, and it's pure backend work with no new
   safety-gate design questions (no LLM in the loop, bounded by definition —
   it terminates when the historical data runs out).
2. **Autonomous loop (Priority A)** second — needs one new design decision
   (per-fixture rate limiting) the backtest doesn't, and its value is best
   demonstrated live during the remaining hackathon window, so there's less
   urgency than the backtest's "demo insurance" role.
3. Do **not** build a from-scratch third decision engine for either — if a
   step doesn't reduce to "construct a `TxLineEvent`, call the existing
   round function," that's a signal the design has drifted from "shared
   core."
