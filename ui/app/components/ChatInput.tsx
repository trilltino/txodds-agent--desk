/**
 * ChatInput
 *
 * Bottom input bar of the chat surface. Free-text messages route through
 * useAgentDesk.sendChat, which translates natural language into fixture
 * selection + Coral rounds. Quick-action chips pre-fill the common asks.
 */

import { useState } from 'react'
import type { Fixture } from '../../types'

interface Props {
  disabled: boolean
  selectedFixture?: Fixture
  /** True when the fixture board is showing a past (completed-match) day. */
  historical?: boolean
  onSend: (text: string) => void
}

export function ChatInput({ disabled, selectedFixture, historical, onSend }: Props) {
  const [draft, setDraft] = useState('')

  function submit(text: string) {
    const trimmed = text.trim()
    if (!trimmed || disabled) return
    onSend(trimmed)
    setDraft('')
  }

  const analyzeText = selectedFixture
    ? `Analyze ${selectedFixture.home} vs ${selectedFixture.away}`
    : 'Analyze'

  const quickActions = [
    { label: '🔍 Analyze', text: analyzeText, needsFixture: true, needsHistorical: false },
    { label: '⚖️ Verify on-chain', text: selectedFixture ? `Run a settlement round on ${selectedFixture.home} vs ${selectedFixture.away}` : '', needsFixture: true, needsHistorical: false },
    { label: '📊 Score', text: "What's the current arena score?", needsFixture: false, needsHistorical: false },
    { label: '🕰 Backtest', text: selectedFixture ? `Backtest ${selectedFixture.home} vs ${selectedFixture.away}` : '', needsFixture: true, needsHistorical: true },
  ]

  return (
    <div className="chatInputBar">
      <div className="chatQuickActions">
        {quickActions.map((action) => (
          <button
            key={action.label}
            type="button"
            className="chatChip"
            disabled={disabled || (action.needsFixture && !selectedFixture) || (action.needsHistorical && !historical)}
            onClick={() => submit(action.text)}
          >
            {action.label}
          </button>
        ))}
      </div>
      <form
        className="chatInputRow"
        onSubmit={(e) => {
          e.preventDefault()
          submit(draft)
        }}
      >
        <input
          type="text"
          className="chatTextInput"
          placeholder={
            selectedFixture
              ? `Ask about ${selectedFixture.home} vs ${selectedFixture.away}…`
              : 'Ask the agent — e.g. "Analyze Norway vs England"'
          }
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          disabled={disabled}
          aria-label="Message the agent"
        />
        <button type="submit" className="chatSendBtn" disabled={disabled || !draft.trim()}>
          {disabled ? '…' : 'Send'}
        </button>
      </form>
    </div>
  )
}
