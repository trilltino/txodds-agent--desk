# tooling

Developer and build helper scripts live here.

## Files

- `prepare-sidecars.mjs`: copies the local Windows Node runtime into `runtime/sidecars/bin/node.exe` for Tauri bundling. Run via `npm run prepare:sidecars` / `just prepare-sidecars`.
- `check-bundle-deps.mjs`: verifies that npm packages required by the bundled sidecars (`protobufjs`, `@grpc/grpc-js`, `@grpc/proto-loader`, `@triton-one/yellowstone-grpc`, and their transitive deps) are present in `node_modules`. Run via `npm run check:bundle-deps` / `just check-bundle-deps`; part of `just check`.
- `txline-onboard.mjs`: mints free TxLINE World Cup credentials (guest JWT + API token) into `.env`. Run via `npm run txline:onboard` / `just txline-onboard`.

## Rules

- Keep one-off build helpers here instead of mixing them into app runtime directories.
- Tooling may create ignored/generated files, but should not mutate source code unexpectedly.
