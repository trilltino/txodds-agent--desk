set shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"]
set dotenv-load := true

alias start := desktop
alias dev   := desktop
alias d     := desktop
alias c     := check
alias b     := build

# ── RUSTFLAGS ─────────────────────────────────────────────────────────────────
# -C target-cpu=native: let LLVM use every instruction the local CPU supports.
# Applied to dev/check/test; NOT applied to tauri-build (release profile handles
# opt-level/LTO itself and native-cpu binaries shouldn't be shipped).
RUSTFLAGS_DEV := "-C target-cpu=native"

# ── CARGO_INCREMENTAL ─────────────────────────────────────────────────────────
# 1 for dev/check/test (fast iteration), 0 for release (profile already sets it).
CARGO_INC_DEV := "1"
CARGO_INC_REL := "0"

default:
    @just --list

# ── Setup ─────────────────────────────────────────────────────────────────────

# Install JS dependencies.
install:
    npm install

# Create .env from the example when one does not exist.
init-env:
    if (-not (Test-Path -LiteralPath '.env')) { Copy-Item -LiteralPath '.env.example' -Destination '.env'; Write-Host 'created .env from .env.example' } else { Write-Host '.env already exists' }

# First-run setup for a fresh checkout.
setup: install init-env prepare-sidecars

# ── Dev ───────────────────────────────────────────────────────────────────────

# Start the Tauri desktop app in dev mode (hot-reload webview + Rust rebuild).
# Depends on coralos-up so the CoralOS Console is live on every launch.
desktop: coralos-up
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; \
    $env:RUSTFLAGS = "{{RUSTFLAGS_DEV}}"; \
    $env:CARGO_INCREMENTAL = "{{CARGO_INC_DEV}}"; \
    cmd /c "cd /d native && npx tauri dev 2>&1" | ForEach-Object { if ($_ -notmatch 'BeforeDevCommand|BeforeBuildCommand') { Write-Host $_ } }

# Bring up the CoralOS Console server (single container) and wait for health.
# Idempotent: re-running is a no-op when the container is already healthy.
# When the Docker daemon is down but Docker Desktop is installed, launches it
# and waits (up to 90 s) for the daemon; skips silently only when Docker is
# genuinely unavailable so the desktop app still launches without it.
# Opens the Console page in the default browser once the server is healthy.
coralos-up:
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) { Write-Host '[coralos] docker not found on PATH; skipping Console bootstrap'; exit 0 }; \
    docker info *> $null; \
    if ($LASTEXITCODE -ne 0) { \
      $dd = Join-Path $env:ProgramFiles 'Docker\Docker\Docker Desktop.exe'; \
      if (-not (Test-Path $dd)) { Write-Host '[coralos] Docker daemon not running and Docker Desktop not found; skipping Console'; exit 0 }; \
      Write-Host '[coralos] Docker daemon not running - starting Docker Desktop (waiting up to 90s)...'; \
      Start-Process $dd; \
      $deadline = (Get-Date).AddSeconds(90); \
      do { Start-Sleep -Seconds 3; docker info *> $null } while ($LASTEXITCODE -ne 0 -and (Get-Date) -lt $deadline); \
      if ($LASTEXITCODE -ne 0) { Write-Host '[coralos] Docker daemon did not become ready in 90s; skipping Console (app runs without it)'; exit 0 }; \
      Write-Host '[coralos] Docker daemon is up' \
    }; \
    Write-Host '[coralos] starting coral-server (docker compose)...'; \
    docker compose -f docker-compose.coralos.yml up -d --wait *> $null; \
    if ($LASTEXITCODE -ne 0) { Write-Host '[coralos] Console unavailable (Docker not ready); continuing without it' } else { Write-Host '[coralos] Console ready at http://localhost:5555/ui/console'; Start-Process 'http://localhost:5555/ui/console' }


# Stop and remove the CoralOS Console server container.
coralos-down:
    docker compose -f docker-compose.coralos.yml down


# Prepare bundled sidecar runtime files used by Tauri builds.
prepare-sidecars:
    npm run prepare:sidecars

# Mint free TxLINE World Cup credentials (guest JWT + API token) into .env.
# Defaults to devnet level 1 (60s delay); pass --network mainnet --level 12 for real-time.
txline-onboard *ARGS:
    node tooling/txline-onboard.mjs {{ARGS}}

# ── Standalone agents ─────────────────────────────────────────────────────────
# Run one of the crates/agents/* binaries directly (outside the Tauri app),
# e.g. for local testing of a single agent's LLM/rig behavior.

run-agent-match-intelligence *ARGS:
    cargo run -p match-intelligence -- {{ARGS}}

run-agent-contrarian *ARGS:
    cargo run -p contrarian -- {{ARGS}}

run-agent-arena-coordinator *ARGS:
    cargo run -p arena-coordinator -- {{ARGS}}

run-agent-sharp-movement-detector *ARGS:
    cargo run -p sharp-movement-detector -- {{ARGS}}

# ── Build ─────────────────────────────────────────────────────────────────────

# Build the webview assets and prepare sidecars.
build:
    npm run build:desktop

# Build the optimised, packaged Tauri app/installer (uses release profile).
tauri-build:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; \
    $env:CARGO_INCREMENTAL = "{{CARGO_INC_REL}}"; \
    cmd /c "cd /d native && npx tauri build"

# ── Quality gates ─────────────────────────────────────────────────────────────

# Typecheck the React/TypeScript frontend.
typecheck:
    npm run lint:types

# Check the entire Rust workspace (no codegen). Fast pre-commit gate.
rust-check:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; \
    $env:RUSTFLAGS = "{{RUSTFLAGS_DEV}}"; \
    $env:CARGO_INCREMENTAL = "{{CARGO_INC_DEV}}"; \
    cargo check --workspace

# Run Clippy across the entire Rust workspace.
clippy:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; \
    $env:RUSTFLAGS = "{{RUSTFLAGS_DEV}}"; \
    $env:CARGO_INCREMENTAL = "{{CARGO_INC_DEV}}"; \
    cargo clippy --workspace --all-targets -- -D warnings

# Run all Rust workspace tests.
test:
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"; \
    $env:RUSTFLAGS = "{{RUSTFLAGS_DEV}}"; \
    $env:CARGO_INCREMENTAL = "{{CARGO_INC_DEV}}"; \
    cargo test --workspace

# Syntax-check Node sidecar entrypoints.
sidecars-check:
    node --check runtime\sidecars\coralos-bridge.mjs
    node --check runtime\sidecars\txoracle-validation-bridge.mjs
    node --check runtime\sidecars\yellowstone-bridge.mjs

# Verify sidecar-bundled npm deps (protobufjs, grpc, yellowstone-grpc, etc.) are installed.
check-bundle-deps:
    npm run check:bundle-deps

# Run the full local verification set (typecheck + rust-check + sidecars + bundle deps).
check: typecheck rust-check sidecars-check check-bundle-deps

# ── Housekeeping ──────────────────────────────────────────────────────────────

# Remove generated build output but keep installed dependencies.
clean:
    if (Test-Path -LiteralPath 'dist')                     { Remove-Item -LiteralPath 'dist'                     -Recurse -Force }
    if (Test-Path -LiteralPath 'runtime\sidecars\bin')     { Remove-Item -LiteralPath 'runtime\sidecars\bin'     -Recurse -Force }

# Remove generated output, local JS dependencies, AND the Rust target directory.
clean-all: clean
    if (Test-Path -LiteralPath 'node_modules') { Remove-Item -LiteralPath 'node_modules' -Recurse -Force }
    if (Test-Path -LiteralPath 'target')       { Remove-Item -LiteralPath 'target'       -Recurse -Force }

# ── Git ───────────────────────────────────────────────────────────────────────

# Show git branch state.
status:
    git status --short --branch

# Push the current main branch.
push:
    git push origin main
