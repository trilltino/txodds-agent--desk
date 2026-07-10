# tests/core/txline/

Unit tests for `ui/core/txline/` — the TxLINE data-ingestion and event-detection layer.

## Files

| File | Source module | What is covered |
|------|--------------|-----------------|
| `events.test.ts` | `ui/core/txline/events.ts` | `detectOddsMove` (threshold logic, body text, statKeys), `eventShouldStartRound` (all event kinds) |
| `fixtures.test.ts` | `ui/core/txline/fixtures.ts` | `normalizeFixtures` (PascalCase / camelCase / bare array / invalid input), `epochDayNow` |

## Key contracts under test

- `detectOddsMove(prev, next, thresholdPP?)` — returns `null` if no outcome moves by ≥ threshold; returns a `TxLineEvent` with `kind: 'odds_move'` otherwise.
- `eventShouldStartRound(event)` — returns `true` for `goal | red_card | final_whistle | odds_move | proof_received`, `false` for informational kinds.
- `normalizeFixtures(raw)` — accepts any JSON shape from the TxLINE API and returns a sorted `Fixture[]`. Never throws on malformed input.
