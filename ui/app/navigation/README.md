# ui/app/navigation/

Navigation chrome: shell layout, sidebar, and active-page tracking.

## Files

| File | Purpose |
|------|---------|
| `Shell.tsx` | Persistent app frame — renders sidebar navigation and mounts the active `UserAppPage` panel |

## `UserAppPage` contract

Navigation is driven by the `UserAppPage` literal union in `ui/types.ts`:

```ts
export type UserAppPage = 'agent'
```

`Shell` receives the current page as a prop and renders the matching app panel from `ui/apps/`.

## Adding a navigation item

1. Extend the `UserAppPage` union in `ui/types.ts`.
2. Add the corresponding route entry in `Shell.tsx`.
3. Create the panel under `ui/apps/<page>/`.
