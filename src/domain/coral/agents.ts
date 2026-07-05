import type { CoralAgentManifest } from '../../types'
import { listCoralAgentsNative, native } from '../../desktop/transport'

export const fallbackCoralAgents: CoralAgentManifest[] = [
  {
    id: 'worldcup-buyer-agent',
    displayName: 'World Cup Buyer',
    coralRole: 'buyer',
    service: 'txline',
    manifestPath: 'coral-agents/worldcup-buyer-agent/coral-agent.toml',
    description: 'Turns TxLINE triggers into WANTs, collects bids, awards the best seller, and starts policy-gated settlement.'
  },
  {
    id: 'seller-worldcup-edge',
    displayName: 'World Cup Edge Seller',
    coralRole: 'seller',
    service: 'txline.edge',
    manifestPath: 'coral-agents/seller-worldcup-edge/coral-agent.toml',
    description: 'Bids on odds movement WANTs and delivers a fixture-bound fair-line read.'
  },
  {
    id: 'seller-risk-policy',
    displayName: 'Risk Policy Seller',
    coralRole: 'seller',
    service: 'risk.policy',
    manifestPath: 'coral-agents/seller-risk-policy/coral-agent.toml',
    description: 'Prices downside, caps exposure, and outputs no-action/observe/simulate decisions.'
  },
  {
    id: 'seller-fan-card',
    displayName: 'Fan Card Seller',
    coralRole: 'seller',
    service: 'fan.card',
    manifestPath: 'coral-agents/seller-fan-card/coral-agent.toml',
    description: 'Converts match events into shareable fan-facing explanations.'
  },
  {
    id: 'verifier-agent',
    displayName: 'Verifier',
    coralRole: 'verifier',
    service: 'delivery.verify',
    manifestPath: 'coral-agents/verifier-agent/coral-agent.toml',
    description: 'Checks delivery hash, fixture binding, proof structure, and policy gates before release.'
  },
  {
    id: 'settlement-arbiter-agent',
    displayName: 'Settlement Arbiter',
    coralRole: 'settlement',
    service: 'settlement.release',
    manifestPath: 'coral-agents/settlement-arbiter-agent/coral-agent.toml',
    description: 'Bridges a verified run to the CoralOS settlement sidecar and devnet escrow observation.'
  }
]

export async function loadCoralAgents(): Promise<CoralAgentManifest[]> {
  if (!native) return fallbackCoralAgents
  return listCoralAgentsNative().catch(() => fallbackCoralAgents)
}
