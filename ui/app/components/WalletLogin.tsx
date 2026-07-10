/**
 * WalletLogin
 *
 * Full-screen gate shown before the user has authenticated.
 *
 * ## Render map
 *
 * | `stage`         | What renders                                             |
 * |-----------------|----------------------------------------------------------|
 * | `idle`          | "Connect Wallet" button                                  |
 * | `connecting`    | Spinner                                                  |
 * | `popup-waiting` | Spinner + "Waiting for Phantom popup…" + cancel link     |
 * | `manual-pubkey` | Paste public-key form                                    |
 * | `registering`   | Username + cluster form (inline, no modal)               |
 * | `error`         | Inline error message with retry button                   |
 *
 * The component is intentionally thin: it owns no async logic — all state
 * transitions live in `useWalletAuth`.
 */

import React, { useState } from 'react'
import { useWalletAuth } from '../hooks/useWalletAuth'

// ── sub-components ─────────────────────────────────────────────────────────────

function Spinner() {
  return (
    <div
      role="status"
      aria-label="Connecting wallet…"
      style={{
        width: 32,
        height: 32,
        border: '3px solid rgba(255,255,255,0.15)',
        borderTopColor: '#9945FF',
        borderRadius: '50%',
        animation: 'spin 0.7s linear infinite',
      }}
    />
  )
}

function RegistrationForm({
  onSubmit,
}: {
  onSubmit: (username: string, cluster: string) => void
}) {
  const [username, setUsername] = useState('')
  const [cluster, setCluster] = useState<'devnet' | 'mainnet-beta'>('devnet')

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (username.trim()) onSubmit(username.trim(), cluster)
  }

  return (
    <form
      onSubmit={handleSubmit}
      style={{ display: 'flex', flexDirection: 'column', gap: 12, width: 280 }}
    >
      <h2 style={{ margin: 0, fontSize: 16, fontWeight: 600, color: '#fff' }}>
        Create your profile
      </h2>
      <label style={{ fontSize: 13, color: '#aaa' }}>
        Display name
        <input
          autoFocus
          value={username}
          onChange={e => setUsername(e.target.value)}
          maxLength={32}
          placeholder="e.g. AlphaPunter"
          style={{
            display: 'block',
            marginTop: 4,
            width: '100%',
            padding: '6px 8px',
            background: '#1a1a2e',
            border: '1px solid #333',
            borderRadius: 6,
            color: '#fff',
            fontSize: 14,
            boxSizing: 'border-box',
          }}
        />
      </label>
      <label style={{ fontSize: 13, color: '#aaa' }}>
        Cluster
        <select
          value={cluster}
          onChange={e => setCluster(e.target.value as 'devnet' | 'mainnet-beta')}
          style={{
            display: 'block',
            marginTop: 4,
            width: '100%',
            padding: '6px 8px',
            background: '#1a1a2e',
            border: '1px solid #333',
            borderRadius: 6,
            color: '#fff',
            fontSize: 14,
          }}
        >
          <option value="devnet">Devnet</option>
          <option value="mainnet-beta">Mainnet-Beta</option>
        </select>
      </label>
      <button
        type="submit"
        disabled={!username.trim()}
        style={{
          padding: '8px 0',
          background: username.trim() ? '#9945FF' : '#333',
          border: 'none',
          borderRadius: 8,
          color: '#fff',
          fontSize: 14,
          fontWeight: 600,
          cursor: username.trim() ? 'pointer' : 'not-allowed',
          transition: 'background 0.2s',
        }}
      >
        Save & continue
      </button>
    </form>
  )
}

// ── WalletLogin ────────────────────────────────────────────────────────────────

/**
 * Drop this as the sole child of the top-level auth gate in `App.tsx`.
 * It calls `useWalletAuth` internally so the parent does not need to manage
 * auth state — pass `onAuthenticated` to receive the authenticated callback.
 */
function ManualPubkeyForm({ onSubmit, error }: { onSubmit: (pubkey: string) => void; error: string | null }) {
  const [pubkey, setPubkey] = React.useState('')

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (pubkey.trim()) onSubmit(pubkey.trim())
  }

  return (
    <form
      onSubmit={handleSubmit}
      style={{ display: 'flex', flexDirection: 'column', gap: 12, width: 320 }}
    >
      <h2 style={{ margin: 0, fontSize: 16, fontWeight: 600, color: '#fff' }}>
        Paste your wallet address
      </h2>
      <p style={{ margin: 0, fontSize: 12, color: '#aaa', lineHeight: 1.5 }}>
        Phantom has opened in your default browser. Copy your public key from
        there and paste it below.
      </p>
      <label style={{ fontSize: 13, color: '#aaa' }}>
        Solana public key
        <input
          autoFocus
          value={pubkey}
          onChange={e => setPubkey(e.target.value)}
          placeholder="e.g. 9xDUc…"
          spellCheck={false}
          style={{
            display: 'block',
            marginTop: 4,
            width: '100%',
            padding: '6px 8px',
            background: '#1a1a2e',
            border: '1px solid #333',
            borderRadius: 6,
            color: '#fff',
            fontSize: 13,
            fontFamily: 'monospace',
            boxSizing: 'border-box',
          }}
        />
      </label>
      {error && (
        <p style={{ margin: 0, color: '#ff6b6b', fontSize: 12 }}>{error}</p>
      )}
      <button
        type="submit"
        disabled={!pubkey.trim()}
        style={{
          padding: '8px 0',
          background: pubkey.trim() ? '#9945FF' : '#333',
          border: 'none',
          borderRadius: 8,
          color: '#fff',
          fontSize: 14,
          fontWeight: 600,
          cursor: pubkey.trim() ? 'pointer' : 'not-allowed',
          transition: 'background 0.2s',
        }}
      >
        Connect
      </button>
    </form>
  )
}

// ── WalletLogin ────────────────────────────────────────────────────────────────

/**
 * Drop this as the sole child of the top-level auth gate in `App.tsx`.
 * It calls `useWalletAuth` internally so the parent does not need to manage
 * auth state — pass `onAuthenticated` to receive the authenticated callback.
 */
export function WalletLogin({ onAuthenticated }: { onAuthenticated: () => void }) {
  const [{ stage, error }, { connectWallet, connectWithPubkey, register, fallbackToManualPubkey }] = useWalletAuth()

  // Propagate to parent when authentication completes.
  React.useEffect(() => {
    if (stage === 'authenticated') onAuthenticated()
  }, [stage, onAuthenticated])

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100vh',
        gap: 24,
        background: '#0d0d1a',
      }}
    >
      {/* Global keyframe for spinner — injected once at mount. */}
      <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>

      <img src="/favicon.ico" alt="TxOdds" width={48} height={48} />

      <h1 style={{ margin: 0, fontSize: 22, fontWeight: 700, color: '#fff' }}>
        TxOdds Agent Desk
      </h1>

      {stage === 'idle' && (
        <button
          onClick={connectWallet}
          style={{
            padding: '10px 28px',
            background: '#9945FF',
            border: 'none',
            borderRadius: 10,
            color: '#fff',
            fontSize: 15,
            fontWeight: 600,
            cursor: 'pointer',
          }}
        >
          Connect Wallet
        </button>
      )}

      {stage === 'connecting' && <Spinner />}

      {stage === 'popup-waiting' && <Spinner />}

      {stage === 'manual-pubkey' && (
        <ManualPubkeyForm onSubmit={connectWithPubkey} error={error} />
      )}

      {stage === 'registering' && (
        <RegistrationForm onSubmit={(username, cluster) => register(username, cluster)} />
      )}

      {stage === 'error' && (
        <div style={{ textAlign: 'center' }}>
          <p style={{ color: '#ff6b6b', fontSize: 13, marginBottom: 12 }}>
            {error ?? 'An unknown error occurred.'}
          </p>
          <button
            onClick={connectWallet}
            style={{
              padding: '8px 20px',
              background: '#333',
              border: 'none',
              borderRadius: 8,
              color: '#fff',
              fontSize: 13,
              cursor: 'pointer',
            }}
          >
            Retry
          </button>
        </div>
      )}
    </div>
  )
}
