import { useState } from 'react'
import type { AgentTraceEvent, AgentTracePhase } from '../../../types'

// Phase icons and color classes to make the trace visually scannable.
const PHASE_META: Record<AgentTracePhase, { icon: string; colorClass: string; label: string }> = {
  observe: { icon: '👁', colorClass: 'phaseObserve', label: 'Observe' },
  derive: { icon: '⚙', colorClass: 'phaseDerive', label: 'Derive' },
  tool_call: { icon: '🔧', colorClass: 'phaseToolCall', label: 'Tool call' },
  tool_result: { icon: '📥', colorClass: 'phaseToolResult', label: 'Tool result' },
  llm_reasoning: { icon: '💬', colorClass: 'phaseLlm', label: 'LLM reasoning' },
  decision: { icon: '⚡', colorClass: 'phaseDecision', label: 'Decision' },
  action: { icon: '▶', colorClass: 'phaseAction', label: 'Action' },
  proof: { icon: '🔒', colorClass: 'phaseProof', label: 'Proof' },
  payment: { icon: '💳', colorClass: 'phasePayment', label: 'Payment' },
  evaluation: { icon: '📊', colorClass: 'phaseEvaluation', label: 'Evaluation' },
}

// Safety-adjacent phases that should be highlighted in amber/red.
const SAFETY_PHASES = new Set<AgentTracePhase>(['decision', 'action'])

function isSafetyEvent(item: AgentTraceEvent): boolean {
  if (!SAFETY_PHASES.has(item.phase)) return false
  const text = item.summary.toLowerCase()
  return text.includes('kill') || text.includes('budget') || text.includes('blocked') || text.includes('tripped')
}

function TraceItem({ item }: { item: AgentTraceEvent }) {
  const [expanded, setExpanded] = useState(false)
  const meta = PHASE_META[item.phase] ?? { icon: '•', colorClass: '', label: item.phase }
  const safetyFlag = isSafetyEvent(item)
  const hasPayload = item.payload !== undefined && item.payload !== null

  return (
    <li className={`traceItem ${meta.colorClass} ${safetyFlag ? 'traceItemSafety' : ''}`}>
      <div className="traceItemHead">
        <span className="tracePhaseIcon" title={meta.label}>{meta.icon}</span>
        <span className="tracePhaseLabel">{meta.label}</span>
        <span className="traceSummary">{item.summary}</span>
        <span className="traceRound muted">r{item.round}</span>
        {hasPayload && (
          <button
            className="traceExpandBtn"
            onClick={() => setExpanded((p) => !p)}
            aria-expanded={expanded}
            title="Inspect payload"
          >
            {expanded ? '▲' : '▼'}
          </button>
        )}
      </div>
      {expanded && hasPayload && (
        <pre className="tracePayload">
          {JSON.stringify(item.payload, null, 2)}
        </pre>
      )}
    </li>
  )
}

// AgentTracePanel renders the full per-run decision trace with phase icons,
// expandable JSON payloads, and highlighted safety events. Replaces the
// previous stub that was limited to 12 items and showed no phase icons.
export function AgentTracePanel({
  trace,
  maxVisible = 60,
}: {
  trace: AgentTraceEvent[]
  maxVisible?: number
}) {
  const [showAll, setShowAll] = useState(false)
  const visible = showAll ? trace : trace.slice(0, maxVisible)
  const hasMore = trace.length > maxVisible && !showAll

  // Counts for the header summary line.
  const toolCalls = trace.filter((t) => t.phase === 'tool_call').length
  const decisions = trace.filter((t) => t.phase === 'decision').length
  const safetyFlags = trace.filter(isSafetyEvent).length

  return (
    <article className="card tracePanel">
      <div className="cardHead">
        <h2>Agent Trace</h2>
        <span className="pill">{trace.length} steps</span>
        {toolCalls > 0 && <span className="pill phaseToolCall">{toolCalls} tool calls</span>}
        {decisions > 0 && <span className="pill phaseDecision">{decisions} decisions</span>}
        {safetyFlags > 0 && <span className="pill pillWarning">{safetyFlags} safety events</span>}
      </div>

      {trace.length === 0 ? (
        <p className="muted">No trace yet — start a round to see the full decision cycle.</p>
      ) : (
        <>
          <ol className="traceList traceListFull">
            {visible.map((item) => (
              <TraceItem key={item.id} item={item} />
            ))}
          </ol>
          {hasMore && (
            <button className="secondary traceLoadMore" onClick={() => setShowAll(true)}>
              Show all {trace.length} steps
            </button>
          )}
        </>
      )}
    </article>
  )
}
