# tests/__helpers__/

Shared test utilities: domain factories, assertion helpers, and seed data.

## Files

| File | Exports |
|------|---------|
| `fixtures.ts` | `makeFixture`, `makeOddsQuote`, `makeTxLineEvent`, `makeProofReceipt`, `makeAgentBid`, `makeArenaPosition`, `makeSettlementRecord`, `makeArenaScore`, `makeSignalRecord`, `makeAgentSafetyStatus`, `makeLeaderboardEntry`, `uid`, `TS` |

## Design principles

### Partial overrides

Every factory accepts `Partial<T>` so tests declare only the fields they care about:

```ts
// ✅ Minimal, expressive
const quote = makeOddsQuote({ outcome: 'away', impliedProbability: 0.35 })

// ❌ Verbose, brittle
const quote: OddsQuote = {
  fixtureId: 1001, outcome: 'away', decimal: 2.86,
  impliedProbability: 0.35, source: 'txline', ts: '2026-…',
}
```

### Deterministic defaults

- `TS` is a fixed ISO timestamp (`'2026-06-14T14:00:00.000Z'`) for snapshot stability.
- `uid(prefix)` is an auto-incrementing string ID unique within a test run.

### No side effects

Helper functions are pure — they do not import `vitest`, set up spies, or
modify global state. Import `vi` only inside test files.

## Adding a new factory

1. Add the factory to `fixtures.ts` following the existing pattern.
2. Export it from the same file (no barrel index needed).
3. Keep defaults valid and representative — all fields that are required by the
   TypeScript type must be present in the default spread.
