import { readFileSync } from 'node:fs'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// public/worklet.js and public/dsp.wasm are served as-is, outside the bundle:
// an AudioWorklet module is loaded by URL and can't import the app's chunks
// (ADR-0003).
// The version is the workspace's (Cargo.toml); the commit comes from the build (ALGO_BUILD_SHA,
// which the engine is built with too), so the page can tell when its dsp.wasm is another build (#197).
const version = (() => {
  try {
    return /^version\s*=\s*"([^"]+)"/m.exec(readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8'))?.[1] ?? 'unknown'
  } catch {
    return 'unknown'
  }
})()

export default defineConfig({
  plugins: [vue()],
  define: {
    __APP_VERSION__: JSON.stringify(version),
    __APP_BUILD__: JSON.stringify(process.env.ALGO_BUILD_SHA || 'dev'),
    __APP_BUILT__: JSON.stringify(new Date().toISOString()),
  },
  build: { target: 'es2022' },
  // 63xx range (Makefile DEV_PORT); `make serve` uses 6340.
  server: { port: 6341, strictPort: true },
  preview: { port: 6341, strictPort: true },
})
