# tests/__mocks__/

Manual mocks for modules that require a Tauri runtime or other external infrastructure.

## Files

| File | What it mocks | Used by |
|------|--------------|---------|
| `transport.ts` | All exports of `ui/desktop/transport.ts` | Any test that imports a module with a transitive dependency on the Tauri native bridge |

## How the mock is activated

`vite.config.ts` registers an alias so that any `import … from '…/desktop/transport'`
resolves to `tests/__mocks__/transport.ts` during test runs:

```ts
// vite.config.ts (test.resolve.alias)
'ui/desktop/transport': path.resolve(__dirname, 'tests/__mocks__/transport.ts')
```

Alternatively, call `vi.mock('../../ui/desktop/transport')` at the top of a test file to opt in
explicitly (Vitest will hoist it automatically).

## Overriding stubs per test

All stubs are `vi.fn()`. To simulate a specific response in one test:

```ts
import { txlineFixturesSnapshotNative } from '../../ui/desktop/transport'
import { vi } from 'vitest'

vi.mocked(txlineFixturesSnapshotNative).mockResolvedValueOnce([
  { FixtureId: 42, Participant1: 'Brazil', Participant2: 'Argentina' },
])
```

Remember to call `vi.clearAllMocks()` in `afterEach` if stubs accumulate call history across tests.
