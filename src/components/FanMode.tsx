import type { AgentRun, TxLineEvent } from '../types'
import { exportFanCardNative, native } from '../desktop/transport'

export function FanMode({ run, selectedEvent }: { run?: AgentRun; selectedEvent: TxLineEvent }) {
  const payload = run?.delivery?.fanCopy ?? selectedEvent.body
  async function exportCard() {
    if (!run || !native) return
    const result = await exportFanCardNative(run.runId)
    console.info(`Fan card exported to ${result.path}`)
  }

  return (
    <article className="card fan">
      <div className="cardHead">
        <h2>Fan Mode</h2>
        <span className="pill">Track 3</span>
      </div>
      <h3>{selectedEvent.title}</h3>
      <p>{payload}</p>
      <div className="shareCard">
        <strong>Shareable card</strong>
        <p>{selectedEvent.title} - {selectedEvent.body}</p>
      </div>
      <button className="secondary" disabled={!run || !native} onClick={exportCard}>Export fan card</button>
    </article>
  )
}
