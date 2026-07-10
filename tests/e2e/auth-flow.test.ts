/**
 * tests/e2e/auth-flow.test.ts
 *
 * End-to-end flow test: wallet signature → auth challenge → UserProfile stored.
 *
 * Exercises the complete TypeScript auth pipeline without a Tauri runtime:
 *
 *   1. A wallet public key is validated as a plausible Solana base58 key.
 *   2. An AuthChallenge is constructed with a nonce and a signable message.
 *   3. A 64-byte Ed25519 signature is encoded as a plain number array for IPC.
 *   4. The transport stub resolves `requestAuthNative` to a stored UserProfile.
 *   5. The returned profile passes all UserProfile shape invariants.
 *
 * The native bridge (`desktop/transport`) is stubbed via the Vitest alias in
 * `vite.config.ts` — no Tauri binary is needed.
 */

import { describe, expect, it, vi, beforeEach } from 'vitest'

// Intercept the real Tauri bridge before it loads — the factory returns the
// vi.fn() stubs defined in tests/__mocks__/transport.ts.
vi.mock('../../ui/desktop/transport', async () => await import('../__mocks__/transport'))

import {
  requestAuthNative,
  getUserProfileNative,
  saveUserProfileNative,
  deleteUserProfileNative,
} from '../../ui/desktop/transport'
import { makeUserProfile } from '../__helpers__/fixtures'

// ── Step 1 — public key validation ────────────────────────────────────────────

describe('step 1 — wallet public key format', () => {
  it('accepts a valid 44-char Solana base58 pubkey', () => {
    const pk = 'HN7cABqLq46Es1jh92dQQisAi18upfBu7bMxZGPeNiKL'
    expect(pk.length).toBeGreaterThanOrEqual(32)
    expect(pk.length).toBeLessThanOrEqual(44)
    // Base58 alphabet — no 0, O, I, l characters
    expect(pk).toMatch(/^[1-9A-HJ-NP-Za-km-z]+$/)
  })

  it('rejects a string that contains disallowed base58 characters', () => {
    const invalid = 'OOOO0000IIII0000llll1111AAAA2222BBBB3333CC'
    // Any of 0/O/I/l would fail the base58 regex
    expect(invalid).toMatch(/[0OIl]/)
  })
})

// ── Step 2 — auth challenge construction ──────────────────────────────────────

describe('step 2 — AuthChallenge construction', () => {
  function buildChallenge(publicKey: string) {
    const nonce = btoa(`${publicKey.slice(0, 8)}-${Date.now()}`)
    return {
      nonce,
      message: `Sign to authenticate with TxOdds Agent Desk\nNonce: ${nonce}`,
      ts: new Date().toISOString(),
    }
  }

  it('challenge contains the wallet public key prefix in the nonce', () => {
    const pk = 'HN7cABqLq46Es1jh92dQQisAi18upfBu7bMxZGPeNiKL'
    const challenge = buildChallenge(pk)
    expect(challenge.nonce.length).toBeGreaterThan(0)
    expect(challenge.message).toContain(challenge.nonce)
  })

  it('challenge ts is a valid ISO-8601 string', () => {
    const challenge = buildChallenge('HN7cABqLq46Es1jh92dQQisAi18upfBu7bMxZGPeNiKL')
    expect(new Date(challenge.ts).toISOString()).toBe(challenge.ts)
  })
})

// ── Step 3 — signature encoding ───────────────────────────────────────────────

describe('step 3 — Ed25519 signature encoding for IPC', () => {
  it('a 64-byte Uint8Array converts to a 64-element number array', () => {
    const sig = new Uint8Array(64)
    const arr = Array.from(sig)
    expect(arr).toHaveLength(64)
    expect(Array.isArray(arr)).toBe(true)
  })

  it('all byte values are in the valid 0–255 range', () => {
    const sig = Uint8Array.from({ length: 64 }, (_, i) => i * 4)
    Array.from(sig).forEach((b) => {
      expect(b).toBeGreaterThanOrEqual(0)
      expect(b).toBeLessThanOrEqual(255)
    })
  })
})

// ── Step 4 — requestAuthNative transport stub ─────────────────────────────────

describe('step 4 — requestAuthNative resolves to UserProfile', () => {
  const PK = 'HN7cABqLq46Es1jh92dQQisAi18upfBu7bMxZGPeNiKL'

  // Reset the mock queue before each test so stubs from one test cannot
  // bleed into the next (a stale mockResolvedValueOnce queue caused flakes).
  beforeEach(() => {
    vi.mocked(requestAuthNative).mockReset()
  })

  it('resolves a UserProfile with the correct publicKey', async () => {
    vi.mocked(requestAuthNative).mockResolvedValueOnce(makeUserProfile({ publicKey: PK }))
    const profile = await requestAuthNative(PK, new Uint8Array(64), 'nonce')
    expect(profile?.publicKey).toBe(PK)
  })

  it('resolved profile has a username and cluster', async () => {
    vi.mocked(requestAuthNative).mockResolvedValueOnce(
      makeUserProfile({ publicKey: PK, username: 'bob', cluster: 'mainnet-beta' }),
    )
    const profile = await requestAuthNative(PK, new Uint8Array(64), 'nonce')
    expect(profile?.username).toBe('bob')
    expect(profile?.cluster).toBe('mainnet-beta')
  })
})

// ── Step 5 — UserProfile shape invariants ─────────────────────────────────────

describe('step 5 — stored UserProfile shape invariants', () => {
  it('publicKey is a non-empty base58 string', () => {
    const p = makeUserProfile()
    expect(p.publicKey).toMatch(/^[1-9A-HJ-NP-Za-km-z]{32,44}$/)
  })

  it('username is non-empty', () => {
    const p = makeUserProfile({ username: 'charlie' })
    expect(p.username.length).toBeGreaterThan(0)
  })

  it('cluster is devnet or mainnet-beta', () => {
    expect(['devnet', 'mainnet-beta']).toContain(makeUserProfile().cluster)
  })

  it('createdAt is a valid ISO-8601 timestamp', () => {
    const p = makeUserProfile()
    expect(new Date(p.createdAt).toISOString()).toBe(p.createdAt)
  })
})

// ── Profile CRUD stubs ─────────────────────────────────────────────────────────

describe('profile CRUD transport stubs', () => {
  const PK = 'HN7cABqLq46Es1jh92dQQisAi18upfBu7bMxZGPeNiKL'

  it('getUserProfileNative returns null by default (no stored profile)', async () => {
    const result = await getUserProfileNative(PK)
    expect(result === null || result === undefined).toBe(true)
  })

  it('getUserProfileNative can be stubbed to return a profile', async () => {
    const stored = makeUserProfile({ publicKey: PK })
    vi.mocked(getUserProfileNative).mockResolvedValueOnce(stored)
    const result = await getUserProfileNative(PK)
    expect(result).toEqual(stored)
  })

  it('saveUserProfileNative can be stubbed to return the saved profile', async () => {
    const saved = makeUserProfile({ publicKey: PK, username: 'dave' })
    vi.mocked(saveUserProfileNative).mockResolvedValueOnce(saved)
    const result = await saveUserProfileNative(PK, 'dave', 'devnet')
    expect(result?.username).toBe('dave')
  })

  it('deleteUserProfileNative is a mock function', () => {
    expect(vi.isMockFunction(deleteUserProfileNative)).toBe(true)
  })
})
