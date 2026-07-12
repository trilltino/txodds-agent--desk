/**
 * tests/core/txline/snapshots.test.ts
 *
 * Unit tests for the odds/score snapshot parsers in `ui/core/txline/fixtures.ts`.
 *
 * The market-row and nested-score fixtures below are trimmed captures of real
 * TxLINE responses (fixture 18213979, Norway vs England, 2026-07-11) — the
 * shapes the original flat-field parsers silently dropped:
 *  - odds: market rows with parallel PriceNames/Prices arrays in milli-odds
 *  - scores: action entries with Score.ParticipantN.Total.Goals
 */

import { describe, expect, it } from 'vitest'
import { oddsRetryAsOfMs, parseOddsSnapshot, parseScoreSnapshot } from '../../../ui/core/txline/fixtures'
import type { Fixture } from '../../../ui/types'

// ── odds: TxLINE market rows ───────────────────────────────────────────────────

const MARKET_ROW = {
  FixtureId: 18213979,
  MessageId: '1837366859:00003:000569-1-10021-stab',
  Ts: 1783811396747,
  Bookmaker: 'TXLineStablePriceDemargined',
  BookmakerId: 10021,
  SuperOddsType: 'OVERUNDER_PARTICIPANT_GOALS',
  GameState: null,
  InRunning: true,
  MarketParameters: 'line=0.75',
  MarketPeriod: 'et',
  PriceNames: ['over', 'under'],
  Prices: [2075, 1930],
  Pct: ['NA', 'NA'],
}

// Real pre-kickoff capture: TxLINE names 1X2 outcomes part1/draw/part2.
const MATCH_WINNER_ROW = {
  FixtureId: 18213979,
  Ts: 1783803000000,
  Bookmaker: 'TXLineStablePriceDemargined',
  SuperOddsType: '1X2_PARTICIPANT_RESULT',
  MarketParameters: '',
  PriceNames: ['part1', 'draw', 'part2'],
  Prices: [4885, 2371, 2677],
}

describe('parseOddsSnapshot — TxLINE market rows', () => {
  it('flattens PriceNames/Prices pairs into quotes with milli-odds decoded', () => {
    const quotes = parseOddsSnapshot([MARKET_ROW], 18213979)
    expect(quotes).toHaveLength(2)
    expect(quotes[0].decimal).toBeCloseTo(2.075)
    expect(quotes[1].decimal).toBeCloseTo(1.93)
    expect(quotes[0].fixtureId).toBe(18213979)
    expect(quotes[0].source).toBe('TXLineStablePriceDemargined')
    expect(quotes[0].ts).toBe(new Date(1783811396747).toISOString())
  })

  it('labels line markets with their parameter', () => {
    const quotes = parseOddsSnapshot([MARKET_ROW], 18213979)
    expect(quotes.map((q) => q.outcome)).toEqual(['over 0.75', 'under 0.75'])
  })

  it('normalizes part1/part2 to home/away and puts full-time 1X2 first', () => {
    const quotes = parseOddsSnapshot([MARKET_ROW, MATCH_WINNER_ROW], 18213979)
    expect(quotes.slice(0, 3).map((q) => q.outcome)).toEqual(['home', 'draw', 'away'])
    expect(quotes[0].decimal).toBeCloseTo(4.885)
    expect(quotes[1].decimal).toBeCloseTo(2.371)
    expect(quotes[2].decimal).toBeCloseTo(2.677)
  })

  it('keeps period-market 1X2 (e.g. extra time) behind the full-time market', () => {
    const etRow = { ...MATCH_WINNER_ROW, MarketPeriod: 'et', Prices: [1500, 2000, 2500] }
    const quotes = parseOddsSnapshot([etRow, MATCH_WINNER_ROW], 18213979)
    expect(quotes[0].decimal).toBeCloseTo(4.885) // full-time row leads
  })

  it('computes implied probability from the decoded decimal', () => {
    const quotes = parseOddsSnapshot([MATCH_WINNER_ROW], 18213979)
    expect(quotes[0].impliedProbability).toBeCloseTo(1 / 4.885)
  })

  it('still parses flat single-quote shapes', () => {
    const quotes = parseOddsSnapshot([{ outcome: 'home', decimal: 1.85 }], 42)
    expect(quotes).toHaveLength(1)
    expect(quotes[0].decimal).toBe(1.85)
    expect(quotes[0].fixtureId).toBe(42)
  })

  it('returns empty for an empty snapshot', () => {
    expect(parseOddsSnapshot([], 42)).toEqual([])
  })
})

// ── scores: TxLINE nested action entries ──────────────────────────────────────

const SCORE_ACTION = {
  FixtureId: 18213979,
  GameState: 'scheduled',
  StartTime: 1783803600000,
  Participant1IsHome: true,
  Participant1Id: 2661,
  Participant2Id: 1888,
  Action: 'action_amend',
  Seq: 1131,
  Score: {
    Participant1: {
      H1: { Goals: 1 },
      Total: { Goals: 1, YellowCards: 1, Corners: 7 },
    },
    Participant2: {
      H1: { Goals: 1, Corners: 2 },
      ET1: { Goals: 1, Corners: 1 },
      Total: { Goals: 2, Corners: 4 },
    },
  },
}

describe('parseScoreSnapshot — TxLINE nested shape', () => {
  it('reads cumulative goals from Score.ParticipantN.Total', () => {
    expect(parseScoreSnapshot([SCORE_ACTION])).toEqual({ home: 1, away: 2 })
  })

  it('uses the latest action entry', () => {
    const earlier = {
      ...SCORE_ACTION,
      Score: {
        Participant1: { Total: { Goals: 0 } },
        Participant2: { Total: { Goals: 0 } },
      },
    }
    expect(parseScoreSnapshot([earlier, SCORE_ACTION])).toEqual({ home: 1, away: 2 })
  })

  it('swaps sides when Participant1 is not home', () => {
    const swapped = { ...SCORE_ACTION, Participant1IsHome: false }
    expect(parseScoreSnapshot([swapped])).toEqual({ home: 2, away: 1 })
  })

  it('still parses flat home/away shapes', () => {
    expect(parseScoreSnapshot({ home: 3, away: 1 })).toEqual({ home: 3, away: 1 })
  })

  it('returns undefined when no score is present', () => {
    expect(parseScoreSnapshot([{ Action: 'comment', Data: {} }])).toBeUndefined()
  })
})

// ── oddsRetryAsOfMs ─────────────────────────────────────────────────────────────

const NOW = new Date(2026, 6, 12, 12, 0, 0, 0).getTime()

function fixtureAt(startTime: string | undefined): Fixture {
  return { fixtureId: 1, home: 'Home', away: 'Away', startTime }
}

describe('oddsRetryAsOfMs', () => {
  it('retries a minute before kickoff when a live fetch for a past kickoff came back empty', () => {
    const kickoff = new Date(NOW - 3 * 60 * 60 * 1000) // kicked off 3h ago
    const retry = oddsRetryAsOfMs(fixtureAt(kickoff.toISOString()), undefined, 0, NOW)
    expect(retry).toBe(kickoff.getTime() - 60_000)
  })

  it('does not retry when the caller already pinned a specific moment', () => {
    const kickoff = new Date(NOW - 3 * 60 * 60 * 1000)
    expect(oddsRetryAsOfMs(fixtureAt(kickoff.toISOString()), NOW - 1000, 0, NOW)).toBeUndefined()
  })

  it('does not retry when the live fetch already returned quotes', () => {
    const kickoff = new Date(NOW - 3 * 60 * 60 * 1000)
    expect(oddsRetryAsOfMs(fixtureAt(kickoff.toISOString()), undefined, 5, NOW)).toBeUndefined()
  })

  it('does not retry a fixture that has not kicked off yet (empty is expected)', () => {
    const kickoff = new Date(NOW + 60 * 60 * 1000) // kicks off in 1h
    expect(oddsRetryAsOfMs(fixtureAt(kickoff.toISOString()), undefined, 0, NOW)).toBeUndefined()
  })

  it('does not retry when the fixture has no startTime', () => {
    expect(oddsRetryAsOfMs(fixtureAt(undefined), undefined, 0, NOW)).toBeUndefined()
  })
})
