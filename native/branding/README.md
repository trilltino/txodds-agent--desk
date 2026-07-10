# branding

Branding assets for the desktop package live here.

## Contents

- `icon-1024.png`: source image used to generate Tauri icon assets under `src-tauri/icons/`.

## Rules

- Keep source brand assets here instead of mixing them with generated Tauri icon output.
- Regenerate platform icons with `npx tauri icon branding/icon-1024.png` when this source icon changes.
- Do not place runtime UI images here unless they are part of the app identity or installer branding.
