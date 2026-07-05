import { useEffect, useMemo, useState } from 'react'
import type { AgentRun, CoralAgentManifest, TrackMode, TxLineEvent } from './types'
import { mockEvents } from './domain/txline/mock'
import { runLocalAgentRound } from './domain/coral/localRound'
import { fallbackCoralAgents, loadCoralAgents } from './domain/coral/agents'
import { getConfig, listRunsNative, native, onTxLineEvent, runAgentRoundNative, startTxLine, stopTxLine } from './desktop/transport'
import { Shell } from './components/Shell'
import { LiveFeed } from './components/LiveFeed'
import { AgentArena } from './components/AgentArena'
import { SettlementLab } from './components/SettlementLab'
import { FanMode } from './components/FanMode'
import { ProofPanel } from './components/ProofPanel'
import { TrackScorecard } from './components/TrackScorecard'

export default function App() {
  const [track, setTrack] = useState<TrackMode>('trading')
  const [events, setEvents] = useState<TxLineEvent[]>(native ? [] : mockEvents)
  const [selectedEvent, setSelectedEvent] = useState<TxLineEvent>(mockEvents[0])
  const [runs, setRuns] = useState<AgentRun[]>([])
  const [agents, setAgents] = useState<CoralAgentManifest[]>(fallbackCoralAgents)
  const currentRun = useMemo(() => runs[0], [runs])

  useEffect(() => {
    void loadCoralAgents().then(setAgents)
    if (!native) return

    const offTxLine = onTxLineEvent((event) => {
      setEvents((prev) => [event, ...prev.filter((item) => item.id !== event.id)].slice(0, 50))
      setSelectedEvent((prev) => prev ?? event)
    })

    void listRunsNative().then(setRuns).catch(console.error)
    void getConfig()
      .then((cfg) => startTxLine(cfg.txlineConfigured ? 'live' : 'mock'))
      .catch(() => startTxLine('mock'))

    return () => {
      offTxLine()
      void stopTxLine()
    }
  }, [])

  async function startRound(event = selectedEvent, nextTrack = track) {
    const run = native
      ? await runAgentRoundNative(event, nextTrack)
      : await runLocalAgentRound(event, nextTrack)
    setRuns((prev) => [run, ...prev])
  }

  return (
    <Shell track={track} setTrack={setTrack} onStart={() => startRound()}>
      <section className="grid two">
        <LiveFeed events={events} selected={selectedEvent} onSelect={setSelectedEvent} onStartRound={startRound} />
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
