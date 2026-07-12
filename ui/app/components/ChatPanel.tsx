/**
 * ChatPanel
 *
 * The primary surface of the Agent Desk: a full-height conversation with the
 * intelligence agent. Streams from useAgentDesk arrive pre-merged as ChatItem
 * entries; this component owns only presentation concerns — welcome state,
 * auto-scroll, and the typing indicator while a round is in flight.
 */

import { useEffect, useRef } from 'react'
import type { Fixture } from '../../types'
import type { ChatItem } from '../../core/chat/types'
import { ChatMessage } from './ChatMessage'
import { ChatInput } from './ChatInput'

interface Props {
  items: ChatItem[]
  busy: boolean
  /** Latest trace summary while the agent works — shown next to the typing dots. */
  busyLabel?: string
  selectedFixture?: Fixture
  /** True when the fixture board is showing a past (completed-match) day. */
  historical?: boolean
  /** Whether the autonomous live-trigger loop is currently allowed to act. */
  autonomousEnabled: boolean
  onToggleAutonomous: (enabled: boolean) => void
  onSend: (text: string) => void
}

function WelcomeMessage() {
  return (
    <div className="chatRow agent">
      <span className="chatAvatar" aria-hidden="true">
        <span className="chatAvatarDot" />
      </span>
      <div className="chatRowBody">
        <div className="chatBubble agentBubble">
          <span className="chatSender">agent desk</span>
          Hi — I’m your World Cup intelligence agent. Pick a fixture on the right,
          then ask me things like <strong>“Analyze Norway vs England”</strong>,{' '}
          <strong>“What’s the sharp movement on France vs Spain?”</strong> or{' '}
          <strong>“What’s the current arena score?”</strong>. For past matches, use
          the <strong>◀ ▶</strong> arrows on the fixture board to browse earlier
          days, or add a time phrase like <strong>“as of yesterday 18:00”</strong>.
          For a completed match, try <strong>“Backtest {'{team}'} vs {'{team}'}”</strong> to
          replay its real odds history and see how Follow vs Fade would have scored.
          I’ll narrate signals, positions, and settlements here as they happen.
        </div>
      </div>
    </div>
  )
}

function TypingIndicator({ label }: { label?: string }) {
  return (
    <div className="chatRow agent">
      <span className="chatAvatar" aria-hidden="true">
        <span className="chatAvatarDot" />
      </span>
      <div className="chatRowBody">
        <div className="chatBubble agentBubble typing">
          <span className="typingDots" aria-label="Agent is thinking">
            <span />
            <span />
            <span />
          </span>
          {label && <span className="typingLabel">{label}</span>}
        </div>
      </div>
    </div>
  )
}

export function ChatPanel({
  items,
  busy,
  busyLabel,
  selectedFixture,
  historical,
  autonomousEnabled,
  onToggleAutonomous,
  onSend,
}: Props) {
  const scrollRef = useRef<HTMLDivElement>(null)

  // Follow the conversation: scroll to the newest message whenever the log
  // grows or the typing indicator toggles.
  useEffect(() => {
    const el = scrollRef.current
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
  }, [items.length, busy])

  return (
    <section className="chatPanel" aria-label="Chat with agent">
      <header className="chatHeader">
        <span className="chatAvatar large" aria-hidden="true">
          <span className="chatAvatarDot" />
        </span>
        <div className="chatHeaderCopy">
          <strong>Intelligence Agent</strong>
          <span className="chatHeaderStatus">
            {busy
              ? 'working…'
              : selectedFixture
              ? `watching ${selectedFixture.home} vs ${selectedFixture.away}`
              : 'waiting for a fixture'}
          </span>
        </div>
        <button
          type="button"
          className={`chatAutoToggle${autonomousEnabled ? ' on' : ''}`}
          onClick={() => onToggleAutonomous(!autonomousEnabled)}
          title={
            autonomousEnabled
              ? 'Autonomous loop is on — it can trigger rounds on its own when live odds move. Click to pause.'
              : 'Autonomous loop is paused — rounds only run when you ask. Click to let it act on its own.'
          }
        >
          <span className="chatAutoDot" aria-hidden="true" />
          Auto {autonomousEnabled ? 'on' : 'off'}
        </button>
      </header>

      <div className="chatScroll" ref={scrollRef}>
        <WelcomeMessage />
        {items.map((item) => (
          <ChatMessage key={item.id} item={item} />
        ))}
        {busy && <TypingIndicator label={busyLabel} />}
      </div>

      <ChatInput disabled={busy} selectedFixture={selectedFixture} historical={historical} onSend={onSend} />
    </section>
  )
}
