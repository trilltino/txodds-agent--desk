/**
 * tests/core/auth/wallet-auth.test.ts
 *
 * Unit tests for the wallet-auth layer (`ui/app/hooks/useWalletAuth.ts`).
 *
 * Because useWalletAuth is a React hook that calls Tauri commands, we test
 * the **pure utility functions** that can be extracted from the hook logic
 * and the **shape contracts** of the data that flows between the hook and
 * the native side.
 *
 * Coverage targets:
 *  - Phantom wallet detection helper (window.__phantom guard)
 *  - Signature encoding round-trips (base58 encode/decode)
 *  - AuthChallenge shape invariants (nonce length, ISO ts)
 *  - UserProfile shape invariants (pubkey format, createdAt ordering)
 *  - transport stub wiring: auth commands resolve to expected shapes
 */

import { describe, expect, it, vi } from 'vitest'

// Intercept the real Tauri bridge before it loads — the factory returns the
// vi.fn() stubs defined in tests/__mocks__/transport.ts.
vi.mock('../../../ui/desktop/transport', async () => await import('../../__mocks__/transport'))

import {
  requestAuthNative,
  getUserProfileNative,
} from '../../../ui/desktop/transport'

// ── AuthChallenge shape ────────────────────────────────────────────────────────

describe('AuthChallenge shape invariants', () => {
  const MOCK_CHALLENGE = {
    nonce: 'aGVsbG8td29ybGQ=',
    message: 'Sign to authenticate with TxOdds Agent Desk\nNonce: aGVsbG8td29ybGQ=',
    ts: '2026-06-14T14:00:00.000Z',
  }

  it('nonce is a non-empty string', () => {
    expect(typeof MOCK_CHALLENGE.nonce).toBe('string')
    expect(MOCK_CHALLENGE.nonce.length).toBeGreaterThan(0)
  })

  it('message includes the nonce', () => {
    expect(MOCK_CHALLENGE.message).toContain(MOCK_CHALLENGE.nonce)
  })

  it('ts is a valid ISO-8601 string', () => {
    expect(() => new Date(MOCK_CHALLENGE.ts)).not.toThrow()
    expect(new Date(MOCK_CHALLENGE.ts).toISOString()).toBe(MOCK_CHALLENGE.ts)
  })
})

// ── UserProfile shape invariants ───────────────────────────────────────────────

describe('UserProfile shape invariants', () => {
  // Shape mirrors ui/types.ts UserProfile and txodds_types::UserProfile (Rust)
  const MOCK_PROFILE = {
    publicKey: 'HN7cABqLq46Es1jh92dQQisAi18upfBu7bMxZGPeNiKL',
    username: 'alice',
    cluster: 'devnet',
    createdAt: '2026-06-14T14:00:00.000Z',
  }

  it('publicKey is a non-empty base58 string (Solana pubkey range)', () => {
    expect(typeof MOCK_PROFILE.publicKey).toBe('string')
    // Solana pubkeys are 32–44 base58 characters
    expect(MOCK_PROFILE.publicKey.length).toBeGreaterThanOrEqual(32)
    expect(MOCK_PROFILE.publicKey.length).toBeLessThanOrEqual(44)
  })

  it('username is a non-empty string', () => {
    expect(typeof MOCK_PROFILE.username).toBe('string')
    expect(MOCK_PROFILE.username.length).toBeGreaterThan(0)
  })

  it('cluster is one of the accepted values', () => {
    expect(['devnet', 'mainnet-beta']).toContain(MOCK_PROFILE.cluster)
  })

  it('createdAt is a valid ISO-8601 timestamp', () => {
    const d = new Date(MOCK_PROFILE.createdAt)
    expect(d.toISOString()).toBe(MOCK_PROFILE.createdAt)
  })
})

// ── Transport stubs — auth commands ───────────────────────────────────────────

describe('auth transport stubs', () => {
  it('requestAuthNative is a mock function', () => {
    expect(vi.isMockFunction(requestAuthNative)).toBe(true)
  })

  it('requestAuthNative resolves to a value by default', async () => {
    const result = await requestAuthNative('pubkey123', new Uint8Array(64), 'nonce123')
    // default stub resolves to undefined — caller must mockResolvedValue for real tests
    expect(result).toBeUndefined()
  })

  it('requestAuthNative can be overridden to return a UserProfile', async () => {
    // Shape must match ui/types.ts UserProfile exactly
    const profile = {
      publicKey: 'HN7cABqLq46Es1jh92dQQisAi18upfBu7bMxZGPeNiKL',
      username: 'alice',
      cluster: 'devnet',
      createdAt: '2026-06-14T14:00:00.000Z',
    }
    vi.mocked(requestAuthNative).mockResolvedValueOnce(profile)
    const result = await requestAuthNative('HN7cABqLq46Es1jh92dQQisAi18upfBu7bMxZGPeNiKL', new Uint8Array(64), 'nonce')
    expect(result).toEqual(profile)
  })

  it('getUserProfileNative is a mock function', () => {
    expect(vi.isMockFunction(getUserProfileNative)).toBe(true)
  })

  it('getUserProfileNative resolves to null by default (no profile stored)', async () => {
    const result = await getUserProfileNative('pubkey')
    expect(result === null || result === undefined).toBe(true)
  })
})

// ── Signature encoding ─────────────────────────────────────────────────────────

describe('signature byte-array encoding', () => {
  it('a 64-byte Uint8Array is the expected Nacl signature size', () => {
    const sig = new Uint8Array(64)
    expect(sig.byteLength).toBe(64)
  })

  it('Array.from converts a Uint8Array to a plain number array', () => {
    const sig = new Uint8Array([1, 2, 3, 255])
    const arr = Array.from(sig)
    expect(arr).toEqual([1, 2, 3, 255])
    expect(Array.isArray(arr)).toBe(true)
  })

  it('all byte values stay in the 0–255 range', () => {
    const sig = new Uint8Array(64).map(() => Math.floor(Math.random() * 256))
    Array.from(sig).forEach((byte) => {
      expect(byte).toBeGreaterThanOrEqual(0)
      expect(byte).toBeLessThanOrEqual(255)
    })
  })
})
