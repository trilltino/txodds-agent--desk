# src-tauri/src/services/txline

TxLINE ingestion lives in the Rust backend.

## Files

- `api.rs`: documented TxLINE HTTP API helpers and path allowlist.
- `ingest.rs`: live SSE ingestion and event emission.
- `mod.rs`: module exports.

## Documented API Surface

The current TxLINE OpenAPI source is published at `https://txline.txodds.com/docs/docs.yaml`.

Rust exposes typed Tauri commands for:

- `txline_fixtures_snapshot`: `GET /api/fixtures/snapshot`
- `txline_odds_snapshot`: `GET /api/odds/snapshot/{fixtureId}`
- `txline_odds_updates`: `GET /api/odds/updates/{fixtureId}`
- `txline_odds_interval`: `GET /api/odds/updates/{epochDay}/{hourOfDay}/{interval}`
- `txline_scores_snapshot`: `GET /api/scores/snapshot/{fixtureId}`
- `txline_scores_updates`: `GET /api/scores/updates/{fixtureId}`
- `txline_scores_historical`: `GET /api/scores/historical/{fixtureId}`
- `txline_scores_interval`: `GET /api/scores/updates/{epochDay}/{hourOfDay}/{interval}`
- `txline_scores_stat_validation`: `GET /api/scores/stat-validation`

Live streams stay under `start_txline`:

- `GET /api/odds/stream`
- `GET /api/scores/stream`

## Rules

- Rust owns live TxLINE credentials and network calls.
- Data calls use `Authorization: Bearer <guest JWT>` plus `X-Api-Token`.
- Generic `fetch_txline` is allowlisted to documented GET data/proof endpoints only.
- Mock and replay modes are disabled at the command boundary; live TxLINE is the only app ingest mode.
- Emit status events whenever ingestion connects, stops, or fails.
