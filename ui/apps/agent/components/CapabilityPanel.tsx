import type { AgentRosterEntry, CapabilityKind } from '../../../core/agent/types'

// ── Capability definitions (mirrors agent-core/src/capability.rs) ──────────────
//
// Each entry here corresponds to one Rust ZST (zero-sized type) capability
// token. The compiler enforces that only the agent holding the matching token
// can call the restricted tool — no runtime permission check is possible.

interface CapInfo {
  short: string
  full: string
  agentId: string
  description: string
  colorClass: string
}

const CAP_INFO: Record<CapabilityKind, CapInfo> = {
  FollowCap: {
    short: 'FOLLOW',
    full: 'FollowCap',
    agentId: 'match-intelligence',
    description:
      'Grants the ability to commit a follow-sharp-movement position. ' +
      'Held exclusively by match-intelligence-agent. ' +
      'A FadeCap cannot be widened to FollowCap — the mismatch is a compile error.',
    colorClass: 'capCard--follow',
  },
  FadeCap: {
    short: 'FADE',
    full: 'FadeCap',
    agentId: 'contrarian',
    description:
      'Grants the ability to commit a fade-sharp-movement (contrarian) position. ' +
      'Held exclusively by contrarian-agent. ' +
      'Running two separate OS processes prevents capability escalation.',
    colorClass: 'capCard--fade',
  },
  SettleCap: {
    short: 'SETTLE',
    full: 'SettleCap',
    agentId: 'arena-coordinator',
    description:
      'Grants the ability to read arena positions and write settlement records. ' +
      'Held exclusively by arena-coordinator. ' +
      'Neither match-intelligence nor contrarian hold this token.',
    colorClass: 'capCard--settle',
  },
  DetectCap: {
    short: 'DETECT',
    full: 'DetectCap',
    agentId: 'sharp-movement-detector',
    description:
      'Grants the ability to detect sharp odds movements and log signal records. ' +
      'Held exclusively by sharp-movement-detector. ' +
      'DetectCap is defined locally in the sidecar binary, not in agent-core.',
    colorClass: 'capCard--detect',
  },
}

const CAP_ORDER: CapabilityKind[] = ['FollowCap', 'FadeCap', 'SettleCap', 'DetectCap']

function statusColor(entry: AgentRosterEntry | undefined) {
  if (!entry) return '#475569'
  switch (entry.status) {
    case 'running': return '#22c55e'
    case 'error': return '#f97316'
    case 'stopped': return '#64748b'
    default: return '#475569'
  }
}

function statusLabel(entry: AgentRosterEntry | undefined) {
  if (!entry) return 'not registered'
  return entry.status
}

/**
 * CapabilityPanel visualises the four compile-time ZST capability tokens.
 * It shows which agent holds each token, that agent's current status, and
 * a description of what the capability permits — directly from capability.rs.
 */
export function CapabilityPanel({ roster }: { roster: AgentRosterEntry[] }) {
  return (
    <article className="card">
      <div className="cardHead">
        <h2>Capability Tokens</h2>
        <span className="pill">compile-time · ZST · sealed</span>
      </div>
      <p className="muted capPanelSubtitle">
        Each token is a zero-sized type enforced by the Rust compiler — no runtime
        permission check can be circumvented or injected by model output.
      </p>
      <div className="capTokenGrid">
        {CAP_ORDER.map((kind) => {
          const info = CAP_INFO[kind]
          const rosterEntry = roster.find((a) => a.id === info.agentId)
          const dotColor = statusColor(rosterEntry)
          const sl = statusLabel(rosterEntry)

          return (
            <div key={kind} className={`capCard ${info.colorClass}`}>
              <div className="capCardHead">
                <span className="capTokenBadge">{info.short}</span>
                <span className="capZSTBadge" title="Zero-sized type — no heap allocation">
                  ZST
                </span>
              </div>
              <div className="capCardFull">{info.full}</div>
              <div className="capCardAgent">
                <span
                  className="capAgentDot"
                  style={{ background: dotColor }}
                  title={`Status: ${sl}`}
                />
                <span className="capAgentId">{info.agentId}</span>
                <span className="muted capAgentStatus">{sl}</span>
              </div>
              <p className="capCardDesc">{info.description}</p>
              {rosterEntry?.leaderboard && (
                <div className="capLeaderboardHint">
                  <span className="muted">
                    {rosterEntry.leaderboard.positionsTaken} pos
                    &nbsp;·&nbsp;
                    {Math.round(rosterEntry.leaderboard.winRate * 100)}% win rate
                    &nbsp;·&nbsp;
                    {rosterEntry.leaderboard.totalPnlPoints >= 0 ? '+' : ''}
                    {rosterEntry.leaderboard.totalPnlPoints.toFixed(1)} pts
                  </span>
                </div>
              )}
            </div>
          )
        })}
      </div>
    </article>
  )
}
