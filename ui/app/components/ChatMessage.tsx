/**
 * ChatMessage
 *
 * Renders one ChatItem as a conversation bubble. Agent output sits on the
 * left (pitch green), user turns on the right (white). Structured records
 * reuse the feedCard designs from activity-feed.css so signals, positions,
 * and settlements look identical wherever they surface.
 */

import type { ReactNode } from 'react'
import type { AgentRun, CoralMessage, OddsQuote } from '../../types'
import type {
  ArenaPosition,
  SettlementRecord,
  SignalRecord,
} from '../../core/agent/types'
import { explainRunOutcome, type ChatItem } from '../../core/chat/types'

// ── Helpers ────────────────────────────────────────────────────────────────────

function timeLabel(ms: number): string {
  const delta = Date.now() - ms
  if (delta < 60_000) return 'just now'
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h ago`
  return new Date(ms).toLocaleDateString()
}

function moveBadgeColor(pct: number): string {
  if (pct >= 5) return '#ff6b6b'
  if (pct >= 2) return '#ffa94d'
  return '#69db7c'
}

function confidenceColor(conf: number): string {
  if (conf >= 0.8) return '#69db7c'
  if (conf >= 0.5) return '#ffa94d'
  return '#ff6b6b'
}

function AgentAvatar() {
  return (
    <span className="chatAvatar" aria-hidden="true">
      <span className="chatAvatarDot" />
    </span>
  )
}

/** Left-side wrapper: avatar + bubble + timestamp. */
function AgentRow({ ts, children, wide }: { ts: number; children: ReactNode; wide?: boolean }) {
  return (
    <div className={`chatRow agent${wide ? ' wide' : ''}`}>
      <AgentAvatar />
      <div className="chatRowBody">
        {children}
        <span className="chatTime">{timeLabel(ts)}</span>
      </div>
    </div>
  )
}

// ── Structured cards (rendered inside agent rows) ──────────────────────────────

function SignalCard({ signal }: { signal: SignalRecord }) {
  return (
    <div className="feedCard feedSignal chatCard">
      <div className="feedCardIcon">📡</div>
      <div className="feedCardBody">
        <div className="feedCardHeader">
          <span className="feedCardTitle">Sharp Movement Detected</span>
        </div>
        <p className="feedCardFixture">{signal.fixtureName}</p>
        <div className="feedCardMeta">
          <span className="feedBadge" style={{ background: moveBadgeColor(signal.movePct) }}>
            {signal.direction === 'shortened' ? '↓' : '↑'} {signal.movePct.toFixed(1)}%
          </span>
          <span className="feedDetail">
            {signal.selection} · {signal.oddsPrev.toFixed(2)} → {signal.oddsNow.toFixed(2)}
          </span>
          <span className="feedConfidence">
            <span className="confDot" style={{ background: confidenceColor(signal.confidence) }} />
            {(signal.confidence * 100).toFixed(0)}% conf
          </span>
        </div>
        {signal.narrative && <p className="feedNarrative">{signal.narrative}</p>}
      </div>
    </div>
  )
}

function PositionCard({ position }: { position: ArenaPosition }) {
  const isFollow = position.strategy === 'FollowSharp'
  return (
    <div className="feedCard feedPosition chatCard">
      <div className="feedCardIcon">{isFollow ? '📈' : '📉'}</div>
      <div className="feedCardBody">
        <div className="feedCardHeader">
          <span className="feedCardTitle">
            Position Taken · {isFollow ? 'Follow Sharp' : 'Fade Sharp'}
          </span>
        </div>
        <div className="feedCardMeta">
          <span className="feedBadge" style={{ background: isFollow ? '#9945FF' : '#e599f7' }}>
            {position.direction}
          </span>
          <span className="feedDetail">
            {position.selection} @ {position.oddsAtEntry.toFixed(2)}
          </span>
          <span className="feedConfidence">
            <span className="confDot" style={{ background: confidenceColor(position.confidence) }} />
            {(position.confidence * 100).toFixed(0)}%
          </span>
        </div>
        {position.outcome && (
          <div className="feedOutcome" data-won={position.outcome.selectionWon}>
            {position.outcome.selectionWon ? '✅' : '❌'} {position.outcome.finalScore} ·{' '}
            <span style={{ fontWeight: 700 }}>
              {position.outcome.pnlPoints >= 0 ? '+' : ''}
              {position.outcome.pnlPoints.toFixed(2)} pts
            </span>
          </div>
        )}
      </div>
    </div>
  )
}

function SettlementCard({ record }: { record: SettlementRecord }) {
  const isWin = record.result === 'win'
  return (
    <div className={`feedCard feedSettlement chatCard ${isWin ? 'feedWin' : 'feedLoss'}`}>
      <div className="feedCardIcon">{isWin ? '🏆' : '❌'}</div>
      <div className="feedCardBody">
        <div className="feedCardHeader">
          <span className="feedCardTitle">Settlement · {isWin ? 'Win' : 'Loss'}</span>
        </div>
        <div className="feedCardMeta">
          <span className="feedBadge" style={{ background: isWin ? '#69db7c' : '#ff6b6b' }}>
            {record.pnlUnits >= 0 ? '+' : ''}
            {record.pnlUnits.toFixed(2)} units
          </span>
          <span className="feedDetail">
            {record.selection} · {record.strategy} · odds {record.oddsAtEntry.toFixed(2)}
          </span>
        </div>
      </div>
    </div>
  )
}

/** First quote per distinct outcome, capped — one line of 1X2-style chips. */
function uniqueOutcomeQuotes(odds: OddsQuote[], max = 3): OddsQuote[] {
  const seen = new Set<string>()
  const picked: OddsQuote[] = []
  for (const quote of odds) {
    if (seen.has(quote.outcome)) continue
    seen.add(quote.outcome)
    picked.push(quote)
    if (picked.length >= max) break
  }
  return picked
}

/**
 * Round result card — the agent's answer to "analyze this fixture". Shows the
 * market snapshot the round actually saw (odds + score) and the outcome in
 * plain language instead of raw verdict strings.
 */
function RoundCard({ run }: { run: AgentRun }) {
  const outcome = explainRunOutcome(run)
  const quotes = uniqueOutcomeQuotes(run.trigger.odds ?? [])
  const score = run.trigger.score
  const historical = run.trigger.body.includes('historical snapshot')

  return (
    <div className="chatRoundCard">
      <div className="chatRoundHead">
        <span className="chatRoundTitle">{run.trigger.title}</span>
        <span className="chatRoundTrack">
          {run.track} round{historical ? ' · historical' : ''}
        </span>
      </div>

      {(score || quotes.length > 0) && (
        <div className="chatRoundMarket">
          {score && (
            <span className="chatRoundScorePill">
              FT {score.home}–{score.away}
            </span>
          )}
          {quotes.map((quote) => (
            <span key={quote.outcome} className="chatOddsChip">
              {quote.outcome} <strong>{quote.decimal.toFixed(2)}</strong>
              <em>{Math.round(quote.impliedProbability * 100)}%</em>
            </span>
          ))}
        </div>
      )}
      {quotes.length === 0 && (
        <p className="chatRoundNoOdds">No odds quotes in this snapshot.</p>
      )}

      <div className="chatRoundOutcome">
        <span className="chatRoundIcon">{outcome.icon}</span>
        <div>
          <strong>{outcome.headline}</strong>
          {outcome.detail && <p>{outcome.detail}</p>}
        </div>
      </div>

      {run.delivery && (
        <p className="chatRoundDelivery">
          📦 {run.delivery.title}
          {run.delivery.strategy ? ` — ${run.delivery.strategy}` : ''}
        </p>
      )}
    </div>
  )
}

// ── Coral message rendering ─────────────────────────────────────────────────────

function payloadPreview(payload: unknown): string | undefined {
  if (payload === undefined || payload === null) return undefined
  try {
    return JSON.stringify(payload, null, 2)
  } catch {
    return String(payload)
  }
}

function CoralBubble({ message }: { message: CoralMessage }) {
  switch (message.verb) {
    case 'AGENT_THOUGHT':
      return (
        <div className="chatBubble agentBubble thought">
          <span className="chatSender">{message.from}</span>
          <em>{message.text}</em>
        </div>
      )
    case 'TOOL_CALL':
    case 'TOOL_RESULT': {
      const preview = payloadPreview(message.payload)
      return (
        <details className="chatToolCard">
          <summary>
            <span className="chatToolIcon">🛠</span>
            {message.from} {message.verb === 'TOOL_CALL' ? 'used a tool' : 'got a tool result'}
            <span className="chatToolText">{message.text}</span>
          </summary>
          {preview && <pre className="chatToolPayload">{preview}</pre>}
        </details>
      )
    }
    case 'SIGNAL':
      return (
        <div className="chatBubble agentBubble alert">
          <span className="chatSender">{message.from}</span>
          <span className="chatVerb">📡 SIGNAL</span>
          {message.text}
        </div>
      )
    default:
      return (
        <div className="chatBubble agentBubble">
          <span className="chatSender">{message.from}</span>
          <span className="chatVerb">{message.verb.split('_').join(' ')}</span>
          {message.text}
        </div>
      )
  }
}

// ── ChatMessage ─────────────────────────────────────────────────────────────────

export function ChatMessage({ item }: { item: ChatItem }) {
  switch (item.kind) {
    case 'user':
      return (
        <div className="chatRow user">
          <div className="chatRowBody">
            <div className="chatBubble userBubble">{item.text}</div>
            <span className="chatTime">{timeLabel(item.ts)}</span>
          </div>
        </div>
      )
    case 'agent':
      return (
        <AgentRow ts={item.ts}>
          <div className="chatBubble agentBubble">{item.text}</div>
        </AgentRow>
      )
    case 'coral':
      return (
        <AgentRow ts={item.ts} wide={item.message.verb === 'TOOL_CALL' || item.message.verb === 'TOOL_RESULT'}>
          <CoralBubble message={item.message} />
        </AgentRow>
      )
    case 'round':
      return (
        <AgentRow ts={item.ts} wide>
          <RoundCard run={item.run} />
        </AgentRow>
      )
    case 'signal':
      return (
        <AgentRow ts={item.ts} wide>
          <SignalCard signal={item.signal} />
        </AgentRow>
      )
    case 'position':
      return (
        <AgentRow ts={item.ts} wide>
          <PositionCard position={item.position} />
        </AgentRow>
      )
    case 'settlement':
      return (
        <AgentRow ts={item.ts} wide>
          <SettlementCard record={item.settlement} />
        </AgentRow>
      )
  }
}
