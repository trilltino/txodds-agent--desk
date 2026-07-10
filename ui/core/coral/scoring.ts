import type { AgentBid, TrackMode } from '../../types'

// Score a bid by confidence, price, ETA, and role/track fit. This mirrors the
// deterministic Rust round scoring used by the desktop app.
export function scoreBid(track: TrackMode, bid: AgentBid): number {
  // Role boost is where specialized strategies start. The desk currently runs a
  // single trading track, which favors sharp/risk sellers; the other roles keep
  // a neutral (1x) weight so they can still bid without a track-specific boost.
  const isTrading = track === 'trading'
  const roleBoost: Record<string, number> = {
    sharp: isTrading ? 1.25 : 1,
    risk: isTrading ? 1.15 : 1,
    settlement: 1,
    verifier: 1,
    pundit: 1,
    fan: 1
  }

  const pricePenalty = Math.max(0.2, 1 - bid.priceSol * 4)
  const etaBonus = bid.etaMs < 1500 ? 1.05 : 1
  return bid.confidence * pricePenalty * etaBonus * (roleBoost[bid.role] ?? 1)
}

// Pick the highest scoring bid without mutating the original bid array.
export function chooseWinner(track: TrackMode, bids: AgentBid[]): AgentBid | undefined {
  return [...bids].sort((a, b) => scoreBid(track, b) - scoreBid(track, a))[0]
}
