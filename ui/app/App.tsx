import { useState } from 'react'
import { clearWalletSessionNative, native } from '../desktop/transport'
import { Shell } from './navigation/Shell'
import { epochDayLabel, useAgentDesk } from './hooks/useAgentDesk'
import { ChatPanel } from './components/ChatPanel'
import { WalletLogin } from './components/WalletLogin'
import { FixtureBoard } from '../apps/shared/components/FixtureBoard'

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

function AppShell({ onSignOut }: { onSignOut: () => void }) {
  const desk = useAgentDesk()
  const selectedFixture = desk.fixtures.find((f) => f.fixtureId === desk.selectedFixtureId)

  return (
    <Shell onSignOut={onSignOut}>
      <section className="productPage agent chatPage">
        <div className="chatLayout">
          <ChatPanel
            items={desk.chatItems}
            busy={desk.chatBusy}
            busyLabel={desk.chatBusyLabel}
            selectedFixture={selectedFixture}
            onSend={(text) => void desk.sendChat(text)}
          />
          <aside className="chatRail">
            <FixtureBoard
              fixtures={desk.fixtures}
              loading={desk.fixturesLoading}
              error={desk.fixturesError}
              selectedFixtureId={desk.selectedFixtureId}
              dayLabel={epochDayLabel(desk.fixturesDay)}
              historical={desk.fixturesDayHistorical}
              onSelect={(fixture) => void desk.selectFixture(fixture)}
              onRefresh={() => void desk.refreshFixtures()}
              onPrevDay={() => void desk.changeFixturesDay(desk.fixturesDay - 1)}
              onNextDay={() => void desk.changeFixturesDay(desk.fixturesDay + 1)}
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
// wallet and the local sled UserProfile is found or created. Once
// `authenticated` is signalled, the chat surface mounts directly — the old
// AppPicker launcher step is gone; the Intelligence Agent is the only app.
export default function App() {
  // Hooks must run unconditionally on every render (Rules of Hooks) — the
  // desktop-only gate returns AFTER them, not before.
  const [authed, setAuthed] = useState(false)

  if (!native) return <DesktopOnlyScreen />

  if (!authed) {
    return <WalletLogin onAuthenticated={() => setAuthed(true)} />
  }

  return (
    <AppShell
      onSignOut={() => {
        // Forget the remembered session BEFORE remounting the login gate —
        // its restore lookup runs on mount and must not find the old session.
        void clearWalletSessionNative()
          .catch(console.error)
          .finally(() => setAuthed(false))
      }}
    />
  )
}
