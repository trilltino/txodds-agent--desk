# ui/app/

The shell-level React application — routing, layout, page composition, and global state hooks.

This folder contains only orchestration code. Domain logic lives in `ui/core/`; screen-level components live in `ui/apps/`.

## Structure

```
app/
├── App.tsx           # Root component — mounts Shell, injects global providers
├── components/       # Shared presentational components reused across pages
├── hooks/            # App-wide React hooks (useAgentDesk, useWallet, …)
└── navigation/       # Shell chrome: sidebar, header, active-route tracker
```

## Responsibilities

- **App.tsx** — Single entry point. Owns no domain state directly; delegates to `useAgentDesk`.
- **navigation/Shell.tsx** — Persistent sidebar + header. Driven by `UserAppPage` from `ui/types.ts`.
- **hooks/useAgentDesk.ts** — Central orchestrator hook. Aggregates TxLINE events, agent bids, run state, and settlement into one surface for page components to subscribe to.
- **components/** — Stateless or near-stateless panels (e.g. `AgentDashboard`) that receive props and render.

## Data flow

```
Tauri IPC events
      ↓
 desktop/transport (listen / invoke)
      ↓
 hooks/useAgentDesk.ts  ←── app-level state
      ↓
 App.tsx → Shell → page components
```
