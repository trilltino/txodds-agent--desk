import type { ReactNode } from 'react'

interface Props {
  children: ReactNode
}

/**
 * Shell — branded top bar + page wrapper for the single-page Agent Desk.
 * The multi-tab nav (Pulse / Markets / Agent) has been removed; there is only
 * one surface: the Intelligence Agent.
 */
export function Shell({ children }: Props) {
  return (
    <main className="appFrame">

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
      </header>

      {/* Page content */}
      {children}

    </main>
  )
}
