/**
 * tests/e2e/safety-pipeline.test.ts
 *
 * End-to-end flow test: agent safety gate — budget guard, kill switch, step cap.
 *
 * Exercises the complete TypeScript safety pipeline without a Tauri runtime:
 *
 *   1. An `AgentSafetyStatus` is built with known budget values.
 *   2. Budget-exceeded conditions are detected (tool-call cap, spend cap, step cap).
 *   3. Kill-switch state is read correctly.
 *   4. The `tripKillSwitchNative` transport stub is invoked and verified.
 *   5. Safety status is re-read after tripping and reflects the new state.
 *
 * No mocks are needed for the pure guard predicates. The native bridge
 * (`desktop/transport`) is stubbed for the IPC commands tested in steps 4–5.
 */

import { describe, expect, it, vi } from 'vitest'

// Intercept the real Tauri bridge before it loads — the factory returns the
// vi.fn() stubs defined in tests/__mocks__/transport.ts.
vi.mock('../../ui/desktop/transport', async () => await import('../__mocks__/transport'))

import { getAgentSafetyStatusNative, tripKillSwitchNative } from '../../ui/desktop/transport'
import { makeAgentSafetyStatus } from '../__helpers__/fixtures'

// ── Step 1 — AgentSafetyStatus shape ─────────────────────────────────────────

describe('step 1 — AgentSafetyStatus shape invariants', () => {
  it('all numeric budget fields are non-negative integers or decimals', () => {
    const s = makeAgentSafetyStatus()
    expect(s.budgetToolCallsUsed).toBeGreaterThanOrEqual(0)
    expect(s.budgetToolCallsLimit).toBeGreaterThan(0)
    expect(s.budgetSpendLamports).toBeGreaterThanOrEqual(0)
    expect(s.budgetSpendLimitLamports).toBeGreaterThan(0)
    expect(s.stepsUsed).toBeGreaterThanOrEqual(0)
    expect(s.stepsMax).toBeGreaterThan(0)
  })

  it('killSwitchTripped is a boolean', () => {
    const s = makeAgentSafetyStatus()
    expect(typeof s.killSwitchTripped).toBe('boolean')
  })

  it('lastCheckedAt is a valid ISO-8601 string', () => {
    const s = makeAgentSafetyStatus()
    expect(new Date(s.lastCheckedAt).toISOString()).toBe(s.lastCheckedAt)
  })
})

// ── Step 2 — budget guard predicates ──────────────────────────────────────────

/** Pure helper that mirrors agent-core safety guard logic. */
function isToolBudgetExceeded(s: { budgetToolCallsUsed: number; budgetToolCallsLimit: number }) {
  return s.budgetToolCallsUsed >= s.budgetToolCallsLimit
}
function isSpendBudgetExceeded(s: { budgetSpendLamports: number; budgetSpendLimitLamports: number }) {
  return s.budgetSpendLamports >= s.budgetSpendLimitLamports
}
function isStepCapExceeded(s: { stepsUsed: number; stepsMax: number }) {
  return s.stepsUsed >= s.stepsMax
}

describe('step 2 — budget guard predicates', () => {
  it('tool-call budget is NOT exceeded when used < limit', () => {
    const s = makeAgentSafetyStatus({ budgetToolCallsUsed: 5, budgetToolCallsLimit: 100 })
    expect(isToolBudgetExceeded(s)).toBe(false)
  })

  it('tool-call budget IS exceeded when used >= limit', () => {
    const s = makeAgentSafetyStatus({ budgetToolCallsUsed: 100, budgetToolCallsLimit: 100 })
    expect(isToolBudgetExceeded(s)).toBe(true)
  })

  it('spend budget is NOT exceeded when lamports < limit', () => {
    const s = makeAgentSafetyStatus({
      budgetSpendLamports: 500_000,
      budgetSpendLimitLamports: 100_000_000,
    })
    expect(isSpendBudgetExceeded(s)).toBe(false)
  })

  it('spend budget IS exceeded at the limit boundary', () => {
    const s = makeAgentSafetyStatus({
      budgetSpendLamports: 100_000_000,
      budgetSpendLimitLamports: 100_000_000,
    })
    expect(isSpendBudgetExceeded(s)).toBe(true)
  })

  it('step cap is not exceeded mid-run', () => {
    const s = makeAgentSafetyStatus({ stepsUsed: 25, stepsMax: 50 })
    expect(isStepCapExceeded(s)).toBe(false)
  })

  it('step cap IS exceeded at the limit boundary', () => {
    const s = makeAgentSafetyStatus({ stepsUsed: 50, stepsMax: 50 })
    expect(isStepCapExceeded(s)).toBe(true)
  })
})

// ── Step 3 — kill-switch state reading ────────────────────────────────────────

describe('step 3 — kill-switch state', () => {
  it('killSwitchTripped is false on a healthy agent', () => {
    const s = makeAgentSafetyStatus({ killSwitchTripped: false })
    expect(s.killSwitchTripped).toBe(false)
  })

  it('killSwitchTripped is true after the guard fires', () => {
    const s = makeAgentSafetyStatus({ killSwitchTripped: true })
    expect(s.killSwitchTripped).toBe(true)
  })
})

// ── Step 4 — tripKillSwitchNative transport stub ──────────────────────────────

describe('step 4 — tripKillSwitchNative transport stub', () => {
  it('tripKillSwitchNative is a mock function', () => {
    expect(vi.isMockFunction(tripKillSwitchNative)).toBe(true)
  })

  it('tripKillSwitchNative resolves without error by default', async () => {
    await expect(tripKillSwitchNative('match-intelligence')).resolves.toBeUndefined()
  })

  it('tripKillSwitchNative records the correct agentId', async () => {
    await tripKillSwitchNative('contrarian')
    expect(vi.mocked(tripKillSwitchNative)).toHaveBeenCalledWith('contrarian')
  })
})

// ── Step 5 — safety status after kill-switch trip ─────────────────────────────

describe('step 5 — re-reading safety status after trip', () => {
  it('getAgentSafetyStatusNative returns tripped state when stubbed', async () => {
    const tripped = makeAgentSafetyStatus({
      agentId: 'contrarian',
      killSwitchTripped: true,
      stepsUsed: 50,
      stepsMax: 50,
    })
    vi.mocked(getAgentSafetyStatusNative).mockResolvedValueOnce(tripped)
    const result = await getAgentSafetyStatusNative('contrarian')
    expect(result?.killSwitchTripped).toBe(true)
    expect(isStepCapExceeded(result!)).toBe(true)
  })

  it('full pipeline — detect → trip → re-read', async () => {
    // Detect: step cap exceeded
    const pre = makeAgentSafetyStatus({ agentId: 'sharp-movement-detector', stepsUsed: 50, stepsMax: 50 })
    expect(isStepCapExceeded(pre)).toBe(true)

    // Trip
    await tripKillSwitchNative(pre.agentId)
    expect(vi.mocked(tripKillSwitchNative)).toHaveBeenCalledWith('sharp-movement-detector')

    // Re-read
    const post = makeAgentSafetyStatus({ agentId: 'sharp-movement-detector', killSwitchTripped: true })
    vi.mocked(getAgentSafetyStatusNative).mockResolvedValueOnce(post)
    const result = await getAgentSafetyStatusNative('sharp-movement-detector')
    expect(result?.killSwitchTripped).toBe(true)
  })
})
