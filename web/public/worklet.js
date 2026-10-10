// The AudioWorklet shim (ADR-0001). It holds no music logic: it instantiates
// dsp.wasm, forwards messages to the C ABI and copies each rendered block out.
// Served from public/, outside the bundler: a worklet module is loaded by URL.

// Report the playhead about every 20 ms (8 blocks at 48 kHz).
const POSITION_EVERY = 8
// Report the DSP load about twice a second (188 blocks at 48 kHz).
const LOAD_EVERY = 188
// How often a hand's changes are folded into the song (ADR-0027): about
// five times a second, so a knob drag gives a few song updates, not hundreds.
const FOLD_EVERY = 75
// How far ahead a deck's start is cued, at least: time for the cue to reach its
// worker before it renders that block (16 blocks, 43 ms at 48 kHz; ADR-0029).
const CUE_MARGIN = 16
// Bars ahead of the playhead the deck lane shows (#449).
const LANE_BARS = 8
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
    // How many models this engine knows, so the view can tell when dsp.wasm is older than it is
    // (an engine without `model_count` is older than any).
    // The same for its version and the commit it was built from (#197); 0 when there is none to tell.
    const num = (name) => (typeof this.w[name] === 'function' ? this.w[name]() : 0)
    this.port.postMessage({ t: 'ready', models: num('model_count'), version: num('version_code'), build: num('build_id') })
    this.block = this.w.block_len()
    // The block number every deck's ring counts in, and the attached decks by number (1–3).
    this.blockNo = 0
    this.decks = new Map()
    // The master's next bar line already announced to synced decks, as a frame; and its tempo as last sent.
    this.nextBar = -Infinity
    this.deckBpm = 0
    this.tick = 0
    this.busy = 0
    this.peak = 0
    this.blocks = 0
    // While the view has an edit of the song text not yet applied, folding
    // waits, so a knob never overwrites what is being typed (ADR-0027).
    this.foldHeld = false
    for (let s = 0; s < this.w.strip_count(); s++) this.sendParams(s)
    this.port.onmessage = ({ data }) => {
      const w = this.w
      switch (data.t) {
        case 'param': w.set_param(data.s, data.id, data.v); break
        case 'on': w.note_on(data.s, data.n, data.v); break
        case 'off': w.note_off(data.s, data.n); break
        // A controller's message as it came, and the synth its keys play (#10).
        case 'midiIn': w.midi_in(data.b[0], data.b[1], data.b[2]); break
        case 'midiTarget': w.midi_target(data.s); break
        case 'panic': w.all_off(); break
        case 'foldHold': this.foldHeld = !!data.on; break
        // Every synth on screen is a song track (ADR-0027).
        case 'trackAdd': w.track_add(data.s, data.preset); this.sendSong(true); break
        case 'trackRemove': w.track_remove(data.s); this.sendSong(true); break
        case 'clear':
          // Start over (#325): every strip's values and the empty song go back to the view.
          w.engine_clear()
          for (let s = 0; s < w.strip_count(); s++) this.sendParams(s)
          this.sendSong(true)
          break
        case 'midi': this.importMidi(new Uint8Array(data.bytes)); break
        case 'sysex': this.loadSysex(new Uint8Array(data.bytes)); break
        case 'sysexApply': w.sysex_apply(data.s, data.i); this.sendParams(data.s); break
        case 'preset': w.mono_preset(data.s, data.id); this.sendParams(data.s); break
        case 'reset': w.synth_reset(data.s); this.sendParams(data.s); break
        case 'defaults': w.synth_defaults(data.s); break
        case 'dump': this.sendParams(data.s); break
        case 'sample': this.loadSample(data.slot, new Uint8Array(data.bytes)); break
        case 'sampleClear':
          w.sample_clear(data.slot)
          this.port.postMessage({ t: 'sampleCleared', slot: data.slot, used: w.sample_used() })
          break
        case 'zone': w.zone_set(data.s, data.zone, data.field, data.v); break
        case 'zonesClear': w.zones_clear(data.s); break
        case 'zonesDump': this.sendZones(data.s); break
        case 'song': this.loadSong(new Uint8Array(data.bytes), data.id); break
        case 'code': this.setCode(data.s, new Uint8Array(data.bytes)); break
        case 'samples': this.setSamples(data.s, new Uint8Array(data.bytes)); break
        case 'groupName': this.setGroupName(data.g, new Uint8Array(data.bytes)); break
        case 'step': w.set_step(data.f, data.l, data.s, data.level); this.sendSong(true); break
        case 'ratchet': w.set_ratchet(data.f, data.l, data.s, data.r); this.sendSong(true); break
        // A track's mute and solo (#355): bit 0 mute, bit 1 solo.
        case 'trackFlags': w.set_track_flags(data.track, data.flags); this.sendSong(true); break
        case 'note':
          // op 0 adds a sixteenth at tick, 1 removes the note, 2 sets its length in ticks.
          if (data.op === 0) w.note_add(data.f, data.tick, data.note)
          else if (data.op === 1) w.note_remove(data.f, data.tick, data.note)
          else w.note_len(data.f, data.tick, data.note, data.len)
          this.sendSong(true)
          break
        case 'freeze': w.freeze(data.f); this.sendSong(true); break
        case 'songRoute': w.song_route(data.track, data.s); this.sendSong(true); break
        case 'songTempo': w.song_tempo(data.v); this.sendSong(true); break
        case 'songSwing': w.song_swing(data.v); this.sendSong(true); break
        case 'songDump': this.sendSong(true); break
        case 'arr': w.arr_edit(data.op, data.a ?? 0, data.b ?? 0, data.c ?? 0); this.sendSong(true); break
        case 'track': {
          // A preset or setting picked in the composer is set on the track's synth (#213).
          const ok = w.track_edit(data.op, data.track, data.a ?? 0) === 0
          this.sendSong(true)
          const s = w.song_routed(data.track)
          if (ok && s < 255) this.sendParams(s)
          break
        }
        case 'songSeek': w.song_seek_bar(data.bar); break
        case 'mixWrite': w.song_write_mixer(); this.sendSong(true); break
        // Decks B–D (ADR-0029): a worker renders each into a ring; this only copies blocks.
        case 'deckAttach': this.attachDeck(data.deck, data.ring, data.port); break
        case 'deckDetach': this.decks.delete(data.deck); break
        case 'deckSet': w.deck_set(data.deck, data.field, data.v); break
        case 'deckXfade': w.deck_crossfade(data.x); break
        case 'deckCue': this.cueDeck(data.deck, data.every); break
        case 'deckSync': { const r = this.decks.get(data.deck); if (r) r.sync = !!data.on; break }
        case 'songPlay': w.song_play(); break
        case 'songPause': w.song_pause(); break
        case 'songStop': w.song_stop(); break
        case 'songCue': w.song_cue(data.f); break
        case 'pad': w.pad_set(data.s, data.pad, data.field, data.v); break
        case 'padsClear': w.pads_clear(data.s); break
        case 'padsDump': this.sendPads(data.s); break
      }
    }
  }

  // Every parameter's current value on synth `s`, indexed by id, so the view
  // shows what the engine holds (at start, after a preset or a reset).
  // A synth's parameters, and a Modular synth's code (ADR-0024) with the
  // last edit's error, if any: a preset or a song changes both.
  sendParams(s, error = null) {
    const w = this.w
    const values = new Float32Array(w.param_count())
    for (let id = 0; id < values.length; id++) values[id] = w.param_value(s, id)
    this.port.postMessage({ t: 'params', s, values }, [values.buffer])
    const len = w.code_text ? w.code_text(s) : 0
    if (len || error) {
      const text = new Uint8Array(w.memory.buffer, w.code_text_ptr(), len).slice()
      this.port.postMessage({ t: 'code', s, text, error })
      // Its knobs, one per number of the code (#329).
      const n = w.knob_list ? w.knob_list(s) : 0
      const knobs = new Uint8Array(w.memory.buffer, n ? w.knob_list_ptr() : 0, n).slice()
      this.port.postMessage({ t: 'knobs', s, knobs })
    }
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

  // Every field of every pad of synth `s`, pad by pad.
  sendPads(s) {
    const pads = this.w.pad_count()
    const fields = this.w.pad_fields()
    const values = new Float32Array(pads * fields)
    for (let p = 0; p < pads; p++) for (let f = 0; f < fields; f++) values[p * fields + f] = this.w.pad_get(s, p, f)
    this.port.postMessage({ t: 'pads', s, values }, [values.buffer])
  }

  // Every field of every zone of synth `s`, zone by zone.
  sendZones(s) {
    const zones = this.w.zone_count()
    const fields = this.w.zone_fields()
    const values = new Float32Array(zones * fields)
    for (let z = 0; z < zones; z++) for (let f = 0; f < fields; f++) values[z * fields + f] = this.w.zone_get(s, z, f)
    this.port.postMessage({ t: 'zones', s, values }, [values.buffer])
  }

  // Copy the file into the engine's buffer and import it as the song there
  // (ADR-0022); the song comes back as any song does.
  importMidi(bytes) {
    const w = this.w
    const ptr = w.midi_buf(bytes.length)
    if (!ptr) {
      this.port.postMessage({ t: 'imported', code: -6 })
      return
    }
    new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
    const code = w.midi_import()
    // The import set each part's patch on its synth (#327): show them.
    if (code >= 0) for (let s = 0; s < w.strip_count(); s++) this.sendParams(s)
    this.sendSong(code >= 0)
    this.port.postMessage({ t: 'imported', code })
  }

  // Copy the song's text into the engine and have it parsed there; the
  // summary below comes back whether it played or not.
  // A Modular synth's SynthDef from its faceplate (ADR-0024): the engine
  // builds it, or keeps the synth as it was and says where it went wrong.
  setCode(s, bytes) {
    const w = this.w
    const ptr = w.song_buf(bytes.length)
    if (!ptr) return
    new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
    const ok = w.code_set(s) === 0
    const msg = (p, n) => new Uint8Array(w.memory.buffer, p, n).slice()
    const error = ok ? null : { line: w.code_error_line(), col: w.code_error_col(), msg: msg(w.code_error_ptr(), w.code_error_len()) }
    this.sendParams(s, error)
  }

  // A pack or kit picked on synth `s` (#214): its id becomes the track's `samples` line.
  setSamples(s, bytes) {
    const w = this.w
    const ptr = w.song_buf(bytes.length)
    if (!ptr) return
    new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
    if (w.samples_set(s) === 0) this.sendSong(true)
  }

  // A group bus named in the view (#214): the song's `group` line takes the name; none takes it away.
  setGroupName(g, bytes) {
    const w = this.w
    const ptr = w.song_buf(bytes.length)
    if (bytes.length && !ptr) return
    if (bytes.length) new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
    if (w.group_name_set(g) === 0) this.sendSong(true)
  }

  // The reply carries the load's `id`, so the view knows which text answers it (#465).
  loadSong(bytes, id) {
    const w = this.w
    const ptr = w.song_buf(bytes.length)
    if (!ptr) {
      this.port.postMessage({ t: 'song', ok: false, tooLong: true, id })
      return
    }
    new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
    const ok = w.song_load() === 0
    this.sendSong(ok, id)
    // A track's preset or setting (#210) and the mixer lines (ADR-0018) may have
    // just been set: every strip and group the view shows follows.
    if (ok) for (let s = 0; s < w.strip_count(); s++) this.sendParams(s)
  }

  // The song as the engine holds it: its printed text, the tracks and their
  // synths, every fragment's lanes and steps, and the last load's error.
  // Text travels as bytes (the worklet has no TextDecoder).
  sendSong(ok, id) {
    const w = this.w
    const bytes = (ptr, len) => new Uint8Array(w.memory.buffer, ptr, len).slice()
    const tracks = []
    for (let t = 0; t < w.song_tracks(); t++) {
      tracks.push({
        name: bytes(w.track_name_ptr(t), w.track_name_len(t)), synth: w.song_routed(t), kind: w.track_kind(t),
        flags: w.track_flags ? w.track_flags(t) : 0,
        preset: w.track_preset(t), setting: w.track_setting(t),
      })
    }
    const frags = []
    for (let f = 0; f < w.song_frags(); f++) {
      const lanes = []
      for (let l = 0; l < w.frag_lanes(f); l++) {
        const steps = new Uint8Array(w.lane_steps(f, l))
        for (let s = 0; s < steps.length; s++) steps[s] = w.step_level(f, l, s)
        // How often each step plays in its span (#242): 1 from an engine without ratchets.
        const ratchets = new Uint8Array(steps.length).fill(1)
        if (w.step_ratchet) for (let s = 0; s < steps.length; s++) ratchets[s] = w.step_ratchet(f, l, s)
        lanes.push({ pad: w.lane_pad(f, l), steps, ratchets })
      }
      // Its steps to a bar (#353): 16 from an engine that has no other.
      const grid = w.frag_grid ? w.frag_grid(f) : 16
      let notes = null
      if (w.frag_notes_len(f)) {
        // Each note as [start, length, note, accent]; the start and length are in ticks, 48 to a bar.
        const events = []
        for (let k = 0; k < w.frag_events(f); k++) {
          events.push([w.event_start(f, k), w.event_len(f, k), w.event_note(f, k), w.event_accent(f, k)])
        }
        notes = {
          text: bytes(w.frag_notes_ptr(f), w.frag_notes_len(f)), bars: w.frag_bars(f),
          live: w.frag_live(f) === 1, generated: w.frag_generated(f) === 1, events,
        }
      }
      frags.push({ name: bytes(w.frag_name_ptr(f), w.frag_name_len(f)), track: w.frag_track(f), lanes, grid, notes })
    }
    // The arrangement (ADR-0015): sections with what each holds, the order, lanes, scenes, loop.
    const nF = w.song_frags(), nA = w.song_autos(), nC = w.song_scenes()
    const has = (s, kind, n) => Array.from({ length: n }, (_, i) => w.section_has(s, kind, i) === 1)
    const sections = []
    for (let s = 0; s < w.song_sections(); s++) {
      sections.push({
        name: bytes(w.section_name_ptr(s), w.section_name_len(s)), bars: w.section_bars(s),
        frags: has(s, 0, nF), autos: has(s, 1, nA), scenes: has(s, 2, nC),
      })
    }
    const arrange = Array.from({ length: w.arrange_len() }, (_, i) => w.arrange_at(i))
    const autos = Array.from({ length: nA }, (_, a) => bytes(w.auto_name_ptr(a), w.auto_name_len(a)))
    const scenes = Array.from({ length: nC }, (_, c) => bytes(w.scene_name_ptr(c), w.scene_name_len(c)))
    // The song's own settings (#210) and, per track kind, the models that play it (#213).
    const settings = Array.from({ length: w.song_settings() }, (_, i) => ({
      name: bytes(w.setting_name_ptr(i), w.setting_name_len(i)), preset: w.setting_preset(i),
    }))
    const models = w.model_count ? w.model_count() : 0
    const fits = [0, 1, 2].map((k) => Array.from({ length: models }, (_, m) => w.model_fits(k, m) === 1))
    const error = ok
      ? null
      : { line: w.song_error_line(), col: w.song_error_col(), msg: bytes(w.song_error_ptr(), w.song_error_len()) }
    // The group buses' names in the song (#214), empty for none.
    const groups = Array.from({ length: w.group_name_len ? 8 : 0 }, (_, g) => bytes(w.group_name_ptr(g), w.group_name_len(g)))
    // The samples each track wants (#214): the view fetches and loads them.
    const samples = Array.from({ length: w.samples_count ? w.samples_count() : 0 }, (_, i) => ({
      synth: w.samples_synth(i), id: bytes(w.samples_id_ptr(i), w.samples_id_len(i)),
    }))
    this.port.postMessage({
      t: 'song', ok, text: bytes(w.song_text_ptr(), w.song_text_len()), error, tracks, frags,
      tempo: w.clock_tempo(), swing: w.clock_swing(),
      sections, arrange, autos, scenes, settings, fits, loop: [w.loop_from(), w.loop_to()], samples, groups, id,
    })
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
    if (this.decks.size) {
      this.syncDecks()
      this.feedDecks()
    }
    w.process(frames)
    if (this.decks.size) this.readDecks()
    this.blockNo++
    // Rebuild the view every block: it goes stale if wasm memory grows.
    const buf = new Float32Array(w.memory.buffer, w.out_ptr(), 2 * this.block)
    out[0].set(buf.subarray(0, frames))
    if (out[1]) out[1].set(buf.subarray(this.block, this.block + frames))
    if (this.tick % FOLD_EVERY === 0) {
      if (!this.foldHeld && w.song_fold && w.song_fold()) this.sendSong(true)
      // What of each synth's sound the song can't hold yet (#361): sent when it changes.
      if (w.live_only) {
        const live = []
        for (let s = 0; s < 16; s++) live.push(w.live_only(s))
        const key = live.join(',')
        if (key !== this.liveKey) {
          this.liveKey = key
          this.port.postMessage({ t: 'liveOnly', bits: live })
        }
      }
    }
    if (++this.tick % POSITION_EVERY === 0) {
      // The words of the song text playing now (#205), as [start, len] pairs.
      const lit = []
      const n = w.lit_count ? w.lit_count() : 0
      if (n) {
        const raw = new Uint32Array(w.memory.buffer, w.lit_ptr(), n * 2)
        for (let i = 0; i < n; i++) lit.push([raw[i * 2], raw[i * 2 + 1]])
      }
      const step = w.clock_step()
      this.port.postMessage({
        t: 'pos', step, songPlaying: w.song_playing() === 1,
        entry: w.song_entry(), local: w.song_local(), cued: w.song_cued(), lit, ahead: ahead(w, step),
      })
      // Automation or a Revision switch moved these strips' values: show them (ADR-0015, #343).
      const touched = w.auto_touched()
      for (let s = 0; s < 32; s++) if ((touched >>> s) & 1) this.sendParams(s)
      // The knobs a modulation drives (ADR-0019): sent when the set changes.
      const mods = []
      for (let i = 0; i < w.mod_count(); i++) mods.push(w.mod_strip(i) * 1024 + w.mod_param(i))
      const key = mods.join(',')
      if (key !== this.modKey) {
        this.modKey = key
        this.port.postMessage({ t: 'mods', keys: mods })
      }
      // A song loaded while playing took over on the bar line (#208): show it.
      if (w.song_taken()) {
        this.sendSong(true)
        for (let s = 0; s < w.strip_count(); s++) this.sendParams(s)
      }
      // Deck A's level goes to the deck lane with or without other decks (#449).
      this.sendDecks()
      // The meters hold the highest level since the last read.
      const levels = new Float32Array(w.memory.buffer, w.meters_ptr(), w.meters_len()).slice()
      w.meters_clear()
      this.port.postMessage({ t: 'meters', levels }, [levels.buffer])
    }
    this.measure(now() - start, frames)
    return true
  }

  // A deck's ring, and its port: this worklet talks to the deck's worker
  // directly, so cues, tempo and bar lines never wait on the main thread.
  attachDeck(deck, r, port) {
    const ring = {
      ctrl: new Int32Array(r.ctrl), audio: new Float32Array(r.audio), seq: new Int32Array(r.seq),
      live: false, dropped: 0, port, sync: true,
    }
    Atomics.store(ring.ctrl, 1, this.blockNo)
    this.decks.set(deck, ring)
    port.postMessage({ t: 'tempo', bpm: this.w.clock_tempo() })
    this.port.postMessage({ t: 'deckAttached', deck })
  }

  // Hand each deck's block for this block number to the engine, if it is
  // there in time; a late one is dropped and counted (ADR-0029).
  feedDecks() {
    const w = this.w
    const size = 2 * this.block
    for (const [deck, r] of this.decks) {
      const slot = this.blockNo % r.seq.length
      if (Atomics.load(r.ctrl, 0) > this.blockNo && r.seq[slot] === this.blockNo) {
        new Float32Array(w.memory.buffer, w.deck_in_ptr(deck), size).set(r.audio.subarray(slot * size, (slot + 1) * size))
        w.deck_fed(deck)
        r.live = true
      } else if (r.live) {
        r.dropped++
      }
    }
  }

  // Where deck `deck` starts: the master's next multiple of `every` steps
  // (16 a bar, 128 a phrase) far enough ahead, or right after the margin when
  // `every` is 0 or the song is stopped; as a frame on this worklet's clock.
  // Started right after the margin, it starts as far into its bar as the
  // master is then, so the two are in phase from its first bar (#450).
  cueDeck(deck, every) {
    const w = this.w
    const margin = CUE_MARGIN * this.block
    const f = every > 0 ? w.cue_frames(every, margin) : -1
    const at = this.blockNo * this.block + (f < 0 ? margin : f)
    const into = f < 0 ? Math.max(0, w.cue_into(margin)) : 0
    this.decks.get(deck)?.port.postMessage({ t: 'playAt', at, into, bpm: w.clock_tempo() })
  }

  // Sync lock (ADR-0029): each of the master's bar lines, announced to the
  // synced decks at least the margin ahead; and the master's tempo when it moves.
  syncDecks() {
    const w = this.w
    const bpm = w.clock_tempo()
    if (bpm !== this.deckBpm) {
      this.deckBpm = bpm
      for (const r of this.decks.values()) r.port.postMessage({ t: 'tempo', bpm })
    }
    const now = this.blockNo * this.block
    const margin = CUE_MARGIN * this.block
    // One bar line at a time: the next goes out once the worklet has passed the
    // last, which the worker, rendering ahead, has applied by then.
    if (now < this.nextBar) return
    const f = w.cue_frames(16, margin)
    if (f < 0) {
      this.nextBar = -Infinity
      return
    }
    this.nextBar = now + f
    for (const r of this.decks.values()) if (r.sync) r.port.postMessage({ t: 'barAt', at: this.nextBar })
  }

  // This block is read: each worker may render one more.
  readDecks() {
    for (const r of this.decks.values()) {
      Atomics.store(r.ctrl, 1, this.blockNo + 1)
      Atomics.notify(r.ctrl, 1)
    }
  }

  sendDecks() {
    const w = this.w
    const peaks = [0, 1, 2, 3].map((d) => w.deck_peak(d))
    const dropped = [0, 1, 2, 3].map((d) => this.decks.get(d)?.dropped ?? 0)
    this.port.postMessage({ t: 'decks', peaks, dropped, bpm: this.deckBpm })
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

// The arrangement entry of the playing bar and the bars after it (from bar 0
// while stopped), for the deck lane (#449); −1 where there is none.
function ahead(w, step) {
  const from = step >= 0 ? Math.floor(step / 16) : 0
  return { from, entries: Array.from({ length: LANE_BARS }, (_, i) => w.song_bar_entry(from + i)) }
}

registerProcessor('algo-synth', EngineProcessor)
