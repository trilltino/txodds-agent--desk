import type { Fixture } from '../../../types'
import { teamIso } from '../../../core/txline/teamFlag'

// FixtureBoard lists World Cup fixtures from /api/fixtures/snapshot.
// Selecting a fixture asks App to pull its odds/scores snapshots and stage a
// real TxLINE event, so the whole agent pipeline runs on real data. The day
// navigator browses past days — selections there produce historical
// (end-of-day) snapshots instead of live ones.
interface Props {
  fixtures: Fixture[]
  loading: boolean
  error?: string
  selectedFixtureId?: number
  /** Label for the day being shown, e.g. "Today" / "Yesterday" / "Wed, Jul 8". */
  dayLabel: string
  /** True when the board shows a past day (historical snapshots). */
  historical: boolean
  onSelect: (fixture: Fixture) => void
  onRefresh: () => void
  onPrevDay: () => void
  onNextDay: () => void
}

function kickoffLabel(fixture: Fixture): string {
  if (!fixture.startTime) return 'kickoff TBC'
  const kickoff = new Date(fixture.startTime)
  const live = fixture.status?.toLowerCase().includes('live')
  return live ? 'LIVE' : kickoff.toLocaleString(undefined, { weekday: 'short', hour: '2-digit', minute: '2-digit' })
}

function TeamName({ name }: { name: string }) {
  const iso = teamIso(name)
  return (
    <span className="teamName">
      {iso && <span className={`fi fi-${iso}`} style={{ marginRight: '0.35em', verticalAlign: 'middle', borderRadius: 2 }} />}
      {name}
    </span>
  )
}

export function FixtureBoard({
  fixtures,
  loading,
  error,
  selectedFixtureId,
  dayLabel,
  historical,
  onSelect,
  onRefresh,
  onPrevDay,
  onNextDay,
}: Props) {
  return (
    <article className="card">
      <div className="cardHead">
        <h2>Fixtures</h2>
        <span className="pill">
          {loading ? 'loading' : `${fixtures.length} ${historical ? 'played' : 'live'}`}
        </span>
      </div>
      <div className="dayNav">
        <button
          type="button"
          className="secondary dayNavBtn"
          onClick={onPrevDay}
          disabled={loading}
          aria-label="Previous day"
        >
          ◀
        </button>
        <span className="dayNavLabel">{dayLabel}</span>
        <button
          type="button"
          className="secondary dayNavBtn"
          onClick={onNextDay}
          disabled={loading}
          aria-label="Next day"
        >
          ▶
        </button>
      </div>
      <div className="eventList">
        {error ? (
          <div className="emptyState">Fixtures snapshot failed: {error}</div>
        ) : fixtures.length === 0 ? (
          <div className="emptyState">
            {loading
              ? 'Fetching fixtures from TxLINE.'
              : historical
              ? `No fixtures were played ${dayLabel === 'Yesterday' ? 'yesterday' : `on ${dayLabel}`}.`
              : dayLabel === 'Today'
              ? 'No fixtures scheduled for today. If this persists, TxLINE credentials may be missing.'
              : `No fixtures scheduled for ${dayLabel === 'Tomorrow' ? 'tomorrow' : dayLabel}.`}
          </div>
        ) : fixtures.map((fixture) => (
          <button
            key={fixture.fixtureId}
            className={selectedFixtureId === fixture.fixtureId ? 'event selected' : 'event'}
            onClick={() => onSelect(fixture)}
          >
            <strong>
              <TeamName name={fixture.home} />
              {' vs '}
              <TeamName name={fixture.away} />
            </strong>
            <span>{fixture.competition ?? 'TxLINE'} - {kickoffLabel(fixture)}</span>
            {fixture.status && <small>{fixture.status}</small>}
          </button>
        ))}
      </div>
      <button className="secondary" onClick={onRefresh} disabled={loading}>Refresh</button>
    </article>
  )
}
