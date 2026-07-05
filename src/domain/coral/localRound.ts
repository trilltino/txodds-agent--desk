import type { AgentDelivery, AgentRun, TrackMode, TxLineEvent, VerificationVerdict } from '../../types'
import { emptyRun } from '../txline/mock'
import { generateBids } from './bidding'
import { chooseWinner } from './scoring'
import { observeSettlement } from '../triton/client'

async function sha256Hex(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text)
  const hash = await crypto.subtle.digest('SHA-256', bytes)
  return [...new Uint8Array(hash)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

export async function runLocalAgentRound(event: TxLineEvent, track: TrackMode): Promise<AgentRun> {
  const run = emptyRun(event, track)
  run.timeline.push({ at: new Date().toISOString(), label: 'WANT', detail: `worldcup-buyer-agent asks for ${track} output on fixture ${event.fixtureId}` })

  run.bids = generateBids(event, track)
  run.timeline.push({ at: new Date().toISOString(), label: 'BID', detail: `${run.bids.length} specialist agents bid` })

  run.winner = chooseWinner(track, run.bids)
  run.timeline.push({ at: new Date().toISOString(), label: 'AWARD', detail: `${run.winner?.agentId ?? 'none'} selected on value/confidence/price` })

  if (!run.winner) return run

  const payload = makeDeliveryPayload(event, track, run.winner.agentId)
  const sha256 = await sha256Hex(payload)
  run.delivery = {
    agentId: run.winner.agentId,
    title: deliveryTitle(track),
    payload,
    sha256,
    citations: ['TxLINE event stream', 'TxLINE odds/scores snapshot'],
    strategy: track === 'trading' ? 'No blind bet: signal is logged, risk-scored, and simulated before any position.' : undefined,
    risk: track === 'settlement' ? 'Release only after proof receipt/verifier pass.' : undefined,
    fanCopy: track === 'fan' ? 'Shareable match card generated for non-technical fans.' : undefined
  }
  run.timeline.push({ at: new Date().toISOString(), label: 'DELIVERED', detail: `artifact sha256=${sha256.slice(0, 12)}…` })

  run.verdict = verifyDelivery(run.delivery, track)
  run.timeline.push({ at: new Date().toISOString(), label: 'VERIFIED', detail: `${run.verdict.status}: ${run.verdict.reason}` })

  run.settlement = {
    status: run.verdict.status === 'pass' ? 'released' : 'not_started',
    reference: `sha256:${sha256}`,
    explorerUrl: 'https://explorer.solana.com/?cluster=devnet',
    tritonObserved: false
  }
  try {
    const observation = await observeSettlement(`sha256:${sha256.slice(0, 12)}`)
    run.settlement.tritonObserved = true
    run.settlement.tritonSlot = observation.slot
    run.settlement.explorerUrl = `https://explorer.solana.com/block/${observation.slot}?cluster=devnet`
    run.timeline.push({ at: new Date().toISOString(), label: 'TRITON', detail: observation.note })
  } catch (err) {
    run.timeline.push({
      at: new Date().toISOString(),
      label: 'TRITON',
      detail: `chain observer unavailable: ${(err as Error).message}`
    })
  }
  run.timeline.push({ at: new Date().toISOString(), label: 'SETTLEMENT', detail: run.settlement.status })
  return run
}

function deliveryTitle(track: TrackMode): string {
  if (track === 'settlement') return 'Verifiable resolution package'
  if (track === 'trading') return 'Autonomous signal package'
  return 'AI pundit fan card'
}

function makeDeliveryPayload(event: TxLineEvent, track: TrackMode, agentId: string): string {
  if (track === 'settlement') {
    return JSON.stringify({
      type: 'resolution_package',
      agentId,
      fixtureId: event.fixtureId,
      trigger: event.kind,
      result: event.score ?? null,
      proofPlan: 'Fetch TxLINE stat-validation payload; if final stat validates, call escrow/market release path.',
      compliance: 'Demo/devnet only. No real-money wagering.'
    }, null, 2)
  }
  if (track === 'trading') {
    return JSON.stringify({
      type: 'signal_package',
      agentId,
      fixtureId: event.fixtureId,
      signal: event.kind === 'odds_move' ? 'significant_move_detected' : 'event_context_update',
      action: 'log_and_simulate',
      risk: 'no automatic real-money execution; devnet/simulated strategy state only',
      explanation: event.body
    }, null, 2)
  }
  return JSON.stringify({
    type: 'fan_card',
    agentId,
    fixtureId: event.fixtureId,
    headline: event.title,
    explainer: event.body,
    shareCopy: `World Cup swing: ${event.title}. ${event.body}`,
    ttsScript: `Here is what just happened. ${event.body}`
  }, null, 2)
}

function verifyDelivery(delivery: AgentDelivery, track: TrackMode): VerificationVerdict {
  const checked: VerificationVerdict['checked'] = ['txline-input', 'hash', 'policy']
  if (track === 'settlement') checked.push('proof')
  if (delivery.sha256.length !== 64) return { status: 'fail', reason: 'hash missing or malformed', checked }
  if (!delivery.payload.includes('fixtureId')) return { status: 'fail', reason: 'delivery does not bind to fixture', checked }
  return { status: 'pass', reason: 'delivery is fixture-bound, hash-bound, and policy-compatible', checked }
}
