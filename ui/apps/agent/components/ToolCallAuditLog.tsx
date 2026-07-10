import { useState } from 'react'
import type { ToolCallRecord, ToolCallOutcome } from '../../../core/agent/types'

// ── Outcome display ────────────────────────────────────────────────────────────

const OUTCOME_META: Record<
  ToolCallOutcome['kind'],
  { label: string; colorClass: string; icon: string }
> = {
  pending:  { label: 'PENDING',  colorClass: 'tcOutcome--pending',  icon: '⋯' },
  success:  { label: 'SUCCESS',  colorClass: 'tcOutcome--success',  icon: '✓' },
  blocked:  { label: 'BLOCKED',  colorClass: 'tcOutcome--blocked',  icon: '⊘' },
  failed:   { label: 'FAILED',   colorClass: 'tcOutcome--failed',   icon: '✗' },
  timedOut: { label: 'TIMEOUT',  colorClass: 'tcOutcome--timedOut', icon: '⌛' },
}

function OutcomePill({ outcome }: { outcome: ToolCallOutcome }) {
  const meta = OUTCOME_META[outcome.kind]
  const detail =
    outcome.kind === 'blocked'   ? outcome.reason
    : outcome.kind === 'failed'  ? outcome.errorSummary
    : undefined
  return (
    <span className={`tcOutcomePill ${meta.colorClass}`} title={detail}>
      {meta.icon} {meta.label}
    </span>
  )
}

function CapGrantedIcon({ granted }: { granted: boolean }) {
  return granted ? (
    <span className="tcCapGranted" title="Capability check passed">CAP✓</span>
  ) : (
    <span className="tcCapDenied" title="Capability check failed — tool was not executed">CAP✗</span>
  )
}

function truncate(s: string, n = 24) {
  return s.length > n ? s.slice(0, n) + '…' : s
}

function relativeTime(iso: string) {
  try {
    const epoch = Number(iso.replace('Z', ''))
    const diffMs = Date.now() - epoch * 1000
    const secs = Math.floor(diffMs / 1000)
    if (secs < 60)  return `${secs}s ago`
    if (secs < 3600) return `${Math.floor(secs / 60)}m ago`
    return `${Math.floor(secs / 3600)}h ago`
  } catch {
    return iso.slice(0, 10)
  }
}

function ToolCallRow({ record }: { record: ToolCallRecord }) {
  const [expanded, setExpanded] = useState(false)
  const detail =
    record.outcome.kind === 'blocked'
      ? record.outcome.reason
      : record.outcome.kind === 'failed'
      ? record.outcome.errorSummary
      : undefined

  return (
    <li className={`tcRow tcRow--${record.outcome.kind}`}>
      <div className="tcRowMain">
        <span className="tcToolIcon" aria-hidden="true">🔧</span>
        <span className="tcToolName">{record.toolName}</span>
        <span className="tcAgentId muted">{record.agentId}</span>
        <CapGrantedIcon granted={record.capabilityGranted} />
        <span className="tcIdemKey muted" title={record.idempotencyKey}>
          {truncate(record.idempotencyKey, 20)}
        </span>
        <OutcomePill outcome={record.outcome} />
        <span className="tcTime muted">{relativeTime(record.proposedAt)}</span>
        {detail && (
          <button
            className="tcExpandBtn"
            onClick={() => setExpanded((p) => !p)}
            aria-expanded={expanded}
            title="Show error detail"
          >
            {expanded ? '▲' : '▼'}
          </button>
        )}
      </div>
      {expanded && detail && (
        <div className="tcDetailExpander">
          <span className="muted tcDetailLabel">
            {record.outcome.kind === 'blocked' ? 'Blocked reason' : 'Error summary'}:
          </span>
          <code className="tcDetailText">{detail}</code>
        </div>
      )}
    </li>
  )
}

/**
 * ToolCallAuditLog renders the tamper-evident per-agent tool call audit trail.
 * Every entry corresponds to a ToolCallRecord written to the log before
 * execution begins — matching §24 and §38 of the agentic safety checklist.
 */
export function ToolCallAuditLog({
  records,
  maxVisible = 80,
}: {
  records: ToolCallRecord[]
  maxVisible?: number
}) {
  const [showAll, setShowAll] = useState(false)
  const [agentFilter, setAgentFilter] = useState<string>('all')

  const agentIds = Array.from(new Set(records.map((r) => r.agentId)))
  const filtered = agentFilter === 'all'
    ? records
    : records.filter((r) => r.agentId === agentFilter)
  const visible = showAll ? filtered : filtered.slice(0, maxVisible)

  const blocked   = records.filter((r) => r.outcome.kind === 'blocked').length
  const failed    = records.filter((r) => r.outcome.kind === 'failed').length
  const timedOut  = records.filter((r) => r.outcome.kind === 'timedOut').length
  const capDenied = records.filter((r) => !r.capabilityGranted).length

  return (
    <article className="card tcAuditLog">
      <div className="cardHead">
        <h2>Tool Call Audit</h2>
        <span className="pill">{records.length} calls</span>
        {blocked > 0 && <span className="pill pillWarning">{blocked} blocked</span>}
        {failed > 0 && <span className="pill pillDanger">{failed} failed</span>}
        {timedOut > 0 && <span className="pill pillWarning">{timedOut} timeout</span>}
        {capDenied > 0 && <span className="pill pillDanger">{capDenied} cap denied</span>}
      </div>

      {records.length === 0 ? (
        <p className="muted">
          No tool calls yet — the audit log is written before each execution, not after.
        </p>
      ) : (
        <>
          {agentIds.length > 1 && (
            <div className="filterTabs">
              <button
                className={`filterTab ${agentFilter === 'all' ? 'filterTabActive' : ''}`}
                onClick={() => setAgentFilter('all')}
              >
                All
              </button>
              {agentIds.map((id) => (
                <button
                  key={id}
                  className={`filterTab ${agentFilter === id ? 'filterTabActive' : ''}`}
                  onClick={() => setAgentFilter(id)}
                >
                  {id}
                </button>
              ))}
            </div>
          )}

          <ol className="tcAuditList">
            {visible.map((record) => (
              <ToolCallRow key={`${record.traceId}-${record.proposedAt}`} record={record} />
            ))}
          </ol>

          {filtered.length > maxVisible && !showAll && (
            <button className="secondary traceLoadMore" onClick={() => setShowAll(true)}>
              Show all {filtered.length} calls
            </button>
          )}
        </>
      )}
    </article>
  )
}
