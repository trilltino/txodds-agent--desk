import { useState } from 'react'
import { native } from '../desktop/transport'
import type { TrackMode } from '../types'
import { Shell } from './navigation/Shell'
import { useAgentDesk } from './hooks/useAgentDesk'
import { AgentDashboard } from './components/AgentDashboard'
import { WalletLogin } from './components/WalletLogin'
import { FixtureBoard } from '../apps/shared/components/FixtureBoard'

const TRACK_OPTIONS: { value: TrackMode; label: string }[] = [
  { value: 'trading', label: 'Trading (sharp movement)' },
  { value: 'settlement', label: 'Settlement (on-chain verify)' },
  { value: 'fan', label: 'Fan (narrative)' },
]

// AnalyzeControl lets a user actually trigger a Coral round on the selected
// fixture — previously `useAgentDesk.startRound()` existed on the Rust side
// but had no UI caller at all (rig-venice ROADMAP.md Phase 7).
function AnalyzeControl({
  disabled,
  onAnalyze,
}: {
  disabled: boolean
  onAnalyze: (track: TrackMode) => Promise<void>
}) {
  const [track, setTrack] = useState<TrackMode>('trading')
  const [running, setRunning] = useState(false)

  async function handleClick() {
    setRunning(true)
    try {
      await onAnalyze(track)
    } catch (err) {
      console.error('analyze fixture failed', err)
    } finally {
      setRunning(false)
    }
  }

  return (
    <div className="analyzeControl">
      <select
        value={track}
        onChange={(e) => setTrack(e.target.value as TrackMode)}
        disabled={disabled || running}
      >
        {TRACK_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>{opt.label}</option>
        ))}
      </select>
      <button onClick={() => void handleClick()} disabled={disabled || running}>
        {running ? 'Analyzing…' : 'Analyze fixture'}
      </button>
    </div>
  )
}

function DesktopOnlyScreen() {
  return (
    <main className="desktopOnly">
      <div className="desktopOnlyPanel">
        <span className="worldCupMark" aria-hidden="true"><span /></span>
        <div>
          <p className="eyebrow">Desktop runtime required</p>
          <h1>World Cup Agent Desk</h1>
          <p>This app runs only as the Tauri desktop client with Rust-owned live TxLINE credentials.</p>
        </div>
      </div>
    </main>
  )
}

// ── authenticated shell ────────────────────────────────────────────────────────

function AppShell() {
  const desk = useAgentDesk()

  return (
    <Shell>
      <section className="productPage agent">
        <div className="pageTitle">
          <div className="titleCopy">
            <p className="eyebrow">Agent app</p>
            <h2>Intelligence Agent</h2>
          </div>
          <div className="matchRibbon" aria-hidden="true">
            <span />
            <span />
            <span />
          </div>
          <span className="pageStatus">
            {desk.selectedEvent ? `fixture ${desk.selectedEvent.fixtureId}` : 'waiting for TxLINE'}
          </span>
          <AnalyzeControl
            disabled={!desk.selectedEvent}
            onAnalyze={(track) => desk.startRound(track)}
          />
        </div>
        <div className="pageGrid">
          <div className="pageMain">
            <AgentDashboard
              agentRoster={desk.agentRoster}
              safetyStatuses={desk.safetyStatuses}
              arenaScore={desk.arenaScore}
              settlementRecords={desk.settlementRecords}
              leaderboard={desk.leaderboard}
              arenaPositions={desk.arenaPositions}
              signalRecords={desk.signalRecords}
              toolCallRecords={desk.toolCallRecords}
              currentRunTrace={desk.currentRunTrace}
            />
          </div>
          <aside className="contextRail">
            <FixtureBoard
              fixtures={desk.fixtures}
              loading={desk.fixturesLoading}
              error={desk.fixturesError}
              selectedFixtureId={desk.selectedFixtureId}
              onSelect={desk.selectFixture}
              onRefresh={() => void desk.refreshFixtures()}
            />
          </aside>
        </div>
      </section>
    </Shell>
  )
}

// ── App root ───────────────────────────────────────────────────────────────────
//
// App is the webview orchestrator: it owns UI state, subscribes to
// backend event streams via useAgentDesk, and delegates rendering to screens.
// Backend protocols stay behind desktop/transport.ts and native commands.
//
// Auth gate: the webview shows <WalletLogin> until the user connects a Phantom
// wallet and the local sled UserProfile is found or created.  Once
// `authenticated` is signalled the main <AppShell> mounts.
export default function App() {
  if (!native) return <DesktopOnlyScreen />

  const [authed, setAuthed] = useState(false)

  if (!authed) {
    return <WalletLogin onAuthenticated={() => setAuthed(true)} />
  }

  return <AppShell />
}
