// The Spectral Lab's analysis worker (ADR-0017). It holds no logic: it
// instantiates the dsp.wasm the page compiled, copies a WAV in, calls the
// analysis and the resynthesis through the C ABI, and copies their results
// out, so a long analysis never stalls the page or the audio.
// Served from public/, outside the bundler, like deck-worker.js.

let w = null

// The lab's transforms (spec 010 Req 4) and morph (Req 7), by id, before a render or a table.
const setEdits = (edits, morph) => {
  (edits ?? []).forEach((v, id) => w.spectral_set(id, v))
  ;(morph ?? []).forEach((v, id) => w.spectral_morph(id, v))
}

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
      // As A, or as B, the sound to morph with (spec 010 Req 7).
      const analyse = data.which === 1 ? w.spectral_analyse_other : w.spectral_analyse
      const code = analyse(data.rate, data.window, data.hop, data.noise ? 1 : 0)
      if (data.which === 1) {
        self.postMessage({ t: 'analysed', code, tracks: new Float32Array(0), gram: new Uint8Array(0), bands: w.spectral_bands() })
        break
      }
      const tracks = code < 0 ? new Float32Array(0)
        : new Float32Array(w.memory.buffer, w.spectral_tracks_ptr(), w.spectral_tracks_len()).slice()
      const gram = code < 0 ? new Uint8Array(0) : spectrogram(0)
      self.postMessage({ t: 'analysed', code, tracks, gram, bands: w.spectral_bands() }, [tracks.buffer, gram.buffer])
      break
    }
    // A sound the engine makes (a preset at a note, Plaits macros; spec 010 Req 8), as A or B.
    case 'generate': {
      const text = new TextEncoder().encode(data.source)
      const ptr = w.spectral_text_buf(text.length)
      let code = -1
      if (ptr) {
        new Uint8Array(w.memory.buffer, ptr, text.length).set(text)
        code = w.spectral_generate(data.which, data.rate, data.window, data.hop, data.noise ? 1 : 0)
      }
      const tracks = code < 0 || data.which === 1 ? new Float32Array(0)
        : new Float32Array(w.memory.buffer, w.spectral_tracks_ptr(), w.spectral_tracks_len()).slice()
      const gram = code < 0 || data.which === 1 ? new Uint8Array(0) : spectrogram(0)
      self.postMessage({ t: 'analysed', code, tracks, gram, bands: w.spectral_bands(), root: w.spectral_root() }, [tracks.buffer, gram.buffer])
      break
    }
    case 'clearOther':
      w.spectral_other_clear()
      self.postMessage({ t: 'cleared' })
      break
    // The analysed sound as a PPG wavetable, or its attack as a D-50 PCM sample (spec 010 Req 9-10).
    case 'table':
    case 'attack': {
      setEdits(data.edits, data.morph)
      const n = data.t === 'table' ? w.spectral_table() : w.spectral_attack()
      const values = new Float32Array(w.memory.buffer, w.spectral_values_ptr(), n).slice()
      self.postMessage({ t: data.t, values, root: w.spectral_root() }, [values.buffer])
      break
    }
    // The recipe line (ADR-0033) of the table (kind 0) or attack (1) as the lab has it now.
    case 'recipe': {
      const text = new TextEncoder().encode(data.source)
      const ptr = w.spectral_text_buf(text.length)
      let line = ''
      if (ptr) {
        new Uint8Array(w.memory.buffer, ptr, text.length).set(text)
        setEdits(data.edits, data.morph)
        const n = w.spectral_recipe(data.kind, data.slot)
        line = new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.spectral_recipe_ptr(), n))
      }
      self.postMessage({ t: 'recipe', line })
      break
    }
    case 'render': {
      setEdits(data.edits, data.morph)
      const len = w.spectral_render(data.top, data.shift, data.stretch)
      const wav = new Uint8Array(w.memory.buffer, w.spectral_wav_ptr(), len).slice()
      const gram = spectrogram(1)
      self.postMessage({ t: 'rendered', id: data.id, wav: wav.buffer, gram }, [wav.buffer, gram.buffer])
      break
    }
  }
}
