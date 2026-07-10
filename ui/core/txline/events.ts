import type { OddsQuote, TxLineEvent } from '../../types'

// Convert odds quote deltas into a higher-level trigger event once implied
// probability has moved enough to justify an agent round.
export function detectOddsMove(prev: OddsQuote[], next: OddsQuote[], thresholdPct = 5): TxLineEvent | null {
  for (const q of next) {
    const old = prev.find((p) => p.fixtureId === q.fixtureId && p.outcome === q.outcome)
    if (!old) continue
    // Strategy thresholds work in percentage points so a move from 54% to 59%
    // is treated as a 5pp move, not a relative 9.2% move.
    const movePp = Math.abs(q.impliedProbability - old.impliedProbability) * 100
    if (movePp >= thresholdPct) {
      return {
        id: `odds-${q.fixtureId}-${q.outcome}-${Date.now()}`,
        kind: 'odds_move',
        fixtureId: q.fixtureId,
        statKeys: [`odds.${q.outcome}.implied_probability`],
        schemaFamily: 'odds',
        title: `${q.outcome} moved ${movePp.toFixed(1)}pp`,
        body: `Implied probability changed from ${(old.impliedProbability * 100).toFixed(1)}% to ${(q.impliedProbability * 100).toFixed(1)}%.`,
        ts: q.ts,
        odds: next
      }
    }
  }
  return null
}

// These event kinds are strong enough to start autonomous work. Other updates
// can still render in the feed without opening a market round.
export function eventShouldStartRound(event: TxLineEvent): boolean {
  return ['goal', 'red_card', 'final_whistle', 'odds_move', 'proof_received'].includes(event.kind)
}
