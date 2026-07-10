/**
 * Test data factories for domain types used across the test suite.
 *
 * All factories accept a partial override so individual tests can vary only the
 * fields they care about while still getting a fully valid object. This keeps
 * test assertions focused and reduces boilerplate.
 *
 * Usage:
 *   import { makeOddsQuote, makeTxLineEvent } from '../__helpers__/fixtures'
 *   const quote = makeOddsQuote({ outcome: 'away', decimal: 3.5 })
 */

import type {
  AgentBid,
  Fixture,
  OddsQuote,
  TxLineEvent,
  TxLineProofReceipt,
} from '../../ui/types'
import type {
  AgentLeaderboardEntry,
  AgentSafetyStatus,
  ArenaPosition,
  ArenaScore,
  SettlementRecord,
  SignalRecord,
} from '../../ui/core/agent/types'

// ── Primitive helpers ─────────────────────────────────────────────────────────

let _seq = 0
/** Auto-incrementing ID generator — ensures uniqueness within a test run. */
export const uid = (prefix = 'id') => `${prefix}-${++_seq}`
/** Fixed ISO timestamp so snapshot tests stay deterministic. */
export const TS = '2026-06-14T14:00:00.000Z'

// ── TxLINE domain factories ───────────────────────────────────────────────────

export function makeFixture(overrides: Partial<Fixture> = {}): Fixture {
  return {
    fixtureId: 1001,
    home: 'Brazil',
    away: 'Argentina',
    startTime: TS,
    competition: 'FIFA World Cup 2026',
    status: 'PreMatch',
    ...overrides,
  }
}

export function makeOddsQuote(overrides: Partial<OddsQuote> = {}): OddsQuote {
  return {
    fixtureId: 1001,
    outcome: 'home',
    decimal: 2.0,
    impliedProbability: 0.5,
    source: 'txline',
    ts: TS,
    ...overrides,
  }
}

export function makeTxLineEvent(overrides: Partial<TxLineEvent> = {}): TxLineEvent {
  return {
    id: uid('evt'),
    kind: 'odds_update',
    fixtureId: 1001,
    statKeys: ['odds.home.implied_probability'],
    schemaFamily: 'odds',
    title: 'Brazil vs Argentina',
    body: 'Odds update',
    ts: TS,
    ...overrides,
  }
}

export function makeProofReceipt(
  overrides: Partial<TxLineProofReceipt> = {},
): TxLineProofReceipt {
  return {
    fixtureId: 1001,
    seq: 1,
    statKeys: ['goals.home'],
    proofPresent: true,
    rootPresent: true,
    simulationStatus: 'passed',
    verified: true,
    note: 'ok',
    ...overrides,
  }
}

// ── Agent bid factories ───────────────────────────────────────────────────────

export function makeAgentBid(overrides: Partial<AgentBid> = {}): AgentBid {
  return {
    agentId: uid('agent'),
    role: 'sharp',
    priceSol: 0.05,
    confidence: 0.8,
    etaMs: 1000,
    note: 'test bid',
    ...overrides,
  }
}

// ── Arena factories ───────────────────────────────────────────────────────────

export function makeArenaPosition(
  overrides: Partial<ArenaPosition> = {},
): ArenaPosition {
  return {
    positionId: uid('pos'),
    agentId: 'match-intelligence',
    strategy: 'FollowSharp',
    fixtureId: 1001,
    marketKey: '1x2',
    selection: 'home',
    oddsAtEntry: 2.0,
    oddsMovePct: 6.5,
    direction: 'With',
    confidence: 0.75,
    recordedAt: TS,
    ...overrides,
  }
}

export function makeSettlementRecord(
  overrides: Partial<SettlementRecord> = {},
): SettlementRecord {
  return {
    idempotencyKey: uid('settle'),
    fixtureId: 1001,
    agentId: 'match-intelligence',
    strategy: 'FollowSharp',
    marketKey: '1x2',
    selection: 'home',
    direction: 'With',
    oddsAtEntry: 2.0,
    result: 'win',
    pnlUnits: 1.0,
    settledAt: TS,
    ...overrides,
  }
}

export function makeArenaScore(overrides: Partial<ArenaScore> = {}): ArenaScore {
  return {
    followWins: 3,
    followLosses: 1,
    fadeWins: 2,
    fadeLosses: 2,
    followPnl: 2.0,
    fadePnl: 0.0,
    leader: 'FOLLOW (match-intelligence)',
    ...overrides,
  }
}

export function makeSignalRecord(overrides: Partial<SignalRecord> = {}): SignalRecord {
  return {
    idempotencyKey: uid('sig'),
    signalId: uid('signal'),
    fixtureId: 1001,
    fixtureName: 'Brazil vs Argentina',
    marketKey: '1x2',
    selection: 'home',
    oddsNow: 1.9,
    oddsPrev: 2.0,
    movePct: 5.0,
    direction: 'shortened',
    confidence: 0.8,
    detectedAt: TS,
    correctSoFar: true,
    ...overrides,
  }
}

export function makeAgentSafetyStatus(
  overrides: Partial<AgentSafetyStatus> = {},
): AgentSafetyStatus {
  return {
    agentId: 'match-intelligence',
    killSwitchTripped: false,
    budgetToolCallsUsed: 5,
    budgetToolCallsLimit: 100,
    budgetSpendLamports: 1_000_000,
    budgetSpendLimitLamports: 100_000_000,
    sessionDurationSecsUsed: 30,
    sessionDurationSecsLimit: 3600,
    stepsUsed: 5,
    stepsMax: 50,
    lastCheckedAt: TS,
    ...overrides,
  }
}

export function makeLeaderboardEntry(
  overrides: Partial<AgentLeaderboardEntry> = {},
): AgentLeaderboardEntry {
  return {
    agentId: 'match-intelligence',
    strategy: 'FollowSharp',
    sessionsCompleted: 5,
    positionsTaken: 12,
    positionsWon: 8,
    totalPnlPoints: 7.5,
    winRate: 0.667,
    avgWinningConfidence: 0.79,
    ...overrides,
  }
}

// ── Auth / user identity factories ────────────────────────────────────────────

import type { UserProfile } from '../../ui/types'

export function makeUserProfile(overrides: Partial<UserProfile> = {}): UserProfile {
  return {
    publicKey: 'HN7cABqLq46Es1jh92dQQisAi18upfBu7bMxZGPeNiKL',
    username: 'alice',
    cluster: 'devnet',
    createdAt: TS,
    ...overrides,
  }
}
