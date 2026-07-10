import type { AgentTraceEvent } from '../../types'
import type {
  AgentLeaderboardEntry,
  AgentRosterEntry,
  AgentSafetyStatus,
  ArenaPosition,
  ArenaScore,
  SettlementRecord,
  SignalRecord,
  ToolCallRecord,
} from '../../core/agent/types'
import { AgentRosterPanel } from '../../apps/agent/components/AgentRosterPanel'
import { ArenaScorecard } from '../../apps/agent/components/ArenaScorecard'
import { ArenaPositionFeed } from '../../apps/agent/components/ArenaPositionFeed'
import { SignalIntelligencePanel } from '../../apps/agent/components/SignalIntelligencePanel'
import { SafetyGateMonitor } from '../../apps/agent/components/SafetyGateMonitor'
import { AgentTracePanel } from '../../apps/agent/components/AgentTracePanel'
import { CapabilityPanel } from '../../apps/agent/components/CapabilityPanel'
import { ToolCallAuditLog } from '../../apps/agent/components/ToolCallAuditLog'
import { WagerPanel } from '../../apps/agent/components/WagerPanel'

interface AgentDashboardProps {
  agentRoster: AgentRosterEntry[]
  safetyStatuses: AgentSafetyStatus[]
  arenaScore: ArenaScore | undefined
  settlementRecords: SettlementRecord[]
  leaderboard: AgentLeaderboardEntry[]
  arenaPositions: ArenaPosition[]
  signalRecords: SignalRecord[]
  toolCallRecords: ToolCallRecord[]
  currentRunTrace: AgentTraceEvent[]
}

/**
 * AgentDashboard — the full agent power dashboard rendered on the `agent` page.
 *
 * Receives all data as props from the App via `useAgentDesk` so it stays a
 * pure presentational component with no direct backend coupling.
 */
export function AgentDashboard({
  agentRoster,
  safetyStatuses,
  arenaScore,
  settlementRecords,
  leaderboard,
  arenaPositions,
  signalRecords,
  toolCallRecords,
  currentRunTrace,
}: AgentDashboardProps) {
  return (
    <div className="agentPageGrid">
      {/* Row 1: Roster + Safety gates */}
      <div className="agentTopRow">
        <AgentRosterPanel roster={agentRoster} />
        <SafetyGateMonitor statuses={safetyStatuses} />
      </div>
      {/* Row 2: Scoreboard with leaderboard + Capability tokens */}
      <div className="agentMidRow">
        <ArenaScorecard
          score={arenaScore}
          recentSettlements={settlementRecords.slice(0, 5)}
          leaderboard={leaderboard}
        />
        <CapabilityPanel roster={agentRoster} />
      </div>
      {/* Row 3: Position feed + Signal intelligence */}
      <ArenaPositionFeed positions={arenaPositions} />
      <SignalIntelligencePanel signals={signalRecords} />
      {/* Row 4: fundamentals wager proposals (rig-venice ROADMAP.md Phase 4-5) */}
      <WagerPanel trace={currentRunTrace} />
      {/* Tool call audit — full width, collapsible */}
      <details className="traceDrawer">
        <summary className="traceDrawerToggle">
          Tool Call Audit
          {toolCallRecords.length > 0 && (
            <span className="pill" style={{ marginLeft: '0.5rem' }}>
              {toolCallRecords.length} calls
            </span>
          )}
        </summary>
        <ToolCallAuditLog records={toolCallRecords} />
      </details>
      {/* Agent trace — full width, collapsible */}
      <details className="traceDrawer">
        <summary className="traceDrawerToggle">
          Agent Trace
          {currentRunTrace.length > 0 && (
            <span className="pill" style={{ marginLeft: '0.5rem' }}>
              {currentRunTrace.length} steps
            </span>
          )}
        </summary>
        <AgentTracePanel trace={currentRunTrace} />
      </details>
    </div>
  )
}
