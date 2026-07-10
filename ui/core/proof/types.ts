// Proof contract: human-readable receipts over code-only verification
// (TypeScript mirror of src-tauri/src/domain/proof.rs). Every field that gates
// money or market state is computed deterministically; `humanSummary` is the
// only field an LLM may ever write.

export type OnchainValidationStatus =
  | 'not_started'
  | 'simulated_pass'
  | 'simulated_fail'
  | 'tx_pass'
  | 'tx_fail'

export interface VerificationReceipt {
  id: string
  fixtureId: number
  marketId?: string
  seq?: number
  /** The evaluated predicate, verbatim, so receipts are auditable. */
  predicate: string
  txlineValidationFetched: boolean
  merkleProofPresent: boolean
  deterministicPredicatePassed: boolean
  onchainValidationStatus: OnchainValidationStatus
  txSignature?: string
  explorerUrl?: string
  humanSummary: string
  raw?: unknown
}
