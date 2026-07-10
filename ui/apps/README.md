# ui/apps/

Feature-scoped page bundles. Each sub-folder is one navigable page of the application.

## Structure

```
apps/
├── agent/    # Intelligence Agent Desk — the primary product screen
└── shared/   # Components, hooks, and utilities reused across multiple pages
```

## Conventions

- Each `apps/<page>/` folder owns its own page-level components, layout, and page-specific hooks.
- Cross-cutting concerns (auth state, wallet, global events) go in `ui/app/hooks/` or `ui/core/`.
- Shared UI primitives reused by ≥ 2 pages go in `apps/shared/`.
- Pages import from `ui/core/<domain>` but never from another page's `apps/` folder.

## Adding a new page

1. Create `apps/<page>/index.tsx` as the page entry point.
2. Register the route in `ui/app/navigation/Shell.tsx`.
3. Add the page name to `UserAppPage` in `ui/types.ts`.
