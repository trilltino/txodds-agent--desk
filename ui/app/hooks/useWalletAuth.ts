/**
 * useWalletAuth
 *
 * Combines Phantom wallet connection with the local `UserProfile` sled store
 * and Ed25519 challenge/sign authentication for new registrations.
 *
 * ## State machine
 *
 * ```
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
 *   └─ disconnect() ──► idle
 * ```
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

import { useCallback, useEffect, useState } from 'react'
import {
  PhantomNotInjectedError,
  connectPhantom,
  disconnectPhantom,
  isValidSolanaPublicKey,
  openPhantomSite,
  signMessage,
  silentConnect,
} from '../../core/wallet/phantom'
import {
  deleteUserProfileNative,
  getUserProfileNative,
  issueAuthChallengeNative,
  native,
  onPhantomPubkey,
  openPhantomPopupNative,
  requestAuthNative,
  saveUserProfileNative,
} from '../../desktop/transport'
import type { UserProfile } from '../../types'

// ── types ─────────────────────────────────────────────────────────────────────

export type AuthStage =
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
    stage: 'idle',
    publicKey: null,
    profile: null,
    error: null,
    fromPopup: false,
  })

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
      setState(s => ({
        ...s,
        publicKey: ctx!.publicKey ?? null,
        profile,
        stage: profile ? 'authenticated' : 'registering',
      }))
    } catch (err) {
      if (err instanceof PhantomNotInjectedError) {
        // Inside Tauri WebView — extensions can't inject window.solana.
        // Try the Chrome --app popup first so Phantom can inject normally.
        // If Chrome is not found, fall back to the manual pubkey entry flow.
        try {
          await openPhantomPopupNative()
          setState(s => ({ ...s, stage: 'popup-waiting', error: null }))
        } catch {
          openPhantomSite()
          setState(s => ({ ...s, stage: 'manual-pubkey', error: null }))
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
    setState({ stage: 'idle', publicKey: null, profile: null, error: null, fromPopup: false })
  }, [])

  // ── phantom popup listener ───────────────────────────────────────────────────

  // When the Chrome --app popup completes, the Rust server emits `phantom_pubkey`.
  // Subscribe only while in the `popup-waiting` stage; clean up on stage change.
  useEffect(() => {
    if (state.stage !== 'popup-waiting') return
    return onPhantomPubkey((pubkey) => {
      connectWithPubkey(pubkey, true)
    })
  }, [state.stage, connectWithPubkey])

  return [state, { connectWallet, connectWithPubkey, register, deleteProfile, disconnect, fallbackToManualPubkey }]
}
