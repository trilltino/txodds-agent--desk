import type { AgentRun, CoralAgentManifest, TrackMode } from '../types'
import { scoreBid } from '../domain/coral/scoring'

export function AgentArena({
  agents,
  run,
  track,
  onRun
}: {
  agents: CoralAgentManifest[]
  run?: AgentRun
  track: TrackMode
  onRun: () => void
}) {
  const activeIds = new Set(run?.bids.map((bid) => bid.agentId) ?? [])
  const activeAgents = agents.filter((agent) => agent.coralRole === 'buyer' || activeIds.has(agent.id))

  return (
    <article className="card">
      <div className="cardHead">
        <h2>Coral market</h2>
        <span className="pill">{track}</span>
      </div>
      <div className="agentRoster">
        {activeAgents.map((agent) => (
          <div key={agent.id} className="agentChip" title={agent.manifestPath}>
            <strong>{agent.id}</strong>
            <span>{agent.coralRole} · {agent.service}</span>
          </div>
        ))}
      </div>
      {!run ? <p className="muted">Start a round to see Coral sellers bid on the buyer WANT.</p> : null}
      {run?.bids.map((bid) => (
        <div key={bid.agentId} className={run.winner?.agentId === bid.agentId ? 'bid winner' : 'bid'}>
          <div>
            <strong>{bid.agentId}</strong>
            <p>{bid.note}</p>
          </div>
          <div className="metrics">
            <span>{bid.priceSol} SOL</span>
            <span>{Math.round(bid.confidence * 100)}%</span>
            <span>score {scoreBid(track, bid).toFixed(2)}</span>
          </div>
        </div>
      ))}
      <button className="secondary" onClick={onRun}>Re-run market</button>
    </article>
  )
}
