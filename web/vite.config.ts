/// <reference types="vitest/config" />
import { readdirSync, readFileSync } from 'node:fs'
import { extname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig, type Plugin } from 'vite'
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

// examples/ (#20): the example songs and the scores, at /examples/ in the app. The build copies
// them into dist (which algo-synth serve serves); Vite's own server serves them from the source.
const EXAMPLES = fileURLToPath(new URL('../examples', import.meta.url))
const exampleFiles = (sub: string) => {
  try {
    return readdirSync(join(EXAMPLES, sub)).filter((f) => !f.startsWith('.')).sort()
  } catch {
    return []
  }
}
function examples(): Plugin {
  const all = () => ['songs', 'scores'].flatMap((sub) => exampleFiles(sub).map((f) => `${sub}/${f}`))
  return {
    name: 'algo-examples',
    buildStart() {
      for (const f of all()) this.addWatchFile(join(EXAMPLES, f))
    },
    generateBundle() {
      for (const f of all()) this.emitFile({ type: 'asset', fileName: `examples/${f}`, source: readFileSync(join(EXAMPLES, f)) })
    },
    configureServer(server) {
      server.middlewares.use('/examples/', (req, res, next) => {
        const f = decodeURIComponent((req.url ?? '').split('?')[0]).replace(/^\//, '')
        if (!all().includes(f)) return next()
        res.setHeader('Content-Type', extname(f) === '.mid' ? 'audio/midi' : 'text/plain; charset=utf-8')
        res.end(readFileSync(join(EXAMPLES, f)))
      })
    },
  }
}

// Cross-origin isolated, so decks can share SharedArrayBuffer rings (ADR-0029); algo-synth serve sends the same.
const ISOLATED = { 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp' }

export default defineConfig({
  plugins: [vue(), examples()],
  define: {
    __EXAMPLE_SONGS__: JSON.stringify(exampleFiles('songs')),
    __APP_VERSION__: JSON.stringify(version),
    __APP_BUILD__: JSON.stringify(process.env.ALGO_BUILD_SHA || 'dev'),
    __APP_BUILT__: JSON.stringify(new Date().toISOString()),
  },
  // Two pages: the app, and the Assistant in its own window (#387).
  build: {
    target: 'es2022',
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL('index.html', import.meta.url)),
        assistant: fileURLToPath(new URL('assistant.html', import.meta.url)),
      },
    },
  },
  // The app is served by algo-synth serve (make dev 6341, make serve 6340,
  // ADR-0030). Vite's own server (6343) is for UI work only: with ASSIST_URL
  // set it proxies /api there, e.g. to tools/fake-assist.mjs.
  server: {
    port: 6343,
    strictPort: true,
    headers: ISOLATED,
    proxy: process.env.ASSIST_URL ? { '/api': { target: process.env.ASSIST_URL } } : undefined,
  },
  preview: { port: 6343, strictPort: true, headers: ISOLATED },
  // `npm run test:coverage` (make coverage-web): the summary prints, the HTML report goes to coverage/.
  test: {
    coverage: {
      provider: 'v8',
      include: ['src/**/*.ts', 'src/**/*.vue'],
      exclude: ['src/**/*.test.ts'],
      reporter: ['text-summary', 'html'],
    },
  },
})
