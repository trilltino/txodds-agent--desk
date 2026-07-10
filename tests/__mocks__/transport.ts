/**
 * Mock Tauri native bridge for unit tests.
 *
 * Every function exported by `ui/desktop/transport.ts` that touches
 * `window.__TAURI__` is stubbed here so tests run under Node without a Tauri
 * runtime. Import order matters — Vitest's module system requires this file to
 * be listed as an alias in `vite.config.ts` OR imported before the module
 * under test via `vi.mock(...)`.
 *
 * All stubs return resolved `Promise<undefined>` by default so callers that
 * await them do not throw. Override per-test with `vi.mocked(fn).mockResolvedValue(...)`.
 */

import { vi } from 'vitest'

// The native bridge is `null` in non-Tauri environments. Re-exporting null
// here lets imports of `native` resolve to falsy without crashing.
export const native = null

// ── Tauri command stubs ───────────────────────────────────────────────────────

export const txlineFixturesSnapshotNative = vi.fn().mockResolvedValue([])
export const txlineOddsSnapshotNative = vi.fn().mockResolvedValue([])
export const txlineScoresSnapshotNative = vi.fn().mockResolvedValue([])

export const listRunsNative = vi.fn().mockResolvedValue([])
export const getRunNative = vi.fn().mockResolvedValue(undefined)
export const runAgentRoundNative = vi.fn().mockResolvedValue({})

export const listArenaPositionsNative = vi.fn().mockResolvedValue([])
export const listSettlementRecordsNative = vi.fn().mockResolvedValue([])
export const listSignalRecordsNative = vi.fn().mockResolvedValue([])
export const getArenaScoreNative = vi.fn().mockResolvedValue(undefined)
export const listAgentLeaderboardNative = vi.fn().mockResolvedValue([])
export const listToolCallRecordsNative = vi.fn().mockResolvedValue([])
export const getAgentSafetyStatusNative = vi.fn().mockResolvedValue(undefined)
export const tripKillSwitchNative = vi.fn().mockResolvedValue(undefined)
export const listArenaSessionsNative = vi.fn().mockResolvedValue([])

export const listAgentTraceNative = vi.fn().mockResolvedValue([])

export const chainRpcNative = vi.fn().mockResolvedValue(undefined)
export const chainStatusNative = vi.fn().mockResolvedValue(undefined)

// ── Tauri event subscription stubs ───────────────────────────────────────────
// Subscriptions return an unsubscribe function. Tests that need to simulate
// events should capture the callback via mockImplementation.

export const onAgentTrace = vi.fn().mockReturnValue(() => {})
export const onProofReceipt = vi.fn().mockReturnValue(() => {})
export const onArenaPosition = vi.fn().mockReturnValue(() => {})
export const onSettlementRecord = vi.fn().mockReturnValue(() => {})
export const onSignalRecord = vi.fn().mockReturnValue(() => {})
export const onSafetyGateTripped = vi.fn().mockReturnValue(() => {})
export const onArenaScore = vi.fn().mockReturnValue(() => {})
export const onToolCallRecord = vi.fn().mockReturnValue(() => {})
export const onTxLineEvent = vi.fn().mockReturnValue(() => {})
export const onIngestStatus = vi.fn().mockReturnValue(() => {})
export const onChainEvent = vi.fn().mockReturnValue(() => {})

// ── Auth command stubs ────────────────────────────────────────────────────────
// issueAuthChallengeNative: returns an AuthChallenge with nonce + message + ts
export const issueAuthChallengeNative = vi.fn().mockResolvedValue({
  nonce: 'test-nonce-uuid',
  message: 'Sign to authenticate with TxOdds Agent Desk\nNonce: test-nonce-uuid\nIssued: 2026-07-09T00:00:00.000Z',
  ts: '2026-07-09T00:00:00.000Z',
})
// requestAuthNative: receives pubkey + signature bytes + nonce (+ optional username/cluster), returns UserProfile
export const requestAuthNative = vi.fn().mockResolvedValue(undefined)
// getUserProfileNative: lookup by pubkey, returns UserProfile | null
export const getUserProfileNative = vi.fn().mockResolvedValue(null)
// saveUserProfileNative: upsert profile by pubkey, returns saved UserProfile
export const saveUserProfileNative = vi.fn().mockResolvedValue(undefined)
// deleteUserProfileNative: remove profile by pubkey
export const deleteUserProfileNative = vi.fn().mockResolvedValue(undefined)
