import { mockEvents, mockFixtures } from './mock'
import type { Fixture, TxLineEvent } from '../../types'

export interface TxLineConfig {
  apiOrigin: string
  jwt: string
  apiToken: string
  network: 'devnet' | 'mainnet'
}

function headers(cfg: TxLineConfig): HeadersInit {
  return {
    Authorization: `Bearer ${cfg.jwt}`,
    'X-Api-Token': cfg.apiToken,
    'Content-Type': 'application/json'
  }
}

export async function fetchFixtures(cfg?: Partial<TxLineConfig>): Promise<Fixture[]> {
  if (!cfg?.jwt || !cfg?.apiToken || !cfg?.apiOrigin) return mockFixtures
  const res = await fetch(`${cfg.apiOrigin}/api/scores/schedule`, { headers: headers(cfg as TxLineConfig) })
  if (!res.ok) throw new Error(`TxLINE fixtures failed: ${res.status}`)
  return res.json()
}

export async function fetchScoresSnapshot(fixtureId: number, cfg?: Partial<TxLineConfig>) {
  if (!cfg?.jwt || !cfg?.apiToken || !cfg?.apiOrigin) return mockEvents.find((e) => e.fixtureId === fixtureId)?.score ?? null
  const res = await fetch(`${cfg.apiOrigin}/api/scores/snapshot/${fixtureId}?asOf=${Date.now()}`, { headers: headers(cfg as TxLineConfig) })
  if (!res.ok) throw new Error(`TxLINE scores snapshot failed: ${res.status}`)
  return res.json()
}

export async function streamTxLineEvents(
  cfg: TxLineConfig,
  stream: 'odds' | 'scores',
  onEvent: (event: TxLineEvent) => void,
  signal?: AbortSignal
): Promise<void> {
  const streamUrl = `${cfg.apiOrigin}/api/${stream}/stream`
  const res = await fetch(streamUrl, {
    headers: { ...headers(cfg), Accept: 'text/event-stream', 'Cache-Control': 'no-cache' },
    signal
  })
  if (!res.ok || !res.body) throw new Error(`TxLINE ${stream} stream failed: ${res.status}`)

  const reader = res.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''
  while (true) {
    const { done, value } = await reader.read()
    if (done) break
    buffer += decoder.decode(value, { stream: true })
    const blocks = buffer.split(/\r?\n\r?\n/)
    buffer = blocks.pop() ?? ''
    for (const block of blocks) {
      const dataLine = block.split(/\r?\n/).find((line) => line.startsWith('data:'))
      if (!dataLine) continue
      const data = JSON.parse(dataLine.replace(/^data:\s?/, ''))
      onEvent({
        id: `${stream}-${Date.now()}`,
        kind: stream === 'odds' ? 'odds_update' : 'score_update',
        fixtureId: Number(data.fixtureId ?? data.id ?? 0),
        title: `${stream} update`,
        body: 'Live TxLINE SSE event received',
        ts: new Date().toISOString(),
        raw: data
      })
    }
  }
}
