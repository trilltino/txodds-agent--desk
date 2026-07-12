import { useEffect, useMemo, useRef, useState } from 'react'
import type {
  AgentRun,
  AgentTraceEvent,
  CoralAgentManifest,
  CoralMessage,
  Fixture,
  TrackMode,
  TxLineEvent,
  TxLineProofReceipt,
} from '../../types'
import { describeArenaScore, toMs, type ChatItem } from '../../core/chat/types'
import { parseAsOf } from '../../core/chat/time'
import type {
  AgentLeaderboardEntry,
  AgentRosterEntry,
  AgentSafetyStatus,
  ArenaPosition,
  ArenaScore,
  BacktestSummary,
  CapabilityKind,
  SettlementRecord,
  SignalRecord,
  ToolCallRecord,
} from '../../core/agent/types'
import { epochDayNow, loadFixtureEvent, loadLiveFixtures } from '../../core/txline/fixtures'
import { loadCoralAgents } from '../../core/coral/agents'
import {
  getArenaScoreNative,
  listAgentLeaderboardNative,
  listAgentTraceNative,
  listArenaPositionsNative,
  listCoralMessagesNative,
  listRunsNative,
  listSettlementRecordsNative,
  listSignalRecordsNative,
  listToolCallRecordsNative,
  onAgentTrace,
  onArenaPosition,
  onArenaScore,
  onCoralMessage,
  onProofReceipt,
  onSettlementRecord,
  onSignalRecord,
  onToolCallRecord,
  runAgentRoundNative,
  runBacktestNative,
} from '../../desktop/transport'

// Capability token assignment for the four known sidecar agents.
const AGENT_CAPABILITIES: Record<string, CapabilityKind> = {
  'match-intelligence': 'FollowCap',
  'contrarian': 'FadeCap',
  'arena-coordinator': 'SettleCap',
  'sharp-movement-detector': 'DetectCap',
}

// Only SIGNAL coral messages surface as chat bubbles. Everything else —
// lifecycle verbs (OBSERVED, WANT, PROOF_*, EVALUATED, …), tool calls, and
// raw feature dumps like "severity=0.35 actionability=0.00" — is agent
// internals: it stays in the persisted coral transcript, while the chat gets
// the synthesized plain-language turns from sendChat instead.
const CHAT_VERBS = new Set<string>(['SIGNAL'])

const DAY_MS = 86_400_000

/** Human label for an epoch day: "Today", "Yesterday", or a short date. */
export function epochDayLabel(day: number): string {
  const today = epochDayNow()
  if (day === today) return 'Today'
  if (day === today - 1) return 'Yesterday'
  if (day === today + 1) return 'Tomorrow'
  return new Date(day * DAY_MS).toLocaleDateString(undefined, {
    timeZone: 'UTC',
    weekday: 'short',
    month: 'short',
    day: 'numeric',
  })
}

export interface AgentDeskState {
  selectedEvent: TxLineEvent | undefined
  fixtures: Fixture[]
  fixturesLoading: boolean
  fixturesError: string | undefined
  selectedFixtureId: number | undefined
  /** Epoch day the fixture board is showing (defaults to today). */
  fixturesDay: number
  /** True when the board shows a past day — analyses use that day's snapshots. */
  fixturesDayHistorical: boolean
  runs: AgentRun[]
  agents: CoralAgentManifest[]
  agentTrace: AgentTraceEvent[]
  proofReceipts: TxLineProofReceipt[]
  // Agent power state
  arenaPositions: ArenaPosition[]
  settlementRecords: SettlementRecord[]
  arenaScore: ArenaScore | undefined
  signalRecords: SignalRecord[]
  safetyStatuses: AgentSafetyStatus[]
  leaderboard: AgentLeaderboardEntry[]
  toolCallRecords: ToolCallRecord[]
  // Chat state
  chatItems: ChatItem[]
  chatBusy: boolean
  /** Latest trace summary while a round is in flight — drives the typing indicator label. */
  chatBusyLabel: string | undefined
  // Derived
  currentRun: AgentRun | undefined
  currentProof: TxLineProofReceipt | undefined
  currentRunTrace: AgentTraceEvent[]
  agentRoster: AgentRosterEntry[]
  // Actions
  refreshFixtures: () => Promise<void>
  /** Move the fixture board to another epoch day (clears the selection). */
  changeFixturesDay: (day: number) => Promise<void>
  selectFixture: (fixture: Fixture) => Promise<TxLineEvent | undefined>
  startRound: (track?: TrackMode, event?: TxLineEvent) => Promise<AgentRun | undefined>
  sendChat: (text: string) => Promise<void>
}

/**
 * useAgentDesk — orchestrates all backend subscriptions and shared state for
 * the Agent Desk webview. Returns a single typed bundle so consumers can
 * destructure exactly the slices they need.
 *
 * The hook is intentionally side-effect-only at the module boundary: all Tauri
 * IPC calls go through `desktop/transport.ts`; no fetch/XHR inside here.
 */
export function useAgentDesk(): AgentDeskState {
  // ── Shared state ─────────────────────────────────────────────────────────
  const [selectedEvent, setSelectedEvent] = useState<TxLineEvent | undefined>()
  const [fixtures, setFixtures] = useState<Fixture[]>([])
  const [fixturesLoading, setFixturesLoading] = useState(true)
  const [fixturesError, setFixturesError] = useState<string>()
  const [selectedFixtureId, setSelectedFixtureId] = useState<number>()
  const [fixturesDay, setFixturesDay] = useState<number>(epochDayNow())

  const fixturesDayHistorical = fixturesDay < epochDayNow()
  // Historical days snapshot at the last second of that (UTC) epoch day —
  // the fully-played state of every fixture on the board.
  const dayAsOfMs = fixturesDayHistorical ? (fixturesDay + 1) * DAY_MS - 1000 : undefined
  const [runs, setRuns] = useState<AgentRun[]>([])
  const [agents, setAgents] = useState<CoralAgentManifest[]>([])
  const [agentTrace, setAgentTrace] = useState<AgentTraceEvent[]>([])
  const [proofReceipts, setProofReceipts] = useState<TxLineProofReceipt[]>([])

  // ── Agent power state ────────────────────────────────────────────────────
  const [arenaPositions, setArenaPositions] = useState<ArenaPosition[]>([])
  const [settlementRecords, setSettlementRecords] = useState<SettlementRecord[]>([])
  const [arenaScore, setArenaScore] = useState<ArenaScore | undefined>()
  const [signalRecords, setSignalRecords] = useState<SignalRecord[]>([])
  const [safetyStatuses, setSafetyStatuses] = useState<AgentSafetyStatus[]>([])
  const [leaderboard, setLeaderboard] = useState<AgentLeaderboardEntry[]>([])
  const [toolCallRecords, setToolCallRecords] = useState<ToolCallRecord[]>([])

  // ── Chat state ───────────────────────────────────────────────────────────
  const [coralMessages, setCoralMessages] = useState<CoralMessage[]>([])
  // Local turns: user inputs plus synthesized agent replies. Kept separate
  // from streamed records so backend refreshes never clobber the transcript.
  const [localTurns, setLocalTurns] = useState<ChatItem[]>([])
  const [chatBusy, setChatBusy] = useState(false)
  const [chatBusyLabel, setChatBusyLabel] = useState<string>()
  // Read inside sendChat without re-creating the callback per score update.
  const arenaScoreRef = useRef<ArenaScore | undefined>(undefined)
  arenaScoreRef.current = arenaScore
  const localTurnSeq = useRef(0)

  function pushLocalTurn(kind: 'user' | 'agent', text: string) {
    const id = `local-${++localTurnSeq.current}`
    setLocalTurns((prev) => [...prev, { kind, id, text, ts: Date.now() }])
  }

  function pushRoundTurn(run: AgentRun) {
    const id = `local-${++localTurnSeq.current}`
    setLocalTurns((prev) => [...prev, { kind: 'round', id, run, ts: Date.now() }])
  }

  function pushBacktestTurn(summary: BacktestSummary) {
    const id = `local-${++localTurnSeq.current}`
    setLocalTurns((prev) => [...prev, { kind: 'backtest', id, summary, ts: Date.now() }])
  }

  // ── Derived ──────────────────────────────────────────────────────────────
  const currentRun = useMemo(() => runs[0], [runs])

  const currentProof = useMemo(() => {
    if (!currentRun) return proofReceipts[0]
    return (
      proofReceipts.find(
        (proof) =>
          proof.fixtureId === currentRun.trigger.fixtureId &&
          proof.seq === currentRun.trigger.seq,
      ) ??
      currentRun.trigger.proof ??
      proofReceipts[0]
    )
  }, [currentRun, proofReceipts])

  const currentRunTrace = useMemo(() => {
    if (!currentRun) return agentTrace
    return agentTrace.filter((trace) => trace.runId === currentRun.runId)
  }, [agentTrace, currentRun])

  // Backend SIGNAL texts are internal shorthand ("SharpOddsMove for fixture
  // 18179549 confidence 0.64") — rewrite them into plain language, resolving
  // the fixture id to team names when the board knows the fixture.
  function friendlySignalText(text: string): string {
    const match = text.match(/^(\w+) for fixture (\d+)(?:\s+confidence\s+([\d.]+))?/i)
    if (!match) return text
    const [, kind, fixtureId, confidence] = match
    const fixture = fixtures.find((f) => f.fixtureId === Number(fixtureId))
    const name = fixture ? `${fixture.home} vs ${fixture.away}` : `fixture ${fixtureId}`
    const kindLabel =
      kind === 'SharpOddsMove'
        ? 'Sharp odds movement'
        : kind.replace(/([a-z])([A-Z])/g, '$1 $2')
    const conf = confidence ? ` (${Math.round(Number(confidence) * 100)}% confidence)` : ''
    return `${kindLabel} detected on ${name}${conf}`
  }

  // Merge every chat-worthy stream into one ascending-chronological transcript.
  // Coral messages are the agent's own voice; signals / positions / settlements
  // render as contextual cards inline in the conversation.
  const chatItems = useMemo<ChatItem[]>(() => {
    const items: ChatItem[] = [
      ...localTurns,
      ...coralMessages
        .filter((m) => CHAT_VERBS.has(m.verb) && m.text.trim() !== '')
        .map<ChatItem>((m) => ({
          kind: 'coral',
          id: `c-${m.id}`,
          message: m.verb === 'SIGNAL' ? { ...m, text: friendlySignalText(m.text) } : m,
          ts: toMs(m.ts),
        })),
      ...signalRecords.map<ChatItem>((s) => ({
        kind: 'signal',
        id: `s-${s.idempotencyKey}`,
        signal: s,
        ts: toMs(s.detectedAt),
      })),
      ...arenaPositions.map<ChatItem>((p) => ({
        kind: 'position',
        id: `p-${p.positionId}`,
        position: p,
        ts: toMs(p.recordedAt),
      })),
      ...settlementRecords.map<ChatItem>((r) => ({
        kind: 'settlement',
        id: `r-${r.idempotencyKey}`,
        settlement: r,
        ts: toMs(r.settledAt),
      })),
    ]
    items.sort((a, b) => a.ts - b.ts)
    return items.slice(-200)
  }, [localTurns, coralMessages, signalRecords, arenaPositions, settlementRecords, fixtures])

  // Build the agent roster by merging CoralAgentManifest + safety status + leaderboard.
  const agentRoster = useMemo<AgentRosterEntry[]>(() => {
    return agents.map((manifest) => {
      const safety = safetyStatuses.find((s) => s.agentId === manifest.id)
      const capability: CapabilityKind = AGENT_CAPABILITIES[manifest.id] ?? 'DetectCap'
      const lb = leaderboard.find((l) => l.agentId === manifest.id)
      let status: AgentRosterEntry['status'] = 'unknown'
      if (safety) {
        status = 'running'
      }
      return {
        id: manifest.id,
        displayName: manifest.displayName,
        strategy:
          capability === 'FollowCap'
            ? 'FollowSharp'
            : capability === 'FadeCap'
            ? 'FadeSharp'
            : 'DetectAndNarrate',
        capability,
        status,
        safety,
        leaderboard: lb,
      }
    })
  }, [agents, safetyStatuses, leaderboard])

  // ── Subscriptions + initial load ─────────────────────────────────────────
  useEffect(() => {
    void loadCoralAgents().then(setAgents)

    const offAgentTrace = onAgentTrace((trace) => {
      setAgentTrace((prev) =>
        [...prev.filter((item) => item.id !== trace.id), trace].slice(-120),
      )
      // Trace phases narrate the typing indicator while a round is in flight.
      setChatBusyLabel(trace.summary)
    })
    const offCoralMessage = onCoralMessage((message) => {
      setCoralMessages((prev) =>
        [...prev.filter((m) => m.id !== message.id), message].slice(-200),
      )
    })
    const offProofReceipt = onProofReceipt((proof) => {
      setProofReceipts((prev) =>
        [
          proof,
          ...prev.filter(
            (item) =>
              !(item.fixtureId === proof.fixtureId && item.seq === proof.seq),
          ),
        ].slice(0, 40),
      )
    })

    const offArenaPosition = onArenaPosition((pos) => {
      setArenaPositions((prev) => [
        pos,
        ...prev.filter((p) => p.positionId !== pos.positionId),
      ])
    })
    const offSettlementRecord = onSettlementRecord((rec) => {
      setSettlementRecords((prev) => [
        rec,
        ...prev.filter((r) => r.idempotencyKey !== rec.idempotencyKey),
      ])
      void listAgentLeaderboardNative().then(setLeaderboard).catch(console.error)
    })
    const offSignalRecord = onSignalRecord((rec) => {
      setSignalRecords((prev) =>
        [
          rec,
          ...prev.filter((r) => r.idempotencyKey !== rec.idempotencyKey),
        ].slice(0, 200),
      )
    })
    const offArenaScore = onArenaScore((score) => {
      setArenaScore(score)
    })
    const offToolCall = onToolCallRecord((rec) => {
      setToolCallRecords((prev) =>
        [
          rec,
          ...prev.filter(
            (r) =>
              !(r.traceId === rec.traceId && r.idempotencyKey === rec.idempotencyKey),
          ),
        ].slice(0, 500),
      )
    })

    void listRunsNative().then(setRuns).catch(console.error)
    void refreshFixtures()
    void listArenaPositionsNative().then(setArenaPositions).catch(console.error)
    void listSettlementRecordsNative().then(setSettlementRecords).catch(console.error)
    void listSignalRecordsNative().then(setSignalRecords).catch(console.error)
    void getArenaScoreNative().then(setArenaScore).catch(console.error)
    void listAgentLeaderboardNative().then(setLeaderboard).catch(console.error)
    void listToolCallRecordsNative().then(setToolCallRecords).catch(console.error)

    return () => {
      offAgentTrace()
      offCoralMessage()
      offProofReceipt()
      offArenaPosition()
      offSettlementRecord()
      offSignalRecord()
      offArenaScore()
      offToolCall()
    }
  }, [])

  useEffect(() => {
    if (!currentRun) return
    void listAgentTraceNative(currentRun.runId)
      .then((trace) => {
        if (trace.length > 0)
          setAgentTrace((prev) => [...prev, ...trace].slice(-120))
      })
      .catch(console.error)
    void listCoralMessagesNative(currentRun.runId)
      .then((messages) => {
        if (messages.length > 0)
          setCoralMessages((prev) => {
            const known = new Set(prev.map((m) => m.id))
            return [...prev, ...messages.filter((m) => !known.has(m.id))].slice(-200)
          })
      })
      .catch(console.error)
  }, [currentRun?.runId])

  // ── Actions ───────────────────────────────────────────────────────────────

  // TxLINE's fixtures snapshot returns everything from `startEpochDay` onward
  // with no upper bound — for "today" that means every future fixture in
  // every competition (including friendlies scheduled months out), which
  // showed up on the board as unrelated/duplicate-looking entries with no
  // cutoff. Always trim to the single requested day, whether it's in the
  // past, today, or a future day reached via the ▶ arrow — there is no fixed
  // "tournament end" cutoff; a day simply has no fixtures once the
  // tournament is over.
  function fixturesForDay(list: Fixture[], day: number): Fixture[] {
    const start = day * DAY_MS
    return list.filter((fixture) => {
      if (!fixture.startTime) return false
      const kickoff = new Date(fixture.startTime).getTime()
      return kickoff >= start && kickoff < start + DAY_MS
    })
  }

  async function refreshFixtures(day = fixturesDay) {
    setFixturesLoading(true)
    setFixturesError(undefined)
    try {
      setFixtures(fixturesForDay(await loadLiveFixtures(day), day))
    } catch (err) {
      setFixturesError(err instanceof Error ? err.message : String(err))
    } finally {
      setFixturesLoading(false)
    }
  }

  async function changeFixturesDay(day: number) {
    setFixturesDay(day)
    setSelectedFixtureId(undefined)
    setSelectedEvent(undefined)
    await refreshFixtures(day)
  }

  // Pre-match odds live until kickoff; snapshot a minute before it so a
  // historical round sees the full 1X2 market rather than late in-play scraps.
  function preMatchAsOfMs(fixture: Fixture, fallback: number): number {
    if (!fixture.startTime) return fallback
    const kickoff = new Date(fixture.startTime).getTime()
    if (!Number.isFinite(kickoff)) return fallback
    return Math.min(kickoff - 60_000, fallback)
  }

  async function selectFixture(fixture: Fixture): Promise<TxLineEvent | undefined> {
    setSelectedFixtureId(fixture.fixtureId)
    try {
      // On a past day: final score from end of day, odds from just before
      // kickoff (in-play markets are gone by the final whistle).
      const event = await loadFixtureEvent(
        fixture,
        dayAsOfMs,
        dayAsOfMs !== undefined ? preMatchAsOfMs(fixture, dayAsOfMs) : undefined,
      )
      setSelectedEvent(event)
      return event
    } catch (err) {
      console.error('fixture snapshot failed', err)
      return undefined
    }
  }

  async function startRound(
    track: TrackMode = 'trading',
    event = selectedEvent,
  ): Promise<AgentRun | undefined> {
    if (!event) return undefined
    const run = await runAgentRoundNative(event, track)
    setRuns((prev) => [run, ...prev])
    return run
  }

  // ── Chat ──────────────────────────────────────────────────────────────────
  //
  // sendChat maps natural language onto the existing backend surface: score
  // questions are answered locally from arena state; anything else resolves a
  // fixture (by team-name match or current selection) and starts a Coral round
  // on the matching track. A parsed "as of …" phrase swaps the live snapshots
  // for TxLINE's historical ones — rounds are trigger-driven, so nothing else
  // changes. The LLM conversation itself happens agent-side — this is
  // deliberately a thin, deterministic router.

  function matchFixture(lower: string, list: Fixture[]): Fixture | undefined {
    let best: Fixture | undefined
    let bestHits = 0
    for (const fixture of list) {
      const hits =
        (lower.includes(fixture.home.toLowerCase()) ? 1 : 0) +
        (lower.includes(fixture.away.toLowerCase()) ? 1 : 0)
      if (hits > bestHits) {
        best = fixture
        bestHits = hits
      }
    }
    return best
  }

  function trackFor(lower: string): TrackMode {
    if (/(settle|verif|proof|on.?chain)/.test(lower)) return 'settlement'
    if (/(fan|narrat|story|pundit)/.test(lower)) return 'fan'
    return 'trading'
  }

  async function sendChat(text: string) {
    const trimmed = text.trim()
    if (!trimmed || chatBusy) return
    pushLocalTurn('user', trimmed)

    // "as of yesterday 18:00" / "2 hours ago" → historical snapshot round.
    // Browsing a past day on the fixture board makes analyses historical by
    // default (end of that day), no time phrase needed.
    const parsed = parseAsOf(trimmed)
    const asOf =
      parsed ??
      (dayAsOfMs !== undefined
        ? { asOfMs: dayAsOfMs, label: epochDayLabel(fixturesDay), cleaned: trimmed }
        : undefined)
    const lower = (asOf?.cleaned ?? trimmed).toLowerCase()

    // Score / leaderboard questions never need a backend round.
    if (/(score|scoreboard|leaderboard|standing|winning)/.test(lower)) {
      pushLocalTurn('agent', describeArenaScore(arenaScoreRef.current))
      return
    }

    let target = matchFixture(lower, fixtures)
    const useSelected = !target && selectedFixtureId !== undefined
    if (!target) target = fixtures.find((f) => f.fixtureId === selectedFixtureId)
    // Historical asks may name a fixture missing from today's board — look it
    // up in the fixtures snapshot for the requested day instead.
    if (!target && asOf) {
      try {
        const dayFixtures = await loadLiveFixtures(Math.floor(asOf.asOfMs / 86_400_000))
        target = matchFixture(lower, dayFixtures)
      } catch (err) {
        console.error('historical fixtures snapshot failed', err)
      }
    }
    if (!target) {
      pushLocalTurn(
        'agent',
        'I couldn’t match that to a fixture. Pick one from the fixture board, or name both teams — e.g. "Analyze Norway vs England" (add "as of yesterday 18:00" for a historical snapshot).',
      )
      return
    }

    // Backtest: replay a completed fixture's real odds history and settle
    // simulated positions against its real final score — checked before the
    // live-round track logic, since it's a distinct backend path (see
    // ARENA-AUTONOMY-PLAN.md Priority B). Needs a known kickoff time to
    // window the historical odds fetch; day-browsing to a past date (or an
    // explicit "as of" phrase) is how `target` gets resolved to a completed
    // fixture not on today's board — same resolution already used above.
    if (/backtest|back-test/.test(lower)) {
      if (!target.startTime) {
        pushLocalTurn(
          'agent',
          `I don't have a kickoff time for ${target.home} vs ${target.away}, so I can't window the odds history — try selecting it from a past day on the fixture board first.`,
        )
        return
      }
      setChatBusy(true)
      setChatBusyLabel('Replaying historical odds…')
      pushLocalTurn(
        'agent',
        `On it — replaying ${target.home} vs ${target.away}'s real TxLINE odds history and settling simulated Follow/Fade positions against the final score. This is simulated history, not a live result.`,
      )
      try {
        const kickoffMs = new Date(target.startTime).getTime()
        const summary = await runBacktestNative(target.fixtureId, target.home, target.away, kickoffMs)
        pushBacktestTurn(summary)
      } catch (err) {
        pushLocalTurn(
          'agent',
          `That backtest failed: ${err instanceof Error ? err.message : String(err)}`,
        )
      } finally {
        setChatBusy(false)
        setChatBusyLabel(undefined)
      }
      return
    }

    const track = trackFor(lower)
    setChatBusy(true)
    setChatBusyLabel(undefined)
    pushLocalTurn(
      'agent',
      `On it — running a ${track} round on ${target.home} vs ${target.away}${
        asOf ? ` as of ${asOf.label}` : useSelected ? ' (your selected fixture)' : ''
      }. I’ll narrate what I find here.`,
    )
    try {
      // An explicit time phrase pins both snapshots to that moment; the
      // day-browse default takes odds pre-kickoff and the score at day end.
      const event = asOf
        ? await loadFixtureEvent(
            target,
            asOf.asOfMs,
            parsed ? asOf.asOfMs : preMatchAsOfMs(target, asOf.asOfMs),
          )
        : target.fixtureId === selectedFixtureId && selectedEvent
          ? selectedEvent
          : await selectFixture(target)
      if (!event) throw new Error('could not load a TxLINE snapshot for that fixture')
      const run = await startRound(track, event)
      if (run) pushRoundTurn(run)
      else pushLocalTurn('agent', 'The round didn’t produce a result — try again once the fixture has odds.')
    } catch (err) {
      pushLocalTurn(
        'agent',
        `That round failed: ${err instanceof Error ? err.message : String(err)}`,
      )
    } finally {
      setChatBusy(false)
      setChatBusyLabel(undefined)
    }
  }

  return {
    selectedEvent,
    fixtures,
    fixturesLoading,
    fixturesError,
    selectedFixtureId,
    fixturesDay,
    fixturesDayHistorical,
    runs,
    agents,
    agentTrace,
    proofReceipts,
    arenaPositions,
    settlementRecords,
    arenaScore,
    signalRecords,
    safetyStatuses,
    leaderboard,
    toolCallRecords,
    chatItems,
    chatBusy,
    chatBusyLabel,
    currentRun,
    currentProof,
    currentRunTrace,
    agentRoster,
    refreshFixtures,
    changeFixturesDay,
    selectFixture,
    startRound,
    sendChat,
  }
}
