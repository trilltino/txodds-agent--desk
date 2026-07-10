# ui/core/markets/

Market-state helpers: implied probability derivation, odds formatting, and market status tracking.

## Responsibilities

- Convert decimal odds ↔ implied probability.
- Format odds for display (European decimal, US moneyline).
- Derive market status from the fixture `status` field and live event stream.

## Relationship to `ui/core/txline/`

`txline/` produces raw events and odds snapshots. `markets/` consumes those snapshots to compute derived market state (e.g. best-available odds, over-round, steam-move flags) that the UI displays.
