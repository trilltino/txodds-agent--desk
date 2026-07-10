# tests/

End-to-end and unit test suite for the TxOdds Agent Desk TypeScript codebase.

## Stack

| Tool | Purpose |
|------|---------|
| [Vitest](https://vitest.dev) | Test runner — co-located with Vite config, zero extra setup |
| `@vitest/coverage-v8` | V8-native coverage — no instrumentation overhead |

## Folder structure

```
tests/
├── __mocks__/
│   └── transport.ts          # Stubs for every Tauri native-bridge export
├── __helpers__/
│   └── fixtures.ts           # Domain-typed factory functions (makeFixture, makeAgentBid, makeUserProfile …)
├── core/
│   ├── txline/
│   │   ├── events.test.ts    # detectOddsMove, eventShouldStartRound
│   │   └── fixtures.test.ts  # normalizeFixtures, epochDayNow
│   ├── coral/
│   │   └── scoring.test.ts   # scoreBid, chooseWinner
│   ├── agent/
│   │   └── leaderboard.test.ts # shape invariants, ArenaPosition contracts
│   └── auth/
│       └── wallet-auth.test.ts # AuthChallenge shape, UserProfile shape, transport stubs, sig encoding
└── e2e/
    ├── fixture-to-round.test.ts  # Full pipeline: raw fixture → odds move → round
    ├── auth-flow.test.ts         # Full pipeline: wallet pubkey → challenge → signature → UserProfile
    └── safety-pipeline.test.ts  # Full pipeline: budget guard → kill-switch trip → status re-read
```

## Running the tests

```bash
# Run all tests once (CI / pre-commit)
npm test

# Watch mode for development
npm run test:watch

# Generate an lcov coverage report under ./coverage/
npm run test:coverage
```

## Conventions

### Test anatomy

Every test file follows this structure:

```
[JSDoc header — what module is under test, what is covered, pipeline steps]

import { describe, expect, it } from 'vitest'
import { functionUnderTest } from '../../ui/core/…'
import { makeX } from '../__helpers__/fixtures'

describe('functionUnderTest', () => {
  it('describes one observable behaviour', () => { … })
})
```

E2e tests use numbered `describe` blocks that narrate the pipeline step by step:

```
describe('step 1 — raw payload normalises to a Fixture', () => { … })
describe('step 2 — odds-move detected above threshold', () => { … })
…
describe('full pipeline — all steps composed', () => { … })
```

### Factory functions

Use the factories in `__helpers__/fixtures.ts` rather than inline object literals.
Each factory accepts a `Partial<T>` override so tests only declare what they care about:

```ts
// Good — declare only the fields the test exercises
const quote = makeOddsQuote({ outcome: 'away', impliedProbability: 0.35 })
const profile = makeUserProfile({ username: 'alice', cluster: 'mainnet-beta' })

// Avoid — brittle, spreads domain knowledge into every test
const quote: OddsQuote = { fixtureId: 1001, outcome: 'away', decimal: 2.86, … }
```

Available factories:

| Factory | Type produced |
|---------|--------------|
| `makeFixture` | `Fixture` |
| `makeOddsQuote` | `OddsQuote` |
| `makeTxLineEvent` | `TxLineEvent` |
| `makeProofReceipt` | `TxLineProofReceipt` |
| `makeAgentBid` | `AgentBid` |
| `makeArenaPosition` | `ArenaPosition` |
| `makeSettlementRecord` | `SettlementRecord` |
| `makeArenaScore` | `ArenaScore` |
| `makeSignalRecord` | `SignalRecord` |
| `makeAgentSafetyStatus` | `AgentSafetyStatus` |
| `makeLeaderboardEntry` | `AgentLeaderboardEntry` |
| `makeUserProfile` | `UserProfile` |

### Mocking the native bridge

Any module that imports from `ui/desktop/transport` will resolve to
`tests/__mocks__/transport.ts` via the Vitest alias in `vite.config.ts`.
Stubs are `vi.fn()` so individual tests can override them:

```ts
import { requestAuthNative } from '../../ui/desktop/transport'
import { vi } from 'vitest'
import { makeUserProfile } from '../__helpers__/fixtures'

vi.mocked(requestAuthNative).mockResolvedValueOnce(makeUserProfile())
```

### What is NOT tested here

- **Rust / native code** — tested via `cargo test` in the relevant crate.
- **React components** — the UI layer currently has no component test harness; add `@testing-library/react` when needed.
- **Tauri IPC integration** — requires a running Tauri binary; out of scope for unit tests.
- **Live network calls** — TxLINE ingestion, Solana RPC, Yellowstone — integration-tested in Rust with `--features integration-tests`.
