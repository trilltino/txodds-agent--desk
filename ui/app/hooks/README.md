# ui/app/hooks/

App-wide React hooks that bridge Tauri IPC with the component tree.

## Hooks

| Hook | Purpose |
|------|---------|
| `useAgentDesk.ts` | Central state aggregator: subscribes to TxLINE events, manages agent run lifecycle (bids → delivery → verdict → settlement), exposes a stable API to page components |

## Design rules

- Hooks here call `desktop/transport` functions (Tauri `invoke` / `listen`).
- They return plain objects or tuples — never JSX.
- Internal `useEffect` calls clean up their listeners on unmount.
- Each hook has a single clear responsibility; compose multiple hooks in `App.tsx` rather than nesting them.

## Testing

Hooks that depend on Tauri IPC are not unit-tested in `tests/core/`; instead they are covered indirectly by the e2e pipeline tests which exercise the pure functions those hooks delegate to. Component-level hook tests can be added with `@testing-library/react` when needed.
