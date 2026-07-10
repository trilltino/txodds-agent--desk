import type { CoralAgentManifest } from '../../types'
import { listCoralAgentsNative, native } from '../../desktop/transport'

// Load agent metadata from Rust only so the app cannot look live when it is not
// running in Tauri.
export async function loadCoralAgents(): Promise<CoralAgentManifest[]> {
  if (!native) throw new Error('World Cup Agent Desk requires the Tauri desktop runtime')
  return listCoralAgentsNative()
}
