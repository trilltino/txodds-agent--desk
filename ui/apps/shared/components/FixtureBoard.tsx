import type { Fixture } from '../../../types'
import { teamIso } from '../../../core/txline/teamFlag'

// FixtureBoard lists live World Cup fixtures from /api/fixtures/snapshot.
// Selecting a fixture asks App to pull its odds/scores snapshots and stage a
// real TxLINE event, so the whole agent pipeline runs on live data.
interface Props {
  fixtures: Fixture[]
  loading: boolean
  error?: string
  selectedFixtureId?: number
  onSelect: (fixture: Fixture) => void
  onRefresh: () => void
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

export function FixtureBoard({ fixtures, loading, error, selectedFixtureId, onSelect, onRefresh }: Props) {
  return (
    <article className="card">
      <div className="cardHead">
        <h2>Fixtures</h2>
        <span className="pill">{loading ? 'loading' : `${fixtures.length} live`}</span>
      </div>
      <div className="eventList">
        {error ? (
          <div className="emptyState">Fixtures snapshot failed: {error}</div>
        ) : fixtures.length === 0 ? (
          <div className="emptyState">
            {loading ? 'Fetching fixtures from TxLINE.' : 'No fixtures returned. TxLINE credentials may be missing.'}
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
            <small>fixture {fixture.fixtureId}{fixture.status ? ` - ${fixture.status}` : ''}</small>
          </button>
        ))}
      </div>
      <button className="secondary" onClick={onRefresh} disabled={loading}>Refresh</button>
    </article>
  )
}
