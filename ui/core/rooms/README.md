# ui/core/rooms/

CoralOS room-session helpers — session creation, message threading, and round lifecycle.

## Key types (from `ui/types.ts`)

| Type | Purpose |
|------|---------|
| `CoralSession` | One fixture × track session with a stable `threadId` |
| `CoralMessage` | A single round-trip message between agents: `from`, `to[]`, `verb`, `text`, `payload` |
| `CoralVerb` | The full set of message verbs (`OBSERVED`, `SIGNAL`, `VERIFIED`, `SETTLED`, …) |

## Responsibilities

- Construct `CoralSession` metadata from a fixture and track selection.
- Derive chronological message threads from a flat list of `CoralMessage` entries.
- Map `CoralVerb` values to human-readable labels and UI state (e.g. which verbs indicate completion).

## Notes

Room sessions are created by the Rust side (`native/src/services/`). The TypeScript layer subscribes to session and message events via `ui/desktop/transport` and renders the thread.
