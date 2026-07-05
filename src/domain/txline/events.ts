import type { OddsQuote, TxLineEvent } from '../../types'

export function detectOddsMove(prev: OddsQuote[], next: OddsQuote[], thresholdPct = 5): TxLineEvent | null {
  for (const q of next) {
    const old = prev.find((p) => p.fixtureId === q.fixtureId && p.outcome === q.outcome)
    if (!old) continue
    const movePp = Math.abs(q.impliedProbability - old.impliedProbability) * 100
    if (movePp >= thresholdPct) {
      return {
        id: `odds-${q.fixtureId}-${q.outcome}-${Date.now()}`,
        kind: 'odds_move',
        fixtureId: q.fixtureId,
        title: `${q.outcome} moved ${movePp.toFixed(1)}pp`,
        body: `Implied probability changed from ${(old.impliedProbability * 100).toFixed(1)}% to ${(q.impliedProbability * 100).toFixed(1)}%.`,
        ts: q.ts,
        odds: next
      }
    }
  }
  return null
}

export function eventShouldStartRound(event: TxLineEvent): boolean {
  return ['goal', 'red_card', 'final_whistle', 'odds_move', 'proof_received'].includes(event.kind)
}
