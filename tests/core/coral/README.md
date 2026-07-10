# tests/core/coral/

Unit tests for `ui/core/coral/` — the Coral-style agent bid-scoring engine.

## Files

| File | Source module | What is covered |
|------|--------------|-----------------|
| `scoring.test.ts` | `ui/core/coral/scoring.ts` | `scoreBid` (role multipliers, price penalty, ETA bonus, linearity), `chooseWinner` (correctness, immutability, determinism, edge cases) |

## Key contracts under test

### `scoreBid(track, bid)`

```
score = confidence × roleFactor × priceFactor × etaFactor
```

- `roleFactor`: `1.25` for `sharp`, `1.15` for `risk`, `1.0` for all other roles on `'trading'` track.
- `priceFactor`: `max(0.2, 1 − priceSol × 4)` — clamped to 0.2 so expensive agents are penalised but not eliminated.
- `etaFactor`: `1.05` if `etaMs < 1500`, else `1.0`.

### `chooseWinner(track, bids)`

Returns the `AgentBid` with the highest `scoreBid` score. Returns `undefined` for an empty array. Does not sort or mutate the input array.
