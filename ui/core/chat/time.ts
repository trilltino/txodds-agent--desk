// Natural-language time parsing for historical chat requests.
//
// TxLINE's odds/scores snapshot endpoints accept `asOf` as a Unix timestamp
// in milliseconds ("Optional Unix timestamp (ms) for a historical snapshot" —
// docs.yaml). parseAsOf extracts a time phrase from a chat message and
// resolves it against local time, so "analyze France vs Spain as of yesterday
// 18:00" can drive a historical round with no backend changes.

export interface AsOfParse {
  /** Unix timestamp (ms) — the unit `asOf` expects on the wire. */
  asOfMs: number
  /** Human-readable echo of the parsed time for agent replies. */
  label: string
  /** Input with the time phrase removed, for fixture/team matching. */
  cleaned: string
}

const MINUTE_MS = 60_000
const HOUR_MS = 3_600_000
const DAY_MS = 86_400_000

// Supported phrases, most specific first:
//   "as of 2026-07-10 18:00" / "2026-07-10"        (explicit date, opt. time)
//   "as of yesterday 18:00" / "yesterday"           (opt. time)
//   "3 hours ago" / "45 minutes ago" / "2 days ago" (relative)
//   "as of 18:00"                                   (most recent occurrence)
// A bare "18:00" without "as of" is deliberately NOT parsed — times appear in
// fixture talk (kickoffs, scores) too often to treat them as intent.
const DATE_RE = /(?:as\s+of\s+)?\b(\d{4})-(\d{2})-(\d{2})(?:[T\s]+(\d{1,2}):(\d{2}))?/i
const YESTERDAY_RE = /(?:as\s+of\s+)?\byesterday\b(?:\s+(?:at\s+)?(\d{1,2}):(\d{2}))?/i
const AGO_RE = /(?:as\s+of\s+)?\b(\d+(?:\.\d+)?)\s*(minute|min|hour|hr|day)s?\s+ago\b/i
const ASOF_TIME_RE = /as\s+of\s+(?:(?:at|around)\s+)?(\d{1,2}):(\d{2})\b/i

function label(ms: number): string {
  return new Date(ms).toLocaleString(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function strip(text: string, match: RegExpMatchArray): string {
  return (
    text.slice(0, match.index ?? 0) +
    ' ' +
    text.slice((match.index ?? 0) + match[0].length)
  ).replace(/\s+/g, ' ').trim()
}

function valid(h: number, m: number): boolean {
  return h >= 0 && h <= 23 && m >= 0 && m <= 59
}

/**
 * Extract a historical point-in-time from a chat message.
 * Returns undefined when no phrase matches or the time is in the future —
 * callers then run against the live snapshot as before.
 */
export function parseAsOf(text: string, now: Date = new Date()): AsOfParse | undefined {
  const nowMs = now.getTime()

  const dateMatch = text.match(DATE_RE)
  if (dateMatch) {
    const [, y, mo, d, h, min] = dateMatch
    const hasTime = h !== undefined
    if (hasTime && !valid(Number(h), Number(min))) return undefined
    // Without a time, "as of <date>" means the end of that day — the latest
    // snapshot the day produced.
    const when = new Date(
      Number(y),
      Number(mo) - 1,
      Number(d),
      hasTime ? Number(h) : 23,
      hasTime ? Number(min) : 59,
      hasTime ? 0 : 59,
      0,
    )
    const ms = Math.min(when.getTime(), nowMs)
    if (when.getTime() - nowMs > DAY_MS) return undefined // clearly a future date
    if (ms >= nowMs) return undefined
    return { asOfMs: ms, label: label(ms), cleaned: strip(text, dateMatch) }
  }

  const yesterdayMatch = text.match(YESTERDAY_RE)
  if (yesterdayMatch) {
    const [, h, min] = yesterdayMatch
    const when = new Date(now)
    when.setDate(when.getDate() - 1)
    if (h !== undefined) {
      if (!valid(Number(h), Number(min))) return undefined
      when.setHours(Number(h), Number(min), 0, 0)
    }
    const ms = when.getTime()
    return { asOfMs: ms, label: label(ms), cleaned: strip(text, yesterdayMatch) }
  }

  const agoMatch = text.match(AGO_RE)
  if (agoMatch) {
    const amount = Number(agoMatch[1])
    const unit = agoMatch[2].toLowerCase()
    const unitMs = unit.startsWith('min') ? MINUTE_MS : unit.startsWith('h') ? HOUR_MS : DAY_MS
    const ms = nowMs - amount * unitMs
    if (!Number.isFinite(ms) || ms >= nowMs) return undefined
    return { asOfMs: ms, label: label(ms), cleaned: strip(text, agoMatch) }
  }

  const timeMatch = text.match(ASOF_TIME_RE)
  if (timeMatch) {
    const [, h, min] = timeMatch
    if (!valid(Number(h), Number(min))) return undefined
    const when = new Date(now)
    when.setHours(Number(h), Number(min), 0, 0)
    // "as of 18:00" before 18:00 means yesterday's 18:00.
    if (when.getTime() >= nowMs) when.setDate(when.getDate() - 1)
    const ms = when.getTime()
    return { asOfMs: ms, label: label(ms), cleaned: strip(text, timeMatch) }
  }

  return undefined
}
