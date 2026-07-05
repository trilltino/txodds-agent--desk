import type { AgentRun, Fixture, TxLineEvent } from '../../types'

export const mockFixtures: Fixture[] = [
  { fixtureId: 17588245, home: 'Brazil', away: 'England', competition: 'World Cup', status: 'live', startTime: new Date().toISOString() },
  { fixtureId: 17588246, home: 'France', away: 'Argentina', competition: 'World Cup', status: 'scheduled', startTime: new Date(Date.now() + 3600_000).toISOString() }
]

export const mockEvents: TxLineEvent[] = [
  {
    id: 'evt-odds-1',
    kind: 'odds_move',
    fixtureId: 17588245,
    title: 'Brazil price shortened 6.2pp',
    body: 'TxLINE odds moved after sustained pressure. Trigger threshold met for agent round.',
    ts: new Date().toISOString(),
    odds: [
      { fixtureId: 17588245, outcome: 'home', decimal: 1.82, impliedProbability: 0.549, ts: new Date().toISOString() },
      { fixtureId: 17588245, outcome: 'draw', decimal: 3.70, impliedProbability: 0.270, ts: new Date().toISOString() },
      { fixtureId: 17588245, outcome: 'away', decimal: 4.60, impliedProbability: 0.217, ts: new Date().toISOString() }
    ]
  },
  {
    id: 'evt-goal-1',
    kind: 'goal',
    fixtureId: 17588245,
    title: 'Goal: Brazil 1-0 England',
    body: 'Scores stream produced a goal event. Fan mode should explain match and market impact.',
    ts: new Date(Date.now() - 120_000).toISOString(),
    score: { home: 1, away: 0 }
  }
]

export function emptyRun(event: TxLineEvent, track: AgentRun['track']): AgentRun {
  return {
    runId: `${track}-${event.id}-${Date.now()}`,
    track,
    trigger: event,
    bids: [],
    timeline: [{ at: new Date().toISOString(), label: 'TRIGGER', detail: `${event.kind}: ${event.title}` }],
    settlement: { status: 'not_started' }
  }
}
