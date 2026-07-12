# ui/app/components/

Shared React presentational components used across the Agent Desk pages.

## Rules

- Components here accept **props only** — no direct Tauri calls, no `useAgentDesk`.
- Side-effect hooks (`useState` for local UI state, `useEffect` for scroll/focus) are fine.
- Data-fetching and global state belong in `ui/app/hooks/` or `ui/apps/<page>/`.

## Key components

| Component | Purpose |
|-----------|---------|
| `ChatPanel.tsx` | Primary surface: full-height conversation with the agent (welcome state, auto-scroll, typing indicator) |
| `ChatMessage.tsx` | Renders one ChatItem as a bubble — agent text, tool-call cards, signal/position/settlement cards |
| `ChatInput.tsx` | Bottom input bar with quick-action chips; free text routes through `useAgentDesk.sendChat` |
| `WalletLogin.tsx` | Phantom wallet auth gate shown before the app picker mounts |
| `AppPicker.tsx` | App launcher shown after wallet auth; reachable again from the chat's "← Apps" back button |
| `AgentDashboard.tsx` | Legacy panel displaying an agent run's bids, delivery, verdict, and settlement timeline |

## Adding a component

1. Create `<ComponentName>.tsx` in this folder.
2. Export a single default React component.
3. Keep the prop interface in the same file unless it is shared — then move it to `ui/types.ts`.
4. Write a story or test alongside the component if it has non-trivial rendering logic.
