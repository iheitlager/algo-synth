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
// Bars ahead of the playhead the deck lane shows (#449, as in worklet.js).
const LANE_BARS = 8

let w = null
let ring = null
let block = 0
let written = 0
// A cued start: the frame on the worklet's clock it falls on, and the master's tempo.
let start = null
// The master's next bar line, as a frame, for sync lock; and whether to report the sync.
let barAt = null
let synced = false

// From the worklet, directly (ADR-0029): cues, the master's tempo and bar lines.
function fromWorklet({ data }) {
  switch (data.t) {
    case 'playAt': start = { at: data.at, into: data.into ?? 0, bpm: data.bpm }; break
    case 'tempo': w.tempo(data.bpm); break
    case 'barAt': barAt = data.at; break
  }
}

onmessage = ({ data }) => {
  switch (data.t) {
    case 'init': {
      data.port.onmessage = fromWorklet
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
      // Its scenes and their order, for the deck lane's map ahead (#449).
      const text = new TextDecoder()
      const scenes = []
      for (let i = 0; ok && i < w.song_scenes(); i++) {
        scenes.push({ name: text.decode(new Uint8Array(w.memory.buffer, w.scene_name_ptr(i), w.scene_name_len(i))), bars: w.scene_bars(i) })
      }
      const arrange = ok ? Array.from({ length: w.arrange_len() }, (_, i) => w.arrange_at(i)) : []
      postMessage({ t: 'song', ok, line: ok ? 0 : w.song_error_line(), col: ok ? 0 : w.song_error_col(), scenes, arrange })
      break
    }
    case 'stop': start = null; w.song_stop(); break
  }
}

// The cued start falls in the block about to render: at the master's tempo,
// on its exact sample, from the top or as far into its first bar as the
// master is (#450). A cue that arrived too late starts at once, that much
// further in so it stays in phase, and says so.
function startNow() {
  const offset = start.at - written * block
  w.song_stop()
  w.tempo(start.bpm)
  w.song_play_in_bar(Math.max(0, offset), start.into + Math.max(0, -offset))
  if (offset < 0) postMessage({ t: 'late', frames: -offset })
  start = null
}

// The master's bar line falls in the block about to render: the engine pulls
// this song's nearest bar line onto it if it is off. A late one is skipped.
function syncNow() {
  const offset = barAt - written * block
  if (offset >= 0) {
    w.song_sync_bar_in(offset)
    synced = true
  }
  barAt = null
}

// Render until the ring is AHEAD blocks in front of the worklet, then wait for
// it to read one. The wait gives the event loop a turn, so messages arrive.
function pump() {
  const slots = ring.seq.length
  // Stalled for over a second (a suspended tab, say): rather than render the
  // whole gap flat out, the deck jumps to now and slips once.
  if (Atomics.load(ring.ctrl, READ) - written > CATCH_UP) written = Atomics.load(ring.ctrl, READ) + 1
  while (written - Atomics.load(ring.ctrl, READ) < AHEAD) {
    if (start && start.at < (written + 1) * block) startNow()
    if (barAt !== null && barAt < (written + 1) * block) syncNow()
    w.process(block)
    if (synced) {
      synced = false
      if (w.song_playing()) postMessage({ t: 'sync', frames: w.sync_error() })
    }
    const slot = written % slots
    ring.audio.set(new Float32Array(w.memory.buffer, w.out_ptr(), 2 * block), slot * 2 * block)
    ring.seq[slot] = written
    Atomics.store(ring.ctrl, WRITE, ++written)
    if (written % POSITION_EVERY === 0) {
      const step = w.clock_step()
      postMessage({ t: 'pos', step, playing: w.song_playing() === 1, entry: w.song_entry(), ahead: ahead(step) })
    }
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

// The arrangement entry of the playing bar and the bars after it (from bar 0
// while stopped), for the deck lane (#449); −1 where there is none.
function ahead(step) {
  const from = step >= 0 ? Math.floor(step / 16) : 0
  return { from, entries: Array.from({ length: LANE_BARS }, (_, i) => w.song_bar_entry(from + i)) }
}
