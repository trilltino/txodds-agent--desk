import type { IngestStatus, TrackMode, TxLineEvent } from '../types'

// LiveFeed renders normalized TxLINE events regardless of whether they came
// from Rust live ingest, replay, or browser mock fixtures.
interface Props {
  events: TxLineEvent[]
  selected: TxLineEvent
  ingestStatuses: IngestStatus[]
  onSelect: (event: TxLineEvent) => void
  onStartRound: (event: TxLineEvent, track?: TrackMode) => void
}

export function LiveFeed({ events, selected, ingestStatuses, onSelect, onStartRound }: Props) {
  // The component stays intentionally dumb: selecting an event and creating a
  // WANT are callbacks so backend/native mode remains controlled by App.
  const primaryStatus = ingestStatuses[0]
  return (
    <article className="card">
      <div className="cardHead">
        <h2>Live TxLINE feed</h2>
        <span className="pill">{primaryStatus?.state ?? 'starting'}</span>
      </div>
      <p className="muted">Live TxLINE odds and scores are the primary trigger source. Mock and replay modes are visible fallbacks only.</p>
      <div className="statusStack">
        {ingestStatuses.map((status) => (
          <div key={status.source} className="statusLine">
            <strong>{status.source}</strong>
            <span>{status.detail}</span>
          </div>
        ))}
      </div>
      <div className="eventList">
        {events.length === 0 ? (
          <div className="emptyState">Waiting for TxLINE events from Rust.</div>
        ) : events.map((event) => (
          <button key={event.id} className={selected.id === event.id ? 'event selected' : 'event'} onClick={() => onSelect(event)}>
            <strong>{event.title}</strong>
            <span>{event.kind} - fixture {event.fixtureId}</span>
            <small>{event.body}</small>
          </button>
        ))}
      </div>
      <button className="secondary" onClick={() => onStartRound(selected)}>Create WANT from selected event</button>
    </article>
  )
}
