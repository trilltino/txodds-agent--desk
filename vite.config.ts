/// <reference types="vitest" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'url'

const isDebug = !!process.env.TAURI_ENV_DEBUG
const isWindows = process.env.TAURI_ENV_PLATFORM === 'windows'

// Vite is only the internal Tauri webview asset server/bundler. Live TxLINE
// and txoracle validation all go through Rust/sidecars.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/native/**', '**/target/**', '**/.git/**'] }
  },
  // Only VITE_/TAURI_ prefixed variables may enter frontend code. Secrets in
  // .env intentionally use non-VITE names so they stay Rust-side.
  envPrefix: ['VITE_', 'TAURI_'],
  // ── Vitest configuration ──────────────────────────────────────────────────
  // Tests run under Node (no browser, no Tauri IPC). Files that touch
  // `desktop/transport.ts` must mock the native bridge — see
  // `tests/__mocks__/transport.ts`.
  test: {
    environment: 'node',
    globals: false,
    include: ['tests/**/*.test.ts', 'tests/**/*.test.tsx'],
    exclude: ['**/node_modules/**', '**/target/**'],
    // Redirect any import of the native bridge to the stub module so tests
    // run under Node without a Tauri runtime. The alias only applies during
    // `vitest` runs — the real transport is used in the dev/prod builds.
    //
    // A regex alias is used so both the bare specifier "ui/desktop/transport"
    // AND relative paths ("../../../ui/desktop/transport") are intercepted —
    // plain string aliases only match the exact specifier string.
    alias: [
      {
        find: /ui\/desktop\/transport(\.ts)?$/,
        replacement: fileURLToPath(new URL('./tests/__mocks__/transport.ts', import.meta.url)),
      },
    ],
    coverage: {
      provider: 'v8',
      include: ['ui/core/**/*.ts', 'ui/core/**/*.tsx'],
      exclude: ['ui/core/**/*.d.ts'],
      reporter: ['text', 'lcov'],
      reportsDirectory: './coverage'
    }
  },

  build: {
    // Target the Chromium version embedded in the Tauri webview on each OS.
    target: isWindows ? 'chrome105' : 'safari13',
    // esbuild minifier is fastest; disabled in debug for readable stack traces.
    minify: isDebug ? false : 'esbuild',
    sourcemap: isDebug,
    rollupOptions: {
      output: {
        // Split vendor code from app code so incremental rebuilds are cheaper
        // and the Tauri webview can cache third-party assets longer.
        manualChunks: (id) => {
          if (id.includes('node_modules/react') || id.includes('node_modules/react-dom')) {
            return 'vendor-react'
          }
          if (id.includes('node_modules/@tauri-apps')) {
            return 'vendor-tauri'
          }
          if (id.includes('node_modules/')) {
            return 'vendor'
          }
        }
      }
    }
  }
})
