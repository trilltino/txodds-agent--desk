// Native event topics: the TypeScript mirror of src-tauri/src/event_bus.rs.
// Both files are constant tables kept in lockstep; adding a topic means adding
// it to both in the same change. React must subscribe through these constants
// rather than string literals so topic drift is caught in review.

export const NativeEvents = {
  txlineEvent: 'txline://event',
  ingestStatus: 'ingest://status',
  chainAccount: 'chain://account',
  chainTx: 'chain://tx',
  txoracleRoot: 'chain://txoracle-root',
  coralMessage: 'coral://message',
  coralSession: 'coral://session',
  agentTrace: 'agent://trace',
  agentSignal: 'agent://signal',
  agentEvaluation: 'agent://evaluation',
  web3ProofReceipt: 'web3://proof-receipt',
  validationStatus: 'web3://validation-status',
  payIntent: 'pay://intent',
  payStatus: 'pay://status',
  settlementReceipt: 'settle://receipt',
  marketRound: 'market://round',
  appNotification: 'app://notification',
  walletStatus: 'wallet://status'
} as const

export type NativeEventName = (typeof NativeEvents)[keyof typeof NativeEvents]
