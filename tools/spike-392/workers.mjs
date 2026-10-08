// Spike #392 Q3 (first pass, Node): N engines in worker threads, and a mixer
// on this thread taking one block from each ring every 2.667 ms, as the
// AudioWorklet would. Counts underruns and checks the decks stay aligned.
import { readFileSync } from 'node:fs'
import { cpus } from 'node:os'
import { Worker } from 'node:worker_threads'
import { parseIds, SR } from './common.mjs'
import { BLOCK, SLOTS, makeRing, view } from './ring.mjs'

const ids = parseIds(readFileSync(new URL('../../web/src/audio/params.ts', import.meta.url), 'utf8'))
const bytes = readFileSync(new URL('../../web/public/dsp.wasm', import.meta.url))
const SECONDS = Number(process.env.SECONDS ?? 6)
const PERIOD = (BLOCK / SR) * 1e9 // ns

async function run(n, ahead, stress) {
  const burners = Array.from({ length: stress }, () => new Worker(new URL('./stress.mjs', import.meta.url)))
  const rings = Array.from({ length: n }, makeRing)
  const workers = rings.map((ring) => new Worker(new URL('./worker.mjs', import.meta.url), { workerData: { bytes, ids, kind: 'heavy', ahead, ring } }))
  await Promise.all(workers.map((w) => new Promise((r) => w.once('message', r))))
  const views = rings.map(view)
  // Let them fill.
  await new Promise((r) => setTimeout(r, 300))
  const mix = new Float32Array(2 * BLOCK)
  let underruns = 0, misaligned = 0, block = 0
  const total = Math.round((SECONDS * SR) / BLOCK)
  let next = process.hrtime.bigint()
  while (block < total) {
    next += BigInt(Math.round(PERIOD))
    while (process.hrtime.bigint() < next);
    // One block each, or silence for a deck that isn't ready (the read
    // counter still advances so the deck stays on the same block number).
    mix.fill(0)
    let missed = false
    for (const v of views) {
      if (Atomics.load(v.ctrl, 0) <= block) { missed = true; continue }
      const slot = block % SLOTS
      if (v.seq[slot] !== block) misaligned++
      const src = v.audio.subarray(slot * 2 * BLOCK, (slot + 1) * 2 * BLOCK)
      for (let i = 0; i < src.length; i++) mix[i] += src[i]
    }
    if (missed) underruns++
    block++
    for (const v of views) { Atomics.store(v.ctrl, 1, block); Atomics.notify(v.ctrl, 1) }
  }
  for (const v of views) { Atomics.store(v.ctrl, 1, -1); Atomics.notify(v.ctrl, 1) }
  await Promise.all(workers.map((w) => w.terminate()))
  await Promise.all(burners.map((w) => w.terminate()))
  return { underruns, total, misaligned }
}

const cores = cpus().length
console.log(`${cpus()[0]?.model} (${cores} cores) · heavy engines (64 voices each) · ${SECONDS}s per run`)
for (const stress of (process.env.STRESS ?? `0,${cores}`).split(",").map(Number))
  for (const n of [2, 4])
    for (const ahead of [2, 4, 8]) {
      const r = await run(n, ahead, stress)
      const ms = ((ahead * BLOCK) / SR * 1000).toFixed(1)
      console.log(`stress ${String(stress).padStart(2)}  decks ${n}  ahead ${ahead} (+${ms} ms)  underruns ${r.underruns}/${r.total}  misaligned ${r.misaligned}`)
    }
