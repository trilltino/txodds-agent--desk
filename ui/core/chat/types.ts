// Chat contract for the conversational Agent Desk surface.
//
// The chat log is a merged, chronological view over several backend streams
// (CoralMessage, SignalRecord, ArenaPosition, SettlementRecord) plus local
// user inputs and synthesized agent replies. Each variant keeps its source
// record intact so renderers can show the full detail without re-fetching.

import type { AgentRun, CoralMessage } from '../../types'
import type {
  ArenaPosition,
  ArenaScore,
  BacktestSummary,
  SettlementRecord,
  SignalRecord,
} from '../agent/types'

export type ChatItem =
  /** Text the user typed (or a quick-action they tapped). */
  | { kind: 'user'; id: string; text: string; ts: number }
  /** Locally synthesized agent reply (acknowledgements, score answers, errors). */
  | { kind: 'agent'; id: string; text: string; ts: number }
  /** A CoralMessage streamed from the Rust agent runtime. */
  | { kind: 'coral'; id: string; message: CoralMessage; ts: number }
  /** Completed agent round rendered as a rich result card. */
  | { kind: 'round'; id: string; run: AgentRun; ts: number }
  /** Sharp movement signal rendered as an alert bubble. */
  | { kind: 'signal'; id: string; signal: SignalRecord; ts: number }
  /** Arena position rendered as a compact position card. */
  | { kind: 'position'; id: string; position: ArenaPosition; ts: number }
  /** Settlement result rendered as a win/loss card. */
  | { kind: 'settlement'; id: string; settlement: SettlementRecord; ts: number }
  /** Backtest replay result — simulated history, never a live result. */
  | { kind: 'backtest'; id: string; summary: BacktestSummary; ts: number }

/** Parse an ISO timestamp defensively — bad input sorts to "now". */
export function toMs(iso: string): number {
  const ms = new Date(iso).getTime()
  return Number.isFinite(ms) ? ms : Date.now()
}

/** Plain-language translation of a run's outcome for the round result card. */
export interface RunOutcome {
  icon: string
  headline: string
  detail?: string
}

/**
 * Translate a run's verification verdict into user-facing language. Raw
 * verdict strings ("needs_review", "event stayed below autonomous signal
 * threshold") are agent internals — the card says what happened and why in
 * plain words instead.
 */
export function explainRunOutcome(run: AgentRun): RunOutcome {
  const verdict = run.verdict
  if (!verdict) {
    return {
      icon: '🔍',
      headline: 'Round completed',
      detail: 'No verification verdict was produced this round.',
    }
  }
  switch (verdict.status) {
    case 'pass':
      return { icon: '✅', headline: 'Verified', detail: humanizeReason(verdict.reason) }
    case 'fail':
      return {
        icon: '❌',
        headline: 'Verification failed',
        detail: humanizeReason(verdict.reason),
      }
    default:
      return {
        icon: '💤',
        headline: 'No action taken',
        detail: humanizeReason(verdict.reason),
      }
  }
}

// Known backend reason strings mapped to plain language; anything unmapped
// passes through as-is (better an odd sentence than silence).
function humanizeReason(reason: string): string | undefined {
  if (!reason) return undefined
  if (reason.includes('below autonomous signal threshold')) {
    return 'The market was quiet — odds didn’t move enough to cross the autonomous trading threshold, so the agent held off. Sharp pre-match or in-play moves are what trigger positions.'
  }
  if (reason.includes('incomplete 1X2')) {
    return 'The odds snapshot was missing part of the 1X2 market, so no wager could be evaluated safely.'
  }
  if (reason.includes('non-settlement decision recorded')) {
    return 'This was a simulated decision, not a real settlement — nothing was staked or paid out. Run a settlement-track round (⚖️ Verify on-chain) to trigger the real proof-gated flow.'
  }
  return reason
}

/** One-line natural-language answer for "what's the score?" style questions. */
export function describeArenaScore(score: ArenaScore | undefined): string {
  if (!score) return 'No settlements yet — the Follow vs Fade contest starts once positions settle.'
  return (
    `Follow Sharp is ${score.followWins}W–${score.followLosses}L (${score.followPnl >= 0 ? '+' : ''}${score.followPnl.toFixed(1)} pts), ` +
    `Fade Sharp is ${score.fadeWins}W–${score.fadeLosses}L (${score.fadePnl >= 0 ? '+' : ''}${score.fadePnl.toFixed(1)} pts). ` +
    `Current leader: ${score.leader}.`
  )
}
