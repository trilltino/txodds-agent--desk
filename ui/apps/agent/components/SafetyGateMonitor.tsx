import type { AgentSafetyStatus } from '../../../core/agent/types'

// SafetyGateMonitor renders a compact per-agent row showing all four
// BudgetGuard dimensions (tool calls, SOL spend, session duration) and the
// StepCounter. There is no kill switch in this system — see
// crates/rig-venice/ROADMAP.md, "Removing the kill switch".
function SafetyRow({ status }: { status: AgentSafetyStatus }) {
  const callPct = Math.min(100, (status.budgetToolCallsUsed / Math.max(status.budgetToolCallsLimit, 1)) * 100)
  const stepPct = Math.min(100, (status.stepsUsed / Math.max(status.stepsMax, 1)) * 100)
  const spendPct = Math.min(
    100,
    (status.budgetSpendLamports / Math.max(status.budgetSpendLimitLamports, 1)) * 100
  )
  // Session duration — only rendered when the backend provides the fields.
  const hasDuration =
    typeof status.sessionDurationSecsUsed === 'number' &&
    typeof status.sessionDurationSecsLimit === 'number' &&
    status.sessionDurationSecsLimit > 0
  const durationPct = hasDuration
    ? Math.min(100, (status.sessionDurationSecsUsed / status.sessionDurationSecsLimit) * 100)
    : 0
  const durationMins = hasDuration
    ? `${Math.floor(status.sessionDurationSecsUsed / 60)}m / ${Math.floor(status.sessionDurationSecsLimit / 60)}m`
    : null

  const atRisk = callPct >= 80 || stepPct >= 80 || spendPct >= 80 || durationPct >= 80

  function barColor(pct: number, accentOk: string) {
    if (pct >= 100) return '#ef4444'
    if (pct >= 80)  return '#f97316'
    return accentOk
  }

  return (
    <div className={`safetyGateRow ${atRisk ? 'safetyGateWarn' : ''}`}>
      <div className="safetyGateHead">
        <strong className="safetyAgentId">{status.agentId}</strong>
        {atRisk && <span className="pill pillWarning">AT RISK</span>}
      </div>

      <div className="safetyMetrics">
        <div className="safetyMetricRow">
          <span className="safetyMetricLabel">Calls</span>
          <div className="safetyBar">
            <div className="safetyBarFill" style={{
              width: `${callPct}%`,
              background: barColor(callPct, '#38bdf8'),
            }} />
          </div>
          <span className="safetyMetricVal">{status.budgetToolCallsUsed}/{status.budgetToolCallsLimit}</span>
        </div>
        <div className="safetyMetricRow">
          <span className="safetyMetricLabel">Steps</span>
          <div className="safetyBar">
            <div className="safetyBarFill" style={{
              width: `${stepPct}%`,
              background: barColor(stepPct, '#a78bfa'),
            }} />
          </div>
          <span className="safetyMetricVal">{status.stepsUsed}/{status.stepsMax}</span>
        </div>
        <div className="safetyMetricRow">
          <span className="safetyMetricLabel">Spend</span>
          <div className="safetyBar">
            <div className="safetyBarFill" style={{
              width: `${spendPct}%`,
              background: barColor(spendPct, '#34d399'),
            }} />
          </div>
          <span className="safetyMetricVal">
            {(status.budgetSpendLamports / 1_000_000_000).toFixed(4)} SOL
          </span>
        </div>
        {hasDuration && durationMins && (
          <div className="safetyMetricRow">
            <span className="safetyMetricLabel">Session</span>
            <div className="safetyBar">
              <div className="safetyBarFill" style={{
                width: `${durationPct}%`,
                background: barColor(durationPct, '#f59e0b'),
              }} />
            </div>
            <span className="safetyMetricVal">{durationMins}</span>
          </div>
        )}
      </div>
    </div>
  )
}

export function SafetyGateMonitor({ statuses }: { statuses: AgentSafetyStatus[] }) {
  const atRisk = statuses.filter((s) => {
    const callPct = (s.budgetToolCallsUsed / Math.max(s.budgetToolCallsLimit, 1)) * 100
    const stepPct = (s.stepsUsed / Math.max(s.stepsMax, 1)) * 100
    return callPct >= 80 || stepPct >= 80
  }).length

  return (
    <article className="card">
      <div className="cardHead">
        <h2>Safety Gates</h2>
        {atRisk > 0 && <span className="pill pillWarning">{atRisk} at risk</span>}
        {atRisk === 0 && statuses.length > 0 && (
          <span className="pill">all nominal</span>
        )}
      </div>

      {statuses.length === 0 ? (
        <p className="muted">Safety gate telemetry not yet available from Rust.</p>
      ) : (
        <div className="safetyGateList">
          {statuses.map((s) => (
            <SafetyRow key={s.agentId} status={s} />
          ))}
        </div>
      )}
    </article>
  )
}
