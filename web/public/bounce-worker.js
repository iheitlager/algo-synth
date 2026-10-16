// A bounce's engine in a Web Worker (#705). Like worklet.js it holds no music
// logic: it instantiates dsp.wasm at the bounce's rate, loads the song text,
// sends it the samples, zones, pads and user tables the app's engine holds (as
// the worklet's own messages, web/src/audio/resources.ts), then steps the
// bounce in Rust and hands the WAV back. Served from public/, as the worklet is.

// Blocks a step: about a second of audio at 48 kHz between progress reports.
const STEP = 384
// `ZoneField.LoopStart` and `LoopEnd` (params.ts): frames at the engine's rate.
const LOOP_FIELDS = [9, 10]

let w = null

const bytesInto = (ptr, bytes) => new Uint8Array(w.memory.buffer, ptr, bytes.byteLength).set(new Uint8Array(bytes))

// One of the app's messages to its worklet, as worklet.js handles it. `scale`
// turns the app's frames into this engine's, for a zone's loop points.
function apply(data, scale) {
  switch (data.t) {
    case 'sample': {
      const ptr = w.sample_buf(data.bytes.byteLength)
      if (ptr) {
        bytesInto(ptr, data.bytes)
        w.sample_load(data.slot)
      }
      break
    }
    case 'zone': w.zone_set(data.s, data.zone, data.field, LOOP_FIELDS.includes(data.field) ? Math.round(data.v * scale) : data.v); break
    case 'zoneSets':
      w.zones_clear(data.s)
      for (let i = 0; i + 2 < data.sets.length; i += 3) {
        const field = data.sets[i + 1]
        const v = data.sets[i + 2]
        w.zone_set(data.s, data.sets[i], field, LOOP_FIELDS.includes(field) ? Math.round(v * scale) : v)
      }
      w.zones_settle(data.s)
      break
    case 'pad': w.pad_set(data.s, data.pad, data.field, data.v); break
    case 'padSets':
      w.pads_clear(data.s)
      for (let i = 0; i + 2 < data.sets.length; i += 3) w.pad_set(data.s, data.sets[i], data.sets[i + 1], data.sets[i + 2])
      w.pads_settle(data.s)
      break
    case 'userTable':
    case 'userAttack': {
      const values = new Float32Array(data.values)
      const ptr = w.user_buf(values.length)
      if (!ptr) break
      new Float32Array(w.memory.buffer, ptr, values.length).set(values)
      if (data.t === 'userTable') w.table_load(data.slot)
      else w.attack_load_at(data.slot, data.root, data.rate)
      break
    }
  }
}

// Cancel is the app terminating this worker.
onmessage = ({ data }) => {
  if (data.t !== 'bounce') return
  // { module, rate, appRate, song: ArrayBuffer, resources: Msg[], bars, bits }
  w = new WebAssembly.Instance(data.module, {}).exports
  w.init(data.rate)
  const ptr = w.song_buf(data.song.byteLength)
  if (!ptr) return postMessage({ t: 'error', why: 'the song is too long' })
  bytesInto(ptr, data.song)
  if (w.song_load() !== 0) return postMessage({ t: 'error', why: `the song does not parse (line ${w.song_error_line()})` })
  const scale = data.rate / data.appRate
  for (const m of data.resources) apply(m, scale)
  const end = w.bounce_start(data.bars, data.bits)
  if (end < 0) {
    const why = { [-2]: 'the song is too long to bounce (over 1 GiB)', [-3]: 'not enough memory for the file', [-4]: 'no such bit depth' }
    return postMessage({ t: 'error', why: why[end] ?? `the engine refused (${end})` })
  }
  while (!w.bounce_done()) {
    const frames = w.bounce_step(STEP)
    postMessage({ t: 'progress', frames, end })
  }
  const wav = new Uint8Array(w.memory.buffer, w.bounce_wav_ptr(), w.bounce_wav_len()).slice()
  postMessage({ t: 'done', wav: wav.buffer, peak: w.bounce_peak(), frames: w.bounce_step(0), end }, [wav.buffer])
}
