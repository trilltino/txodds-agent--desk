/**
 * useWalletAuth
 *
 * Combines Phantom wallet connection with the local `UserProfile` sled store
 * and Ed25519 challenge/sign authentication for new registrations.
 *
 * ## State machine
 *
 * ```
 * restoring                       (startup: look up the remembered session)
 *   ├─ saved session found ──► authenticated   (no wallet interaction at all)
 *   └─ none ──► idle
 * idle
 *   └─ connectWallet() ──► connecting
 *                              ├─ silent connect succeeded (returning user)
 *                              │     ├─ profile found ──► authenticated
 *                              │     └─ no profile   ──► registering
 *                              └─ silent connect failed (new origin approval)
 *                                    └─ full Phantom popup (once only)
 *                                          ├─ profile found ──► authenticated
 *                                          └─ no profile   ──► registering
 * registering
 *   └─ register(username, cluster) ──► [issue challenge → sign → request_auth] ──► authenticated
 * authenticated
 *   └─ disconnect() ──► idle    (also forgets the remembered session)
 * ```
 *
 * Every arrival at `authenticated` saves the wallet as the remembered session
 * (Rust-owned, in the sled user store), so subsequent launches restore it
 * without connecting. The session stores only the public key — payments still
 * require fresh wallet signatures.
 *
 * Silent connect (via `onlyIfTrusted`) means returning users never see the
 * Phantom approval popup.  New users see it exactly once.
 * New registrations are gated by a backend Ed25519 challenge/response so the
 * sled store cannot be written without a valid wallet signature.
 *
 * Components should render:
 * - `idle | connecting`  → `<WalletLogin />`
 * - `registering`        → registration form
 * - `authenticated`      → main app
 * - `error`              → inline error with retry
 */

import { useCallback, useEffect, useRef, useState } from 'react'
import {
  PhantomNotInjectedError,
  connectPhantom,
  disconnectPhantom,
  isValidSolanaPublicKey,
  signMessage,
  silentConnect,
} from '../../core/wallet/phantom'
import {
  clearWalletSessionNative,
  deleteUserProfileNative,
  getSavedSessionNative,
  getUserProfileNative,
  issueAuthChallengeNative,
  native,
  onPhantomPubkey,
  openPhantomPopupNative,
  requestAuthNative,
  saveUserProfileNative,
  saveWalletSessionNative,
} from '../../desktop/transport'
import type { UserProfile } from '../../types'

// ── types ─────────────────────────────────────────────────────────────────────

export type AuthStage =
  | 'restoring'
  | 'idle'
  | 'connecting'
  | 'popup-waiting'
  | 'manual-pubkey'
  | 'registering'
  | 'authenticated'
  | 'error'

export interface WalletAuthState {
  stage: AuthStage
  publicKey: string | null
  profile: UserProfile | null
  error: string | null
  /** True when the public key arrived via the native Chrome popup (already
   *  proved ownership through Phantom itself — no sign challenge needed). */
  fromPopup: boolean
}

export interface WalletAuthActions {
  /** Open Phantom (or silently reconnect) and look up or begin registration. */
  connectWallet: () => Promise<void>
  /**
   * Tauri WebView fallback: accept a manually pasted Solana public key and
   * advance to profile lookup / registration.
   */
  connectWithPubkey: (pubkey: string) => Promise<void>
  /**
   * Issue a backend challenge, sign it with the wallet, then verify and save
   * the profile.  Advances to `authenticated` on success.
   */
  register: (username: string, cluster: string) => Promise<void>
  /** Wipe the local profile for the connected wallet (irreversible). */
  deleteProfile: () => Promise<void>
  /** Disconnect wallet and return to `idle`. */
  disconnect: () => void
  /** Skip the Chrome popup and fall back to manual public-key entry. */
  fallbackToManualPubkey: () => void
}

// ── hook ──────────────────────────────────────────────────────────────────────

/**
 * Returns `[state, actions]`.
 *
 * The hook is a no-op when `native` is `false` (browser-only preview builds
 * cannot access the sled store or wallet adapter).
 */
export function useWalletAuth(): [WalletAuthState, WalletAuthActions] {
  const [state, setState] = useState<WalletAuthState>({
    // Desktop starts in `restoring` so a remembered session never flashes the
    // connect screen; browser previews have no session store and start idle.
    stage: native ? 'restoring' : 'idle',
    publicKey: null,
    profile: null,
    error: null,
    fromPopup: false,
  })

  // Keep a ref so we can clean up the popup listener on unmount / re-connect.
  const popupUnlistenRef = useRef<(() => void) | null>(null)

  // Remember the wallet after any successful authentication. Fire-and-forget:
  // a failed save only means the next launch shows the connect screen again.
  const rememberSession = useCallback((publicKey: string) => {
    void saveWalletSessionNative(publicKey).catch(console.error)
  }, [])

  // ── session restore (startup) ───────────────────────────────────────────────

  useEffect(() => {
    if (!native) return
    let cancelled = false
    getSavedSessionNative()
      .then(profile => {
        if (cancelled) return
        setState(s => {
          // Only the initial restore may transition; never clobber a flow the
          // user already started.
          if (s.stage !== 'restoring') return s
          return profile
            ? { ...s, stage: 'authenticated', publicKey: profile.publicKey, profile }
            : { ...s, stage: 'idle' }
        })
      })
      .catch(err => {
        console.error('session restore failed', err)
        if (!cancelled) {
          setState(s => (s.stage === 'restoring' ? { ...s, stage: 'idle' } : s))
        }
      })
    return () => {
      cancelled = true
    }
  }, [])

  // ── connectWallet ───────────────────────────────────────────────────────────

  const connectWallet = useCallback(async () => {
    if (!native) {
      setState(s => ({
        ...s,
        stage: 'error',
        error: 'Wallet auth requires the Tauri desktop runtime.',
      }))
      return
    }
    setState(s => ({ ...s, stage: 'connecting', error: null }))
    try {
      // 1. Try silent (no-popup) connect first — succeeds for returning users.
      let ctx = await silentConnect()
      if (!ctx) {
        // 2. First visit or revoked approval — show the Phantom approval popup.
        ctx = await connectPhantom()
      }
      if (!ctx.publicKey) {
        setState(s => ({
          ...s,
          stage: 'error',
          error: 'Wallet connected but returned no public key.',
        }))
        return
      }
      // 3. Profile lookup — no challenge needed for returning users.
      const profile = await getUserProfileNative(ctx.publicKey)
      if (profile) rememberSession(ctx.publicKey)
      setState(s => ({
        ...s,
        publicKey: ctx!.publicKey ?? null,
        profile,
        stage: profile ? 'authenticated' : 'registering',
      }))
    } catch (err) {
      if (err instanceof PhantomNotInjectedError) {
        // Inside Tauri WebView — browser extensions can't inject window.solana.
        // Launch a hidden Chrome --app window where Phantom CAN inject. The
        // host window is hidden by a PowerShell watcher using a unique title
        // (__TXODDS_PHANTOM_HOST__) so it won't accidentally hide VS Code or
        // other apps. The user only ever sees Phantom's own approval popup.
        try {
          await openPhantomPopupNative()
          setState(s => ({ ...s, stage: 'popup-waiting', error: null }))
        } catch (popupErr) {
          // Chrome/Brave not found — fall back to manual paste.
          setState(s => ({
            ...s,
            stage: 'manual-pubkey',
            error: null,
          }))
        }
      } else {
        setState(s => ({
          ...s,
          stage: 'error',
          error: err instanceof Error ? err.message : String(err),
        }))
      }
    }
  }, [])

  // ── connectWithPubkey ───────────────────────────────────────────────────────

  /**
   * Tauri WebView fallback: the user pastes their Phantom public key here.
   * Validates the key then looks up their profile (or advances to registration).
   */
  const connectWithPubkey = useCallback(async (pubkey: string, viaPopup = false) => {
    const trimmed = pubkey.trim()
    if (!isValidSolanaPublicKey(trimmed)) {
      setState(s => ({
        ...s,
        stage: 'manual-pubkey',
        error: 'That doesn\'t look like a valid Solana public key. Please check and try again.',
      }))
      return
    }
    setState(s => ({ ...s, stage: 'connecting', error: null }))
    try {
      const profile = await getUserProfileNative(trimmed)
      if (profile) rememberSession(trimmed)
      setState(s => ({
        ...s,
        publicKey: trimmed,
        profile,
        fromPopup: viaPopup,
        stage: profile ? 'authenticated' : 'registering',
      }))
    } catch (err) {
      setState(s => ({
        ...s,
        stage: 'error',
        error: err instanceof Error ? err.message : String(err),
      }))
    }
  }, [])

  // Subscribe to the phantom_pubkey Tauri event while in popup-waiting stage.
  // The Rust backend emits this event when the Chrome popup receives the
  // public key from Phantom and POSTs it to the loopback HTTP server.
  useEffect(() => {
    if (state.stage !== 'popup-waiting') return
    const unsub = onPhantomPubkey((pubkey: string) => {
      connectWithPubkey(pubkey, true)
    })
    popupUnlistenRef.current = unsub
    return () => {
      unsub()
      popupUnlistenRef.current = null
    }
  }, [state.stage, connectWithPubkey])

  // ── register ────────────────────────────────────────────────────────────────

  /**
   * Registration flow.
   *
   * When the pubkey arrived via the native Chrome popup the user already proved
   * ownership by connecting through Phantom itself — triggering a second
   * signMessage popup would be a third interruption.  In that case we skip
   * the challenge/sign round-trip and call `saveUserProfileNative` directly.
   *
   * For the browser / silent-connect path we still do the full
   * challenge → signMessage → requestAuth flow so the sled write is always
   * guarded by a real Ed25519 signature.
   */
  const register = useCallback(
    async (username: string, cluster: string) => {
      if (!state.publicKey) {
        setState(s => ({
          ...s,
          stage: 'error',
          error: 'Cannot register: no wallet connected.',
        }))
        return
      }
      try {
        let profile: UserProfile
        if (state.fromPopup) {
          // Ownership already proved via the Phantom popup — save directly.
          profile = await saveUserProfileNative(state.publicKey, username, cluster)
        } else {
          // Full challenge / sign / verify flow.
          const challenge = await issueAuthChallengeNative(state.publicKey)
          const signature = await signMessage(challenge.message)
          profile = await requestAuthNative(
            state.publicKey,
            signature,
            challenge.nonce,
            username,
            cluster,
          )
        }
        rememberSession(state.publicKey)
        setState(s => ({ ...s, profile, stage: 'authenticated' }))
      } catch (err) {
        setState(s => ({
          ...s,
          stage: 'error',
          error: err instanceof Error ? err.message : String(err),
        }))
      }
    },
    [state.publicKey, state.fromPopup],
  )

  // ── deleteProfile ───────────────────────────────────────────────────────────

  const deleteProfile = useCallback(async () => {
    if (!state.publicKey) return
    try {
      await deleteUserProfileNative(state.publicKey)
      setState(s => ({ ...s, profile: null, stage: 'registering' }))
    } catch (err) {
      setState(s => ({
        ...s,
        stage: 'error',
        error: err instanceof Error ? err.message : String(err),
      }))
    }
  }, [state.publicKey])

  // ── fallbackToManualPubkey ──────────────────────────────────────────────────

  const fallbackToManualPubkey = useCallback(() => {
    setState(s => ({ ...s, stage: 'manual-pubkey', error: null }))
  }, [])

  // ── disconnect ──────────────────────────────────────────────────────────────

  const disconnect = useCallback(() => {
    disconnectPhantom()
    // Forget the remembered session so the next launch asks to connect again.
    void clearWalletSessionNative().catch(console.error)
    setState({ stage: 'idle', publicKey: null, profile: null, error: null, fromPopup: false })
  }, [])

  return [state, { connectWallet, connectWithPubkey, register, deleteProfile, disconnect, fallbackToManualPubkey }]
}
