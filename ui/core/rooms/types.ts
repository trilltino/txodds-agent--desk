// Consumer track contract: Pulse Rooms (TypeScript mirror of
// src-tauri/src/domain/rooms.rs). Staged ahead of the room engine (PR 3) so
// UI and backend build against one reviewed shape. No wagering or settlement
// concepts belong in consumer mode.

export type RoomMode = 'sweepstake' | 'prediction_streak' | 'mixed'

export interface RoomMember {
  id: string
  displayName: string
  joinedAt: string
}

export interface RoomPick {
  memberId: string
  pick: string
  submittedAt: string
}

export interface LeaderboardEntry {
  memberId: string
  points: number
  /** Human-readable reason for the latest delta, shown next to the score. */
  lastDelta?: string
}

/** Fan-facing card with before/after implied probability so odds moves are explainable. */
export interface PulseCard {
  id: string
  fixtureId: number
  sourceEventId: string
  title: string
  body: string
  impliedBefore?: number
  impliedAfter?: number
  createdAt: string
}

export interface PulseRoom {
  id: string
  fixtureId: number
  name: string
  mode: RoomMode
  members: RoomMember[]
  picks: RoomPick[]
  leaderboard: LeaderboardEntry[]
  timeline: PulseCard[]
  createdAt: string
  updatedAt: string
}
