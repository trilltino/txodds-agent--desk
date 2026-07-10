import { chainRpcNative, chainStatusNative, native, observeSettlementNative } from '../../desktop/transport'

// Solana RPC reads are desktop-only. Rust owns endpoints/tokens and the
// webview never talks to RPC providers directly.

export type Cluster = 'devnet' | 'mainnet'

export interface ChainStatus {
  cluster: Cluster
  slot: number
  solanaCore: string
  latencyMs: number
  ts: string
}

export interface ChainObservation {
  kind: 'deposit' | 'release' | 'refund' | 'account_update' | 'program_tx'
  signature?: string
  slot?: number
  blockhash?: string
  account?: string
  programId?: string
  note: string
}

function desktopOnly(): never {
  throw new Error('Chain access is desktop-only; Rust owns RPC credentials')
}

export async function solanaRpc<T>(cluster: Cluster, method: string, params: unknown[] = []): Promise<T> {
  if (!native) desktopOnly()
  return chainRpcNative<T>(cluster, method, params)
}

export const getVersion = (cluster: Cluster) => solanaRpc<{ 'solana-core': string }>(cluster, 'getVersion')

export const getBalanceSol = (cluster: Cluster, pubkey: string) =>
  solanaRpc<{ value: number }>(cluster, 'getBalance', [pubkey]).then((r) => r.value / 1_000_000_000)

export const getSignaturesForAddress = (cluster: Cluster, address: string, limit = 10) =>
  solanaRpc<Array<{ signature: string; slot: number; err: unknown }>>(cluster, 'getSignaturesForAddress', [
    address,
    { limit }
  ])

export async function getChainStatus(cluster: Cluster): Promise<ChainStatus> {
  if (!native) desktopOnly()
  return chainStatusNative(cluster)
}

/**
 * Stamp a settlement reference with live devnet chain state: current slot and
 * latest blockhash. Once real escrow PDAs exist, pass the account address to
 * also surface its most recent signature.
 */
export async function observeSettlement(reference: string, escrowAccount?: string): Promise<ChainObservation> {
  if (!native) desktopOnly()
  return observeSettlementNative(reference, escrowAccount)
}
