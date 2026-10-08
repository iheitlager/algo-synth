// Spike #392: one engine in a worker, rendering `ahead` blocks in front of
// the mixer into its ring. Sleeps on the read counter when the ring is full.
import { parentPort, workerData } from 'node:worker_threads'
import { makeEngine } from './common.mjs'
import { BLOCK, SLOTS, view } from './ring.mjs'

const { bytes, ids, kind, ahead, ring } = workerData
const w = makeEngine(new WebAssembly.Module(bytes), ids, kind)
const { ctrl, audio, seq } = view(ring)
let written = 0
parentPort.postMessage('ready')
for (;;) {
  const read = Atomics.load(ctrl, 1)
  if (read < 0) break
  if (written - read >= ahead) {
    Atomics.wait(ctrl, 1, read, 50)
    continue
  }
  w.process(BLOCK)
  const slot = written % SLOTS
  audio.set(new Float32Array(w.memory.buffer, w.out_ptr(), 2 * BLOCK), slot * 2 * BLOCK)
  seq[slot] = written
  Atomics.store(ctrl, 0, ++written)
}
