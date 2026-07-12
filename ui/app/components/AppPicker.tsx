/**
 * AppPicker
 *
 * Shown once the wallet is connected, before any product surface mounts.
 * Three slots reserved for the app's product tracks; only the Intelligence
 * Agent is implemented today, so it is the sole enabled button (rightmost).
 * Selecting it fades this screen out; the caller mounts the real interface
 * once the fade finishes.
 *
 * Reachable again from inside the chat via its "← Apps" back button
 * (see Shell.tsx), which returns here without disconnecting the wallet.
 */

import { useState } from 'react'

const FADE_MS = 260

function LightningGlyph() {
  return (
    <svg width="30" height="30" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M13 2 4 14h7l-2 8 11-13h-7l0-7z" fill="currentColor" />
    </svg>
  )
}

function SoonGlyph() {
  return (
    <svg width="26" height="26" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle cx="12" cy="12" r="8" stroke="currentColor" strokeWidth="1.6" />
    </svg>
  )
}

const SLOTS = [
  { key: 'pulse', label: 'Pulse Rooms', hint: 'Coming soon', enabled: false },
  { key: 'markets', label: 'Verified Markets', hint: 'Coming soon', enabled: false },
  { key: 'agent', label: 'Intelligence Agent', hint: 'Launch', enabled: true },
] as const

export function AppPicker({ onLaunch }: { onLaunch: () => void }) {
  const [closing, setClosing] = useState(false)

  function handlePick(enabled: boolean) {
    if (!enabled || closing) return
    setClosing(true)
    setTimeout(onLaunch, FADE_MS)
  }

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100vh',
        gap: 40,
        background: '#0d0d1a',
        opacity: closing ? 0 : 1,
        transition: `opacity ${FADE_MS}ms ease`,
      }}
    >
      <div style={{ textAlign: 'center' }}>
        <p
          style={{
            margin: '0 0 6px',
            color: '#8b8ba7',
            fontSize: 11,
            fontWeight: 800,
            textTransform: 'uppercase',
            letterSpacing: 1,
          }}
        >
          Choose an app
        </p>
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 700, color: '#fff' }}>
          TxOdds Agent Desk
        </h1>
      </div>

      <div style={{ display: 'flex', gap: 20 }}>
        {SLOTS.map((slot) => (
          <button
            key={slot.key}
            type="button"
            onClick={() => handlePick(slot.enabled)}
            disabled={!slot.enabled}
            aria-label={slot.label}
            style={{
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              gap: 10,
              width: 148,
              height: 148,
              background: slot.enabled ? '#171730' : '#141425',
              border: slot.enabled ? '1px solid #9945FF' : '1px solid #2a2a40',
              borderRadius: 16,
              color: slot.enabled ? '#fff' : '#5c5c73',
              boxShadow: 'none',
            }}
          >
            {slot.key === 'agent' ? <LightningGlyph /> : <SoonGlyph />}
            <span style={{ fontSize: 13, fontWeight: 700 }}>{slot.label}</span>
            <span style={{ fontSize: 11, color: slot.enabled ? '#c9a6ff' : '#4b4b60' }}>
              {slot.hint}
            </span>
          </button>
        ))}
      </div>
    </div>
  )
}
