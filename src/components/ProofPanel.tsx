import type { AgentRun } from '../types'

// ProofPanel is the judge/operator audit view. It renders the timeline that
// Rust/browser fallback code appends as each phase completes.
export function ProofPanel({ run }: { run?: AgentRun }) {
  return (
    <article className="card">
      <div className="cardHead">
        <h2>Proof Panel</h2>
        <span className="pill">audit trail</span>
      </div>
      {!run ? <p className="muted">No run yet.</p> : (
        <>
          <div className="receipt">
            <span>Rail</span><strong>{run.settlement?.rail ?? '-'}</strong>
            <span>Pay reference</span><code>{run.settlement?.paymentReference ?? '-'}</code>
            <span>Pay signature</span><code>{run.settlement?.paymentSignature ?? '-'}</code>
          </div>
          <ol className="timeline">
            {run.timeline.map((item, idx) => (
              <li key={`${item.label}-${idx}`}><strong>{item.label}</strong><span>{item.detail}</span></li>
            ))}
          </ol>
          <pre>{JSON.stringify({ runId: run.runId, verdict: run.verdict, settlement: run.settlement }, null, 2)}</pre>
        </>
      )}
    </article>
  )
}
