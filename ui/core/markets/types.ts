// Web3 track contract: verified prediction markets (TypeScript mirror of
// src-tauri/src/domain/markets.rs). Markets resolve exclusively through the
// deterministic proof gate; rules are machine-readable predicates, never LLM
// output.

export type MarketStatus = 'draft' | 'open' | 'locked' | 'resolving' | 'resolved' | 'voided'

export type EscrowMode = 'none' | 'simulated' | 'devnet'

/** Machine-readable settlement rule evaluated by the proof gate. */
export interface SettlementRule {
  predicate: string
  statKey?: number
  description: string
}

export interface MarketOutcome {
  id: string
  label: string
  won?: boolean
}

export interface PredictionMarket {
  id: string
  fixtureId: number
  title: string
  rule: SettlementRule
  outcomes: MarketOutcome[]
  status: MarketStatus
  escrowMode: EscrowMode
  escrowPda?: string
  receiptId?: string
}
