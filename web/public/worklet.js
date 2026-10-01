// The AudioWorklet shim (ADR-0001). It holds no music logic: it instantiates
// dsp.wasm, forwards messages to the C ABI and copies each rendered block out.
// Served from public/, outside the bundler: a worklet module is loaded by URL.

// Report the playhead about every 20 ms (8 blocks at 48 kHz).
const POSITION_EVERY = 8

class EngineProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super()
    // The worklet scope can't fetch, so the main thread compiles the module
    // and passes it in (web/src/audio/engine.ts).
    const instance = new WebAssembly.Instance(options.processorOptions.module, {})
    this.w = instance.exports
    this.w.init(sampleRate)
    this.block = this.w.block_len()
    this.tick = 0
    this.port.onmessage = ({ data }) => {
      const w = this.w
      switch (data.t) {
        case 'param': w.set_param(data.id, data.v); break
        case 'on': w.note_on(data.s, data.n, data.v); break
        case 'off': w.note_off(data.s, data.n); break
        case 'panic': w.all_off(); break
        case 'midi': this.loadMidi(new Uint8Array(data.bytes)); break
        case 'play': w.play(); break
        case 'stop': w.stop(); break
        case 'seek': w.seek(data.sec); break
        case 'route': w.route(data.ch, data.s); break
      }
    }
  }

  // Copy the file into the engine's buffer, parse it there, and send the
  // summary back. Names travel as bytes: the worklet has no TextDecoder.
  loadMidi(bytes) {
    const w = this.w
    const ptr = w.midi_buf(bytes.length)
    if (!ptr) {
      this.port.postMessage({ t: 'midi', code: -6 })
      return
    }
    new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
    const code = w.midi_load()
    if (code < 0) {
      this.port.postMessage({ t: 'midi', code })
      return
    }
    const parts = []
    for (let i = 0; i < code; i++) {
      const ch = w.part_channel(i)
      parts.push({
        channel: ch,
        notes: w.part_notes(i),
        start: w.part_start(i),
        end: w.part_end(i),
        name: new Uint8Array(w.memory.buffer, w.part_name_ptr(i), w.part_name_len(i)).slice(),
        source: w.routed(ch),
      })
    }
    const n = w.event_count()
    const packed = new Uint32Array(n)
    const times = new Float32Array(n)
    for (let i = 0; i < n; i++) {
      packed[i] = w.event_packed(i)
      times[i] = w.event_time(i)
    }
    this.port.postMessage(
      { t: 'midi', code, parts, packed, times, length: w.song_length(), bar: w.song_bar() },
      [packed.buffer, times.buffer],
    )
  }

  process(_inputs, outputs) {
    const w = this.w
    const out = outputs[0]
    const frames = out[0].length
    w.process(frames)
    // Rebuild the view every block: it goes stale if wasm memory grows.
    const buf = new Float32Array(w.memory.buffer, w.out_ptr(), 2 * this.block)
    out[0].set(buf.subarray(0, frames))
    if (out[1]) out[1].set(buf.subarray(this.block, this.block + frames))
    if (++this.tick % POSITION_EVERY === 0) {
      this.port.postMessage({ t: 'pos', sec: w.position(), playing: w.playing() === 1 })
    }
    return true
  }
}

registerProcessor('algo-synth', EngineProcessor)
