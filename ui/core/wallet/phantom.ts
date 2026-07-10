import type { WalletContext } from '../../types'

type PhantomProvider = {
  isPhantom?: boolean
  publicKey?: { toString(): string }
  connect(opts?: { onlyIfTrusted?: boolean }): Promise<{ publicKey: { toString(): string } }>
  disconnect?(): Promise<void>
  /** Sign arbitrary UTF-8 bytes. Returns the 64-byte Ed25519 signature. */
  signMessage?(message: Uint8Array, encoding?: string): Promise<{ signature: Uint8Array }>
}

declare global {
  interface Window {
    solana?: PhantomProvider
  }
}

/** True when the Phantom extension has injected `window.solana` into this page. */
export function phantomAvailable(): boolean {
  return typeof window !== 'undefined' && Boolean(window.solana?.isPhantom)
}

/**
 * Open the Phantom website in a new browser tab.
 *
 * Used only as a last-resort fallback in browser (non-Tauri) builds.
 * In Tauri builds, prefer `openPhantomPopupNative()` from the transport layer
 * which launches a Chrome --app popup that Phantom can inject into.
 */
export function openPhantomSite(): void {
  window.open('https://phantom.app', '_blank', 'noopener,noreferrer')
}

/**
 * Attempt a silent (no-popup) wallet connect using Phantom's `onlyIfTrusted`
 * flag.  Returns a `WalletContext` if the user previously approved this origin,
 * or `null` if approval is still needed.  Never throws — callers should fall
 * back to `connectPhantom()` when this returns `null`.
 *
 * Returns `null` immediately if the extension is not injected (Tauri WebView).
 */
export async function silentConnect(
  cluster: WalletContext['cluster'] = 'devnet',
): Promise<WalletContext | null> {
  const provider = window.solana
  if (!provider?.isPhantom) return null
  try {
    const result = await provider.connect({ onlyIfTrusted: true })
    return {
      provider: 'phantom',
      publicKey: result.publicKey.toString(),
      connected: true,
      cluster,
    }
  } catch {
    // User has not yet approved this origin — a full connect popup is needed.
    return null
  }
}

/**
 * Connect to Phantom and return a `WalletContext`.
 *
 * If the Phantom extension is injected (`window.solana`) this triggers the
 * normal approval popup.  If it is not injected (which is always the case
 * inside Tauri's WebView), this throws a `PhantomNotInjectedError` so the
 * caller can fall through to the manual-pubkey flow.
 */
export async function connectPhantom(
  cluster: WalletContext['cluster'] = 'devnet',
): Promise<WalletContext> {
  const provider = window.solana
  if (!provider?.isPhantom) {
    throw new PhantomNotInjectedError()
  }
  const result = await provider.connect()
  if (!result?.publicKey) {
    throw new Error('Phantom returned no public key after connect.')
  }
  return {
    provider: 'phantom',
    publicKey: result.publicKey.toString(),
    connected: true,
    cluster,
  }
}

export async function disconnectPhantom(
  cluster: WalletContext['cluster'] = 'devnet',
): Promise<WalletContext> {
  await window.solana?.disconnect?.()
  return { provider: 'phantom', connected: false, cluster }
}

/**
 * Ask Phantom to sign a UTF-8 message string.
 *
 * Encodes `message` to UTF-8 bytes and passes them to
 * `window.solana.signMessage()`.  Returns the 64-byte Ed25519 signature as a
 * `Uint8Array`, ready to be forwarded to `requestAuthNative`.
 *
 * Throws if Phantom is not available or if `signMessage` is not supported.
 */
export async function signMessage(message: string): Promise<Uint8Array> {
  const provider = window.solana
  if (!provider?.isPhantom) {
    throw new Error('Phantom wallet is not available.')
  }
  if (!provider.signMessage) {
    throw new Error('This version of Phantom does not support signMessage.')
  }
  const encoded = new TextEncoder().encode(message)
  const result = await provider.signMessage(encoded, 'utf8')
  return result.signature
}

/**
 * Thrown by `connectPhantom` when `window.solana` is not injected.
 *
 * This always happens inside Tauri's WebView because browser extensions cannot
 * inject into a native WebView context.  Callers catch this error to enter the
 * manual public-key entry flow.
 */
export class PhantomNotInjectedError extends Error {
  constructor() {
    super(
      'The Phantom browser extension is not available in this context. ' +
        'Please open Phantom in your default browser and paste your wallet address below.',
    )
    this.name = 'PhantomNotInjectedError'
  }
}

/**
 * Validate a base-58 Solana public key string (32-byte Ed25519 key).
 * Does not do full decoding — just checks character set and plausible length.
 */
export function isValidSolanaPublicKey(value: string): boolean {
  return /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(value.trim())
}
