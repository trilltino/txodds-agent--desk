/**
 * tests/core/txline/events.test.ts
 *
 * Unit tests for `ui/core/txline/events.ts`.
 *
 * Coverage targets:
 *  - detectOddsMove: threshold logic, no-previous-quote, below-threshold,
 *    above-threshold, correct event shape.
 *  - eventShouldStartRound: positive and negative kind classification.
 */

import { describe, expect, it } from 'vitest'
import { detectOddsMove, eventShouldStartRound } from '../../../ui/core/txline/events'
import { makeOddsQuote, makeTxLineEvent } from '../../__helpers__/fixtures'

// ── detectOddsMove ─────────────────────────────────────────────────────────────

describe('detectOddsMove', () => {
  it('returns null when there are no previous quotes to compare', () => {
    const next = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.55 })]
    expect(detectOddsMove([], next)).toBeNull()
  })

  it('returns null when the move is below the default 5pp threshold', () => {
    const prev = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.50 })]
    // 0.54 - 0.50 = 0.04 → 4pp — below threshold
    const next = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.54 })]
    expect(detectOddsMove(prev, next)).toBeNull()
  })

  it('returns null when the move is exactly below a custom threshold', () => {
    const prev = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.50 })]
    const next = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.59 })]
    // 9pp move but custom threshold is 10pp
    expect(detectOddsMove(prev, next, 10)).toBeNull()
  })

  it('returns an event when the move meets the default 5pp threshold', () => {
    const prev = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.50 })]
    // 0.55 - 0.50 = 0.05 → exactly 5pp — should trigger
    const next = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.55 })]
    const event = detectOddsMove(prev, next)
    expect(event).not.toBeNull()
    expect(event?.kind).toBe('odds_move')
    expect(event?.fixtureId).toBe(1001)
  })

  it('returns an event when the move exceeds the threshold', () => {
    const prev = [makeOddsQuote({ outcome: 'away', impliedProbability: 0.30 })]
    const next = [makeOddsQuote({ outcome: 'away', impliedProbability: 0.42 })]
    const event = detectOddsMove(prev, next)
    expect(event).not.toBeNull()
    expect(event?.odds).toBe(next)
  })

  it('detects a move in either direction (lengthened)', () => {
    const prev = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.65 })]
    const next = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.58 })]
    const event = detectOddsMove(prev, next)
    expect(event).not.toBeNull()
  })

  it('skips quotes with no matching previous entry', () => {
    const prev = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.50 })]
    // 'draw' has no prev entry; 'home' move is below threshold
    const next = [
      makeOddsQuote({ outcome: 'draw', impliedProbability: 0.25 }),
      makeOddsQuote({ outcome: 'home', impliedProbability: 0.52 }),
    ]
    expect(detectOddsMove(prev, next)).toBeNull()
  })

  it('emits an event containing the full next quotes array', () => {
    const prev = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.50 })]
    const next = [
      makeOddsQuote({ outcome: 'home', impliedProbability: 0.60 }),
      makeOddsQuote({ outcome: 'away', impliedProbability: 0.25 }),
    ]
    const event = detectOddsMove(prev, next)
    expect(event?.odds).toHaveLength(2)
  })

  it('produces a statKeys array pointing at the moved outcome', () => {
    const prev = [makeOddsQuote({ outcome: 'draw', impliedProbability: 0.28 })]
    const next = [makeOddsQuote({ outcome: 'draw', impliedProbability: 0.35 })]
    const event = detectOddsMove(prev, next)
    expect(event?.statKeys).toContain('odds.draw.implied_probability')
  })

  it('includes human-readable body text with from/to percentages', () => {
    const prev = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.50 })]
    const next = [makeOddsQuote({ outcome: 'home', impliedProbability: 0.60 })]
    const event = detectOddsMove(prev, next)
    expect(event?.body).toContain('50.0%')
    expect(event?.body).toContain('60.0%')
  })
})

// ── eventShouldStartRound ──────────────────────────────────────────────────────

describe('eventShouldStartRound', () => {
  it.each(['goal', 'red_card', 'final_whistle', 'odds_move', 'proof_received'])(
    'returns true for kind "%s"',
    (kind) => {
      const event = makeTxLineEvent({ kind: kind as never })
      expect(eventShouldStartRound(event)).toBe(true)
    },
  )

  it.each(['fixture', 'score_update', 'odds_update'])(
    'returns false for informational kind "%s"',
    (kind) => {
      const event = makeTxLineEvent({ kind: kind as never })
      expect(eventShouldStartRound(event)).toBe(false)
    },
  )
})
