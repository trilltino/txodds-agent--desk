import type { AgentTraceEvent } from '../../../types'
import type { Wager, WagerRuling } from '../../../core/agent/types'
import { WagerPaymentApproval } from './WagerPaymentApproval'

// WagerPanel renders the fundamentals agent's proposed wagers (rig-venice
// ROADMAP.md Phases 4-5) as legible cards instead of raw trace JSON, plus
// the wallet-approval-before-settlement flow (Phase 7, item 3) for wagers
// that carry a real stake. There is no dedicated Tauri command/event for
// wager rulings themselves yet — rulings are pulled out of the existing run
// trace's `wagerRuling` payload (see `wager_ruling_payload` in
// native/src/services/agent/runtime.rs).

interface RulingWithRun {
  ruling: WagerRuling
  runId: string
}

function isWagerRuling(value: unknown): value is WagerRuling {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Record<string, unknown>
  return typeof candidate.reason === 'string' && typeof candidate.wager === 'object' && candidate.wager !== null
}

function extractRuling(event: AgentTraceEvent): RulingWithRun | undefined {
  if (typeof event.payload !== 'object' || event.payload === null) return undefined
  const payload = event.payload as Record<string, unknown>
  return isWagerRuling(payload.wagerRuling) ? { ruling: payload.wagerRuling, runId: event.runId } : undefined
}

const STATUS_LABELS: Record<Wager['status'], string> = {
  proposed: 'Proposed',
  debated: 'Debated',
  no_bet: 'No Bet',
  proof_passed: 'Proof Passed',
  proof_failed: 'Proof Failed',
  signed: 'Signed',
  settled: 'Settled',
  refunded: 'Refunded',
}

function statusColor(status: Wager['status']): string {
  switch (status) {
    case 'no_bet':
    case 'proof_failed':
      return '#94a3b8'
    case 'proof_passed':
    case 'settled':
      return '#22c55e'
    case 'proposed':
    case 'debated':
      return '#38bdf8'
    default:
      return '#a78bfa'
  }
}

function WagerCard({ ruling, runId }: { ruling: WagerRuling; runId: string }) {
  const { wager } = ruling
  const isBet = wager.status !== 'no_bet'
  const canRequestPayment = isBet && wager.stakeSol > 0
  return (
    <div className="wagerCard">
      <div className="wagerCardHead">
        <strong>{wager.selection}</strong>
        <span className="pill" style={{ background: statusColor(wager.status) }}>
          {STATUS_LABELS[wager.status]}
        </span>
      </div>
      <div className="wagerCardStats">
        <span>model {(wager.modelProb * 100).toFixed(1)}%</span>
        <span>market {(wager.marketImplied * 100).toFixed(1)}%</span>
        <span className={wager.edge > 0 ? 'pnlPositive' : 'pnlNegative'}>
          edge {(wager.edge * 100).toFixed(1)}%
        </span>
        {isBet && <span>{wager.stakeSol.toFixed(5)} SOL</span>}
      </div>
      <p className="wagerCardThesis">{wager.thesis}</p>
      <p className="muted" style={{ fontSize: '0.75rem' }}>{ruling.reason}</p>
      {canRequestPayment && <WagerPaymentApproval wager={wager} runId={runId} />}
    </div>
  )
}

export function WagerPanel({ trace }: { trace: AgentTraceEvent[] }) {
  const rulings = trace
    .map(extractRuling)
    .filter((r): r is RulingWithRun => r !== undefined)
    // Newest first; trace arrives in round order.
    .reverse()

  return (
    <article className="card">
      <div className="cardHead">
        <h2>Wager Proposals</h2>
        {rulings.length > 0 && <span className="pill">{rulings.length}</span>}
      </div>
      {rulings.length === 0 ? (
        <p className="muted">No wager reasoning yet for this run.</p>
      ) : (
        <div className="wagerCardList">
          {rulings.map(({ ruling, runId }, i) => (
            <WagerCard key={`${ruling.wager.wagerId}-${i}`} ruling={ruling} runId={runId} />
          ))}
        </div>
      )}
    </article>
  )
}
