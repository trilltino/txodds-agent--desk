import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type {
  AgentRun,
  AgentTraceEvent,
  AuthChallenge,
  CoralAgentManifest,
  CoralMessage,
  CoralSession,
  IngestStatus,
  SolanaPayIntent,
  TrackMode,
  TxLineEvent,
  TxLineProofReceipt,
  TxOracleRootEvent,
  UserProfile,
} from '../types'
import type { ChainStatus, Cluster, ChainObservation } from '../core/chain/client'
import type {
  AgentLeaderboardEntry,
  AgentSafetyStatus,
  ArenaPosition,
  ArenaScore,
  ArenaSession,
  BacktestSummary,
  SettlementRecord,
  SignalRecord,
  ToolCallRecord,
} from '../core/agent/types'
import { NativeEvents } from './events'

// Runtime feature flag used to block direct browser rendering. The app's data
// and privileged operations are desktop-only.
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
  rpcConfigured: boolean
  rpcDevnetConfigured: boolean
  rpcMainnetConfigured: boolean
  yellowstoneConfigured: boolean
  solanaPayConfigured: boolean
  coralosConfigured: boolean
  coralosConsoleEnabled: boolean
  llmConfigured: boolean
  llmProvider: string
  llmModel: string
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

export async function listCoralMessagesNative(runId: string): Promise<CoralMessage[]> {
  return command<CoralMessage[]>('coral_list_messages', { runId })
}

export async function listAgentTraceNative(runId: string): Promise<AgentTraceEvent[]> {
  return command<AgentTraceEvent[]>('agent_list_trace', { runId })
}

export async function chainRpcNative<T>(cluster: Cluster, method: string, params: unknown[] = []): Promise<T> {
  return command<T>('chain_rpc', { cluster, method, params })
}

export async function chainStatusNative(cluster: Cluster): Promise<ChainStatus> {
  return command<ChainStatus>('chain_status', { cluster })
}

export async function observeSettlementNative(reference: string, escrowAccount?: string): Promise<ChainObservation> {
  return command<ChainObservation>('observe_settlement', { reference, escrowAccount })
}

export async function startTxLine(fixtureId?: string): Promise<void> {
  if (!native) throw new Error('World Cup Agent Desk requires the Tauri desktop runtime')
  return command<void>('start_txline', { mode: 'live', fixtureId })
}

export async function stopTxLine(): Promise<void> {
  if (!native) throw new Error('World Cup Agent Desk requires the Tauri desktop runtime')
  return command<void>('stop_txline')
}

export async function runAgentRoundNative(trigger: TxLineEvent, track: TrackMode): Promise<AgentRun> {
  return command<AgentRun>('run_agent_round', { trigger, track })
}

export async function txlineFixturesSnapshotNative(startEpochDay?: number, competitionId?: number): Promise<unknown> {
  return command<unknown>('txline_fixtures_snapshot', { startEpochDay, competitionId })
}

export async function txlineOddsSnapshotNative(fixtureId: number, asOf?: number): Promise<unknown> {
  return command<unknown>('txline_odds_snapshot', { fixtureId, asOf })
}

export async function txlineOddsUpdatesNative(fixtureId: number): Promise<unknown> {
  return command<unknown>('txline_odds_updates', { fixtureId })
}

export async function txlineOddsIntervalNative(epochDay: number, hourOfDay: number, interval: number): Promise<unknown> {
  return command<unknown>('txline_odds_interval', { epochDay, hourOfDay, interval })
}

export async function txlineScoresSnapshotNative(fixtureId: number, asOf?: number): Promise<unknown> {
  return command<unknown>('txline_scores_snapshot', { fixtureId, asOf })
}

export async function txlineScoresUpdatesNative(fixtureId: number): Promise<unknown> {
  return command<unknown>('txline_scores_updates', { fixtureId })
}

export async function txlineScoresHistoricalNative(fixtureId: number): Promise<unknown> {
  return command<unknown>('txline_scores_historical', { fixtureId })
}

export async function txlineScoresIntervalNative(epochDay: number, hourOfDay: number, interval: number): Promise<unknown> {
  return command<unknown>('txline_scores_interval', { epochDay, hourOfDay, interval })
}

export async function txlineScoresStatValidationNative(args: {
  fixtureId: number
  seq: number
  statKey?: number
  statKey2?: number
  statKeys?: string
}): Promise<unknown> {
  return command<unknown>('txline_scores_stat_validation', args)
}

export async function fetchTxLineNative(path: string): Promise<unknown> {
  return command<unknown>('fetch_txline', { path })
}

export async function listRunsNative(): Promise<AgentRun[]> {
  return command<AgentRun[]>('list_runs')
}

export async function createSolanaPayIntentNative(
  runId: string,
  options?: { amountSol?: number; label?: string; memo?: string },
): Promise<SolanaPayIntent> {
  return command<SolanaPayIntent>('create_solana_pay_intent', {
    runId,
    amountSol: options?.amountSol,
    label: options?.label,
    memo: options?.memo,
  })
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

export const onTxLineEvent = (cb: (event: TxLineEvent) => void) => onNativeEvent<TxLineEvent>(NativeEvents.txlineEvent, cb)
export const onIngestStatus = (cb: (status: IngestStatus) => void) => onNativeEvent<IngestStatus>(NativeEvents.ingestStatus, cb)
export const onSolanaPayIntent = (cb: (intent: SolanaPayIntent) => void) => onNativeEvent<SolanaPayIntent>(NativeEvents.payIntent, cb)
export const onSolanaPayStatus = (cb: (intent: SolanaPayIntent) => void) => onNativeEvent<SolanaPayIntent>(NativeEvents.payStatus, cb)
export const onCoralSession = (cb: (session: CoralSession) => void) => onNativeEvent<CoralSession>(NativeEvents.coralSession, cb)
export const onCoralMessage = (cb: (message: CoralMessage) => void) => onNativeEvent<CoralMessage>(NativeEvents.coralMessage, cb)
export const onAgentTrace = (cb: (trace: AgentTraceEvent) => void) => onNativeEvent<AgentTraceEvent>(NativeEvents.agentTrace, cb)
export const onProofReceipt = (cb: (receipt: TxLineProofReceipt) => void) => onNativeEvent<TxLineProofReceipt>(NativeEvents.web3ProofReceipt, cb)
export const onTxOracleRoot = (cb: (root: TxOracleRootEvent) => void) => onNativeEvent<TxOracleRootEvent>(NativeEvents.txoracleRoot, cb)

// ── Arena / agent power commands ──────────────────────────────────────────────

/** List positions recorded by match-intelligence and contrarian agents. */
export async function listArenaPositionsNative(agentId?: string): Promise<ArenaPosition[]> {
  return command<ArenaPosition[]>('list_arena_positions', agentId ? { agentId } : {})
}

/** Retrieve settlement records written by arena-coordinator. */
export async function listSettlementRecordsNative(): Promise<SettlementRecord[]> {
  return command<SettlementRecord[]>('list_settlement_records')
}

/** Get the current FollowSharp vs FadeSharp scoreboard. */
export async function getArenaScoreNative(): Promise<ArenaScore> {
  return command<ArenaScore>('get_arena_score')
}

/** List signals detected by sharp-movement-detector, newest first. */
export async function listSignalRecordsNative(): Promise<SignalRecord[]> {
  return command<SignalRecord[]>('list_signal_records')
}

// ── Backtest commands ──────────────────────────────────────────────────────────

/**
 * Replay `fixtureId`'s real historical TxLINE odds (fetched hour-by-hour
 * server-side, never the whole-fixture endpoint) and settle simulated
 * FollowSharp/FadeSharp positions against its real final score.
 *
 * `home`/`away`/`kickoffTsMs` come from the caller's own fixture data — no
 * redundant lookup happens server-side. Rejects with an error if the
 * fixture has no final score yet (a backtest needs a completed match).
 */
export async function runBacktestNative(
  fixtureId: number,
  home: string,
  away: string,
  kickoffTsMs: number,
): Promise<BacktestSummary> {
  return command<BacktestSummary>('run_backtest', { fixtureId, home, away, kickoffTsMs })
}

/** List persisted backtest settlement rows, optionally scoped to one fixture. */
export async function listBacktestSettlementsNative(fixtureId?: number): Promise<unknown[]> {
  return command<unknown[]>('list_backtest_settlements', fixtureId !== undefined ? { fixtureId } : {})
}

/**
 * Fetch live safety gate telemetry for one sidecar agent.
 * Returns budget consumption and step counter telemetry.
 */
export async function getAgentSafetyStatusNative(agentId: string): Promise<AgentSafetyStatus> {
  return command<AgentSafetyStatus>('get_agent_safety_status', { agentId })
}

// ── Arena / agent power event streams ─────────────────────────────────────────

/** Fired when match-intelligence or contrarian records a new arena position. */
export const onArenaPosition = (cb: (pos: ArenaPosition) => void) =>
  onNativeEvent<ArenaPosition>('arena_position', cb)

/** Fired when arena-coordinator writes a new settlement record. */
export const onSettlementRecord = (cb: (rec: SettlementRecord) => void) =>
  onNativeEvent<SettlementRecord>('settlement_record', cb)

/** Fired when sharp-movement-detector emits a new signal. */
export const onSignalRecord = (cb: (rec: SignalRecord) => void) =>
  onNativeEvent<SignalRecord>('signal_record', cb)

// ── Leaderboard commands ───────────────────────────────────────────────────────

/**
 * Retrieve aggregate leaderboard stats for all agents.
 * Derived by the backend from settled ArenaPositions.
 */
export async function listAgentLeaderboardNative(): Promise<AgentLeaderboardEntry[]> {
  return command<AgentLeaderboardEntry[]>('list_agent_leaderboard')
}

// ── Arena session commands ─────────────────────────────────────────────────────

/**
 * List all arena sessions (one per World Cup fixture), newest first.
 * Each session aggregates its positions and settlement status.
 */
export async function listArenaSessionsNative(): Promise<ArenaSession[]> {
  return command<ArenaSession[]>('list_arena_sessions')
}

// ── Tool call audit commands ───────────────────────────────────────────────────

/**
 * Retrieve the tool call audit log for a single agent or all agents.
 * Entries are written before execution begins (§24, §38).
 */
export async function listToolCallRecordsNative(agentId?: string): Promise<ToolCallRecord[]> {
  return command<ToolCallRecord[]>('list_tool_call_records', agentId ? { agentId } : {})
}

// ── Additional live event streams ──────────────────────────────────────────────

/**
 * Fired whenever a tool call record is committed to the audit log.
 * Written pre-execution so the UI sees attempted calls even if they fail.
 */
export const onToolCallRecord = (cb: (rec: ToolCallRecord) => void) =>
  onNativeEvent<ToolCallRecord>('tool_call_record', cb)

/**
 * Fired when the arena-coordinator recomputes the contest score after
 * a settlement. Gives the scoreboard live push updates without polling.
 */
export const onArenaScore = (cb: (score: ArenaScore) => void) =>
  onNativeEvent<ArenaScore>('arena_score_updated', cb)

// ── Auth / user profile commands ───────────────────────────────────────────────

/**
 * Request a one-time sign challenge from the backend for `publicKey`.
 *
 * The returned `AuthChallenge.message` should be signed by the wallet and the
 * resulting signature + `nonce` forwarded to `requestAuthNative`.
 */
export async function issueAuthChallengeNative(publicKey: string): Promise<AuthChallenge> {
  return command<AuthChallenge>('issue_auth_challenge', { publicKey })
}

/**
 * Verify a wallet signature and return (or create) the stored user profile.
 *
 * `signature` is the raw 64-byte Ed25519 signature as a `Uint8Array` or plain
 * number array.  `nonce` must match the challenge issued by `issueAuthChallengeNative`.
 * Pass `username` + `cluster` to register a new profile on first sign-up;
 * omit them for re-authentication of an existing profile.
 */
export async function requestAuthNative(
  publicKey: string,
  signature: Uint8Array | number[],
  nonce: string,
  username?: string,
  cluster?: string,
): Promise<UserProfile> {
  return command<UserProfile>('request_auth', {
    publicKey,
    signature: Array.from(signature),
    nonce,
    username,
    cluster,
  })
}

/**
 * Look up a locally-stored user profile by Solana public key.
 * Returns `null` if the wallet has never been registered.
 */
export async function getUserProfileNative(publicKey: string): Promise<UserProfile | null> {
  return command<UserProfile | null>('get_user_profile', { publicKey })
}

/**
 * Create or overwrite the local profile for a wallet.
 * `cluster` must be `"devnet"` or `"mainnet-beta"`.
 */
export async function saveUserProfileNative(
  publicKey: string,
  username: string,
  cluster: string,
): Promise<UserProfile> {
  return command<UserProfile>('save_user_profile', { publicKey, username, cluster })
}

/**
 * Permanently delete the local profile for a wallet.
 * No-op if no profile exists.
 */
export async function deleteUserProfileNative(publicKey: string): Promise<void> {
  return command<void>('delete_user_profile', { publicKey })
}

/**
 * Return the profile of the remembered wallet session, or `null` when no
 * session is saved (or the remembered wallet's profile was deleted).
 * Called once on startup so returning users skip the connect flow.
 */
export async function getSavedSessionNative(): Promise<UserProfile | null> {
  return command<UserProfile | null>('get_saved_session')
}

/**
 * Remember `publicKey` as the active wallet session for auto-login.
 * The key must already have a registered profile.
 */
export async function saveWalletSessionNative(publicKey: string): Promise<void> {
  return command<void>('save_wallet_session', { publicKey })
}

/** Forget the remembered wallet session (sign out). */
export async function clearWalletSessionNative(): Promise<void> {
  return command<void>('clear_wallet_session')
}

/**
 * Launch a Chrome --app popup that loads the Phantom wallet extension.
 *
 * Because Phantom cannot inject `window.solana` into Tauri's WebView, this
 * opens a real Chrome window (or Brave) where the extension works normally.
 * The popup POSTs the public key back to a short-lived loopback HTTP server,
 * which re-emits it as a `phantom_pubkey` Tauri event.
 *
 * Listen for the result with `onPhantomPubkey`.
 */
export async function openPhantomPopupNative(): Promise<void> {
  return command<void>('open_phantom_popup')
}

/**
 * Subscribe to the `phantom_pubkey` event emitted by the Rust popup server
 * once the user connects Phantom in the Chrome popup window.
 *
 * Returns an unsubscribe function — call it on component unmount.
 */
export const onPhantomPubkey = (cb: (pubkey: string) => void): (() => void) =>
  onNativeEvent<string>('phantom_pubkey', cb)
