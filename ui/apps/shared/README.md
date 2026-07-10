# ui/apps/shared/

UI primitives and utilities shared across two or more page apps.

## Purpose

Keeps common building blocks DRY without coupling individual pages to each other. Components here must have no opinion about which page uses them.

## What belongs here

- Generic display components (cards, badges, status indicators, loading skeletons).
- Shared formatting utilities (odds formatting, SOL formatting, date/time helpers).
- Reusable layout primitives (panels, grids, section headers).

## What does NOT belong here

- Page-specific logic → `ui/apps/<page>/`.
- Domain business logic → `ui/core/<domain>/`.
- Tauri IPC calls → `ui/desktop/transport.ts`.
