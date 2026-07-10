# tests/core/

Unit tests that mirror the `ui/core/` module tree one-for-one.

Each sub-folder corresponds to a domain layer:

| Folder | Source layer | What is tested |
|--------|-------------|----------------|
| `txline/` | `ui/core/txline/` | Raw-payload normalisation, odds-move detection, event classification |
| `coral/` | `ui/core/coral/` | Bid scoring algorithm, winner selection |
| `agent/` | `ui/core/agent/` | Leaderboard shape invariants, arena position contracts |
| `auth/` | `ui/app/hooks/useWalletAuth.ts` | `AuthChallenge` shape, `UserProfile` shape, transport stubs, signature encoding |

## Adding a new test file

1. Create `tests/core/<domain>/<file>.test.ts` matching the source path.
2. Import only from `vitest` (no globals) and from `../__helpers__/fixtures`.
3. Keep each `describe` block focused on one exported symbol.
4. Pure functions — no mocks needed. Functions touching `transport` — use `vi.mocked(...)`.
5. Add a `README.md` to the new sub-folder documenting what is and is not covered.
