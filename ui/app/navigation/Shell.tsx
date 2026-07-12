import type { ReactNode } from 'react'

interface Props {
  children: ReactNode
  /** Forget the remembered wallet session and return to the login screen. */
  onSignOut?: () => void
  /** Return to the app picker without disconnecting the wallet. */
  onBack?: () => void
}

/**
 * Shell — branded top bar + page wrapper for the single-page Agent Desk.
 * The multi-tab nav (Pulse / Markets / Agent) has been removed; there is only
 * one surface: the agent chat. It mounts right after the app picker, so it
 * fades in rather than popping in.
 */
export function Shell({ children, onSignOut, onBack }: Props) {
  return (
    <main className="appFrame fadeIn">

      {/* ── Desktop top bar ── */}
      <header className="topBar" role="banner">
        <div className="brandBlock">
          <div className="brandLockup">
            <span className="worldCupMark" aria-hidden="true"><span /></span>
            <div>
              <p className="eyebrow">TxLINE / Solana</p>
              <h1>World Cup Agent Desk</h1>
            </div>
          </div>
        </div>
        {(onBack || onSignOut) && (
          <div className="topActions" style={{ gridColumn: 3 }}>
            {onBack && <button className="secondary" onClick={onBack}>← Apps</button>}
            {onSignOut && <button onClick={onSignOut}>Sign out</button>}
          </div>
        )}
      </header>

      {/* Page content */}
      {children}

    </main>
  )
}
