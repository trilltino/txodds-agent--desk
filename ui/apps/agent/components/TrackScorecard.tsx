// Static mapping between app surfaces and submission tracks. This is product
// narrative, not runtime behaviour.
const rows = [
  ['Verified Markets', 'TxLINE stream and validation data drive proof receipts; settlement remains code-gated and observable on Solana.'],
  ['Match Intelligence Agent', 'One runtime watches TxLINE, detects significant moves, applies policy, acts, and evaluates outcomes.'],
  ['Pulse Rooms', 'Goals, cards, and odds shifts become room cards, leaderboard movement, and shareable fan moments.']
]

// TrackScorecard keeps the three-track story visible without coupling it to the
// actual market engine.
export function TrackScorecard() {
  return (
    <article className="card">
      <div className="cardHead">
        <h2>Three-track scorecard</h2>
        <span className="pill">submission story</span>
      </div>
      {rows.map(([title, body]) => (
        <div className="scoreRow" key={title}>
          <strong>{title}</strong>
          <p>{body}</p>
        </div>
      ))}
    </article>
  )
}
