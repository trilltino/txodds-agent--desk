# World Cup Agent Desk — Full-Native Tauri Migration Plan

**Goal:** turn the current app from *"React app in a webview with a Vite proxy doing the real work"* into a **full Tauri desktop application** where the Rust core owns ingestion, chain observation, secrets, settlement, and persistence — and the webview is a pure renderer.

This is not a rewrite. Every phase keeps the app demoable. Each phase moves one responsibility across the IPC boundary and has an acceptance test you can run.

**Implementation status (2026-07-05):** the repo now contains the desktop implementation path described here: native Triton RPC, Rust-owned TxLINE ingestion/replay, SQLite ledger, Rust market rounds, a Rust-managed Yellowstone gRPC sidecar, and a Rust-managed CoralOS settlement sidecar. The Yellowstone sidecar uses Triton's official Node SDK pinned to the Windows-compatible `4.x` line; the packaged app bundles the sidecar scripts, runtime Node packages, and `sidecars/bin/node.exe`.

---

## 1. Where the app is today (verified 2026-07-05)

| Layer | Current state | Problem for a "full" desktop app |
| --- | --- | --- |
| UI | React 19 + Vite, three track surfaces (Settlement Lab / Signal Arena / Fan Mode), Proof Panel, chain-status strip | Fine — stays |
| Triton One | Browser-dev fallback lives in `src/domain/triton/client.ts`; native production calls live in `src-tauri/src/triton/rpc.rs` | Packaged `.exe` uses Rust RPC, not the Vite proxy. |
| TxLINE | Browser-dev fallback lives in `src/domain/txline/client.ts`; native production ingestion lives in `src-tauri/src/txline/ingest.rs` | Browser SSE from a webview hits CORS/token-exposure problems; tokens would live in JS |
| Rust core | Commands live in `src-tauri/src/lib.rs`; implementation is split across `coral/`, `triton/`, `txline/`, and `ledger/` | Rust owns production RPC, ingestion, events, persistence, settlement bridging, and native state |
| Secrets | `.env` read by Vite config (dev only), gitignored | Tokens must move to the Rust side permanently |
| Yellowstone gRPC | Not used anywhere | gRPC is impossible from a webview; **only** the Rust core can do this |
| Capabilities | `core:default`, `opener:default` | Will need `shell`/`notification`/`tray` additions as features land |
| Packaging | `bundle.icon: []` | `tauri build` **fails on Windows without an icon** — must fix before shipping |

Verified working endpoints (2026-07-05):

- `https://xfsoluti-solanad-d155.devnet.rpcpool.com` — x-token auth, developer rate tier, solana-core 4.1.0-rc.1
- `https://xfsoluti-solanam-739d.mainnet.rpcpool.com` — x-token auth, tier3, solana-core 4.1.0
- Both accept the token as an `x-token` header **and** as a URL path segment. Allowed Origins are locked (`__blocked.rpcpool.com`), which is correct: browsers can never call these directly. **Native Rust code has no Origin header, so it is unaffected — the whole CORS problem evaporates once RPC moves into the Rust core.**

---

## 2. Target architecture

```text
┌────────────────────────────── Tauri app (single .exe) ──────────────────────────────┐
│                                                                                      │
│  ┌───────────────────────────  Rust core (src-tauri)  ────────────────────────────┐  │
│  │                                                                                │  │
│  │  ingest/txline.rs      reqwest SSE → parse → emit("txline://event")            │  │
│  │  ingest/replay.rs      JSONL recorder + deterministic replayer (demo mode)     │  │
│  │  chain/rpc.rs          JSON-RPC client for both rpcpool endpoints (x-token)    │  │
│  │  chain/yellowstone.rs  Dragon's Mouth gRPC: slots / accounts / transactions    │  │
│  │  chain/observer.rs     watchProgram / watchAccount / watchReference            │  │
│  │  market/engine.rs      WANT → BID → AWARD → DELIVERED → VERIFIED state machine │  │
│  │  market/policy.rs      spend caps, allowlists, verifier-gated release          │  │
│  │  settle/escrow.rs      devnet escrow calls (or Node sidecar bridge, see §8)    │  │
│  │  ledger/store.rs       SQLite run ledger (rusqlite) — every round persisted    │  │
│  │  config.rs             dotenvy in dev, OS keychain (keyring) in prod           │  │
│  │                                                                                │  │
│  │        commands (invoke) ↑↓            events (emit/listen) ↑↓                 │  │
│  └────────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                      │
│  ┌───────────────────────────  Webview (src/) — pure renderer  ──────────────────┐   │
│  │  transport.ts — ONE seam: isTauri() ? invoke/listen : fetch/poll (dev web)    │   │
│  │  LiveFeed · AgentArena · SettlementLab · FanMode · ProofPanel · ChainStatus   │   │
│  └────────────────────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────────────┘
         │                                │                                │
         ▼                                ▼                                ▼
   TxLINE SSE API              Triton RPC Pool (HTTP)          Triton Yellowstone (gRPC)
   (guest JWT + X-Api-Token)   devnet + mainnet, x-token       Dragon's Mouth subscribe
```

Design rules that make it "full Tauri" rather than a wrapped website:

1. **The webview never holds a secret.** No token, JWT, or keypair ever crosses the IPC boundary. The UI asks for *results*, not credentials.
2. **The webview never opens a network connection** (except the Vite dev server in dev). Strict CSP enforces this — see §10.
3. **Push, not poll.** Rust emits events; React listens. The current 5s slot-polling in `ChainStatus.tsx` becomes a `chain://slot` event stream driven by Yellowstone's `SubscribeRequestFilterSlots`.
4. **The app works unplugged.** Replay mode (recorded TxLINE JSONL + cached chain receipts from the SQLite ledger) lets you demo on judging day with zero live dependencies.

---

## 3. Rust module layout

```text
src-tauri/
  Cargo.toml
  tauri.conf.json
  capabilities/default.json
  icons/                      ← generated by `npx tauri icon` (§11)
  src/
    main.rs                   (unchanged: calls lib run())
    lib.rs                    (builder: plugins, managed state, command registry)
    config.rs                 AppConfig: env in dev, keyring in prod
    error.rs                  one AppError enum, `impl Serialize` for IPC
    ingest/
      mod.rs
      txline.rs               SSE client (reqwest stream) → TxLineEvent → emit
      replay.rs               record to JSONL / replay with original timing
    chain/
      mod.rs
      rpc.rs                  JSON-RPC over reqwest to both rpcpool endpoints
      yellowstone.rs          yellowstone-grpc-client subscriptions + reconnect
      observer.rs             ChainObserver trait: program/account/reference watches
    market/
      mod.rs
      engine.rs               round state machine (port of agentMarket.ts)
      strategies.rs           bid generation (port of strategies.ts/scoring.ts)
      policy.rs               fund-movement choke point (port of coralOS policy)
    settle/
      mod.rs
      escrow.rs               devnet escrow create/deposit/release/refund
      sidecar.rs              option B: drive solana_coralOS Node runtime (§8)
    ledger/
      mod.rs
      store.rs                rusqlite: runs, bids, deliveries, receipts, slots
```

`Cargo.toml` additions:

```toml
[dependencies]
# existing: tauri 2, tauri-plugin-opener, serde, serde_json, reqwest, tokio, sha2, hex
dotenvy = "0.15"                              # .env in dev
keyring = "3"                                 # Windows Credential Manager in prod
rusqlite = { version = "0.32", features = ["bundled"] }
yellowstone-grpc-client = "8"                 # Dragon's Mouth subscriber
yellowstone-grpc-proto  = "8"
futures = "0.3"
tokio-stream = "0.1"
thiserror = "2"
tauri-plugin-notification = "2"               # phase 5
tauri-plugin-shell = "2"                      # only if sidecar option chosen
```

> Version note: check `yellowstone-grpc-client` against the proto version your
> Triton plan serves (`docs.triton.one` → Dragon's Mouth). Pin exactly; the
> proto evolves.

---

## 4. The IPC contract (commands + events)

This is the seam between Rust and React. Freeze it early; everything else can move underneath it.

### Commands (webview → Rust, request/response)

| Command | Args | Returns | Replaces |
| --- | --- | --- | --- |
| `chain_status` | `cluster: "devnet"\|"mainnet"` | `{ slot, solanaCore, latencyMs, ts }` | `getChainStatus()` in `triton.ts` |
| `chain_rpc` | `cluster, method, params` (allowlisted methods only) | JSON result | `tritonRpc()` / Vite proxy |
| `observe_settlement` | `reference, escrowAccount?` | `TritonObservation` | `observeSettlement()` |
| `start_txline` | `mode: "live"\|"mock"\|"replay", fixtureId?` | `()` — events follow | browser SSE in `txline.ts` |
| `stop_txline` | — | `()` | — |
| `run_agent_round` | `eventId, track` | `runId` — progress arrives as events | `runLocalAgentRound()` |
| `get_run` / `list_runs` | `runId?` | `AgentRun` / `AgentRun[]` | in-memory `runs` state in `App.tsx` |
| `hash_delivery` | `payload` | `{ sha256, reference }` | already exists in Rust ✔ |
| `get_config` | — | non-secret config + feature flags | already exists ✔ (extend: `tritonConfigured` should check the RPC vars, not `TRITON_GRPC_ENDPOINT` only) |
| `export_fan_card` | `runId` | PNG path / share text | new (phase 5) |

**Rule:** `chain_rpc` validates `method` against an allowlist (`getSlot`, `getVersion`, `getBalance`, `getLatestBlockhash`, `getSignaturesForAddress`, `getAccountInfo`, `getTransaction`). The webview must not be able to make the Rust core send arbitrary RPC.

### Events (Rust → webview, push)

| Event | Payload | Emitted by |
| --- | --- | --- |
| `txline://event` | `TxLineEvent` (same shape as `src/types.ts`) | `ingest/txline.rs`, `replay.rs` |
| `chain://slot` | `{ cluster, slot, ts }` | `yellowstone.rs` slot subscription (fallback: 5s RPC poll) |
| `chain://account` | `{ account, lamports, slot, dataLen }` | account subscription |
| `chain://tx` | `{ signature, slot, programIds, err }` | transaction subscription |
| `market://round` | `{ runId, phase: "WANT"\|"BID"\|"AWARD"\|"DELIVERED"\|"VERIFIED"\|"TRITON"\|"SETTLEMENT", detail }` | `market/engine.rs` — the Proof Panel timeline becomes live instead of arriving all at once |
| `settle://receipt` | `SettlementReceipt` | `settle/escrow.rs` |
| `ingest://status` | `{ source, state: "connected"\|"reconnecting"\|"stopped", detail }` | all ingest tasks |

### Frontend seam: `src/desktop/transport.ts`

```ts
import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

// Web-dev mode (plain `npm run dev`) keeps the current fetch/poll paths so the
// browser workflow stays alive. Native mode routes everything through IPC.
export const native = isTauri()

export async function chainRpc<T>(cluster: Cluster, method: string, params: unknown[] = []): Promise<T> {
  if (native) return invoke<T>('chain_rpc', { cluster, method, params })
  return tritonRpcViaProxy<T>(cluster, method, params)   // current implementation
}

export function onTxLineEvent(cb: (e: TxLineEvent) => void): () => void {
  if (native) { const un = listen<TxLineEvent>('txline://event', (ev) => cb(ev.payload)); return () => { un.then((f) => f()) } }
  return startMockTicker(cb)                             // current mock path
}
```

Only `transport.ts` knows which world it's in. `triton.ts`, `txline.ts`, and the components call through it. This keeps `npm run dev` (browser) working for fast UI iteration for the whole migration.

---

## 5. Phase plan

Each phase is shippable. Do them in order; none is longer than a focused day or two.

### Phase 1 — Native RPC core (kills the Vite-proxy dependency) ★ do first

*The packaged app currently cannot talk to Triton at all. This phase fixes that.*

1. `config.rs`: load `TRITON_*` from `.env` via `dotenvy` at startup (dev); managed `AppConfig` state.
2. `chain/rpc.rs`: reqwest JSON-RPC POST with `x-token` header, per-cluster; shared `reqwest::Client` in managed state; 10s timeout; map errors into `AppError`.
3. Commands `chain_rpc` (allowlisted), `chain_status`, `observe_settlement` — direct ports of what `src/domain/triton/client.ts` does in browser-dev fallback mode, minus the proxy.
4. Frontend: add `transport.ts`; point `triton.ts` at it.
5. Keep the Vite proxy config for browser-mode dev. Delete nothing yet.

**Accept:** `npm run tauri:dev` → chain pills go green **and** `npm run build && npm run tauri:build` produces an installer whose exe shows live slots with no dev server running. That exe is the first artifact that is truly a desktop app.

### Phase 2 — Yellowstone gRPC (the actual Triton flex)

*This is the feature a webview cannot have, and the judge-facing differentiator.*

1. Confirm Dragon's Mouth access on your plan: endpoint is typically `https://<your-endpoint>.rpcpool.com:443` with the same x-token (check the Triton dashboard "Metrics"/plan page; gRPC may need enabling per-token).
2. `chain/yellowstone.rs`:
   - `GeyserGrpcClient::build_from_shared(endpoint)?.x_token(Some(token))?.connect()`
   - One long-lived tokio task per subscription set. Subscribe: `slots: { "desk": {} }`, plus `accounts`/`transactions` filters added dynamically when Settlement Lab registers an escrow PDA or program ID.
   - Reconnect with exponential backoff (1s → 30s cap); emit `ingest://status` transitions so the UI shows `reconnecting` honestly.
3. Emit `chain://slot`, `chain://account`, `chain://tx`.
4. `ChainStatus.tsx`: prefer event stream; fall back to RPC polling if gRPC unavailable (keep the code path — it's the resilience story).
5. `observer.rs`: implement the adapter shape from the product plan —

```rust
#[async_trait]
pub trait ChainObserver: Send + Sync {
    async fn watch_program(&self, program_id: &str) -> Result<(), AppError>;   // → chain://tx
    async fn watch_account(&self, account: &str) -> Result<(), AppError>;      // → chain://account
    async fn watch_reference(&self, reference: &str) -> Result<(), AppError>;  // → settle://receipt
}
```

**Accept:** slot pill updates sub-second (visibly faster than the 5s poll); pulling the network cable shows `reconnecting` then recovers; `TRITON` timeline entries in the Proof Panel carry Yellowstone-observed slots, not polled ones.

### Phase 3 — Native TxLINE ingestion + replay

1. `src-tauri/src/txline/ingest.rs`: port the SSE loop from `src/domain/txline/client.ts` to reqwest `bytes_stream()`; same block-splitting logic; JWT + `X-Api-Token` from config (secrets never reach the webview). Auto-reconnect with `Last-Event-ID` if TxLINE supports it.
2. `ingest/replay.rs`: every live event appends to `%APPDATA%/agent-desk/replays/<fixture>.jsonl`; `start_txline { mode: "replay" }` re-emits with original inter-event timing (or 10× speed flag). **This is the judging-day insurance** — mock mode stays for zero-data demos, replay mode shows real recorded TxLINE data.
3. `mock` mode moves to Rust too (port `mock.ts` fixtures) so all three modes emit the same `txline://event`.
4. `LiveFeed.tsx` subscribes via `transport.ts`; delete the static `useState(mockEvents)` in `App.tsx`.

**Accept:** desk shows live (or replayed) events with no browser fetch; DevTools network tab is silent during a full demo.

### Phase 4 — Market engine, policy, ledger in the core

1. Port `agentMarket.ts` / `strategies.ts` / `scoring.ts` to `market/engine.rs` + `strategies.rs`. Emit `market://round` at each phase transition — the Proof Panel timeline animates live.
2. `market/policy.rs`: port the coralOS policy checks that matter here — max spend per round, session cap, service allowlist, verifier-gated release. Policy is the *only* code path that can trigger `settle/`.
3. `ledger/store.rs`: SQLite (`rusqlite`, bundled). Tables: `runs`, `bids`, `deliveries`, `verdicts`, `receipts`, `chain_observations`. `list_runs` reads from here → run history survives restart (the current app forgets everything on refresh).
4. Trigger detection moves to Rust: implied-probability move ≥ `ODDS_MOVE_TRIGGER_PCT` auto-creates a WANT — Signal Arena becomes genuinely autonomous (Track 2's "operates without manual input" criterion), with the "Run agent round" button kept as a manual override.

**Accept:** restart the app → run history intact; leave it running on a replay → rounds fire with zero clicks; every settled round has a Yellowstone-stamped slot in SQLite.

### Phase 5 — Settlement + native UX + packaging

1. **Escrow** — pick one (§8): (A) port devnet escrow calls to Rust, or (B) run the existing `solana_coralOS` TypeScript runtime as a Tauri **sidecar** and bridge over stdin/stdout JSON lines. B is less work and reuses audited code; A removes the Node dependency. Recommend **B first, A later**.
2. Native desktop touches that make it feel like a product, not a page:
   - `tauri-plugin-notification`: goal / settlement toasts even when minimized
   - System tray: live score + slot in the tooltip; show/hide window
   - `export_fan_card`: render the Fan Mode card to PNG (offscreen webview capture or `resvg`) + copy share text — Track 3's "shareable" story
   - Optional second window: borderless always-on-top "match ticker"
3. Packaging (§11): icons, NSIS installer, version bump, CSP tightened.

**Accept:** `tauri build` installer installs on a clean Windows machine; full demo (replay → auto round → verified → devnet receipt → Yellowstone slot → toast) runs offline except for Solana devnet + Triton.

---

## 6. Secrets model

| Secret | Dev | Packaged app |
| --- | --- | --- |
| Triton x-tokens (2) | `.env` via `dotenvy` | `keyring` crate → Windows Credential Manager; first-run settings dialog writes them |
| TxLINE guest JWT + API token | `.env` | same keyring flow |
| Devnet payer keypair | `PAYER_KEYPAIR_PATH` file | keep as file path, but only the Rust side (or sidecar) ever reads it |

Rules: secrets never serialize across IPC; `get_config` returns booleans (`tritonConfigured: true`), never values; the Vite `VITE_`-prefix trap (client-bundled env) stays irrelevant because no secret ever has that prefix.

> Housekeeping: the current tokens were pasted into external AI chats as
> screenshots. Rotate both in the Triton dashboard before the submission video.

---

## 7. Where each current file ends up

| Today (`src/`) | After migration |
| --- | --- |
| `lib/triton.ts` | Thin typed wrapper over `transport.ts`; polling kept only as non-Tauri fallback |
| `lib/txline.ts` | Deleted from webview → `ingest/txline.rs`; UI keeps only the `TxLineEvent` type |
| `lib/agentMarket.ts` | → `market/engine.rs`; UI keeps only rendering types |
| `lib/strategies.ts`, `lib/scoring.ts` | → `market/strategies.rs` |
| `lib/settlement.ts` | → `settle/escrow.rs` (or sidecar bridge) |
| `lib/mock.ts` | Fixtures → Rust `mock` mode; UI copy stays for storybook-style previews |
| `components/*` | Unchanged except: subscribe to events instead of receiving one-shot props; ProofPanel renders `market://round` stream |
| `vite.config.ts` proxy | Kept **only** for browser-mode dev; unused by the packaged app |

---

## 8. Settlement: port vs sidecar (the one real fork in the road)

| | A: pure Rust (`solana-sdk`) | B: Node sidecar running `solana_coralOS` runtime |
| --- | --- | --- |
| Effort | High — reimplement escrow client, Solana Pay reference flow, coralOS wire format | Low — the TS runtime already does WANT→…→RELEASED against devnet |
| Binary | Single exe, no Node needed | Ships a Node runtime or requires it installed |
| Risk | New bugs in money-touching code | IPC plumbing only; escrow code already exercised |
| Story | "Fully native" | "Desktop console driving the same coralOS rails as the repo" — honestly a *better* hackathon story |

**Recommendation: B for the hackathon, A afterwards if the product lives on.** Sidecar mechanics: bundle via `tauri.conf.json > bundle > externalBin`, spawn with `tauri-plugin-shell`, speak newline-delimited JSON (`{cmd:"createEscrow",...}` / `{evt:"receipt",...}`), and have `settle/sidecar.rs` translate to `settle://receipt` events. The keypair path is passed to the sidecar via env, never through the webview.

---

## 9. Failure handling (what makes it feel professional)

- Every long-lived task (SSE, gRPC, sidecar) owns a supervisor loop: backoff reconnect, `ingest://status` on every transition. The UI must never show a stale green pill — that's why status events carry `state`, not just data.
- `chain_rpc` errors return typed `AppError` variants (`RateLimited`, `Unreachable`, `RpcError{code,message}`) so the UI can say "devnet tier limit hit — backing off" instead of a generic toast. Remember the devnet token is on the **developer** tier: cap RPC polling at ≥4s and let Yellowstone carry the real-time load (streams don't burn the request budget the same way).
- The market engine is a state machine in SQLite: if the app dies mid-round, restart resumes or marks the round `abandoned` — no phantom escrows. Policy refuses new WANTs while a round on the same fixture is unresolved.
- Clock discipline: every timeline entry stores both wall time and (when available) observed slot, so the Proof Panel can show "VERIFIED at 14:02:11 · slot 474,120,882".

---

## 10. Security hardening checklist

- [ ] `tauri.conf.json > app > security > csp` — currently `null`. Set: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost` (dev mode needs `http://localhost:1420 ws://localhost:1420` added conditionally).
- [ ] Capabilities: add only what each phase needs (`notification:default`, `shell:allow-execute` scoped to the sidecar binary, `tray`). Never `shell:default` unscoped.
- [ ] `chain_rpc` method allowlist (§4) — webview cannot originate arbitrary RPC.
- [ ] No `withGlobalTauri`; keep the API surface import-scoped.
- [ ] Secrets audit: grep the built `dist/` for token substrings before every release (`grep -r "rpcpool" dist/` should match nothing but hostnames if even that).
- [ ] Devtools disabled in release builds (default in Tauri 2 — don't re-enable).

---

## 11. Packaging & release (Windows first)

1. **Icons (blocking):** `bundle.icon` is `[]` and `tauri build` fails without it. Make a 1024×1024 PNG, run `npx tauri icon path/to/icon.png` → writes `src-tauri/icons/*`, then set `"icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.ico"]`.
2. `tauri build` → NSIS `.exe` installer + MSI under `src-tauri/target/release/bundle/`.
3. Unsigned binaries trip SmartScreen. For the hackathon: mention it in the README and provide the "More info → Run anyway" instruction, or zip the portable exe. Signing certs are out of scope pre-deadline.
4. Version stamping: keep `package.json`, `tauri.conf.json`, `Cargo.toml` versions in lockstep (one `npm version` script that rewrites all three).
5. CI (post-hackathon): GitHub Actions `tauri-apps/tauri-action` matrix (Windows/macOS/Linux); cache `~/.cargo` and `target/`.

---

## 12. What this does for each track

| Track | Webview version says | Full-native version says |
| --- | --- | --- |
| Markets & Settlement | "we poll RPC through a dev proxy" | "escrow PDAs watched via **Yellowstone gRPC**; receipts persisted in a local ledger; resolution survives restarts" |
| Trading Agents | "click the button to run a round" | "odds trigger detection runs in the core; rounds fire **with the window minimized**; toast on settlement; run ledger is queryable" |
| Fan Experience | "a web page that looks alive" | "an installable app with native notifications on goals, a tray ticker, and one-click PNG share cards" |
| Triton usage | 1 HTTP endpoint through a proxy | HTTP RPC **and** Dragon's Mouth slots/accounts/transactions with visible reconnect handling — a real integration, not a health check |

---

## 13. Suggested order of work (condensed)

```text
Phase 1  native RPC core + transport.ts seam        ← unblocks a real .exe   (~½–1 day)
Phase 2  Yellowstone slots → accounts/tx filters    ← the Triton showpiece   (~1 day)
Phase 3  TxLINE SSE in Rust + JSONL replay          ← judging-day insurance  (~1 day)
Phase 4  market engine + policy + SQLite ledger     ← autonomy + persistence (~1–2 days)
Phase 5  sidecar settlement + notifications/tray/   ← product polish + ship  (~1–2 days)
         share cards + icons + tauri build
```

Cut line if time runs short: Phases 1–3 alone already produce an installable desktop app with live Yellowstone streaming and recorded-replay demos — that is defensibly "a full Tauri app" even before the engine port.

---

## 14. Command cheat sheet

```powershell
# dev (desktop window, hot reload)          # requires cargo on PATH:
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
npm run tauri:dev

# dev (browser only, uses Vite proxy path)
npm run dev

# type-check TS                             # check Rust without running
npx tsc --noEmit                            cargo check --manifest-path src-tauri/Cargo.toml

# icons (one-time, before first build)
npx tauri icon assets/icon-1024.png

# release build → src-tauri/target/release/bundle/
npm run tauri:build
```
