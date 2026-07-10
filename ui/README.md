# ui

React frontend source for the Tauri webview lives here (entry point: `ui/main.tsx`, loaded from root `index.html`).

## Directories

- `app/`: webview orchestrator, global chrome, and navigation (`App.tsx`, `navigation/Shell.tsx`, shared hooks).
- `apps/`: feature-scoped page bundles — see [apps/README.md](apps/README.md).
  - `apps/agent/`: the Intelligence Agent Desk, the primary product screen.
  - `apps/shared/`: components/hooks/utilities reused across pages.
- `core/`: pure TypeScript domain types and clients, one subfolder per domain (`agent/`, `chain/`, `coral/`, `markets/`, `proof/`, `rooms/`, `txline/`, `wallet/`).
- `desktop/`: the Tauri IPC and native event boundary (`transport.ts`, `events.ts`).

## Rules

- Desktop behavior must call Rust through `desktop/transport.ts`.
- Browser rendering and direct browser network paths are blocked.
- Secrets must never be imported, rendered, or bundled into frontend code.
- Page components should consume typed events and commands from `core/`, not raw TxLINE, RPC, or Yellowstone clients.
- Pages under `apps/` import from `core/` but never from another page's `apps/` folder (see [apps/README.md](apps/README.md)).
