// The AudioWorklet shim (ADR-0001). It holds no music logic: it instantiates
// dsp.wasm, forwards messages to the C ABI and copies each rendered block out.
// Served from public/, outside the bundler: a worklet module is loaded by URL.

// Report the playhead about every 20 ms (8 blocks at 48 kHz).
const POSITION_EVERY = 8
// Report the DSP load about twice a second (188 blocks at 48 kHz).
const LOAD_EVERY = 188
// Some worklet scopes lack performance.now(); Date.now() only ticks in
// milliseconds, so then only the average over many blocks means anything.
const precise = typeof globalThis.performance?.now === 'function'
const now = precise ? () => globalThis.performance.now() : () => Date.now()

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
    this.busy = 0
    this.peak = 0
    this.blocks = 0
    for (let s = 0; s < this.w.strip_count(); s++) this.sendParams(s)
    this.port.onmessage = ({ data }) => {
      const w = this.w
      switch (data.t) {
        case 'param': w.set_param(data.s, data.id, data.v); break
        case 'on': w.note_on(data.s, data.n, data.v); break
        case 'off': w.note_off(data.s, data.n); break
        case 'panic': w.all_off(); break
        case 'midi': this.loadMidi(new Uint8Array(data.bytes)); break
        case 'sysex': this.loadSysex(new Uint8Array(data.bytes)); break
        case 'sysexApply': w.sysex_apply(data.s, data.i); this.sendParams(data.s); break
        case 'play': w.play(); break
        case 'stop': w.stop(); break
        case 'seek': w.seek(data.sec); break
        case 'route': w.route(data.ch, data.s); break
        case 'preset': w.mono_preset(data.s, data.id); this.sendParams(data.s); break
        case 'reset': w.synth_reset(data.s); this.sendParams(data.s); break
        case 'dump': this.sendParams(data.s); break
        case 'sample': this.loadSample(data.slot, new Uint8Array(data.bytes)); break
        case 'sampleClear':
          w.sample_clear(data.slot)
          this.port.postMessage({ t: 'sampleCleared', slot: data.slot, used: w.sample_used() })
          break
        case 'zone': w.zone_set(data.s, data.zone, data.field, data.v); break
        case 'zonesClear': w.zones_clear(data.s); break
        case 'zonesDump': this.sendZones(data.s); break
      }
    }
  }

  // Every parameter's current value on synth `s`, indexed by id, so the view
  // shows what the engine holds (at start, after a preset or a reset).
  sendParams(s) {
    const values = new Float32Array(this.w.param_count())
    for (let id = 0; id < values.length; id++) values[id] = this.w.param_value(s, id)
    this.port.postMessage({ t: 'params', s, values }, [values.buffer])
  }

  // Copy a WAV file into the engine's buffer and have it parsed and resampled into `slot`;
  // the summary and the waveform's peaks come back (the view only draws them).
  loadSample(slot, bytes) {
    const w = this.w
    const ptr = w.sample_buf(bytes.length)
    if (!ptr) {
      this.port.postMessage({ t: 'sample', slot, code: -6 })
      return
    }
    new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
    const code = w.sample_load(slot)
    if (code < 0) {
      this.port.postMessage({ t: 'sample', slot, code })
      return
    }
    const n = w.sample_peaks(slot, 512)
    const peaks = new Float32Array(w.memory.buffer, w.peaks_ptr(), n).slice()
    this.port.postMessage(
      {
        t: 'sample', slot, code, frames: code, root: w.sample_root(slot),
        loopStart: w.sample_loop(slot, 0), loopEnd: w.sample_loop(slot, 1),
        used: w.sample_used(), cap: w.sample_cap(), peaks,
      },
      [peaks.buffer],
    )
  }

  // Every field of every zone of synth `s`, zone by zone.
  sendZones(s) {
    const zones = this.w.zone_count()
    const fields = this.w.zone_fields()
    const values = new Float32Array(zones * fields)
    for (let z = 0; z < zones; z++) for (let f = 0; f < fields; f++) values[z * fields + f] = this.w.zone_get(s, z, f)
    this.port.postMessage({ t: 'zones', s, values }, [values.buffer])
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
        synth: w.routed(ch),
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

  // The same for a DX7 SysEx file: the voices' names come back as bytes.
  loadSysex(bytes) {
    const w = this.w
    const ptr = w.sysex_buf(bytes.length)
    if (!ptr) {
      this.port.postMessage({ t: 'sysex', code: -6 })
      return
    }
    new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
    const code = w.sysex_load()
    if (code < 0) {
      this.port.postMessage({ t: 'sysex', code })
      return
    }
    const names = []
    for (let i = 0; i < code; i++) {
      names.push(new Uint8Array(w.memory.buffer, w.sysex_name_ptr(i), w.sysex_name_len(i)).slice())
    }
    this.port.postMessage({ t: 'sysex', code, names })
  }

  process(_inputs, outputs) {
    const start = now()
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
      // The meters hold the highest level since the last read.
      const levels = new Float32Array(w.memory.buffer, w.meters_ptr(), w.meters_len()).slice()
      w.meters_clear()
      this.port.postMessage({ t: 'meters', levels }, [levels.buffer])
    }
    this.measure(now() - start, frames)
    return true
  }

  // Time spent in this callback as a share of the block's real time.
  measure(ms, frames) {
    const budget = (1000 * frames) / sampleRate
    this.busy += ms
    this.peak = Math.max(this.peak, ms / budget)
    if (++this.blocks < LOAD_EVERY) return
    this.port.postMessage({
      t: 'load',
      load: this.busy / (this.blocks * budget),
      peak: precise ? this.peak : null,
      voices: this.w.active_voices(),
      reduction: this.w.gain_reduction_db(),
    })
    this.busy = 0
    this.peak = 0
    this.blocks = 0
  }
}

registerProcessor('algo-synth', EngineProcessor)
