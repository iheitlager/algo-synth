// The Spectral Lab's analysis worker (ADR-0017). It holds no logic: it
// instantiates the dsp.wasm the page compiled, copies a WAV in, calls the
// analysis and the resynthesis through the C ABI, and copies their results
// out, so a long analysis never stalls the page or the audio.
// Served from public/, outside the bundler, like deck-worker.js.

let w = null

// The lab's transforms (spec 010 Req 4), by id, before a render or a table.
const setEdits = (edits) => (edits ?? []).forEach((v, id) => w.spectral_set(id, v))

// A copy of the original's (0) or the resynthesis's (1) spectrogram bytes.
const spectrogram = (which) => new Uint8Array(w.memory.buffer, w.spectral_gram_ptr(which), w.spectral_gram_len(which)).slice()

self.onmessage = ({ data }) => {
  switch (data.t) {
    case 'init':
      w = new WebAssembly.Instance(data.module, {}).exports
      self.postMessage({ t: 'ready' })
      break
    case 'analyse': {
      const bytes = new Uint8Array(data.bytes)
      const ptr = w.spectral_buf(bytes.length)
      if (!ptr) {
        self.postMessage({ t: 'analysed', code: -6 })
        break
      }
      new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
      const code = w.spectral_analyse(data.rate, data.window, data.hop, data.noise ? 1 : 0)
      const tracks = code < 0 ? new Float32Array(0)
        : new Float32Array(w.memory.buffer, w.spectral_tracks_ptr(), w.spectral_tracks_len()).slice()
      const gram = code < 0 ? new Uint8Array(0) : spectrogram(0)
      self.postMessage({ t: 'analysed', code, tracks, gram, bands: w.spectral_bands() }, [tracks.buffer, gram.buffer])
      break
    }
    // The analysed sound as a PPG wavetable, or its attack as a D-50 PCM sample (spec 010 Req 9-10).
    case 'table':
    case 'attack': {
      setEdits(data.edits)
      const n = data.t === 'table' ? w.spectral_table() : w.spectral_attack()
      const values = new Float32Array(w.memory.buffer, w.spectral_values_ptr(), n).slice()
      self.postMessage({ t: data.t, values, root: w.spectral_root() }, [values.buffer])
      break
    }
    case 'render': {
      setEdits(data.edits)
      const len = w.spectral_render(data.top, data.shift, data.stretch)
      const wav = new Uint8Array(w.memory.buffer, w.spectral_wav_ptr(), len).slice()
      const gram = spectrogram(1)
      self.postMessage({ t: 'rendered', id: data.id, wav: wav.buffer, gram }, [wav.buffer, gram.buffer])
      break
    }
  }
}
