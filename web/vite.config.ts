import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// public/worklet.js and public/dsp.wasm are served as-is, outside the bundle:
// an AudioWorklet module is loaded by URL and can't import the app's chunks
// (ADR-0003).
export default defineConfig({
  plugins: [vue()],
  build: { target: 'es2022' },
  // 63xx range (Makefile DEV_PORT); `make serve` uses 6340.
  server: { port: 6341, strictPort: true },
  preview: { port: 6341, strictPort: true },
})
