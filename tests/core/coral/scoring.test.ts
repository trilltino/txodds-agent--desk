/**
 * tests/core/coral/scoring.test.ts
 *
 * Unit tests for `ui/core/coral/scoring.ts`.
 *
 * `TrackMode` is currently the single literal `'trading'`, so all tests use
 * that track.  The `scoreBid` implementation contains guards for `'fan'` and
 * `'settlement'` tracks which are intentionally unreachable today — those
 * branches are not tested here to stay in-sync with the type system.
 *
 * Coverage targets:
 *  1. `scoreBid` – role boosts on the trading track, price penalty, ETA bonus.
 *  2. `chooseWinner` – correct winner, no mutation, determinism, edge cases.
 */

import { describe, expect, it } from 'vitest'
import { scoreBid, chooseWinner } from '../../../ui/core/coral/scoring'
import { makeAgentBid } from '../../__helpers__/fixtures'

const TRACK = 'trading' as const

// ── scoreBid ──────────────────────────────────────────────────────────────────

describe('scoreBid', () => {
  it('returns a positive score for a valid bid', () => {
    const bid = makeAgentBid({ confidence: 0.8, priceSol: 0.05, etaMs: 1000 })
    expect(scoreBid(TRACK, bid)).toBeGreaterThan(0)
  })

  it('applies 1.25× role boost for sharp in the trading track', () => {
    // pundit has boost 1.0 in trading; sharp has 1.25
    const pundit = makeAgentBid({ role: 'pundit', confidence: 1, priceSol: 0, etaMs: 2000 })
    const sharp = makeAgentBid({ role: 'sharp', confidence: 1, priceSol: 0, etaMs: 2000 })
    expect(scoreBid(TRACK, sharp)).toBeGreaterThan(scoreBid(TRACK, pundit))
  })

  it('applies 1.15× role boost for risk in the trading track', () => {
    const pundit = makeAgentBid({ role: 'pundit', confidence: 1, priceSol: 0, etaMs: 2000 })
    const risk = makeAgentBid({ role: 'risk', confidence: 1, priceSol: 0, etaMs: 2000 })
    expect(scoreBid(TRACK, risk)).toBeGreaterThan(scoreBid(TRACK, pundit))
  })

  it('sharp outscores risk in the trading track (1.25 > 1.15)', () => {
    const sharp = makeAgentBid({ role: 'sharp', confidence: 1, priceSol: 0, etaMs: 2000 })
    const risk = makeAgentBid({ role: 'risk', confidence: 1, priceSol: 0, etaMs: 2000 })
    expect(scoreBid(TRACK, sharp)).toBeGreaterThan(scoreBid(TRACK, risk))
  })

  it('non-trading-specialist roles have no boost (multiplier 1.0)', () => {
    // pundit, fan, settlement, verifier all get boost=1 in trading track
    const roles = ['pundit', 'fan', 'settlement', 'verifier'] as const
    for (const role of roles) {
      const bid = makeAgentBid({ role, confidence: 1, priceSol: 0, etaMs: 2000 })
      // priceSol=0 → penalty=1, etaMs≥1500 → bonus=1, boost=1 → score = 1*1*1*1 = 1
      expect(scoreBid(TRACK, bid)).toBeCloseTo(1, 5)
    }
  })

  it('price penalty: higher priceSol lowers the score', () => {
    const free = makeAgentBid({ role: 'pundit', confidence: 1, priceSol: 0, etaMs: 2000 })
    const expensive = makeAgentBid({ role: 'pundit', confidence: 1, priceSol: 0.2, etaMs: 2000 })
    // 0.2 → 1 - 0.2*4 = 0.2 penalty
    expect(scoreBid(TRACK, expensive)).toBeLessThan(scoreBid(TRACK, free))
  })

  it('price penalty is clamped at 0.2 minimum', () => {
    // priceSol=1 → 1 - 1*4 = -3 → clamped to 0.2
    const bid = makeAgentBid({ confidence: 1, priceSol: 1, etaMs: 2000 })
    const score = scoreBid(TRACK, bid)
    expect(score).toBeGreaterThanOrEqual(0.2)
  })

  it('ETA bonus of 1.05× when etaMs < 1500', () => {
    const fast = makeAgentBid({ role: 'pundit', confidence: 1, priceSol: 0, etaMs: 1000 })
    const slow = makeAgentBid({ role: 'pundit', confidence: 1, priceSol: 0, etaMs: 2000 })
    expect(scoreBid(TRACK, fast)).toBeGreaterThan(scoreBid(TRACK, slow))
  })

  it('ETA boundary: exactly 1500ms does NOT get the bonus', () => {
    const at1500 = makeAgentBid({ confidence: 1, priceSol: 0, etaMs: 1500 })
    const at1499 = makeAgentBid({ confidence: 1, priceSol: 0, etaMs: 1499 })
    expect(scoreBid(TRACK, at1499)).toBeGreaterThan(scoreBid(TRACK, at1500))
  })

  it('unknown role gets no boost (multiplier 1.0)', () => {
    const bid = makeAgentBid({ role: 'wizard' as never, confidence: 1, priceSol: 0, etaMs: 2000 })
    expect(scoreBid(TRACK, bid)).toBeCloseTo(1, 5)
  })

  it('score scales linearly with confidence', () => {
    const low = makeAgentBid({ confidence: 0.5, priceSol: 0, etaMs: 2000, role: 'pundit' })
    const high = makeAgentBid({ confidence: 1.0, priceSol: 0, etaMs: 2000, role: 'pundit' })
    expect(scoreBid(TRACK, high)).toBeCloseTo(scoreBid(TRACK, low) * 2, 5)
  })
})

// ── chooseWinner ───────────────────────────────────────────────────────────────

describe('chooseWinner', () => {
  it('returns undefined for an empty bids array', () => {
    expect(chooseWinner(TRACK, [])).toBeUndefined()
  })

  it('returns the only bid when array has one entry', () => {
    const bid = makeAgentBid()
    expect(chooseWinner(TRACK, [bid])).toBe(bid)
  })

  it('returns the bid with the highest score', () => {
    const weak = makeAgentBid({ confidence: 0.4, priceSol: 0.2, etaMs: 2000, role: 'pundit' })
    const strong = makeAgentBid({ confidence: 0.9, priceSol: 0, etaMs: 500, role: 'sharp' })
    expect(chooseWinner(TRACK, [weak, strong])).toBe(strong)
  })

  it('does not mutate the original bids array', () => {
    const bids = [
      makeAgentBid({ confidence: 0.5 }),
      makeAgentBid({ confidence: 0.9 }),
      makeAgentBid({ confidence: 0.3 }),
    ]
    const originalOrder = bids.map((b) => b.agentId)
    chooseWinner(TRACK, bids)
    expect(bids.map((b) => b.agentId)).toEqual(originalOrder)
  })

  it('is deterministic: calling twice returns the same winner', () => {
    const bids = [
      makeAgentBid({ confidence: 0.7, priceSol: 0.01, etaMs: 800, role: 'sharp' }),
      makeAgentBid({ confidence: 0.6, priceSol: 0.0, etaMs: 2000, role: 'risk' }),
    ]
    expect(chooseWinner(TRACK, bids)).toBe(chooseWinner(TRACK, bids))
  })

  it('prefers lower price when all other factors are equal', () => {
    const pricey = makeAgentBid({ confidence: 0.8, priceSol: 0.15, etaMs: 2000, role: 'pundit' })
    const cheap = makeAgentBid({ confidence: 0.8, priceSol: 0.0, etaMs: 2000, role: 'pundit' })
    expect(chooseWinner(TRACK, [pricey, cheap])).toBe(cheap)
  })

  it('picks winner among three bids correctly', () => {
    const a = makeAgentBid({ confidence: 0.6, priceSol: 0, etaMs: 2000, role: 'risk' })
    const b = makeAgentBid({ confidence: 0.9, priceSol: 0, etaMs: 500, role: 'sharp' })
    const c = makeAgentBid({ confidence: 0.5, priceSol: 0, etaMs: 2000, role: 'pundit' })
    expect(chooseWinner(TRACK, [a, b, c])).toBe(b)
  })
})
