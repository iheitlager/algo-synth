// Spike #392: the AudioWorklet in two modes.
// 'inline': N engines render here, on the audio thread (today's model, scaled).
// 'rings':  N engines render in workers; this only mixes their rings.
// Reports load (time in process / block period), underruns and misalignment.
const BLOCK = 128, SLOTS = 16
const now = typeof globalThis.performance?.now === 'function' ? () => performance.now() : () => Date.now()

class Mixer extends AudioWorkletProcessor {
  constructor({ processorOptions: o }) {
    super()
    this.mode = o.mode
    this.block = 0
    this.underruns = 0
    this.misaligned = 0
    this.busy = 0
    this.worst = 0
    if (o.mode === 'inline') {
      this.engines = o.setups.map(() => new WebAssembly.Instance(o.module, {}).exports)
      // The setup runs on the main thread's copy of common.mjs: here we only
      // replay its calls, recorded as [fn, ...args] (worklets can't import it).
      o.setups.forEach((calls, i) => {
        const w = this.engines[i]
        for (const [fn, ...args] of calls) {
          if (fn === '#song') {
            const t = new Uint8Array(args[0]); const p = w.song_buf(t.length)
            new Uint8Array(w.memory.buffer, p, t.length).set(t)
          } else w[fn](...args)
        }
      })
    } else {
      this.rings = o.rings.map((r) => ({ ctrl: new Int32Array(r.ctrl), audio: new Float32Array(r.audio), seq: new Int32Array(r.seq) }))
    }
    this.port.onmessage = () => this.port.postMessage({ block: this.block, underruns: this.underruns, misaligned: this.misaligned, load: this.busy / (this.block * BLOCK / sampleRate * 1000), worst: this.worst / (BLOCK / sampleRate * 1000) })
  }
  process(_, [out]) {
    const t0 = now()
    const [l, r] = out
    if (this.mode === 'inline') {
      for (const w of this.engines) {
        w.process(BLOCK)
        const o = new Float32Array(w.memory.buffer, w.out_ptr(), 2 * BLOCK)
        for (let i = 0; i < BLOCK; i++) { l[i] += o[i] * 0.25; r[i] += o[BLOCK + i] * 0.25 }
      }
    } else {
      let missed = false
      const slot = this.block % SLOTS
      for (const v of this.rings) {
        if (Atomics.load(v.ctrl, 0) <= this.block) { missed = true; continue }
        if (v.seq[slot] !== this.block) this.misaligned++
        const o = v.audio.subarray(slot * 2 * BLOCK, (slot + 1) * 2 * BLOCK)
        for (let i = 0; i < BLOCK; i++) { l[i] += o[i] * 0.25; r[i] += o[BLOCK + i] * 0.25 }
      }
      if (missed) this.underruns++
      for (const v of this.rings) { Atomics.store(v.ctrl, 1, this.block + 1); Atomics.notify(v.ctrl, 1) }
    }
    this.block++
    const dt = now() - t0
    this.busy += dt
    if (this.block > 200) this.worst = Math.max(this.worst, dt)
    return true
  }
}
registerProcessor('mixer', Mixer)
