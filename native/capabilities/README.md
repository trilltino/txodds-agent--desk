# src-tauri/capabilities

Tauri capability files define which frontend commands are allowed from the webview.

## Files

- `default.json`: current default command permissions.

## Rules

- Keep permissions narrow and explicit.
- Add permissions only when a feature needs them.
- Never enable broad shell or filesystem access without a scoped reason.
