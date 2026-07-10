/**
 * tests/core/txline/fixtures.test.ts
 *
 * Unit tests for `ui/core/txline/fixtures.ts`.
 *
 * Coverage targets:
 *  - normalizeFixtures: shape tolerance for camelCase / PascalCase / mixed
 *    TxLINE payloads, missing fields, numeric epoch start times, array vs
 *    nested-object envelopes, de-dupe of invalid entries, sort by startTime.
 *  - epochDayNow: returns a finite integer representing today's epoch day.
 *
 * loadLiveFixtures and loadFixtureEvent call the native bridge; they are
 * integration concerns tested implicitly through the e2e suite. The pure
 * parsing functions are tested exhaustively here.
 */

import { describe, expect, it } from 'vitest'
import { normalizeFixtures, epochDayNow } from '../../../ui/core/txline/fixtures'

// ── epochDayNow ────────────────────────────────────────────────────────────────

describe('epochDayNow', () => {
  it('returns a finite integer', () => {
    const day = epochDayNow()
    expect(Number.isInteger(day)).toBe(true)
    expect(Number.isFinite(day)).toBe(true)
  })

  it('is consistent with Math.floor(Date.now() / 86_400_000)', () => {
    const expected = Math.floor(Date.now() / 86_400_000)
    const actual = epochDayNow()
    // Allow for a tick crossing midnight between the two calls
    expect(actual).toBeGreaterThanOrEqual(expected - 1)
    expect(actual).toBeLessThanOrEqual(expected + 1)
  })
})

// ── normalizeFixtures — input shape tolerance ──────────────────────────────────

describe('normalizeFixtures — camelCase payload', () => {
  const PAYLOAD_CAMEL = {
    fixtures: [
      {
        fixtureId: 1001,
        home: 'Brazil',
        away: 'Argentina',
        startTime: '2026-06-14T14:00:00Z',
        competition: 'FIFA World Cup 2026',
        status: 'PreMatch',
      },
    ],
  }

  it('returns one fixture from a standard camelCase payload', () => {
    const result = normalizeFixtures(PAYLOAD_CAMEL)
    expect(result).toHaveLength(1)
    expect(result[0].fixtureId).toBe(1001)
    expect(result[0].home).toBe('Brazil')
    expect(result[0].away).toBe('Argentina')
  })

  it('preserves the competition and status fields', () => {
    const [fixture] = normalizeFixtures(PAYLOAD_CAMEL)
    expect(fixture.competition).toBe('FIFA World Cup 2026')
    expect(fixture.status).toBe('PreMatch')
  })

  it('normalises the startTime to ISO-8601', () => {
    const [fixture] = normalizeFixtures(PAYLOAD_CAMEL)
    expect(fixture.startTime).toBe('2026-06-14T14:00:00.000Z')
  })
})

describe('normalizeFixtures — PascalCase TxLINE payload', () => {
  const PAYLOAD_PASCAL = {
    Fixtures: [
      {
        FixtureId: 2002,
        Participant1: 'France',
        Participant2: 'Germany',
        StartTime: '2026-06-20T16:00:00Z',
        Competition: 'Group Stage',
        Status: 'InPlay',
      },
    ],
  }

  it('maps PascalCase TxLINE field names correctly', () => {
    const result = normalizeFixtures(PAYLOAD_PASCAL)
    expect(result).toHaveLength(1)
    const [fixture] = result
    expect(fixture.fixtureId).toBe(2002)
    expect(fixture.home).toBe('France')
    expect(fixture.away).toBe('Germany')
  })
})

describe('normalizeFixtures — epoch-second start time', () => {
  it('converts a 10-digit epoch-second timestamp to ISO', () => {
    const epochSec = 1750000000 // some future unix timestamp
    const payload = {
      fixtures: [{ fixtureId: 3003, home: 'Spain', away: 'Portugal', startTime: epochSec }],
    }
    const [fixture] = normalizeFixtures(payload)
    const expected = new Date(epochSec * 1000).toISOString()
    expect(fixture.startTime).toBe(expected)
  })

  it('converts a 13-digit epoch-millisecond timestamp to ISO', () => {
    const epochMs = 1750000000000
    const payload = {
      fixtures: [{ fixtureId: 4004, home: 'Spain', away: 'Portugal', startTime: epochMs }],
    }
    const [fixture] = normalizeFixtures(payload)
    const expected = new Date(epochMs).toISOString()
    expect(fixture.startTime).toBe(expected)
  })
})

describe('normalizeFixtures — missing / invalid entries', () => {
  it('drops entries with no fixtureId', () => {
    const payload = {
      fixtures: [
        { home: 'Spain', away: 'Portugal' },
        { fixtureId: 5005, home: 'Italy', away: 'Belgium' },
      ],
    }
    const result = normalizeFixtures(payload)
    expect(result).toHaveLength(1)
    expect(result[0].fixtureId).toBe(5005)
  })

  it('drops non-object entries', () => {
    const payload = { fixtures: [null, 42, 'bad', { fixtureId: 6006, home: 'A', away: 'B' }] }
    const result = normalizeFixtures(payload)
    expect(result).toHaveLength(1)
  })

  it('returns empty array for empty fixtures array', () => {
    expect(normalizeFixtures({ fixtures: [] })).toEqual([])
  })

  it('returns empty array for null input', () => {
    expect(normalizeFixtures(null)).toEqual([])
  })

  it('returns empty array for a plain string', () => {
    expect(normalizeFixtures('not-a-payload')).toEqual([])
  })
})

describe('normalizeFixtures — array envelope variants', () => {
  it('accepts a top-level array directly (no wrapper object)', () => {
    const payload = [
      { fixtureId: 7007, home: 'X', away: 'Y', startTime: '2026-07-01T12:00:00Z' },
    ]
    const result = normalizeFixtures(payload)
    expect(result).toHaveLength(1)
    expect(result[0].fixtureId).toBe(7007)
  })

  it('handles a "snapshot" envelope key', () => {
    const payload = {
      snapshot: [{ fixtureId: 8008, home: 'A', away: 'B', startTime: '2026-07-01T12:00:00Z' }],
    }
    const result = normalizeFixtures(payload)
    expect(result).toHaveLength(1)
  })
})

describe('normalizeFixtures — sort order', () => {
  it('sorts fixtures by startTime ascending', () => {
    const payload = {
      fixtures: [
        { fixtureId: 3, home: 'C', away: 'D', startTime: '2026-07-03T12:00:00Z' },
        { fixtureId: 1, home: 'A', away: 'B', startTime: '2026-07-01T12:00:00Z' },
        { fixtureId: 2, home: 'E', away: 'F', startTime: '2026-07-02T12:00:00Z' },
      ],
    }
    const result = normalizeFixtures(payload)
    expect(result.map((f) => f.fixtureId)).toEqual([1, 2, 3])
  })

  it('places fixtures with no startTime at the end', () => {
    const payload = {
      fixtures: [
        { fixtureId: 20, home: 'A', away: 'B' }, // no startTime
        { fixtureId: 10, home: 'C', away: 'D', startTime: '2026-06-01T00:00:00Z' },
      ],
    }
    const result = normalizeFixtures(payload)
    // fixture with startTime should sort before undefined
    expect(result[0].fixtureId).toBe(10)
  })
})

describe('normalizeFixtures — default team names', () => {
  it('falls back to "Home" / "Away" when participant names are missing', () => {
    const payload = { fixtures: [{ fixtureId: 9001 }] }
    const [fixture] = normalizeFixtures(payload)
    expect(fixture.home).toBe('Home')
    expect(fixture.away).toBe('Away')
  })
})
