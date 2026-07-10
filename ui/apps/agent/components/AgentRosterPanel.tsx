import type { AgentRosterEntry, CapabilityKind } from '../../../core/agent/types'

// Capability badge labels and their descriptions, matching capability.rs.
const CAP_LABELS: Record<CapabilityKind, { short: string; detail: string }> = {
  FollowCap: { short: 'FOLLOW', detail: 'Records FollowSharp positions against the arena ledger' },
  FadeCap: { short: 'FADE', detail: 'Records FadeSharp positions against the arena ledger' },
  SettleCap: { short: 'SETTLE', detail: 'Reads arena positions and writes settlement records' },
  DetectCap: { short: 'DETECT', detail: 'Detects significant odds moves and emits signal records' },
}

const STATUS_DOTS: Record<string, string> = {
  running: '#22c55e',
  stopped: '#94a3b8',
  error: '#f97316',
  unknown: '#94a3b8',
}

function CapabilityBadge({ cap }: { cap: CapabilityKind }) {
  const info = CAP_LABELS[cap]
  return (
    <span className="capBadge" title={info.detail}>
      {info.short}
    </span>
  )
}

function AgentCard({ entry }: { entry: AgentRosterEntry }) {
  const dotColor = STATUS_DOTS[entry.status] ?? STATUS_DOTS.unknown
  const safety = entry.safety
  const lb = entry.leaderboard

  return (
    <div className="agentCard">
      <div className="agentCardHead">
        <span className="statusDot" style={{ background: dotColor }} />
        <strong>{entry.displayName}</strong>
        <CapabilityBadge cap={entry.capability} />
        <span className="muted strategyLabel">{entry.strategy}</span>
      </div>

      {safety && (
        <div className="agentCardSafety">
          <div className="safetyRow">
            <span className="safetyLabel">Tool calls</span>
            <div className="safetyBar">
              <div
                className="safetyBarFill"
                style={{
                  width: `${Math.min(100, (safety.budgetToolCallsUsed / Math.max(safety.budgetToolCallsLimit, 1)) * 100)}%`,
                  background: safety.budgetToolCallsUsed >= safety.budgetToolCallsLimit ? '#ef4444' : '#38bdf8',
                }}
              />
            </div>
            <span className="safetyCount">
              {safety.budgetToolCallsUsed}/{safety.budgetToolCallsLimit}
            </span>
          </div>
          <div className="safetyRow">
            <span className="safetyLabel">Steps</span>
            <div className="safetyBar">
              <div
                className="safetyBarFill"
                style={{
                  width: `${Math.min(100, (safety.stepsUsed / Math.max(safety.stepsMax, 1)) * 100)}%`,
                  background: safety.stepsUsed >= safety.stepsMax ? '#ef4444' : '#a78bfa',
                }}
              />
            </div>
            <span className="safetyCount">
              {safety.stepsUsed}/{safety.stepsMax}
            </span>
          </div>
        </div>
      )}

      {/* Leaderboard mini-stats — rendered once settlements arrive */}
      {lb && lb.positionsTaken > 0 && (
        <div className="agentCardLeaderboard">
          <div className="lbRow">
            <span className="lbLabel">Win rate</span>
            <div className="safetyBar">
              <div
                className="safetyBarFill"
                style={{
                  width: `${Math.round(lb.winRate * 100)}%`,
                  background: lb.winRate >= 0.6 ? '#4ade80' : lb.winRate >= 0.4 ? '#f59e0b' : '#f87171',
                }}
              />
            </div>
            <span className="safetyCount">{Math.round(lb.winRate * 100)}%</span>
          </div>
          <div className="lbRow lbStatsRow">
            <span className="lbStat">{lb.positionsTaken} pos</span>
            <span className="lbStat">{lb.positionsWon}W / {lb.positionsTaken - lb.positionsWon}L</span>
            <span className={`lbStat ${lb.totalPnlPoints >= 0 ? 'pnlPositive' : 'pnlNegative'}`}>
              {lb.totalPnlPoints >= 0 ? '+' : ''}{lb.totalPnlPoints.toFixed(1)} pts
            </span>
          </div>
        </div>
      )}
    </div>
  )
}

// AgentRosterPanel renders all four sidecar agents with capability tokens and
// safety gate progress bars. There is no per-agent kill switch in this
// system — see crates/rig-venice/ROADMAP.md, "Removing the kill switch".
export function AgentRosterPanel({ roster }: { roster: AgentRosterEntry[] }) {
  const running = roster.filter((a) => a.status === 'running').length

  return (
    <article className="card">
      <div className="cardHead">
        <h2>Agent Roster</h2>
        <span className="pill">{running}/{roster.length} running</span>
      </div>
      {roster.length === 0 ? (
        <p className="muted">No agents registered — waiting for Rust sidecar registry.</p>
      ) : (
        <div className="agentRosterGrid">
          {roster.map((entry) => (
            <AgentCard key={entry.id} entry={entry} />
          ))}
        </div>
      )}
    </article>
  )
}
