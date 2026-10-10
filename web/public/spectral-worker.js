// The Spectral Lab's analysis worker (ADR-0017). It holds no logic: it
// instantiates the dsp.wasm the page compiled, copies a WAV in, calls the
// analysis and the resynthesis through the C ABI, and copies their results
// out, so a long analysis never stalls the page or the audio.
// Served from public/, outside the bundler, like deck-worker.js.

let w = null

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
      const code = w.spectral_analyse(data.rate, data.window, data.hop)
      const tracks = code < 0 ? new Float32Array(0)
        : new Float32Array(w.memory.buffer, w.spectral_tracks_ptr(), w.spectral_tracks_len()).slice()
      self.postMessage({ t: 'analysed', code, tracks }, [tracks.buffer])
      break
    }
    case 'render': {
      const len = w.spectral_render(data.top, data.shift, data.stretch)
      const wav = new Uint8Array(w.memory.buffer, w.spectral_wav_ptr(), len).slice()
      self.postMessage({ t: 'rendered', id: data.id, wav: wav.buffer }, [wav.buffer])
      break
    }
  }
}
