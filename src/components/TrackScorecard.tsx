const rows = [
  ['Prediction Markets & Settlement', 'TxLINE stream triggers markets; proof receipt/verifier gates escrow release; Triton confirms on-chain state.'],
  ['Trading Tools & Agents', 'Coral sellers bid on TxLINE WANTs; risk policy and fair-line reads are scored deterministically.'],
  ['Consumer & Fan Experiences', 'The fan-card seller explains goals, cards, and odds shifts in a mainstream UI.']
]

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
