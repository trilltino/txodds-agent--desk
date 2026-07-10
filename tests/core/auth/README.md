# tests/core/auth/

Unit tests for the wallet authentication layer.

## Files

| File | Source layer | What is tested |
|------|-------------|----------------|
| `wallet-auth.test.ts` | `ui/app/hooks/useWalletAuth.ts` | `AuthChallenge` shape invariants, `UserProfile` shape invariants, transport stub wiring, signature byte-array encoding |

## What is covered

- `AuthChallenge` — nonce is non-empty, message contains the nonce, `ts` is valid ISO-8601
- `UserProfile` — `publicKey` is in Solana base58 range (32–44 chars), `username` non-empty, `cluster` is one of `devnet | mainnet-beta`, `createdAt` is valid ISO-8601
- Transport stubs — `requestAuthNative` and `getUserProfileNative` are mock functions whose return values can be overridden per-test with `vi.mocked(...).mockResolvedValueOnce(...)`
- Signature encoding — a 64-byte `Uint8Array` serialises to a plain `number[]` with values in `0–255`

## What is NOT covered here

- React hook lifecycle (`useWalletAuth` state machine) — add `@testing-library/react` when a component harness is available.
- Phantom wallet adapter (`window.phantom` object) — requires `jsdom` environment.
- Actual Ed25519 signature verification — that logic lives in Rust (`native/src/commands/auth.rs`) and is covered by `cargo test -p txodds-native`.
