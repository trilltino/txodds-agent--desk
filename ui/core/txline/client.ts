import type { Fixture, TxLineEvent } from '../../types'

export interface TxLineConfig {
  apiOrigin: string
  jwt: string
  apiToken: string
  network: 'devnet' | 'mainnet'
}

function desktopOnly(): never {
  throw new Error('TxLINE live data is desktop-only; Rust owns credentials and ingestion')
}

export async function fetchFixtures(_cfg?: Partial<TxLineConfig>): Promise<Fixture[]> {
  desktopOnly()
}

export async function fetchScoresSnapshot(_fixtureId: number, _cfg?: Partial<TxLineConfig>) {
  desktopOnly()
}

export async function streamTxLineEvents(
  _cfg: TxLineConfig,
  _stream: 'odds' | 'scores',
  _onEvent: (event: TxLineEvent) => void,
  _signal?: AbortSignal
): Promise<void> {
  desktopOnly()
}
