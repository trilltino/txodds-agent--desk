# tests/e2e/

End-to-end flow tests that exercise the complete TypeScript pipeline without a Tauri runtime.

These tests are not unit tests of individual functions — they tell a **narrative** through the
application's data flow and act as living documentation of how the pieces compose.

## Files

| File | Pipeline exercised |
|------|--------------------|
| `fixture-to-round.test.ts` | Raw TxLINE payload → `normalizeFixtures` → `detectOddsMove` → `eventShouldStartRound` → `chooseWinner` |
| `auth-flow.test.ts` | Wallet pubkey validation → `AuthChallenge` construction → Ed25519 signature encoding → `requestAuthNative` → `UserProfile` shape |
| `safety-pipeline.test.ts` | `AgentSafetyStatus` shape → budget guard predicates → kill-switch state → `tripKillSwitchNative` → status re-read |

## What "e2e" means here

No browser, no Tauri binary, no network. "End-to-end" refers to the **TypeScript business logic
pipeline** from raw external data through to an actionable result. The native bridge is stubbed
via the Vitest alias in `vite.config.ts`; IPC calls are exercised through `vi.fn()` mocks.

Actual Tauri IPC integration tests (commands, events, sidecar lifecycle) live in the Rust test
suite under `native/src/` and require `cargo test`.

## Structure convention

Each e2e test file is structured as sequential `describe` blocks that narrate the pipeline:

```
describe('step 1 — …', () => { … })
describe('step 2 — …', () => { … })
…
describe('full pipeline — all steps composed', () => { … })
```

This makes the file readable as prose — a reader can follow the data flow step by step.

## Adding a new e2e test

1. Identify the pipeline you want to narrate (e.g. `proof-receipt → settlement-decision`).
2. Create `tests/e2e/<pipeline-name>.test.ts`.
3. Start with a JSDoc block naming each step and what the test will NOT call.
4. Structure it as sequential numbered `describe` blocks ending with a `full pipeline` block.
5. Compose existing `__helpers__/fixtures` factories; avoid inline objects.
6. Keep each `it` self-contained — no shared mutable state between `it` blocks.
7. Add the new file to the table above.
