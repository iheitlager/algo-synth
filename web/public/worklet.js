// The AudioWorklet shim (ADR-0001). It holds no music logic: it instantiates
// dsp.wasm, forwards messages to the C ABI and copies each rendered block out.
// Served from public/, outside the bundler: a worklet module is loaded by URL.

class EngineProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super()
    // The worklet scope can't fetch, so the main thread compiles the module
    // and passes it in (web/src/audio/engine.ts).
    const instance = new WebAssembly.Instance(options.processorOptions.module, {})
    this.w = instance.exports
    this.w.init(sampleRate)
    this.block = this.w.block_len()
    this.port.onmessage = ({ data }) => {
      switch (data.t) {
        case 'param': this.w.set_param(data.id, data.v); break
        case 'on': this.w.note_on(data.s, data.n, data.v); break
        case 'off': this.w.note_off(data.s, data.n); break
        case 'panic': this.w.all_off(); break
      }
    }
  }

  process(_inputs, outputs) {
    const out = outputs[0]
    const frames = out[0].length
    this.w.process(frames)
    // Rebuild the view every block: it goes stale if wasm memory grows.
    const buf = new Float32Array(this.w.memory.buffer, this.w.out_ptr(), 2 * this.block)
    out[0].set(buf.subarray(0, frames))
    if (out[1]) out[1].set(buf.subarray(this.block, this.block + frames))
    return true
  }
}

registerProcessor('algo-synth', EngineProcessor)
