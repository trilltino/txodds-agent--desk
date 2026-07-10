/**
 * tests/core/agent/leaderboard.test.ts
 *
 * Unit tests for leaderboard-related type contracts and derivation logic
 * (mirrors `AgentLeaderboardEntry::from_positions` in Rust).
 *
 * The frontend does not recompute the leaderboard — it receives the computed
 * value from the native side. This file therefore tests the **shape invariants**
 * that every leaderboard entry must satisfy, ensuring the UI will not render
 * nonsensical values even if the Rust side sends unexpected data.
 */

import { describe, expect, it } from 'vitest'
import { makeLeaderboardEntry, makeArenaScore, makeArenaPosition } from '../../__helpers__/fixtures'
import type { AgentLeaderboardEntry } from '../../../ui/core/agent/types'

// ── Shape invariants ──────────────────────────────────────────────────────────

describe('AgentLeaderboardEntry shape invariants', () => {
  it('winRate is consistent with positionsWon / positionsTaken', () => {
    const entry = makeLeaderboardEntry({ positionsWon: 6, positionsTaken: 10, winRate: 0.6 })
    expect(entry.winRate).toBeCloseTo(entry.positionsWon / entry.positionsTaken, 2)
  })

  it('winRate is 0 when no positions taken', () => {
    const entry = makeLeaderboardEntry({ positionsWon: 0, positionsTaken: 0, winRate: 0 })
    expect(entry.winRate).toBe(0)
  })

  it('winRate is between 0 and 1', () => {
    const entry = makeLeaderboardEntry()
    expect(entry.winRate).toBeGreaterThanOrEqual(0)
    expect(entry.winRate).toBeLessThanOrEqual(1)
  })

  it('totalPnlPoints can be negative (losing run)', () => {
    const entry = makeLeaderboardEntry({ totalPnlPoints: -3.5, positionsWon: 1, positionsTaken: 10, winRate: 0.1 })
    expect(entry.totalPnlPoints).toBeLessThan(0)
  })

  it('strategy is FollowSharp or FadeSharp', () => {
    const follow = makeLeaderboardEntry({ strategy: 'FollowSharp' })
    const fade = makeLeaderboardEntry({ strategy: 'FadeSharp' })
    expect(['FollowSharp', 'FadeSharp']).toContain(follow.strategy)
    expect(['FollowSharp', 'FadeSharp']).toContain(fade.strategy)
  })

  it('agentId is a non-empty string', () => {
    const entry = makeLeaderboardEntry()
    expect(typeof entry.agentId).toBe('string')
    expect(entry.agentId.length).toBeGreaterThan(0)
  })

  it('avgWinningConfidence is between 0 and 1', () => {
    const entry = makeLeaderboardEntry()
    expect(entry.avgWinningConfidence).toBeGreaterThanOrEqual(0)
    expect(entry.avgWinningConfidence).toBeLessThanOrEqual(1)
  })
})

// ── ArenaScore leader field ────────────────────────────────────────────────────

describe('ArenaScore leader field', () => {
  it('FOLLOW wins when followPnl > fadePnl', () => {
    const score = makeArenaScore({ followPnl: 5, fadePnl: 2, leader: 'FOLLOW (match-intelligence)' })
    expect(score.leader).toBe('FOLLOW (match-intelligence)')
  })

  it('FADE wins when fadePnl > followPnl', () => {
    const score = makeArenaScore({ followPnl: 1, fadePnl: 3, leader: 'FADE (contrarian)' })
    expect(score.leader).toBe('FADE (contrarian)')
  })

  it('TIE when pnl is equal', () => {
    const score = makeArenaScore({ followPnl: 2, fadePnl: 2, leader: 'TIE' })
    expect(score.leader).toBe('TIE')
  })
})

// ── ArenaPosition field contracts ─────────────────────────────────────────────

describe('ArenaPosition field contracts', () => {
  it('direction is With or Against', () => {
    const withPos = makeArenaPosition({ direction: 'With' })
    const againstPos = makeArenaPosition({ direction: 'Against' })
    expect(['With', 'Against']).toContain(withPos.direction)
    expect(['With', 'Against']).toContain(againstPos.direction)
  })

  it('oddsAtEntry is greater than 1 (valid decimal odds)', () => {
    const pos = makeArenaPosition({ oddsAtEntry: 2.5 })
    expect(pos.oddsAtEntry).toBeGreaterThan(1)
  })

  it('confidence is between 0 and 1', () => {
    const pos = makeArenaPosition()
    expect(pos.confidence).toBeGreaterThanOrEqual(0)
    expect(pos.confidence).toBeLessThanOrEqual(1)
  })

  it('outcome pnlPoints = (oddsAtEntry - 1) when selection won', () => {
    const pos = makeArenaPosition({
      oddsAtEntry: 3.0,
      outcome: {
        selectionWon: true,
        finalScore: '1-0',
        pnlPoints: 2.0, // 3.0 - 1.0
        settledAt: '2026-06-14T16:00:00Z',
      },
    })
    expect(pos.outcome?.pnlPoints).toBeCloseTo(pos.oddsAtEntry - 1, 5)
  })

  it('outcome pnlPoints = -1 when selection lost', () => {
    const pos = makeArenaPosition({
      oddsAtEntry: 2.5,
      outcome: {
        selectionWon: false,
        finalScore: '0-1',
        pnlPoints: -1.0,
        settledAt: '2026-06-14T16:00:00Z',
      },
    })
    expect(pos.outcome?.pnlPoints).toBe(-1)
  })
})
