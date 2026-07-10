# ui/core/coral/

The Coral-style agent market engine — bid scoring, winner selection, and round orchestration.

## Key exports

| Export | Purpose |
|--------|---------|
| `scoreBid(track, bid)` | Deterministic score for one agent bid on a given track |
| `chooseWinner(track, bids)` | Selects the highest-scoring bid; returns `undefined` for empty arrays |

## Scoring formula

```
score = confidence × roleFactor(track, role) × priceFactor(priceSol) × etaFactor(etaMs)

roleFactor  : 1.25 for sharp, 1.15 for risk (trading track); 1.0 for all others
priceFactor : max(0.2,  1 − priceSol × 4)
etaFactor   : etaMs < 1500 ? 1.05 : 1.0
```

## Tests

See `tests/core/coral/scoring.test.ts` for exhaustive unit tests covering all multipliers and edge cases.
