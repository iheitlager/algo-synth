// A deck's engine in a Web Worker (ADR-0029). Like worklet.js it holds no music
// logic: it instantiates dsp.wasm, forwards messages to the C ABI and copies
// each rendered block into the ring the worklet reads, a few blocks ahead.
// Served from public/, outside the bundler, as the worklet is.
//
// The ring (web/src/audio/decks.ts makes it): `ctrl` holds the next block this
// worker writes and the next block the worklet reads, both counted on the
// worklet's clock; `audio` holds SLOTS planar stereo blocks; `seq` says which
// block each slot holds. Blocks are rendered in order and never skipped, so a
// late deck stays on its timeline: the worklet drops a block that comes too
// late, and the next one lands on time.

// Blocks rendered ahead of the worklet: 4 × 128 samples, 10.7 ms at 48 kHz (#392).
const AHEAD = 4
const WRITE = 0
const READ = 1
// How often the playhead goes to the view, in blocks (about 20 ms).
const POSITION_EVERY = 8
// Blocks a deck may fall behind and still catch up in order (about 1 s).
const CATCH_UP = 375

let w = null
let ring = null
let block = 0
let written = 0

onmessage = ({ data }) => {
  switch (data.t) {
    case 'init': {
      w = new WebAssembly.Instance(data.module, {}).exports
      w.init(data.sampleRate)
      block = w.block_len()
      ring = { ctrl: new Int32Array(data.ring.ctrl), audio: new Float32Array(data.ring.audio), seq: new Int32Array(data.ring.seq) }
      written = Atomics.load(ring.ctrl, READ) + 1
      postMessage({ t: 'ready' })
      pump()
      break
    }
    case 'song': {
      const ptr = w.song_buf(data.bytes.byteLength)
      if (!ptr) {
        postMessage({ t: 'song', ok: false, line: 0, col: 0 })
        break
      }
      new Uint8Array(w.memory.buffer, ptr, data.bytes.byteLength).set(new Uint8Array(data.bytes))
      const ok = w.song_load() === 0
      postMessage({ t: 'song', ok, line: ok ? 0 : w.song_error_line(), col: ok ? 0 : w.song_error_col() })
      break
    }
    case 'play': w.song_play(); break
    case 'stop': w.song_stop(); break
  }
}

// Render until the ring is AHEAD blocks in front of the worklet, then wait for
// it to read one. The wait gives the event loop a turn, so messages arrive.
function pump() {
  const slots = ring.seq.length
  // Stalled for over a second (a suspended tab, say): rather than render the
  // whole gap flat out, the deck jumps to now and slips once.
  if (Atomics.load(ring.ctrl, READ) - written > CATCH_UP) written = Atomics.load(ring.ctrl, READ) + 1
  while (written - Atomics.load(ring.ctrl, READ) < AHEAD) {
    w.process(block)
    const slot = written % slots
    ring.audio.set(new Float32Array(w.memory.buffer, w.out_ptr(), 2 * block), slot * 2 * block)
    ring.seq[slot] = written
    Atomics.store(ring.ctrl, WRITE, ++written)
    if (written % POSITION_EVERY === 0) postMessage({ t: 'pos', step: w.clock_step(), playing: w.song_playing() === 1 })
  }
  const read = Atomics.load(ring.ctrl, READ)
  if (typeof Atomics.waitAsync === 'function') {
    const r = Atomics.waitAsync(ring.ctrl, READ, read, 50)
    if (r.async) r.value.then(pump)
    else setTimeout(pump, 0)
  } else {
    setTimeout(pump, 1)
  }
}
