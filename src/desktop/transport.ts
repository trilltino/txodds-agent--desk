import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { AgentRun, CoralAgentManifest, IngestStatus, SolanaPayIntent, TrackMode, TxLineEvent } from '../types'
import type { ChainStatus, Cluster, TritonObservation } from '../domain/triton/client'

// Runtime feature flag used by domain helpers to choose Tauri IPC in desktop
// mode and browser fallback logic during plain Vite development.
export const native = isTauri()

// PublicConfig is deliberately non-secret. Rust may know tokens, keypaths, and
// sidecar credentials; the webview only receives booleans and public origins.
export interface PublicConfig {
  txlineApiOrigin: string
  txlineNetwork: string
  solanaCluster: string
  oddsMoveTriggerPct: number
  maxDevnetSpendSol: number
  txlineConfigured: boolean
  tritonConfigured: boolean
  tritonDevnetConfigured: boolean
  tritonMainnetConfigured: boolean
  yellowstoneConfigured: boolean
  solanaPayConfigured: boolean
  coralosConfigured: boolean
  axumEnabled: boolean
}

// Native export commands return a local path plus user-facing copy. The webview
// requests the export but Rust owns filesystem writes.
export interface ExportResult {
  path: string
  shareText: string
}

// Thin invoke wrapper. Keeping this generic function small makes it obvious
// which named Tauri command each exported helper calls.
export function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(name, args)
}

export async function getConfig(): Promise<PublicConfig> {
  return command<PublicConfig>('get_config')
}

export async function listCoralAgentsNative(): Promise<CoralAgentManifest[]> {
  return command<CoralAgentManifest[]>('list_coral_agents')
}

export async function chainRpcNative<T>(cluster: Cluster, method: string, params: unknown[] = []): Promise<T> {
  return command<T>('chain_rpc', { cluster, method, params })
}

export async function chainStatusNative(cluster: Cluster): Promise<ChainStatus> {
  return command<ChainStatus>('chain_status', { cluster })
}

export async function observeSettlementNative(reference: string, escrowAccount?: string): Promise<TritonObservation> {
  return command<TritonObservation>('observe_settlement', { reference, escrowAccount })
}

export async function startTxLine(mode: 'live' | 'mock' | 'replay', fixtureId?: string): Promise<void> {
  // Browser mode has no privileged TxLINE ingest task to start.
  if (!native) return
  return command<void>('start_txline', { mode, fixtureId })
}

export async function stopTxLine(): Promise<void> {
  if (!native) return
  return command<void>('stop_txline')
}

export async function runAgentRoundNative(trigger: TxLineEvent, track: TrackMode): Promise<AgentRun> {
  return command<AgentRun>('run_agent_round', { trigger, track })
}

export async function listRunsNative(): Promise<AgentRun[]> {
  return command<AgentRun[]>('list_runs')
}

export async function createSolanaPayIntentNative(runId: string): Promise<SolanaPayIntent> {
  return command<SolanaPayIntent>('create_solana_pay_intent', { runId })
}

export async function verifySolanaPayIntentNative(reference: string): Promise<SolanaPayIntent> {
  return command<SolanaPayIntent>('verify_solana_pay_intent', { reference })
}

export async function listPaymentIntentsNative(runId?: string): Promise<SolanaPayIntent[]> {
  return command<SolanaPayIntent[]>('list_payment_intents', { runId })
}

export async function exportFanCardNative(runId: string): Promise<ExportResult> {
  return command<ExportResult>('export_fan_card', { runId })
}

export async function watchAccountNative(account: string): Promise<void> {
  if (!native) return
  return command<void>('watch_account', { account })
}

export async function watchProgramNative(programId: string): Promise<void> {
  if (!native) return
  return command<void>('watch_program', { programId })
}

export async function watchReferenceNative(reference: string): Promise<void> {
  if (!native) return
  return command<void>('watch_reference', { reference })
}

export function onNativeEvent<T>(event: string, cb: (payload: T) => void): () => void {
  if (!native) return () => {}
  // Tauri listen returns the unlisten function asynchronously. The active flag
  // prevents late registration from leaking after React unmounts a subscriber.
  let active = true
  let unlisten: (() => void) | undefined
  listen<T>(event, (message) => {
    if (active) cb(message.payload)
  }).then((fn) => {
    if (active) unlisten = fn
    else fn()
  })
  return () => {
    active = false
    unlisten?.()
  }
}

export const onTxLineEvent = (cb: (event: TxLineEvent) => void) => onNativeEvent<TxLineEvent>('txline://event', cb)
export const onIngestStatus = (cb: (status: IngestStatus) => void) => onNativeEvent<IngestStatus>('ingest://status', cb)
export const onSolanaPayIntent = (cb: (intent: SolanaPayIntent) => void) => onNativeEvent<SolanaPayIntent>('pay://intent', cb)
export const onSolanaPayStatus = (cb: (intent: SolanaPayIntent) => void) => onNativeEvent<SolanaPayIntent>('pay://status', cb)
export const onChainSlot = (cb: (status: ChainStatus) => void) => onNativeEvent<ChainStatus>('chain://slot', cb)
