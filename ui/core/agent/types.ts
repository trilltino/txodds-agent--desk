// Agent track contract: Match Intelligence Agent signals and decisions
// (TypeScript mirror of agent-core/src/arena.rs, tools.rs, capability.rs,
// safety.rs). One autonomous runtime observes, decides with deterministic
// formulas, acts, and evaluates itself.
// LLMs may explain a decision that code has already made; they never make one.

export type SignalType =
  | 'sharp_odds_move'
  | 'score_event'
  | 'red_card_reprice'
  | 'late_market_shift'
  | 'proof_ready'

export type SignalSeverity = 'low' | 'medium' | 'high' | 'critical'

export interface AgentSignal {
  id: string
  fixtureId: number
  sourceEventId: string
  type: SignalType
  severity: SignalSeverity
  confidence: number
  /** Measured inputs behind the signal so every emission is reproducible. */
  features: Record<string, number | string | boolean>
  rationale: string
  createdAt: string
}

export type AgentAction =
  | 'ignore'
  | 'watch'
  | 'notify'
  | 'simulate_position'
  | 'fetch_proof'
  | 'trigger_resolution'

export type ExecutionStatus = 'pending' | 'executed' | 'blocked' | 'failed'

/** One named policy gate with its outcome, so the UI shows why an action ran or was blocked. */
export interface PolicyCheck {
  name: string
  passed: boolean
  detail: string
}

export interface AgentDecision {
  id: string
  signalId: string
  action: AgentAction
  confidence: number
  policyChecks: PolicyCheck[]
  explanation: string
  executionStatus: ExecutionStatus
  createdAt: string
}

/** Rolling self-evaluation metrics surfaced by the AccuracyTracker UI. */
export interface AgentMetrics {
  signalsEmitted: number
  signalsCorrect: number
  signalsIncorrect: number
  signalsExpired: number
  avgTimeToOutcomeSecs?: number
}

// ── Arena types (mirrors agent-core/src/arena.rs) ─────────────────────────────

/** Strategy a position was taken under. */
export type Strategy = 'FollowSharp' | 'FadeSharp'

/** Direction of the position relative to the sharp-money signal. */
export type PositionDirection = 'With' | 'Against'

/**
 * Rich settlement result written by arena-coordinator after a fixture ends.
 * Mirrors PositionOutcome in crates/agent-core/src/arena.rs.
 */
export interface PositionOutcome {
  /** Did the selection win? */
  selectionWon: boolean
  /** Final score string, e.g. "2-1". */
  finalScore: string
  /** Profit/loss in points: (odds - 1.0) if won, else -1.0. */
  pnlPoints: number
  /** ISO-8601 timestamp of settlement. */
  settledAt: string
  /** On-chain settlement TX signature, if landed. */
  settlementTx?: string
}

/**
 * One arena position recorded by match-intelligence (FollowSharp) or
 * contrarian (FadeSharp). Mirrors ArenaPosition in agent-core/src/arena.rs.
 */
export interface ArenaPosition {
  positionId: string
  agentId: string
  strategy: Strategy
  fixtureId: number
  marketKey: string
  selection: string
  oddsAtEntry: number
  oddsMovePct: number
  direction: PositionDirection
  confidence: number
  recordedAt: string
  txSignature?: string
  /** Populated by arena-coordinator after fixture completion. */
  outcome?: PositionOutcome
}

// ── Arena session types (mirrors agent-core/src/arena.rs) ─────────────────────

/**
 * Lifecycle state of an arena session (one World Cup match).
 * Mirrors ArenaSessionStatus in crates/agent-core/src/arena.rs.
 */
export type ArenaSessionStatus = 'active' | 'pending_settlement' | 'settled' | 'aborted'

/**
 * State of one arena session (typically one World Cup match).
 * Mirrors ArenaSession in crates/agent-core/src/arena.rs.
 */
export interface ArenaSession {
  sessionId: string
  fixtureId: number
  fixtureName: string
  positions: ArenaPosition[]
  status: ArenaSessionStatus
  startedAt: string
  endedAt?: string
}

// ── Leaderboard types (mirrors AgentLeaderboardEntry in arena.rs) ──────────────

/**
 * Aggregate performance of one agent across all completed arena sessions.
 * Mirrors AgentLeaderboardEntry in crates/agent-core/src/arena.rs.
 * Derived client-side by AgentLeaderboardEntry::from_positions.
 */
export interface AgentLeaderboardEntry {
  agentId: string
  strategy: Strategy
  sessionsCompleted: number
  positionsTaken: number
  positionsWon: number
  /** Cumulative PnL in points across all settled positions. */
  totalPnlPoints: number
  /** Win rate as a fraction (0.0 – 1.0). */
  winRate: number
  /** Average confidence score of winning positions. */
  avgWinningConfidence: number
}

/**
 * Settlement record written by arena-coordinator after a fixture completes.
 * Mirrors SettlementRecord in crates/agents/arena-coordinator/src/main.rs.
 */
export interface SettlementRecord {
  idempotencyKey: string
  fixtureId: number
  agentId: string
  strategy: string
  marketKey: string
  selection: string
  direction: string
  oddsAtEntry: number
  result: 'win' | 'loss'
  pnlUnits: number
  settledAt: string
}

/**
 * Running score for the FollowSharp vs FadeSharp strategy contest.
 * Mirrors ArenaScore in crates/agents/arena-coordinator/src/main.rs.
 */
export interface ArenaScore {
  followWins: number
  followLosses: number
  fadeWins: number
  fadeLosses: number
  followPnl: number
  fadePnl: number
  leader: 'FOLLOW (match-intelligence)' | 'FADE (contrarian)' | 'TIE'
}

// ── Signal types (mirrors sharp-movement-detector/src/main.rs) ─────────────────

/** Direction the odds moved from the sharp-movement-detector's perspective. */
export type OddsDirection = 'shortened' | 'lengthened'

/**
 * One detected sharp-movement signal with optional Venice LLM narrative.
 * Mirrors SignalRecord in crates/agents/sharp-movement-detector/src/main.rs.
 */
export interface SignalRecord {
  idempotencyKey: string
  signalId: string
  fixtureId: number
  fixtureName: string
  marketKey: string
  selection: string
  oddsNow: number
  oddsPrev: number
  movePct: number
  direction: OddsDirection
  confidence: number
  detectedAt: string
  /** Venice AI one-sentence narrative. NEVER drives position decisions. */
  narrative?: string
  /** True if odds continued in the predicted direction on the next poll. */
  correctSoFar: boolean
  outcome?: string
}

// ── Safety gate types (mirrors agent-core/src/safety.rs) ──────────────────────

/**
 * Live safety gate telemetry for one agent. Mirrors the BudgetGuard /
 * StepCounter pair from agent-core/src/safety.rs. All three dimensions
 * (tool calls, spend, session duration) are surfaced. There is no kill
 * switch in this system — see crates/rig-venice/ROADMAP.md, "Removing the
 * kill switch".
 */
export interface AgentSafetyStatus {
  agentId: string
  budgetToolCallsUsed: number
  budgetToolCallsLimit: number
  budgetSpendLamports: number
  budgetSpendLimitLamports: number
  /** Session wall-clock seconds already elapsed (BudgetGuard.check). */
  sessionDurationSecsUsed: number
  /** Maximum allowed session duration in seconds. */
  sessionDurationSecsLimit: number
  stepsUsed: number
  stepsMax: number
  lastCheckedAt: string
}

// ── Tool call audit types (mirrors agent-core/src/tools.rs) ──────────────────

/**
 * Outcome of a single tool call execution.
 * Mirrors ToolCallOutcome in crates/agent-core/src/tools.rs (§24).
 */
export type ToolCallOutcome =
  | { kind: 'pending' }
  | { kind: 'success' }
  | { kind: 'blocked'; reason: string }
  | { kind: 'failed'; errorSummary: string }
  | { kind: 'timedOut' }

/**
 * Immutable record of one tool invocation written to the audit log
 * before execution begins, updated once the result is known.
 * Mirrors ToolCallRecord in crates/agent-core/src/tools.rs (§24, §38).
 */
export interface ToolCallRecord {
  /** Per-session trace ID propagated from CoralOS session. */
  traceId: string
  /** Agent that made the call. */
  agentId: string
  /** Tool name, e.g. "fetch_live_fixtures" or "record_position". */
  toolName: string
  /** Idempotency key for this invocation — safe to retry on timeout. */
  idempotencyKey: string
  /** ISO-8601 timestamp when the call was proposed (pre-execution). */
  proposedAt: string
  /**
   * Whether the compile-time capability check passed.
   * A denied capability means the tool was never executed.
   */
  capabilityGranted: boolean
  /** Outcome after execution (or Pending if not yet resolved). */
  outcome: ToolCallOutcome
}

// ── Capability token types (mirrors agent-core/src/capability.rs) ─────────────

export type CapabilityKind = 'FollowCap' | 'FadeCap' | 'SettleCap' | 'DetectCap'

/**
 * Compile-time capability proof surfaced to the UI as metadata.
 * This is read-only display information — it reflects the Rust ZST tokens,
 * not a runtime grant. See crates/agent-core/src/capability.rs.
 */
export interface CapabilityToken {
  agentId: string
  capability: CapabilityKind
  /** Human-readable description of what this capability permits. */
  description: string
}

// ── Agent roster entry ─────────────────────────────────────────────────────────

export type AgentRunStatus = 'running' | 'stopped' | 'error' | 'unknown'

/**
 * Full runtime descriptor for one sidecar agent combining manifest, capability,
 * safety status, leaderboard performance, and current run state.
 */
export interface AgentRosterEntry {
  id: string
  displayName: string
  strategy: Strategy | 'DetectAndNarrate'
  capability: CapabilityKind
  status: AgentRunStatus
  safety?: AgentSafetyStatus
  /** Aggregate performance record populated once settlements arrive. */
  leaderboard?: AgentLeaderboardEntry
}

// ── Wager (mirrors txodds_types::wager::Wager) ─────────────────────────────────
//
// Produced by the fundamentals agent (rig-venice ROADMAP.md Phase 4) and
// adjudicated by the Rust Authority — never by the LLM. Surfaced today via
// the run trace's `wagerRuling` payload (see `wager_ruling_payload` in
// `native/src/services/agent/runtime.rs`), not a dedicated Tauri command or
// event yet.

export type WagerSelection = 'HOME' | 'DRAW' | 'AWAY'

export type WagerStatus =
  | 'proposed'
  | 'debated'
  | 'no_bet'
  | 'proof_passed'
  | 'proof_failed'
  | 'signed'
  | 'settled'
  | 'refunded'

export interface Wager {
  wagerId: string
  fixtureId: number
  selection: WagerSelection
  modelProb: number
  marketImplied: number
  edge: number
  fairOdds: number
  stakeSol: number
  thesis: string
  proofRef?: string
  status: WagerStatus
  createdAt: string
}

/** The Authority's ruling on a proposed wager — the wager plus why. */
export interface WagerRuling {
  wager: Wager
  reason: string
}
