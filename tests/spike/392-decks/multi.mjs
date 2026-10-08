// Spike #392 Q1/Q2: N engines on one thread (what one AudioWorklet thread
// would carry), and what a stopped engine costs.
import { readFileSync } from 'node:fs'
import { cpus } from 'node:os'
import { makeEngine, parseIds, SR } from './common.mjs'

const ids = parseIds(readFileSync(new URL('../../../web/src/audio/params.ts', import.meta.url), 'utf8'))
const module = await WebAssembly.compile(readFileSync(new URL('../../../web/public/dsp.wasm', import.meta.url)))
const BLOCK = 128, WARMUP = 200, BLOCKS = 3000
const realtime = (BLOCK / SR) * 1e6

function time(engines) {
  for (let i = 0; i < WARMUP; i++) for (const w of engines) w.process(BLOCK)
  const per = []
  for (let i = 0; i < BLOCKS; i++) {
    const t0 = process.hrtime.bigint()
    for (const w of engines) w.process(BLOCK)
    per.push(Number(process.hrtime.bigint() - t0) / 1000)
  }
  per.sort((a, b) => a - b)
  const mean = per.reduce((a, b) => a + b) / per.length
  return { mean, p99: per[Math.floor(per.length * 0.99)], max: per.at(-1) }
}
const fmt = (r) =>
  `mean ${r.mean.toFixed(0).padStart(5)} µs ${(100 * r.mean / realtime).toFixed(1).padStart(5)}%  p99 ${(100 * r.p99 / realtime).toFixed(1).padStart(5)}%  max ${(100 * r.max / realtime).toFixed(0).padStart(4)}%`

console.log(`${cpus()[0]?.model} · Node ${process.version} · share of one 2667 µs block`)
for (const kind of ['light', 'heavy'])
  for (const n of [1, 2, 3, 4]) {
    const r = time(Array.from({ length: n }, () => makeEngine(module, ids, kind)))
    console.log(`${kind.padEnd(5)} x${n}  ${fmt(r)}`)
  }
// Q2: an engine that was never started, and one stopped after playing (tails).
console.log(`idle (never played)      ${fmt(time([makeEngine(module, ids, 'heavy', false)]))}`)
const stopped = makeEngine(module, ids, 'heavy')
for (let i = 0; i < 400; i++) stopped.process(BLOCK)
stopped.song_stop()
console.log(`idle (stopped, tails)    ${fmt(time([stopped]))}`)
console.log(`skipped (no process call) 0 by construction`)
