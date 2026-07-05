import type { AgentBid, TrackMode } from '../../types'

export function scoreBid(track: TrackMode, bid: AgentBid): number {
  const roleBoost: Record<string, number> = {
    sharp: track === 'trading' ? 1.25 : 1,
    risk: track === 'trading' ? 1.15 : 1,
    settlement: track === 'settlement' ? 1.25 : 1,
    verifier: track === 'settlement' ? 1.2 : 1,
    pundit: track === 'fan' ? 1.25 : 1,
    fan: track === 'fan' ? 1.2 : 1
  }
  const pricePenalty = Math.max(0.2, 1 - bid.priceSol * 4)
  const etaBonus = bid.etaMs < 1500 ? 1.05 : 1
  return bid.confidence * pricePenalty * etaBonus * (roleBoost[bid.role] ?? 1)
}

export function chooseWinner(track: TrackMode, bids: AgentBid[]): AgentBid | undefined {
  return [...bids].sort((a, b) => scoreBid(track, b) - scoreBid(track, a))[0]
}
