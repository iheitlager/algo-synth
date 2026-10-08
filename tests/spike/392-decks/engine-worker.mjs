// Spike #392: one engine in a browser worker, rendering `ahead` blocks in
// front of the worklet mixer. Same loop as worker.mjs (Node).
import { makeEngine } from './common.mjs'
import { BLOCK, SLOTS, view } from './ring.mjs'

onmessage = ({ data: { module, ids, kind, ahead, ring } }) => {
  const w = makeEngine(module, ids, kind)
  const { ctrl, audio, seq } = view(ring)
  let written = 0
  postMessage('ready')
  for (;;) {
    const read = Atomics.load(ctrl, 1)
    if (read < 0) break
    if (written - read >= ahead) { Atomics.wait(ctrl, 1, read, 50); continue }
    w.process(BLOCK)
    const slot = written % SLOTS
    audio.set(new Float32Array(w.memory.buffer, w.out_ptr(), 2 * BLOCK), slot * 2 * BLOCK)
    seq[slot] = written
    Atomics.store(ctrl, 0, ++written)
  }
}
