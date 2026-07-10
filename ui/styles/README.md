# ui/styles/

Feature-scoped CSS modules for the Agent Desk UI.

## Files

| File | Purpose |
|------|---------|
| `agent-track.css` | Styles for the agent track panels: bid cards, timeline, verdict badges, settlement rail indicators |

## Conventions

- Global base styles live in `ui/styles.css` (imported from `ui/main.tsx`).
- Feature CSS files here are imported by the component that owns them — no global auto-import.
- Use CSS custom properties (`--color-*`, `--space-*`) defined in `ui/styles.css` for all theme values.
- Avoid utility-class proliferation; keep selectors scoped to the component's root class.
