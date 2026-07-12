/**
 * tests/core/chat/time.test.ts
 *
 * Unit tests for `ui/core/chat/time.ts` — the natural-language "as of" parser
 * that turns chat phrases into TxLINE `asOf` Unix-ms timestamps.
 *
 * Coverage targets:
 *  - explicit dates with and without times, future-date rejection
 *  - "yesterday" with and without a clock time
 *  - relative "N minutes/hours/days ago" forms
 *  - "as of HH:MM" resolving to the most recent occurrence
 *  - cleaned text has the time phrase removed (team matching unaffected)
 *  - non-matches: bare kickoff times, plain analyze requests
 */

import { describe, expect, it } from 'vitest'
import { parseAsOf } from '../../../ui/core/chat/time'

// Fixed reference: Sat 2026-07-11 12:30 local time.
const NOW = new Date(2026, 6, 11, 12, 30, 0, 0)
const MINUTE_MS = 60_000
const HOUR_MS = 3_600_000
const DAY_MS = 86_400_000

// ── explicit dates ─────────────────────────────────────────────────────────────

describe('parseAsOf — explicit dates', () => {
  it('parses a date with a time', () => {
    const result = parseAsOf('analyze France vs Spain as of 2026-07-10 18:00', NOW)
    expect(result).toBeDefined()
    expect(result!.asOfMs).toBe(new Date(2026, 6, 10, 18, 0, 0, 0).getTime())
  })

  it('parses an ISO-style T separator', () => {
    const result = parseAsOf('analyze as of 2026-07-10T09:15', NOW)
    expect(result!.asOfMs).toBe(new Date(2026, 6, 10, 9, 15, 0, 0).getTime())
  })

  it('treats a bare date as end of that day', () => {
    const result = parseAsOf('analyze France vs Spain as of 2026-07-09', NOW)
    expect(result!.asOfMs).toBe(new Date(2026, 6, 9, 23, 59, 59, 0).getTime())
  })

  it('rejects clearly future dates', () => {
    expect(parseAsOf('analyze as of 2026-08-01 18:00', NOW)).toBeUndefined()
  })

  it('rejects invalid clock times', () => {
    expect(parseAsOf('as of 2026-07-10 25:99', NOW)).toBeUndefined()
  })
})

// ── yesterday ──────────────────────────────────────────────────────────────────

describe('parseAsOf — yesterday', () => {
  it('parses "yesterday HH:MM" to yesterday at that time', () => {
    const result = parseAsOf('analyze France vs Spain as of yesterday 18:00', NOW)
    expect(result!.asOfMs).toBe(new Date(2026, 6, 10, 18, 0, 0, 0).getTime())
  })

  it('parses "yesterday at HH:MM"', () => {
    const result = parseAsOf('sharp movement yesterday at 09:05', NOW)
    expect(result!.asOfMs).toBe(new Date(2026, 6, 10, 9, 5, 0, 0).getTime())
  })

  it('parses bare "yesterday" as the same clock time one day earlier', () => {
    const result = parseAsOf('analyze Norway vs England yesterday', NOW)
    expect(result!.asOfMs).toBe(NOW.getTime() - DAY_MS)
  })
})

// ── relative "ago" ─────────────────────────────────────────────────────────────

describe('parseAsOf — relative ago', () => {
  it('parses "2 hours ago"', () => {
    const result = parseAsOf('analyze France vs Spain 2 hours ago', NOW)
    expect(result!.asOfMs).toBe(NOW.getTime() - 2 * HOUR_MS)
  })

  it('parses "45 minutes ago"', () => {
    const result = parseAsOf('odds 45 minutes ago', NOW)
    expect(result!.asOfMs).toBe(NOW.getTime() - 45 * MINUTE_MS)
  })

  it('parses "3 days ago"', () => {
    const result = parseAsOf('as of 3 days ago', NOW)
    expect(result!.asOfMs).toBe(NOW.getTime() - 3 * DAY_MS)
  })

  it('parses fractional amounts', () => {
    const result = parseAsOf('1.5 hours ago', NOW)
    expect(result!.asOfMs).toBe(NOW.getTime() - 1.5 * HOUR_MS)
  })
})

// ── "as of HH:MM" ──────────────────────────────────────────────────────────────

describe('parseAsOf — as of HH:MM', () => {
  it('resolves a past time to today', () => {
    const result = parseAsOf('analyze as of 09:00', NOW)
    expect(result!.asOfMs).toBe(new Date(2026, 6, 11, 9, 0, 0, 0).getTime())
  })

  it('resolves a future time to yesterday (most recent occurrence)', () => {
    const result = parseAsOf('analyze as of 18:00', NOW)
    expect(result!.asOfMs).toBe(new Date(2026, 6, 10, 18, 0, 0, 0).getTime())
  })
})

// ── cleaned text ───────────────────────────────────────────────────────────────

describe('parseAsOf — cleaned text', () => {
  it('removes the time phrase so team names still match', () => {
    const result = parseAsOf('analyze France vs Spain as of yesterday 18:00', NOW)
    expect(result!.cleaned).toBe('analyze France vs Spain')
  })

  it('removes mid-sentence phrases without joining words', () => {
    const result = parseAsOf('as of 2 hours ago what moved on Norway vs England', NOW)
    expect(result!.cleaned).toBe('what moved on Norway vs England')
  })

  it('produces a non-empty human label', () => {
    const result = parseAsOf('analyze as of yesterday 18:00', NOW)
    expect(result!.label.length).toBeGreaterThan(0)
  })
})

// ── non-matches ────────────────────────────────────────────────────────────────

describe('parseAsOf — non-matches', () => {
  it('ignores plain analyze requests', () => {
    expect(parseAsOf('analyze France vs Spain', NOW)).toBeUndefined()
  })

  it('ignores bare kickoff times without "as of"', () => {
    expect(parseAsOf('France vs Spain kicks off at 18:00', NOW)).toBeUndefined()
  })

  it('ignores score-style numbers', () => {
    expect(parseAsOf('what was the 2-1 comeback about', NOW)).toBeUndefined()
  })

  it('ignores fixture ids', () => {
    expect(parseAsOf('analyze fixture 18182808', NOW)).toBeUndefined()
  })
})
