# crates/agents/contrarian/

**Role:** `pundit` — fades the consensus market position.

## Strategy

Identifies over-bet market favourites using historical closing-line value data and live implied-probability overrounds. Produces contrarian signals when the market is overweight on one outcome.

## When it wins rounds

The contrarian agent wins rounds when:
- The `chooseWinner` track is not `'trading'` (where `sharp` has a 1.25× boost).
- Confidence is high and price is competitive.
- The sharp signal is ambiguous or absent.

## Running tests

```bash
cargo test -p contrarian
```
