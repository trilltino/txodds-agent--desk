export type TrackMode = 'settlement' | 'trading' | 'fan'

export interface CoralAgentManifest {
  id: string
  displayName: string
  coralRole: 'buyer' | 'seller' | 'verifier' | 'settlement' | string
  service: string
  manifestPath: string
  description: string
}

export type TxLineEventKind =
  | 'fixture'
  | 'score_update'
  | 'odds_update'
  | 'goal'
  | 'red_card'
  | 'final_whistle'
  | 'odds_move'
  | 'proof_received'

export interface Fixture {
  fixtureId: number
  home: string
  away: string
  startTime?: string
  competition?: string
  status?: string
}

export interface OddsQuote {
  fixtureId: number
  outcome: 'home' | 'draw' | 'away' | string
  decimal: number
  impliedProbability: number
  source?: string
  ts: string
}

export interface TxLineEvent {
  id: string
  kind: TxLineEventKind
  fixtureId: number
  title: string
  body: string
  ts: string
  raw?: unknown
  odds?: OddsQuote[]
  score?: { home: number; away: number }
  proof?: TxLineProofReceipt
}

export interface TxLineProofReceipt {
  fixtureId: number
  seq?: number
  statKey?: number
  merkleRoot?: string
  statProofHash?: string
  txlineProgram?: string
  verified: boolean
  note: string
}

export interface AgentBid {
  agentId: string
  role: 'sharp' | 'risk' | 'pundit' | 'settlement' | 'fan' | 'verifier'
  priceSol: number
  confidence: number
  etaMs: number
  note: string
}

export interface AgentDelivery {
  agentId: string
  title: string
  payload: string
  sha256: string
  citations: string[]
  strategy?: string
  risk?: string
  fanCopy?: string
}

export interface VerificationVerdict {
  status: 'pass' | 'fail' | 'needs_review'
  reason: string
  checked: Array<'txline-input' | 'hash' | 'proof' | 'policy' | 'settlement'>
}

export interface SettlementReceipt {
  status: 'not_started' | 'escrow_created' | 'deposited' | 'released' | 'refunded'
  reference?: string
  escrowPda?: string
  depositTx?: string
  releaseTx?: string
  explorerUrl?: string
  tritonObserved?: boolean
  tritonSlot?: number
}

export interface AgentRun {
  runId: string
  track: TrackMode
  trigger: TxLineEvent
  bids: AgentBid[]
  winner?: AgentBid
  delivery?: AgentDelivery
  verdict?: VerificationVerdict
  settlement?: SettlementReceipt
  timeline: Array<{ at: string; label: string; detail: string }>
}
