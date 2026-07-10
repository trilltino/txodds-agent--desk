# ui/core/txline/

TxLINE data-ingestion layer — normalising raw API payloads into canonical domain types and detecting actionable market events.

## Key exports

| Export | Module | Purpose |
|--------|--------|---------|
| `normalizeFixtures(raw)` | `fixtures.ts` | Defensive parser — accepts any TxLINE fixture payload shape and returns a sorted `Fixture[]` |
| `epochDayNow()` | `fixtures.ts` | Current day as an epoch-day integer (consistent with Rust `chrono::Utc::today`) |
| `detectOddsMove(prev, next, threshold?)` | `events.ts` | Returns a `TxLineEvent` when any outcome's implied probability shifts by ≥ threshold pp |
| `eventShouldStartRound(event)` | `events.ts` | Returns `true` for events that should trigger a new agent market round |

## Tests

`tests/core/txline/` contains full unit test coverage. See that folder's README for the contract specification.
