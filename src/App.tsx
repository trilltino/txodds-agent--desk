import { useEffect, useMemo, useState } from 'react'
import type { AgentRun, CoralAgentManifest, IngestStatus, TrackMode, TxLineEvent } from './types'
import { mockEvents } from './domain/txline/mock'
import { runLocalAgentRound } from './domain/coral/localRound'
import { fallbackCoralAgents, loadCoralAgents } from './domain/coral/agents'
import { getConfig, listRunsNative, native, onIngestStatus, onTxLineEvent, runAgentRoundNative, startTxLine, stopTxLine } from './desktop/transport'
import { Shell } from './components/Shell'
import { LiveFeed } from './components/LiveFeed'
import { AgentArena } from './components/AgentArena'
import { SettlementLab } from './components/SettlementLab'
import { FanMode } from './components/FanMode'
import { ProofPanel } from './components/ProofPanel'
import { TrackScorecard } from './components/TrackScorecard'

// App is the webview orchestrator: it owns selected UI state, subscribes to
// backend event streams, and delegates rendering to screens. Backend protocols
// stay behind desktop/transport.ts and src-tauri commands.
export default function App() {
  // Track determines which product lens the same TxLINE event and market run
  // are being viewed through: settlement, trading, or fan experience.
  const [track, setTrack] = useState<TrackMode>('trading')
  // Browser-only dev starts with fixtures immediately. Native mode waits for
  // Rust to emit txline://event so credentials never enter the webview.
  const [events, setEvents] = useState<TxLineEvent[]>(native ? [] : mockEvents)
  const [selectedEvent, setSelectedEvent] = useState<TxLineEvent>(mockEvents[0])
  const [ingestStatuses, setIngestStatuses] = useState<IngestStatus[]>(
    native ? [] : [{ source: 'mock', state: 'connected', detail: 'Browser mock TxLINE fallback active' }]
  )
  // Runs are newest-first because all panels render the current run by default.
  const [runs, setRuns] = useState<AgentRun[]>([])
  const [agents, setAgents] = useState<CoralAgentManifest[]>(fallbackCoralAgents)
  const currentRun = useMemo(() => runs[0], [runs])

  useEffect(() => {
    // Native mode asks Rust for the agent registry; browser mode falls back to
    // the same static identities so UI iteration still works without Tauri.
    void loadCoralAgents().then(setAgents)
    if (!native) return

    // TxLINE events are pushed from Rust. Keep the list bounded so a long demo
    // session does not turn the webview into an unbounded event cache.
    const offTxLine = onTxLineEvent((event) => {
      setEvents((prev) => [event, ...prev.filter((item) => item.id !== event.id)].slice(0, 50))
      setSelectedEvent((prev) => prev ?? event)
    })
    const offIngestStatus = onIngestStatus((status) => {
      setIngestStatuses((prev) => [status, ...prev.filter((item) => item.source !== status.source)].slice(0, 6))
    })

    // Restore durable run history from SQLite, then let Rust decide whether it
    // can use live TxLINE credentials or should fall back to mock mode.
    void listRunsNative().then(setRuns).catch(console.error)
    void getConfig()
      .then((cfg) => startTxLine(cfg.txlineConfigured ? 'live' : 'mock'))
      .catch(() => startTxLine('mock'))

    return () => {
      offTxLine()
      offIngestStatus()
      void stopTxLine()
    }
  }, [])

  // The UI calls one function for both native and browser-dev mode. Native mode
  // executes the Rust market engine and persists to SQLite; browser mode uses
  // deterministic local fallback code.
  async function startRound(event = selectedEvent, nextTrack = track) {
    const run = native
      ? await runAgentRoundNative(event, nextTrack)
      : await runLocalAgentRound(event, nextTrack)
    setRuns((prev) => [run, ...prev])
  }

  return (
    <Shell track={track} setTrack={setTrack} onStart={() => startRound()}>
      <section className="grid two">
        <LiveFeed events={events} selected={selectedEvent} ingestStatuses={ingestStatuses} onSelect={setSelectedEvent} onStartRound={startRound} />
        <AgentArena agents={agents} track={track} run={currentRun} onRun={() => startRound()} />
      </section>
      <section className="grid two">
        <SettlementLab run={currentRun} />
        <FanMode run={currentRun} selectedEvent={selectedEvent} />
      </section>
      <section className="grid two">
        <ProofPanel run={currentRun} />
        <TrackScorecard />
      </section>
    </Shell>
  )
}
