import type { AgentLeaderboardEntry, ArenaScore, SettlementRecord } from '../../../core/agent/types'

function pnlLabel(pnl: number) {
  if (pnl > 0) return `+${pnl.toFixed(2)} units`
  if (pnl < 0) return `${pnl.toFixed(2)} units`
  return '0 units'
}

function pnlClass(pnl: number) {
  if (pnl > 0) return 'pnlPositive'
  if (pnl < 0) return 'pnlNegative'
  return ''
}

// ArenaScorecard shows the live FollowSharp vs FadeSharp competition scoreboard,
// per-strategy leaderboard stats, total positions settled, and most recent
// settlement records. It replaces the static TrackScorecard for the agent track.
export function ArenaScorecard({
  score,
  recentSettlements,
  leaderboard,
}: {
  score?: ArenaScore
  recentSettlements: SettlementRecord[]
  leaderboard?: AgentLeaderboardEntry[]
}) {
  const followTotal = (score?.followWins ?? 0) + (score?.followLosses ?? 0)
  const fadeTotal = (score?.fadeWins ?? 0) + (score?.fadeLosses ?? 0)
  const totalSettled = followTotal + fadeTotal

  const followLb = leaderboard?.find((l) => l.strategy === 'FollowSharp')
  const fadeLb   = leaderboard?.find((l) => l.strategy === 'FadeSharp')

  return (
    <article className="card">
      <div className="cardHead">
        <h2>Arena Scoreboard</h2>
        <span className="pill">{totalSettled} settled</span>
        {score && <span className="pill">{(followTotal + fadeTotal)} positions</span>}
      </div>

      {!score ? (
        <p className="muted">Waiting for first settlement from arena-coordinator…</p>
      ) : (
        <>
          {/* Leader badge */}
          <div className={`leaderBadge leaderBadge--${score.leader === 'TIE' ? 'tie' : score.leader.startsWith('FOLLOW') ? 'follow' : 'fade'}`}>
            {score.leader}
          </div>

          {/* Side-by-side columns */}
          <div className="arenaScoreboard">
            <div className="arenaCol arenaCol--follow">
              <span className="arenaColLabel">FOLLOW (match-intelligence)</span>
              <span className="arenaWL">
                <span className="win">{score.followWins}W</span>
                <span className="loss">{score.followLosses}L</span>
              </span>
              <span className={`arenaPnl ${pnlClass(score.followPnl)}`}>
                {pnlLabel(score.followPnl)}
              </span>
              {followLb && (
                <div className="arenaLbHint">
                  <span className="muted arenaLbStat">
                    {Math.round(followLb.winRate * 100)}% win rate
                  </span>
                  <span className="muted arenaLbStat">
                    avg conf {Math.round(followLb.avgWinningConfidence * 100)}%
                  </span>
                </div>
              )}
            </div>
            <div className="arenaDivider" />
            <div className="arenaCol arenaCol--fade">
              <span className="arenaColLabel">FADE (contrarian)</span>
              <span className="arenaWL">
                <span className="win">{score.fadeWins}W</span>
                <span className="loss">{score.fadeLosses}L</span>
              </span>
              <span className={`arenaPnl ${pnlClass(score.fadePnl)}`}>
                {pnlLabel(score.fadePnl)}
              </span>
              {fadeLb && (
                <div className="arenaLbHint">
                  <span className="muted arenaLbStat">
                    {Math.round(fadeLb.winRate * 100)}% win rate
                  </span>
                  <span className="muted arenaLbStat">
                    avg conf {Math.round(fadeLb.avgWinningConfidence * 100)}%
                  </span>
                </div>
              )}
            </div>
          </div>

          {/* Recent settlements */}
          {recentSettlements.length > 0 && (
            <div className="recentSettlements">
              <p className="sectionLabel muted">Recent settlements</p>
              <ol className="settlementList">
                {recentSettlements.slice(0, 5).map((rec) => (
                  <li key={rec.idempotencyKey} className="settlementRow">
                    <span className="settlementFixture">#{rec.fixtureId}</span>
                    <span className="settlementMarket">{rec.marketKey} — {rec.selection}</span>
                    <span className={`settlementStrategy muted`}>{rec.strategy}</span>
                    <span className={`settlementResult ${rec.result === 'win' ? 'win' : 'loss'}`}>
                      {rec.result.toUpperCase()}
                    </span>
                    <span className={pnlClass(rec.pnlUnits)}>{pnlLabel(rec.pnlUnits)}</span>
                  </li>
                ))}
              </ol>
            </div>
          )}
        </>
      )}
    </article>
  )
}
