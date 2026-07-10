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

function parseOddsSnapshot(raw: unknown, fixtureId: number): OddsQuote[] {
  return extractArray(raw, ['odds', 'quotes', 'markets', 'data', 'snapshot'])
    .map((item) => {
      const record = asRecord(item)
      const decimal = pickNumber(record, ['decimal', 'price', 'odds', 'Decimal', 'Price', 'Odds'])
      if (!record || decimal === undefined || decimal <= 1) return undefined
      const quote: OddsQuote = {
        fixtureId: pickNumber(record, ['FixtureId', 'fixtureId', 'fixture_id']) ?? fixtureId,
        outcome: pickString(record, ['outcome', 'selection', 'name', 'side', 'Outcome', 'Selection']) ?? 'unknown',
        decimal,
        impliedProbability: 1 / decimal,
        source: pickString(record, ['source', 'book', 'bookmaker', 'Source']),
        ts: pickString(record, ['ts', 'timestamp', 'Ts', 'Timestamp']) ?? new Date().toISOString()
      }
      return quote
    })
    .filter((quote): quote is OddsQuote => quote !== undefined)
}

function parseScoreSnapshot(raw: unknown): { home: number; away: number } | undefined {
  // The scores snapshot lists one entry per action; the latest entry carries the
  // current score. Fall back to treating the payload itself as the score object.
  const actions = extractArray(raw, ['actions', 'events', 'snapshot', 'data'])
  const candidates = [...actions.reverse(), asRecord(raw)?.score, raw]
  for (const candidate of candidates) {
    const record = asRecord(candidate)
    const home = pickNumber(record, ['home', 'homeScore', 'home_score', 'homeGoals', 'Home', 'HomeScore'])
    const away = pickNumber(record, ['away', 'awayScore', 'away_score', 'awayGoals', 'Away', 'AwayScore'])
    if (home !== undefined && away !== undefined) return { home, away }
  }
  return undefined
}

// Fold a fixture's live odds + score snapshots into the canonical event shape
// so selecting a fixture can trigger agent rounds exactly like streamed events.
export async function loadFixtureEvent(fixture: Fixture): Promise<TxLineEvent> {
  const [oddsResult, scoresResult] = await Promise.allSettled([
    txlineOddsSnapshotNative(fixture.fixtureId),
    txlineScoresSnapshotNative(fixture.fixtureId)
  ])
  const odds = oddsResult.status === 'fulfilled' ? parseOddsSnapshot(oddsResult.value, fixture.fixtureId) : []
  const score = scoresResult.status === 'fulfilled' ? parseScoreSnapshot(scoresResult.value) : undefined

  const kickoff = fixture.startTime ? new Date(fixture.startTime).toLocaleString() : 'TBC'
  const scoreline = score ? ` | ${score.home}-${score.away}` : ''
  return {
    id: `fixture-${fixture.fixtureId}-${Date.now()}`,
    kind: odds.length > 0 ? 'odds_update' : 'fixture',
    fixtureId: fixture.fixtureId,
    statKeys: odds.length > 0 ? ['odds.snapshot'] : ['fixture.snapshot'],
    schemaFamily: odds.length > 0 ? 'odds' : 'fixtures',
    title: `${fixture.home} vs ${fixture.away}${scoreline}`,
    body: `${fixture.competition ?? 'TxLINE'} | kickoff ${kickoff} | live snapshot: ${odds.length} odds quotes${score ? `, score ${score.home}-${score.away}` : ''}`,
    ts: new Date().toISOString(),
    raw: {
      odds: oddsResult.status === 'fulfilled' ? oddsResult.value : String(oddsResult.reason),
      scores: scoresResult.status === 'fulfilled' ? scoresResult.value : String(scoresResult.reason)
    },
    odds: odds.length > 0 ? odds : undefined,
    score
  }
}
