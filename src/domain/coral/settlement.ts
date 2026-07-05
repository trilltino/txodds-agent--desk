import type { AgentDelivery, SettlementReceipt, TxLineProofReceipt } from '../../types'

export function buildReference(delivery: AgentDelivery, proof?: TxLineProofReceipt): string {
  return proof?.merkleRoot ? `txline:${proof.fixtureId}:${proof.merkleRoot}:${delivery.sha256}` : `sha256:${delivery.sha256}`
}

export async function createDevnetEscrowStub(delivery: AgentDelivery, proof?: TxLineProofReceipt): Promise<SettlementReceipt> {
  return {
    status: 'deposited',
    reference: buildReference(delivery, proof),
    escrowPda: 'replace-with-real-escrow-pda',
    depositTx: 'replace-with-real-deposit-signature',
    explorerUrl: 'https://explorer.solana.com/?cluster=devnet',
    tritonObserved: false
  }
}

export async function releaseDevnetEscrowStub(receipt: SettlementReceipt): Promise<SettlementReceipt> {
  return {
    ...receipt,
    status: 'released',
    releaseTx: 'replace-with-real-release-signature',
    explorerUrl: 'https://explorer.solana.com/?cluster=devnet'
  }
}
