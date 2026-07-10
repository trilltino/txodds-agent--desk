import { useEffect, useMemo, useState } from 'react'
import type {
  AgentRun,
  AgentTraceEvent,
  CoralAgentManifest,
  Fixture,
  TrackMode,
  TxLineEvent,
  TxLineProofReceipt,
} from '../../types'
import type {
  AgentLeaderboardEntry,
  AgentRosterEntry,
  AgentSafetyStatus,
  ArenaPosition,
  ArenaScore,
  CapabilityKind,
  SettlementRecord,
  SignalRecord,
  ToolCallRecord,
} from '../../core/agent/types'
import { loadFixtureEvent, loadLiveFixtures } from '../../core/txline/fixtures'
import { loadCoralAgents } from '../../core/coral/agents'
import {
  getArenaScoreNative,
  listAgentLeaderboardNative,
  listAgentTraceNative,
  listArenaPositionsNative,
  listRunsNative,
  listSettlementRecordsNative,
  listSignalRecordsNative,
  listToolCallRecordsNative,
  onAgentTrace,
  onArenaPosition,
  onArenaScore,
  onProofReceipt,
  onSettlementRecord,
  onSignalRecord,
  onToolCallRecord,
  runAgentRoundNative,
} from '../../desktop/transport'

// Capability token assignment for the four known sidecar agents.
const AGENT_CAPABILITIES: Record<string, CapabilityKind> = {
  'match-intelligence': 'FollowCap',
  'contrarian': 'FadeCap',
  'arena-coordinator': 'SettleCap',
  'sharp-movement-detector': 'DetectCap',
}

export interface AgentDeskState {
  selectedEvent: TxLineEvent | undefined
  fixtures: Fixture[]
  fixturesLoading: boolean
  fixturesError: string | undefined
  selectedFixtureId: number | undefined
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
  // Derived
  currentRun: AgentRun | undefined
  currentProof: TxLineProofReceipt | undefined
  currentRunTrace: AgentTraceEvent[]
  agentRoster: AgentRosterEntry[]
  // Actions
  refreshFixtures: () => Promise<void>
  selectFixture: (fixture: Fixture) => Promise<void>
  startRound: (track?: TrackMode, event?: TxLineEvent) => Promise<void>
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
  }, [currentRun?.runId])

  // ── Actions ───────────────────────────────────────────────────────────────
  async function refreshFixtures() {
    setFixturesLoading(true)
    setFixturesError(undefined)
    try {
      setFixtures(await loadLiveFixtures())
    } catch (err) {
      setFixturesError(err instanceof Error ? err.message : String(err))
    } finally {
      setFixturesLoading(false)
    }
  }

  async function selectFixture(fixture: Fixture) {
    setSelectedFixtureId(fixture.fixtureId)
    try {
      const event = await loadFixtureEvent(fixture)
      setSelectedEvent(event)
    } catch (err) {
      console.error('fixture snapshot failed', err)
    }
  }

  async function startRound(track: TrackMode = 'trading', event = selectedEvent) {
    if (!event) return
    const run = await runAgentRoundNative(event, track)
    setRuns((prev) => [run, ...prev])
  }

  return {
    selectedEvent,
    fixtures,
    fixturesLoading,
    fixturesError,
    selectedFixtureId,
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
    currentRun,
    currentProof,
    currentRunTrace,
    agentRoster,
    refreshFixtures,
    selectFixture,
    startRound,
  }
}
