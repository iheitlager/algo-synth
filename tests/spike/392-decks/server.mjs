// Spike #392 Q4: a static server for the browser test. Port 6392 sends the
// cross-origin isolation headers SharedArrayBuffer needs; 6393 doesn't.
import { createServer } from 'node:http'
import { readFileSync } from 'node:fs'
import { parseIds } from './common.mjs'

const here = new URL('./', import.meta.url)
const root = new URL('../../../', import.meta.url)
const types = { html: 'text/html', mjs: 'text/javascript', js: 'text/javascript', wasm: 'application/wasm', json: 'application/json' }
const ids = JSON.stringify(parseIds(readFileSync(new URL('web/src/audio/params.ts', root), 'utf8')))

function serve(port, isolate) {
  createServer((req, res) => {
    const path = new URL(req.url, 'http://x').pathname.slice(1) || 'page.html'
    try {
      const body = path === 'ids.json' ? ids : readFileSync(path === 'dsp.wasm' ? new URL('web/public/dsp.wasm', root) : new URL(path, here))
      const headers = { 'Content-Type': types[path.split('.').pop()] ?? 'application/octet-stream' }
      if (isolate) Object.assign(headers, { 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp' })
      res.writeHead(200, headers).end(body)
    } catch { res.writeHead(404).end() }
  }).listen(port, '127.0.0.1')
}
serve(6392, true)
serve(6393, false)
console.log('isolated http://localhost:6392/  not isolated http://localhost:6393/')
