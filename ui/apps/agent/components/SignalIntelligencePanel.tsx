import { useState } from 'react'
import type { SignalRecord } from '../../../core/agent/types'

function OddsDirectionBadge({ direction }: { direction: string }) {
  return (
    <span className={`oddsDirectionBadge oddsDirectionBadge--${direction}`}>
      {direction === 'shortened' ? '▼ SHORT' : '▲ LEN'}
    </span>
  )
}

function CorrectnessIcon({ correct, outcome }: { correct: boolean; outcome?: string }) {
  if (outcome) {
    const lower = outcome.toLowerCase()
    if (lower === 'win' || lower === 'correct') return <span title="Outcome: correct">✓</span>
    if (lower === 'loss' || lower === 'incorrect') return <span title="Outcome: incorrect">✗</span>
  }
  return correct ? (
    <span className="correctIcon" title="Tracking — odds continuing in predicted direction">✓</span>
  ) : (
    <span className="reversedIcon" title="Reversed — odds moved back against prediction">↩</span>
  )
}

function SignalRow({ rec }: { rec: SignalRecord }) {
  const [expanded, setExpanded] = useState(false)
  const hasMeta = !!rec.narrative

  return (
    <li className="signalRow">
      <div className="signalRowMain">
        <OddsDirectionBadge direction={rec.direction} />
        <div className="signalMeta">
          <strong>{rec.fixtureName}</strong>
          <span className="muted signalMarket">{rec.marketKey} — {rec.selection}</span>
        </div>
        <div className="signalStats">
          <span className="monoSmall">{rec.movePct >= 0 ? '+' : ''}{rec.movePct.toFixed(1)}%</span>
          <span className="muted monoSmall">{rec.oddsNow.toFixed(2)}</span>
          <span className="muted monoSmall" title="Confidence">{Math.round(rec.confidence * 100)}%</span>
        </div>
        <div className="signalCorrectnessWrap">
          <CorrectnessIcon correct={rec.correctSoFar} outcome={rec.outcome} />
        </div>
        {hasMeta && (
          <button
            className="narrativeToggle"
            onClick={() => setExpanded((p) => !p)}
            aria-expanded={expanded}
            title="Toggle AI narrative"
          >
            {expanded ? '▲' : '▼'}
          </button>
        )}
      </div>

      {expanded && rec.narrative && (
        <div className="narrativeExpander">
          <span className="narrativeLabel">AI commentary</span>
          <span className="narrativeNote muted">
            Venice AI — this narrative does not drive any position decision.
          </span>
          <p className="narrativeText">{rec.narrative}</p>
        </div>
      )}
    </li>
  )
}

// SignalIntelligencePanel renders the feed of sharp-movement signals detected
// by the sharp-movement-detector agent. Venice LLM narratives are shown in a
// collapsible section and are clearly labelled as commentary only.
export function SignalIntelligencePanel({ signals }: { signals: SignalRecord[] }) {
  return (
    <article className="card">
      <div className="cardHead">
        <h2>Signal Intelligence</h2>
        <span className="pill">{signals.length} signals</span>
      </div>

      {signals.length === 0 ? (
        <p className="muted">
          No sharp-movement signals yet — sharp-movement-detector is watching TxLINE odds.
        </p>
      ) : (
        <ol className="signalList">
          {signals.map((rec) => (
            <SignalRow key={rec.idempotencyKey} rec={rec} />
          ))}
        </ol>
      )}
    </article>
  )
}
