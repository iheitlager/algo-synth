#!/usr/bin/env node
// A fake assist server (#387) for working on the Assistant without keys or a
// provider: the /api contract of ADR-0028 with a scripted stream. The song it
// proposes is the one it was sent with the tempo raised by 8 (or, sent an
// empty song, examples/four-on-the-floor.song so), which the engine parses.
//
//   node tools/fake-assist.mjs [port]      # default 6342, on 127.0.0.1
//
// A request containing "refuse" is answered 429; one containing "fail" ends in
// an error event.
import { readFileSync } from 'node:fs'
import { createServer } from 'node:http'

const port = Number(process.argv[2] ?? process.env.PORT ?? 6342)
const EXAMPLE = readFileSync(new URL('../examples/four-on-the-floor.song', import.meta.url), 'utf8')
const PROVIDERS = {
  providers: [
    { id: 'anthropic', name: 'Anthropic', models: [{ id: 'claude-opus-5-5', default: true }, { id: 'claude-sonnet-5-5', default: false }] },
    { id: 'fake', name: 'Fake', models: [{ id: 'echo', default: true }] },
  ],
  default: { provider: 'anthropic', model: 'claude-opus-5-5' },
}

const json = (res, status, body) => {
  res.writeHead(status, { 'Content-Type': 'application/json' })
  res.end(JSON.stringify(body))
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const faster = (song) => (song.trim() ? song : EXAMPLE).replace(/^tempo (\d+)/m, (_, n) => `tempo ${Number(n) + 8}`)

createServer(async (req, res) => {
  if (req.method === 'GET' && req.url === '/api/health') return json(res, 200, { ok: true })
  if (req.method === 'GET' && req.url === '/api/providers') return json(res, 200, PROVIDERS)
  if (req.method !== 'POST' || req.url !== '/api/assist') return json(res, 404, { error: 'not found' })
  let raw = ''
  for await (const chunk of req) raw += chunk
  let body
  try {
    body = JSON.parse(raw)
  } catch {
    return json(res, 400, { error: 'not JSON' })
  }
  if (String(body.request).includes('refuse')) return json(res, 429, { error: 'Too many requests: wait a minute' })
  res.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' })
  const send = async (event, data) => {
    // Split each event in two writes, so a reader sees it arrive in pieces.
    const text = `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`
    res.write(text.slice(0, 9))
    await sleep(20)
    res.write(text.slice(9))
    await sleep(150)
  }
  let closed = false
  res.on('close', () => (closed = true))
  await send('progress', { round: 1, message: 'Writing the song' })
  await send('text', { text: `Raising the tempo${body.focus ? `, looking at ${body.focus}` : ''}. ` })
  await send('text', { text: 'Checking it parses.' })
  await send('tool', { round: 1, name: 'check_song', ok: true, summary: 'parses: 2 tracks' })
  await send('tool', { round: 1, name: 'render_song', ok: true, summary: 'finite, peak -3.1 dB' })
  if (closed) return
  if (String(body.request).includes('fail')) {
    await send('error', { message: 'The model gave up after 3 rounds', retryable: true })
  } else {
    await send('song', { song: faster(String(body.song ?? '')), summary: 'The tempo is 8 BPM faster.' })
  }
  await send('done', { rounds: 1, seconds: 0.9, usage: { input: 9000, cached: 7000, output: 1500 } })
  res.end()
}).listen(port, '127.0.0.1', () => console.log(`fake assist server on http://127.0.0.1:${port}`))
