import type { AgentBid, TrackMode, TxLineEvent } from '../../types'

export function generateBids(event: TxLineEvent, track: TrackMode): AgentBid[] {
  const base = event.kind === 'odds_move' ? 0.82 : event.kind === 'goal' ? 0.78 : 0.7
  const bids: AgentBid[] = [
    {
      agentId: 'seller-worldcup-edge',
      role: 'sharp',
      priceSol: 0.018,
      confidence: Math.min(0.94, base + 0.08),
      etaMs: 900,
      note: 'TxLINE seller: detects implied-probability movement, compares the board, and delivers a fair-line read.'
    },
    {
      agentId: 'seller-risk-policy',
      role: 'risk',
      priceSol: 0.012,
      confidence: Math.min(0.9, base + 0.02),
      etaMs: 700,
      note: 'Risk seller: turns a signal into no-action / observe / simulate-position with bounded downside.'
    },
    {
      agentId: 'seller-fan-card',
      role: 'pundit',
      priceSol: 0.01,
      confidence: Math.min(0.88, base + 0.01),
      etaMs: 600,
      note: 'Fan seller: explains the football story and market movement in plain English.'
    },
    {
      agentId: 'verifier-agent',
      role: 'verifier',
      priceSol: 0.009,
      confidence: 0.91,
      etaMs: 800,
      note: 'Independent verifier: checks content hash, fixture binding, TxLINE proof shape, and policy gates.'
    },
    {
      agentId: 'settlement-arbiter-agent',
      role: 'settlement',
      priceSol: 0.016,
      confidence: 0.92,
      etaMs: 1100,
      note: 'Settlement arbiter: packages the verified run for CoralOS escrow release and Triton observation.'
    }
  ]
  return bids.filter((bid) => {
    if (track === 'fan') return ['pundit', 'fan', 'sharp'].includes(bid.role)
    if (track === 'trading') return ['sharp', 'risk', 'pundit'].includes(bid.role)
    return ['settlement', 'verifier', 'sharp', 'risk'].includes(bid.role)
  })
}
