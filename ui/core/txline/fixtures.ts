import type { Fixture, OddsQuote, TxLineEvent } from '../../types'
import {
  native,
  txlineFixturesSnapshotNative,
  txlineOddsSnapshotNative,
  txlineScoresSnapshotNative
} from '../../desktop/transport'

// Live fixtures come from GET /api/fixtures/snapshot via the Rust commands.
// TxLINE payloads mix PascalCase and camelCase field names, so every accessor
// here is tolerant, mirroring the defensive parsing in src-tauri ingest.rs.

export const epochDayNow = () => Math.floor(Date.now() / 86_400_000)

type Raw = Record<string, unknown>

function asRecord(value: unknown): Raw | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Raw) : undefined
}

function pickNumber(value: Raw | undefined, keys: string[]): number | undefined {
  for (const key of keys) {
    const item = value?.[key]
    if (typeof item === 'number' && Number.isFinite(item)) return item
    if (typeof item === 'string' && item.trim() !== '' && Number.isFinite(Number(item))) return Number(item)
  }
  return undefined
}

function pickString(value: Raw | undefined, keys: string[]): string | undefined {
  for (const key of keys) {
    const item = value?.[key]
    if (typeof item === 'string' && item.trim() !== '') return item.trim()
  }
  return undefined
}

// Start times arrive as ISO strings, epoch seconds, or epoch milliseconds.
function normalizeStartTime(value: Raw): string | undefined {
  const text = pickString(value, ['StartTime', 'startTime', 'start_time', 'KickOff', 'kickoff'])
  if (text && Number.isNaN(Number(text))) return new Date(text).toISOString()
  const numeric = pickNumber(value, ['StartTime', 'startTime', 'start_time', 'KickOff', 'kickoff'])
  if (numeric === undefined) return undefined
  return new Date(numeric > 10_000_000_000 ? numeric : numeric * 1000).toISOString()
}

function extractArray(raw: unknown, keys: string[]): unknown[] {
  if (Array.isArray(raw)) return raw
  const record = asRecord(raw)
  for (const key of keys) {
    const item = record?.[key]
    if (Array.isArray(item)) return item
  }
  return []
}

export function normalizeFixtures(raw: unknown): Fixture[] {
  return extractArray(raw, ['fixtures', 'Fixtures', 'data', 'items', 'snapshot'])
    .map((item) => {
      const record = asRecord(item)
      if (!record) return undefined
      const fixtureId = pickNumber(record, ['FixtureId', 'fixtureId', 'fixture_id', 'Id', 'id'])
      if (!fixtureId) return undefined
      const fixture: Fixture = {
        fixtureId,
        home: pickString(record, ['Participant1', 'participant1', 'home', 'homeTeam', 'HomeTeam']) ?? 'Home',
        away: pickString(record, ['Participant2', 'participant2', 'away', 'awayTeam', 'AwayTeam']) ?? 'Away',
        startTime: normalizeStartTime(record),
        competition: pickString(record, ['Competition', 'competition', 'CompetitionName', 'competitionName', 'League', 'league']),
        status: pickString(record, ['Status', 'status', 'State', 'state'])
      }
      return fixture
    })
    .filter((fixture): fixture is Fixture => fixture !== undefined)
    .sort((a, b) => {
      // Fixtures without a startTime always sort to the end.
      if (!a.startTime && !b.startTime) return 0
      if (!a.startTime) return 1
      if (!b.startTime) return -1
      return a.startTime.localeCompare(b.startTime)
    })
}

export async function loadLiveFixtures(startEpochDay = epochDayNow()): Promise<Fixture[]> {
  if (!native) return []
  return normalizeFixtures(await txlineFixturesSnapshotNative(startEpochDay))
}

const MATCH_WINNER_NAMES = new Set(['home', 'draw', 'away', '1', 'x', '2'])

// TxLINE names 1X2 outcomes part1/draw/part2; the rest of the system speaks
// home/draw/away (wager_agent.rs matches "home"|"1", "draw"|"x", "away"|"2",
// so unaliased names mean "incomplete 1X2 market; no wager proposed").
const OUTCOME_ALIASES: Record<string, string> = { part1: 'home', part2: 'away' }

function isMatchWinnerSet(outcomes: string[]): boolean {
  return outcomes.length === 3 && outcomes.every((name) => MATCH_WINNER_NAMES.has(name.toLowerCase()))
}

// TxLINE snapshot entries are market rows with parallel PriceNames/Prices
// arrays, prices in thousandths (2075 → 2.075). One OddsQuote per price.
function marketRowQuotes(record: Raw, fixtureId: number): OddsQuote[] {
  const names = record['PriceNames'] ?? record['priceNames']
  const prices = record['Prices'] ?? record['prices']
  if (!Array.isArray(names) || !Array.isArray(prices)) return []
  const tsMs = pickNumber(record, ['Ts', 'ts'])
  const params = pickString(record, ['MarketParameters', 'marketParameters'])
  const line = params?.startsWith('line=') ? params.slice('line='.length) : undefined
  const quotes: OddsQuote[] = []
  for (let i = 0; i < Math.min(names.length, prices.length); i++) {
    const rawPrice = prices[i]
    if (typeof rawPrice !== 'number' || !Number.isFinite(rawPrice)) continue
    const decimal = rawPrice >= 100 ? rawPrice / 1000 : rawPrice
    if (decimal <= 1) continue
    const rawName = String(names[i])
    const name = OUTCOME_ALIASES[rawName.toLowerCase()] ?? rawName
    quotes.push({
      fixtureId: pickNumber(record, ['FixtureId', 'fixtureId']) ?? fixtureId,
      // Disambiguate handicap/total lines: "over 0.75" rather than just "over".
      outcome: line && !MATCH_WINNER_NAMES.has(name.toLowerCase()) ? `${name} ${line}` : name,
      decimal,
      impliedProbability: 1 / decimal,
      source: pickString(record, ['Bookmaker', 'bookmaker', 'source', 'book']),
      ts: tsMs !== undefined ? new Date(tsMs).toISOString() : new Date().toISOString()
    })
  }
  return quotes
}

export function parseOddsSnapshot(raw: unknown, fixtureId: number): OddsQuote[] {
  // Match-winner (1X2) quotes lead the list: the UI shows the first distinct
  // outcomes and the wager agent evaluates the 1X2 market.
  const primary: OddsQuote[] = []
  const secondary: OddsQuote[] = []
  for (const item of extractArray(raw, ['odds', 'quotes', 'markets', 'data', 'snapshot'])) {
    const record = asRecord(item)
    if (!record) continue

    const rowQuotes = marketRowQuotes(record, fixtureId)
    if (rowQuotes.length > 0) {
      // Only the full-time match-winner market leads; period 1X2 markets
      // (first half, extra time) stay behind it.
      const period = pickString(record, ['MarketPeriod', 'marketPeriod'])?.toLowerCase()
      const fullTime = period === undefined || period === 'ft'
      const target =
        fullTime && isMatchWinnerSet(rowQuotes.map((q) => q.outcome)) ? primary : secondary
      target.push(...rowQuotes)
      continue
    }

    // Flat single-quote shape (legacy/other envelopes).
    const decimal = pickNumber(record, ['decimal', 'price', 'odds', 'Decimal', 'Price', 'Odds'])
    if (decimal === undefined || decimal <= 1) continue
    primary.push({
      fixtureId: pickNumber(record, ['FixtureId', 'fixtureId', 'fixture_id']) ?? fixtureId,
      outcome: pickString(record, ['outcome', 'selection', 'name', 'side', 'Outcome', 'Selection']) ?? 'unknown',
      decimal,
      impliedProbability: 1 / decimal,
      source: pickString(record, ['source', 'book', 'bookmaker', 'Source']),
      ts: pickString(record, ['ts', 'timestamp', 'Ts', 'Timestamp']) ?? new Date().toISOString()
    })
  }
  return [...primary, ...secondary]
}

// TxLINE nests cumulative goals as Score.ParticipantN.Total.Goals.
function participantGoals(score: Raw | undefined, key: string): number | undefined {
  const participant = asRecord(score?.[key])
  const total = asRecord(participant?.['Total'] ?? participant?.['total'])
  return pickNumber(total, ['Goals', 'goals'])
}

export function parseScoreSnapshot(raw: unknown): { home: number; away: number } | undefined {
  // The scores snapshot lists one entry per action; the latest entry carries the
  // current score. Fall back to treating the payload itself as the score object.
  const actions = extractArray(raw, ['actions', 'events', 'snapshot', 'data'])
  const candidates = [...actions.reverse(), asRecord(raw)?.score, raw]
  for (const candidate of candidates) {
    const record = asRecord(candidate)
    const home = pickNumber(record, ['home', 'homeScore', 'home_score', 'homeGoals', 'Home', 'HomeScore'])
    const away = pickNumber(record, ['away', 'awayScore', 'away_score', 'awayGoals', 'Away', 'AwayScore'])
    if (home !== undefined && away !== undefined) return { home, away }

    // TxLINE action shape: { Score: { Participant1: { Total: { Goals } }, … } }.
    const score = asRecord(record?.['Score'] ?? record?.['score'])
    const p1 = participantGoals(score, 'Participant1')
    const p2 = participantGoals(score, 'Participant2')
    if (p1 !== undefined && p2 !== undefined) {
      // Participant1 is home unless the entry says otherwise.
      const p1Home = record?.['Participant1IsHome']
      return p1Home === false ? { home: p2, away: p1 } : { home: p1, away: p2 }
    }
  }
  return undefined
}

// Decide whether an empty "live" odds fetch should be retried against a
// pre-kickoff moment. TxLINE's odds snapshot without `asOf` returns nothing
// once a fixture's market has closed — which happens as soon as it kicks off
// and settles — so a match analyzed any time after full time (whether that
// was minutes or days ago) comes back with zero live quotes even though the
// pre-match line is still retrievable historically. Pure and unit-tested;
// the actual retry fetch happens in `loadFixtureEvent`.
export function oddsRetryAsOfMs(
  fixture: Fixture,
  requestedAsOfMs: number | undefined,
  quotesFound: number,
  now: number = Date.now()
): number | undefined {
  if (requestedAsOfMs !== undefined) return undefined // caller already pinned a moment
  if (quotesFound > 0) return undefined // the live snapshot had data
  if (!fixture.startTime) return undefined
  const kickoff = new Date(fixture.startTime).getTime()
  if (!Number.isFinite(kickoff) || kickoff > now) return undefined // not kicked off yet — empty is expected
  return kickoff - 60_000
}

// Fold a fixture's odds + score snapshots into the canonical event shape so
// selecting a fixture can trigger agent rounds exactly like streamed events.
// Pass `asOfMs` (Unix ms) to build the event from TxLINE's historical
// snapshots instead of the live ones — agent rounds are trigger-driven, so
// a historical event runs through the exact same pipeline.
//
// `oddsAsOfMs` may differ from `asOfMs`: for a finished match the useful odds
// are the pre-match market (at kickoff) while the useful score is the final
// one (end of day) — in-play markets are pulled as the match runs, so a late
// odds snapshot returns little or nothing.
export async function loadFixtureEvent(
  fixture: Fixture,
  asOfMs?: number,
  oddsAsOfMs: number | undefined = asOfMs
): Promise<TxLineEvent> {
  const [oddsResult, scoresResult] = await Promise.allSettled([
    txlineOddsSnapshotNative(fixture.fixtureId, oddsAsOfMs),
    txlineScoresSnapshotNative(fixture.fixtureId, asOfMs)
  ])
  let odds = oddsResult.status === 'fulfilled' ? parseOddsSnapshot(oddsResult.value, fixture.fixtureId) : []
  let oddsRaw: unknown = oddsResult.status === 'fulfilled' ? oddsResult.value : String(oddsResult.reason)

  // A "live" request (no oddsAsOfMs) that came back empty may just mean the
  // match has already kicked off and its market closed — retry against the
  // last minute before kickoff rather than reporting no odds at all.
  const retryAsOfMs = oddsRetryAsOfMs(fixture, oddsAsOfMs, odds.length)
  if (retryAsOfMs !== undefined) {
    try {
      const retryValue = await txlineOddsSnapshotNative(fixture.fixtureId, retryAsOfMs)
      const retryOdds = parseOddsSnapshot(retryValue, fixture.fixtureId)
      if (retryOdds.length > 0) {
        odds = retryOdds
        oddsRaw = retryValue
      }
    } catch {
      // Keep the original (empty) result — better to say "no odds" than throw.
    }
  }

  const score = scoresResult.status === 'fulfilled' ? parseScoreSnapshot(scoresResult.value) : undefined

  const kickoff = fixture.startTime ? new Date(fixture.startTime).toLocaleString() : 'TBC'
  const scoreline = score ? ` | ${score.home}-${score.away}` : ''
  const snapshotLabel = asOfMs !== undefined
    ? `historical snapshot as of ${new Date(asOfMs).toISOString()}`
    : 'live snapshot'
  return {
    id: `fixture-${fixture.fixtureId}-${Date.now()}`,
    kind: odds.length > 0 ? 'odds_update' : 'fixture',
    fixtureId: fixture.fixtureId,
    statKeys: odds.length > 0 ? ['odds.snapshot'] : ['fixture.snapshot'],
    schemaFamily: odds.length > 0 ? 'odds' : 'fixtures',
    title: `${fixture.home} vs ${fixture.away}${scoreline}`,
    body: `${fixture.competition ?? 'TxLINE'} | kickoff ${kickoff} | ${snapshotLabel}: ${odds.length} odds quotes${score ? `, score ${score.home}-${score.away}` : ''}`,
    ts: asOfMs !== undefined ? new Date(asOfMs).toISOString() : new Date().toISOString(),
    raw: {
      asOfMs,
      odds: oddsRaw,
      scores: scoresResult.status === 'fulfilled' ? scoresResult.value : String(scoresResult.reason)
    },
    odds: odds.length > 0 ? odds : undefined,
    score
  }
}
