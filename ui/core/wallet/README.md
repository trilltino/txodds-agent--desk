# ui/core/wallet/

Wallet-connection helpers for Solana Pay and browser wallet adapters.

## Key type

```ts
interface WalletContext {
  provider: 'phantom' | 'solana-pay' | 'unknown'
  publicKey?: string
  connected: boolean
  cluster: 'devnet' | 'mainnet-beta'
}
```

## Responsibilities

- Detect and connect to an available wallet provider (Phantom, Solana Pay QR, etc.).
- Expose `WalletContext` to the rest of the app via `ui/app/hooks/`.
- Produce `SolanaPayIntent` objects (constructed by Rust; rendered here as URL or QR).

## Notes

Signing is never performed in TypeScript — it is delegated to the wallet extension or Solana Pay flow. The TypeScript layer only reads `publicKey` and monitors payment status.
