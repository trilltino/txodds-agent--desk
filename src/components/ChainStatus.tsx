import { useEffect, useState } from 'react'
import type { Cluster, ChainStatus } from '../domain/triton/client'
import { watchSlots } from '../domain/triton/client'

type Entry = { status?: ChainStatus; error?: string }

const CLUSTERS: Cluster[] = ['devnet', 'mainnet']

export function ChainStatusStrip() {
  const [entries, setEntries] = useState<Record<Cluster, Entry>>({ devnet: {}, mainnet: {} })

  useEffect(() => {
    const stops = CLUSTERS.map((cluster) =>
      watchSlots(
        cluster,
        (status) => setEntries((prev) => ({ ...prev, [cluster]: { status } })),
        (err) => setEntries((prev) => ({ ...prev, [cluster]: { ...prev[cluster], error: err.message } }))
      )
    )
    return () => stops.forEach((stop) => stop())
  }, [])

  return (
    <div className="chainStrip">
      {CLUSTERS.map((cluster) => {
        const { status, error } = entries[cluster]
        const state = error ? 'down' : status ? 'live' : 'connecting'
        return (
          <span key={cluster} className={`chainPill ${state}`} title={status ? `solana-core ${status.solanaCore}` : error}>
            <i className="dot" />
            Triton {cluster}
            {status && !error
              ? ` · slot ${status.slot.toLocaleString()} · ${status.latencyMs}ms`
              : error
                ? ' · unreachable'
                : ' · connecting…'}
          </span>
        )
      })}
    </div>
  )
}
