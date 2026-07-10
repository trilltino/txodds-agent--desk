import { useState } from 'react'
import type { ArenaPosition, PositionOutcome } from '../../../core/agent/types'

type FilterTab = 'all' | 'FollowSharp' | 'FadeSharp'

function DirectionBadge({ direction }: { direction: string }) {
  return (
    <span className={`directionBadge directionBadge--${direction === 'With' ? 'with' : 'against'}`}>
      {direction === 'With' ? '↑ WITH' : '↓ AGAINST'}
    </span>
  )
}

/** Renders outcome using the rich PositionOutcome struct from arena-coordinator. */
function OutcomePill({ outcome }: { outcome?: PositionOutcome }) {
  if (!outcome) return <span className="pill outcomePending">pending</span>
  if (outcome.selectionWon) {
    return (
      <span className="pill outcomeWin" title={`Score: ${outcome.finalScore} · +${outcome.pnlPoints.toFixed(2)} pts`}>
        WIN
      </span>
    )
  }
  return (
    <span className="pill outcomeLoss" title={`Score: ${outcome.finalScore} · ${outcome.pnlPoints.toFixed(2)} pts`}>
      LOSS
    </span>
  )
}

/** Settlement TX link badge — only rendered when an on-chain signature is present. */
function TxBadge({ sig }: { sig?: string }) {
  if (!sig) return null
  return (
    <span className="txBadge" title={sig}>
      ⛓ TX
    </span>
  )
}

function ConfidenceBar({ value }: { value: number }) {
  const pct = Math.round(value * 100)
  return (
    <div className="confidenceBarWrap" title={`${pct}% confidence`}>
      <div className="confidenceBarFill" style={{ width: `${pct}%` }} />
    </div>
  )
}

// ArenaPositionFeed renders the live table of FollowSharp and FadeSharp
// positions recorded by match-intelligence and contrarian agents.
// It auto-appends as the parent pushes new positions via onArenaPosition events.
// outcome is now the rich PositionOutcome struct (not a plain string).
export function ArenaPositionFeed({ positions }: { positions: ArenaPosition[] }) {
  const [filter, setFilter] = useState<FilterTab>('all')

  const filtered =
    filter === 'all' ? positions : positions.filter((p) => p.strategy === filter)

  const tabs: FilterTab[] = ['all', 'FollowSharp', 'FadeSharp']

  // Aggregate PnL for visible positions that have settled.
  const settledPositions = filtered.filter((p) => p.outcome)
  const totalPnl = settledPositions.reduce((acc, p) => acc + (p.outcome?.pnlPoints ?? 0), 0)
  const wins = settledPositions.filter((p) => p.outcome?.selectionWon).length

  return (
    <article className="card">
      <div className="cardHead">
        <h2>Arena Positions</h2>
        <span className="pill">{positions.length} recorded</span>
        {settledPositions.length > 0 && (
          <>
            <span className="pill">
              {wins}W / {settledPositions.length - wins}L
            </span>
            <span className={`pill ${totalPnl >= 0 ? 'pillPnlPositive' : 'pillPnlNegative'}`}>
              {totalPnl >= 0 ? '+' : ''}{totalPnl.toFixed(2)} pts
            </span>
          </>
        )}
      </div>

      <div className="filterTabs">
        {tabs.map((tab) => (
          <button
            key={tab}
            className={`filterTab ${filter === tab ? 'filterTabActive' : ''}`}
            onClick={() => setFilter(tab)}
          >
            {tab === 'all' ? 'All' : tab}
          </button>
        ))}
      </div>

      {filtered.length === 0 ? (
        <p className="muted">
          {positions.length === 0
            ? 'No positions yet — waiting for match-intelligence or contrarian to act.'
            : 'No positions match this filter.'}
        </p>
      ) : (
        <div className="positionTableWrap">
          <table className="positionTable">
            <thead>
              <tr>
                <th>Fixture</th>
                <th>Market</th>
                <th>Selection</th>
                <th>Direction</th>
                <th>Confidence</th>
                <th>Entry odds</th>
                <th>Move %</th>
                <th>Outcome</th>
                <th>PnL</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((pos) => (
                <tr key={pos.positionId} className={`positionRow positionRow--${pos.strategy === 'FollowSharp' ? 'follow' : 'fade'}`}>
                  <td className="monoSmall">#{pos.fixtureId}</td>
                  <td>{pos.marketKey}</td>
                  <td>
                    <span className="positionSelection">{pos.selection}</span>
                    <TxBadge sig={pos.outcome?.settlementTx ?? pos.txSignature} />
                  </td>
                  <td><DirectionBadge direction={pos.direction} /></td>
                  <td><ConfidenceBar value={pos.confidence} /></td>
                  <td className="monoSmall">{pos.oddsAtEntry.toFixed(2)}</td>
                  <td className={`monoSmall ${pos.oddsMovePct >= 0 ? 'pnlPositive' : 'pnlNegative'}`}>
                    {pos.oddsMovePct >= 0 ? '+' : ''}{pos.oddsMovePct.toFixed(1)}%
                  </td>
                  <td><OutcomePill outcome={pos.outcome} /></td>
                  <td className={`monoSmall ${pos.outcome ? (pos.outcome.pnlPoints >= 0 ? 'pnlPositive' : 'pnlNegative') : ''}`}>
                    {pos.outcome
                      ? `${pos.outcome.pnlPoints >= 0 ? '+' : ''}${pos.outcome.pnlPoints.toFixed(2)}`
                      : '—'
                    }
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </article>
  )
}
