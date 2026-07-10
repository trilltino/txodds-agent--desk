/**
 * tests/e2e/fixture-to-round.test.ts
 *
 * End-to-end flow test: live fixture → odds move detected → round triggered.
 *
 * This test exercises the complete TypeScript pipeline without a Tauri runtime:
 *
 *   1. A raw TxLINE fixture payload is normalised into a `Fixture`.
 *   2. Two successive odds snapshots trigger `detectOddsMove`.
 *   3. The resulting `TxLineEvent` is classified by `eventShouldStartRound`.
 *   4. The winning agent is selected by `chooseWinner` on the trading track.
 *
 * No mocks are needed for the pure functions exercised here. The native bridge
 * (`desktop/transport`) is not called in this path.
 */

import { describe, expect, it } from 'vitest'
import { normalizeFixtures } from '../../ui/core/txline/fixtures'
import { detectOddsMove, eventShouldStartRound } from '../../ui/core/txline/events'
import { chooseWinner } from '../../ui/core/coral/scoring'
import { makeOddsQuote, makeAgentBid } from '../__helpers__/fixtures'

// ── Full pipeline ─────────────────────────────────────────────────────────────

describe('fixture → odds-move → round trigger pipeline', () => {
  const RAW_PAYLOAD = {
    fixtures: [
      {
        fixtureId: 5001,
        home: 'Brazil',
        away: 'Argentina',
        startTime: '2026-06-14T14:00:00Z',
        competition: 'FIFA World Cup 2026',
        status: 'InPlay',
      },
    ],
  }

  it('step 1 — raw payload normalises to a single Fixture', () => {
    const fixtures = normalizeFixtures(RAW_PAYLOAD)
    expect(fixtures).toHaveLength(1)
    expect(fixtures[0].fixtureId).toBe(5001)
    expect(fixtures[0].home).toBe('Brazil')
    expect(fixtures[0].competition).toBe('FIFA World Cup 2026')
  })

  it('step 2 — sub-threshold move does not trigger a round', () => {
    // 3pp move — below default 5pp threshold
    const prev = [
      makeOddsQuote({ fixtureId: 5001, outcome: 'home', impliedProbability: 0.50 }),
    ]
    const next = [
      makeOddsQuote({ fixtureId: 5001, outcome: 'home', impliedProbability: 0.53 }),
    ]
    const event = detectOddsMove(prev, next)
    expect(event).toBeNull()
  })

  it('step 2 — 6pp odds move produces an odds_move event', () => {
    const prev = [
      makeOddsQuote({ fixtureId: 5001, outcome: 'home', impliedProbability: 0.50 }),
      makeOddsQuote({ fixtureId: 5001, outcome: 'away', impliedProbability: 0.35 }),
    ]
    const next = [
      makeOddsQuote({ fixtureId: 5001, outcome: 'home', impliedProbability: 0.56 }),
      makeOddsQuote({ fixtureId: 5001, outcome: 'away', impliedProbability: 0.33 }),
    ]
    const event = detectOddsMove(prev, next)
    expect(event).not.toBeNull()
    expect(event!.kind).toBe('odds_move')
    expect(event!.fixtureId).toBe(5001)
  })

  it('step 3 — odds_move event is classified as round-triggering', () => {
    const prev = [makeOddsQuote({ fixtureId: 5001, outcome: 'home', impliedProbability: 0.50 })]
    const next = [makeOddsQuote({ fixtureId: 5001, outcome: 'home', impliedProbability: 0.56 })]
    const event = detectOddsMove(prev, next)!
    expect(eventShouldStartRound(event)).toBe(true)
  })

  it('step 4 — chooseWinner selects the best agent bid for the trading round', () => {
    const bids = [
      makeAgentBid({ agentId: 'sharp-agent', role: 'sharp', confidence: 0.85, priceSol: 0.02, etaMs: 800 }),
      makeAgentBid({ agentId: 'risk-agent', role: 'risk', confidence: 0.70, priceSol: 0.05, etaMs: 1200 }),
      makeAgentBid({ agentId: 'fan-agent', role: 'fan', confidence: 0.60, priceSol: 0.01, etaMs: 2500 }),
    ]
    const winner = chooseWinner('trading', bids)
    // sharp-agent has highest confidence + role boost on trading track
    expect(winner?.agentId).toBe('sharp-agent')
  })

  it('full pipeline — fixture normalisation → event detection → winner selection', () => {
    // All four steps in one test for a complete narrative trace
    const fixtures = normalizeFixtures(RAW_PAYLOAD)
    expect(fixtures[0].fixtureId).toBe(5001)

    const prev = [makeOddsQuote({ fixtureId: fixtures[0].fixtureId, outcome: 'home', impliedProbability: 0.48 })]
    const next = [makeOddsQuote({ fixtureId: fixtures[0].fixtureId, outcome: 'home', impliedProbability: 0.55 })]

    const event = detectOddsMove(prev, next)
    expect(event).not.toBeNull()
    expect(eventShouldStartRound(event!)).toBe(true)

    const bids = [
      makeAgentBid({ role: 'sharp', confidence: 0.9, priceSol: 0, etaMs: 1000 }),
      makeAgentBid({ role: 'pundit', confidence: 0.6, priceSol: 0, etaMs: 2000 }),
    ]
    const winner = chooseWinner('trading', bids)
    expect(winner?.role).toBe('sharp')
  })
})

// ── Goal event short-circuit ───────────────────────────────────────────────────

describe('goal event pipeline', () => {
  it('a goal event always triggers a round regardless of odds data', () => {
    const goalEvent = {
      id: 'goal-5001-1',
      kind: 'goal' as const,
      fixtureId: 5001,
      statKeys: ['goals.home'],
      schemaFamily: 'scores',
      title: 'GOAL — Brazil',
      body: 'Brazil 1-0 Argentina (35\')',
      ts: new Date().toISOString(),
    }
    expect(eventShouldStartRound(goalEvent)).toBe(true)
  })
})

// ── Red card repricing ─────────────────────────────────────────────────────────

describe('red card pipeline', () => {
  it('red_card event triggers a round', () => {
    const rcEvent = {
      id: 'rc-5001-1',
      kind: 'red_card' as const,
      fixtureId: 5001,
      statKeys: ['cards.home'],
      schemaFamily: 'scores',
      title: 'RED CARD — Argentina',
      body: 'Argentina down to 10 men',
      ts: new Date().toISOString(),
    }
    expect(eventShouldStartRound(rcEvent)).toBe(true)
  })

  it('score_update event does NOT trigger a round', () => {
    const scoreUpdate = {
      id: 'su-5001-1',
      kind: 'score_update' as const,
      fixtureId: 5001,
      statKeys: ['score'],
      schemaFamily: 'scores',
      title: 'Score update',
      body: 'Brazil 1-0 Argentina',
      ts: new Date().toISOString(),
    }
    expect(eventShouldStartRound(scoreUpdate)).toBe(false)
  })
})
