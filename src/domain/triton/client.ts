import { chainRpcNative, chainStatusNative, native, observeSettlementNative, onChainSlot } from '../../desktop/transport'

// Browser-dev mode still uses the Vite proxy for fast React iteration. Native
// Tauri mode routes Triton calls through Rust so tokens and rpcpool requests
// never enter the webview.

export type Cluster = 'devnet' | 'mainnet'

export interface ChainStatus {
  cluster: Cluster
  slot: number
  solanaCore: string
  latencyMs: number
  ts: string
}

export interface TritonObservation {
  kind: 'deposit' | 'release' | 'refund' | 'account_update' | 'program_tx'
  signature?: string
  slot?: number
  blockhash?: string
  account?: string
  programId?: string
  note: string
}

const RPC_PATH: Record<Cluster, string> = { devnet: '/rpc/devnet', mainnet: '/rpc/mainnet' }

let nextId = 0

export async function tritonRpc<T>(cluster: Cluster, method: string, params: unknown[] = []): Promise<T> {
  if (native) return chainRpcNative<T>(cluster, method, params)

  const res = await fetch(RPC_PATH[cluster], {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: ++nextId, method, params })
  })
  if (!res.ok) throw new Error(`Triton ${cluster} HTTP ${res.status}`)
  const body = await res.json()
  if (body.error) throw new Error(`Triton ${cluster} ${method}: ${body.error.message}`)
  return body.result as T
}

export const getSlot = (cluster: Cluster) => tritonRpc<number>(cluster, 'getSlot')

export const getVersion = (cluster: Cluster) => tritonRpc<{ 'solana-core': string }>(cluster, 'getVersion')

export const getBalanceSol = (cluster: Cluster, pubkey: string) =>
  tritonRpc<{ value: number }>(cluster, 'getBalance', [pubkey]).then((r) => r.value / 1_000_000_000)

export const getSignaturesForAddress = (cluster: Cluster, address: string, limit = 10) =>
  tritonRpc<Array<{ signature: string; slot: number; err: unknown }>>(cluster, 'getSignaturesForAddress', [
    address,
    { limit }
  ])

const versionCache = new Map<Cluster, string>()

export async function getChainStatus(cluster: Cluster): Promise<ChainStatus> {
  if (native) return chainStatusNative(cluster)

  const started = performance.now()
  const slot = await getSlot(cluster)
  const latencyMs = Math.round(performance.now() - started)
  let solanaCore = versionCache.get(cluster)
  if (!solanaCore) {
    solanaCore = (await getVersion(cluster))['solana-core']
    versionCache.set(cluster, solanaCore)
  }
  return { cluster, slot, solanaCore, latencyMs, ts: new Date().toISOString() }
}

/**
 * Poll chain status until the returned stop function is called.
 * The devnet token is on the developer rate tier, so keep intervalMs >= 4000.
 */
export function watchSlots(
  cluster: Cluster,
  onStatus: (status: ChainStatus) => void,
  onError: (err: Error) => void = () => {},
  intervalMs = 5000
): () => void {
  if (native) {
    const stopEvent = onChainSlot((status) => {
      if (status.cluster === cluster) onStatus(status)
    })
    let stopped = false
    const tick = async () => {
      if (stopped) return
      try {
        onStatus(await getChainStatus(cluster))
      } catch (err) {
        if (!stopped) onError(err as Error)
      }
    }
    void tick()
    const timer = setInterval(tick, intervalMs)
    return () => {
      stopped = true
      stopEvent()
      clearInterval(timer)
    }
  }

  let stopped = false
  let inFlight = false
  const tick = async () => {
    if (stopped || inFlight) return
    inFlight = true
    try {
      const status = await getChainStatus(cluster)
      if (!stopped) onStatus(status)
    } catch (err) {
      if (!stopped) onError(err as Error)
    } finally {
      inFlight = false
    }
  }
  void tick()
  const timer = setInterval(tick, intervalMs)
  return () => {
    stopped = true
    clearInterval(timer)
  }
}

/**
 * Stamp a settlement reference with live devnet chain state: current slot and
 * latest blockhash. Once real escrow PDAs exist, pass the account address to
 * also surface its most recent signature.
 */
export async function observeSettlement(reference: string, escrowAccount?: string): Promise<TritonObservation> {
  if (native) return observeSettlementNative(reference, escrowAccount)

  const [slot, blockhashInfo] = await Promise.all([
    getSlot('devnet'),
    tritonRpc<{ value: { blockhash: string } }>('devnet', 'getLatestBlockhash')
  ])
  let signature: string | undefined
  if (escrowAccount) {
    const sigs = await getSignaturesForAddress('devnet', escrowAccount, 1).catch(() => [])
    signature = sigs[0]?.signature
  }
  return {
    kind: 'account_update',
    slot,
    blockhash: blockhashInfo.value.blockhash,
    signature,
    account: escrowAccount,
    note: `Triton devnet observed ${reference} at slot ${slot}`
  }
}
