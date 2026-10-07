// engine.ts against a fake worklet (#232): the Web Audio and wasm globals are
// stubbed, so `power()` builds a real AudioEngine whose port records what the
// view posts, and the test plays the worklet by calling `port.onmessage`.
import { afterEach, describe, expect, it, vi } from 'vitest'
import { page } from './buildinfo'
import { Model, Param, PadField, Preset } from './params'
import { PAD_FIELDS, ZONE_FIELDS, ZONES } from './sampler'
import { buildSetup, MUTE, type Registry } from './setup'
import { GlobalParam, StripParam } from './params'

type Msg = { t: string } & Record<string, unknown>

/** An empty wasm module: enough for `wasmLexers`, which only instantiates it. */
const EMPTY_WASM = new Uint8Array([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00])

function memoryStorage(seed: Record<string, string> = {}) {
  const m = new Map(Object.entries(seed))
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
    clear: () => m.clear(),
    key: (i: number) => [...m.keys()][i] ?? null,
    get length() { return m.size },
    map: m,
  }
}

/** A fresh engine module, powered on against fakes; `posted` is what it sent the worklet. */
async function boot(opts: { storage?: Record<string, string>; fetch?: (url: string) => Promise<Response> } = {}) {
  vi.resetModules()
  const posted: Msg[] = []
  const port = {
    onmessage: null as ((e: { data: Msg }) => void) | null,
    postMessage: (msg: Msg) => void posted.push(msg),
  }
  class FakeAnalyser { connect(d: unknown) { return d } }
  class FakeNode {
    port = port
    connect(a: unknown) { return a }
  }
  class FakeContext {
    sampleRate = 48000
    destination = {}
    audioWorklet = { addModule: async () => {} }
    resume = vi.fn(async () => {})
  }
  const storage = memoryStorage(opts.storage)
  vi.stubGlobal('AudioContext', FakeContext)
  vi.stubGlobal('AudioWorkletNode', FakeNode)
  vi.stubGlobal('AnalyserNode', FakeAnalyser)
  vi.stubGlobal('localStorage', storage)
  vi.stubGlobal('fetch', vi.fn(opts.fetch ?? (async () => new Response(null, { status: 404 }))))
  vi.spyOn(WebAssembly, 'compileStreaming').mockImplementation(async () => new WebAssembly.Module(EMPTY_WASM))
  const mod = await import('./engine')
  await mod.power()
  expect(mod.status.error).toBe('')
  const names = await import('./names')
  /** Play the worklet: deliver `data` to the view. */
  const send = (data: Msg) => port.onmessage?.({ data })
  const take = () => posted.splice(0)
  return { mod, names, posted, send, take, storage }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

const enc = (s: string) => new TextEncoder().encode(s)
const ok = { tempo: 120, swing: 50, frags: [] as unknown[] }
const track = (name: string, synth: number, kind = 0) => ({ name: enc(name), synth, kind })

describe('power', () => {
  it('starts once, reports the sample rate and loads the kept song', async () => {
    const { mod, posted } = await boot({ storage: { 'algo-synth:song': 'tempo 90' } })
    expect(mod.status.running).toBe(true)
    expect(mod.status.sampleRate).toBe(48000)
    expect(posted).toHaveLength(1)
    expect(posted[0]?.t).toBe('song')
    expect(new TextDecoder().decode(posted[0]?.bytes as ArrayBuffer)).toBe('tempo 90')
    expect(mod.song.draft).toBe('tempo 90')
    // A second power resumes the context instead of building another engine.
    const ctx = mod.getEngine()?.ctx as unknown as { resume: ReturnType<typeof vi.fn> }
    await mod.power()
    expect(ctx.resume).toHaveBeenCalledOnce()
  })

  it('reports why the engine did not start', async () => {
    vi.resetModules()
    vi.stubGlobal('AudioContext', class { constructor() { throw new Error('no audio') } })
    const mod = await import('./engine')
    await mod.power()
    expect(mod.status.running).toBe(false)
    expect(mod.status.error).toBe('no audio')
  })
})

describe('applySong', () => {
  it('decodes a song that played: text, tracks, frags, arrangement', async () => {
    const { mod, storage } = await boot()
    mod.applySong({
      ok: true,
      text: enc('tempo 100\n'),
      error: null,
      tempo: 100,
      swing: 55,
      tracks: [track('kick', 0, 0), track('bass', 3, 1), track('pads', 5, 2), track('odd', MUTE, 7)],
      frags: [
        { name: enc('beat'), track: 0, lanes: [{ pad: 2, steps: new Uint8Array([1, 0, 2, 0]) }], notes: null },
        {
          name: enc('line'), track: 1, lanes: [],
          notes: { text: enc('c3 e3'), bars: 2, events: [[0, 3, 48, 1], [6, 3, 52, 0]], generated: true, live: false },
        },
      ],
      sections: [{ name: enc('intro'), bars: 4, frags: [true, false], autos: [], scenes: [] }],
      arrange: [0, 0],
      autos: [enc('cutoff')],
      scenes: [enc('dark')],
      settings: [{ name: enc('nile'), preset: Preset.MiniLead }],
      fits: [[true], [false], [false]],
      loop: [1, 2],
    })
    expect(mod.song.error).toBeNull()
    expect(mod.song.text).toBe('tempo 100\n')
    expect(mod.song.draft).toBe('tempo 100\n')
    expect(storage.map.get('algo-synth:song')).toBe('tempo 100\n')
    expect([mod.song.tempo, mod.song.swing]).toEqual([100, 55])
    expect(mod.song.tracks).toEqual([
      { name: 'kick', synth: 0, kind: 'drums', preset: -1, setting: -1, mute: false, solo: false },
      { name: 'bass', synth: 3, kind: 'synth', preset: -1, setting: -1, mute: false, solo: false },
      { name: 'pads', synth: 5, kind: 'sampler', preset: -1, setting: -1, mute: false, solo: false },
      { name: 'odd', synth: MUTE, kind: 'drums', preset: -1, setting: -1, mute: false, solo: false },
    ])
    // An engine without frag_grid sends none: the lane is 16ths (#353).
    expect(mod.song.frags[0]).toEqual({ name: 'beat', track: 0, lanes: [{ pad: 2, steps: [1, 0, 2, 0] }], grid: 16, notes: null })
    expect(mod.song.frags[1]?.notes).toEqual({
      text: 'c3 e3', bars: 2, generated: true, live: false,
      events: [{ start: 0, len: 3, note: 48, accent: true }, { start: 6, len: 3, note: 52, accent: false }],
    })
    expect(mod.song.sections).toEqual([{ name: 'intro', bars: 4, frags: [true, false], autos: [], scenes: [] }])
    expect(mod.song.arrange).toEqual([0, 0])
    expect(mod.song.autos).toEqual(['cutoff'])
    expect(mod.song.settings).toEqual([{ name: 'nile', preset: Preset.MiniLead }])
    expect(mod.song.fits).toEqual([[true], [false], [false]])
    expect(mod.song.scenes).toEqual(['dark'])
    expect(mod.song.loop).toEqual([1, 2])
  })

  it('shows the routed synths with the presets the engine put on them (#210)', async () => {
    const { mod, take } = await boot()
    take()
    mod.applySong({ ...ok, ok: true, text: enc('x'), error: null, tracks: [track('a', 0), track('b', 4), track('m', MUTE)] })
    expect(mod.synths.list).toEqual([0, 4])
    // Shown as they are: no reset, which would put synth 4 back to the default patch.
    expect(take().filter((m) => m.t === 'reset')).toEqual([])
  })

  it('names a song track\'s synth after the track, underscores as spaces (#327)', async () => {
    const { mod } = await boot()
    const { names } = await import('./names')
    names.strips[5] = 'Strings'
    mod.applySong({
      ...ok, ok: true, text: enc('x'), error: null,
      tracks: [track('kit', 0, 0), track('basso_continuo', 3, 1), track('loop', 4, 2), track('pads', 5, 1)],
    })
    // A name the user typed stays.
    expect([0, 3, 4, 5].map((s) => mod.stripName(s))).toEqual(['kit', 'basso continuo', 'loop', 'Strings'])
    // The next song renames what the last one named.
    mod.applySong({ ...ok, ok: true, text: enc('x'), error: null, tracks: [track('drums', 0, 0), track('lead', 3, 1)] })
    expect([0, 3].map((s) => mod.stripName(s))).toEqual(['drums', 'lead'])
    // Renamed by the user, it is theirs from then on.
    mod.renameSynth(3, 'My lead')
    mod.applySong({ ...ok, ok: true, text: enc('x'), error: null, tracks: [track('drums', 0, 0), track('bass', 3, 1)] })
    expect(mod.stripName(3)).toBe('My lead')
  })

  it('keeps the draft and shows the error of a song that failed', async () => {
    const { mod, storage } = await boot()
    mod.song.draft = 'my edit'
    mod.applySong({
      ...ok, ok: false, text: enc('old'), error: { line: 3, col: 7, msg: enc('unknown word') }, tracks: [track('b', 6)],
    })
    expect(mod.song.error).toEqual({ line: 3, col: 7, msg: 'unknown word' })
    expect(mod.song.draft).toBe('my edit')
    expect(mod.song.text).toBe('old')
    expect(storage.map.has('algo-synth:song')).toBe(false)
    // A failed song changes no synths.
    expect(mod.synths.list).toEqual([0])
    // Without an arrangement the fields fall back to empty.
    expect(mod.song.sections).toEqual([])
    expect(mod.song.arrange).toEqual([])
    expect(mod.song.loop).toEqual([0, 0])
  })

  it('reports a text that is too long and changes nothing else', async () => {
    const { mod } = await boot()
    mod.song.text = 'kept'
    mod.applySong({ tooLong: true })
    expect(mod.song.error).toEqual({ line: 1, col: 1, msg: 'the text is too long' })
    expect(mod.song.text).toBe('kept')
  })

  it('arrives as a `song` message', async () => {
    const { mod, send } = await boot()
    send({ t: 'song', ...ok, ok: true, text: enc('sent'), error: null, tracks: [] })
    expect(mod.song.text).toBe('sent')
  })
})

describe('the song as it is typed (ADR-0027)', () => {
  it('holds folding while typing and applies once typing rests', async () => {
    vi.useFakeTimers()
    try {
      const { mod, take } = await boot()
      mod.song.text = 'tempo 120\n'
      take()
      mod.typeSong('tempo 12')
      mod.typeSong('tempo 128\n')
      expect(take()).toEqual([{ t: 'foldHold', on: true }])
      vi.advanceTimersByTime(mod.APPLY_AFTER_MS - 1)
      expect(take()).toEqual([])
      vi.advanceTimersByTime(1)
      const sent = take()
      expect(sent.map((m) => m.t)).toEqual(['song'])
      expect(new TextDecoder().decode(sent[0]?.bytes as ArrayBuffer)).toBe('tempo 128\n')
    } finally {
      vi.useRealTimers()
    }
  })

  it('takes the canonical text back, releases folding, and never overwrites what was typed since', async () => {
    const { mod, send, take } = await boot()
    mod.song.text = 'tempo 120\n'
    mod.loadSong('tempo  128')
    take()
    // The answer to what was sent replaces the draft with its canonical text.
    send({ t: 'song', ...ok, ok: true, text: enc('tempo 128\n'), error: null, tracks: [] })
    expect(mod.song.draft).toBe('tempo 128\n')
    // Typed on, then a fold arrives: the typing stays.
    mod.typeSong('tempo 128\nswing 60\n')
    take()
    send({ t: 'song', ...ok, ok: true, text: enc('tempo 128\nstrip a: Level 0.5\n'), error: null, tracks: [] })
    expect(mod.song.draft).toBe('tempo 128\nswing 60\n')
    expect(take()).toEqual([])
    // Back to the engine's text: folding is released.
    mod.typeSong('tempo 128\nstrip a: Level 0.5\n')
    expect(take()).toEqual([{ t: 'foldHold', on: false }])
  })
})

describe('Modular knobs (#329)', () => {
  it('parses the engine\'s knob list, one tab-separated line per number', async () => {
    const { mod } = await boot()
    expect(mod.parseKnobs('1\tRLPF\tfreq\t0\t20\t20000\t1\t800\n1\tRLPF\trq\t1\t0.05\t2\t0\t0.3\n')).toEqual([
      { module: 1, ugen: 'RLPF', name: 'freq', ctl: 0, lo: 20, hi: 20000, exp: true, def: 800 },
      { module: 1, ugen: 'RLPF', name: 'rq', ctl: 1, lo: 0.05, hi: 2, exp: false, def: 0.3 },
    ])
    expect(mod.parseKnobs('')).toEqual([])
  })

  it('keeps each synth\'s knobs as they arrive, and asks for the code again', async () => {
    const { mod, send, take } = await boot()
    send({ t: 'knobs', s: 2, knobs: enc('0\tSinOsc\tfreq\t0\t20\t20000\t1\t440\n') })
    expect(mod.codeKnobs[2]?.map((k) => k.ugen)).toEqual(['SinOsc'])
    take()
    mod.requestCode(2)
    expect(take()).toEqual([{ t: 'dump', s: 2 }])
  })
})

describe('New (#325)', () => {
  it('starts fresh on one Modular synth when no song is kept', async () => {
    const { mod, names, posted } = await boot()
    expect(posted.map((m) => m.t)).toEqual(['clear'])
    expect(mod.synths.list).toEqual([0])
    expect(names.stripName(0)).toBe('Synth 1')
  })

  it('clears the view with the engine and forgets the kept song', async () => {
    const { mod, take, storage } = await boot({ storage: { 'algo-synth:song': 'tempo 90' } })
    mod.addSynth()
    mod.addSynth()
    mod.layout.groups = [16]
    mod.files.fileName = 'groove.song'
    take()
    mod.clearAll()
    expect(take().map((m) => m.t)).toEqual(['clear'])
    expect([mod.synths.list, mod.synths.selected, mod.layout.groups, mod.files.fileName]).toEqual([[0], 0, [], ''])
    expect(storage.getItem('algo-synth:song')).toBeNull()
  })

  it('keeps a song with tracks, and forgets one without', async () => {
    const { send, storage } = await boot()
    send({ t: 'song', ...ok, ok: true, text: enc('tempo 100\ntrack a synth\n'), error: null, tracks: [track('a', 0, 1)] })
    expect(storage.getItem('algo-synth:song')).toBe('tempo 100\ntrack a synth\n')
    send({ t: 'song', ...ok, ok: true, text: enc('tempo 120\n'), error: null, tracks: [] })
    expect(storage.getItem('algo-synth:song')).toBeNull()
  })
})

describe('onMessage', () => {
  it('pos: the song clock', async () => {
    const { mod, send } = await boot()
    send({ t: 'pos', step: 12, songPlaying: true, entry: 2, local: 5 })
    expect([mod.song.step, mod.song.playing, mod.song.entry, mod.song.local]).toEqual([12, true, 2, 5])
    // Without an arrangement the engine leaves entry and local out.
    send({ t: 'pos', step: -1, songPlaying: false })
    expect([mod.song.entry, mod.song.local]).toEqual([-1, -1])
  })

  it('load and meters', async () => {
    const { mod, send } = await boot()
    expect(mod.meter.seen).toBe(false)
    send({ t: 'load', load: 0.25, peak: null, voices: 3, reduction: 0.5 })
    expect(mod.meter).toEqual({ load: 0.25, peak: null, voices: 3, reduction: 0.5, seen: true })
    const levels = new Float32Array(mod.METER_STRIPS + 6).fill(0.5)
    send({ t: 'meters', levels })
    expect(mod.levels.values).toBe(levels)
  })

  it('params: replaces a synth`s values with the engine`s', async () => {
    const { mod, send } = await boot()
    send({ t: 'params', s: 2, values: new Float32Array([1, 2, 3]) })
    expect(mod.params.values[2]).toEqual([1, 2, 3])
  })

  it('keeps the knobs a modulation drives, as the engine sends them (#208)', async () => {
    const { mod, send } = await boot()
    expect(mod.modulated.keys.size).toBe(0)
    send({ t: 'mods', keys: [mod.modKey(2, Param.Cutoff), mod.modKey(0, GlobalParam.P2Return)] })
    expect(mod.modulated.keys.has(mod.modKey(2, Param.Cutoff))).toBe(true)
    expect(mod.modulated.keys.has(mod.modKey(3, Param.Cutoff))).toBe(false)
    send({ t: 'mods', keys: [] })
    expect(mod.modulated.keys.size).toBe(0)
  })

  it('sends a SynthDef and keeps the code of each synth and its error (ADR-0024)', async () => {
    const { mod, send, take } = await boot()
    take()
    mod.setCode(2, 'SynthDef(\\a, { Saw.ar(440) }).add;')
    const [msg] = take()
    expect(msg?.t).toBe('code')
    expect(msg?.s).toBe(2)
    expect(new TextDecoder().decode(msg?.bytes as ArrayBuffer)).toBe('SynthDef(\\a, { Saw.ar(440) }).add;')
    send({ t: 'code', s: 2, text: enc('SynthDef(\\a, { Saw.ar(440) }).add;'), error: { line: 1, col: 17, msg: enc('no such UGen') } })
    expect(mod.codes[2]).toEqual({ text: 'SynthDef(\\a, { Saw.ar(440) }).add;', error: { line: 1, col: 17, msg: 'no such UGen' } })
    send({ t: 'code', s: 2, text: enc('x'), error: null })
    expect(mod.codes[2]).toEqual({ text: 'x', error: null })
  })

  it('pads and zones are decoded per synth', async () => {
    const { mod, send } = await boot()
    expect(mod.padsOf(1)).toHaveLength(16)
    expect(mod.zonesOf(1)).toHaveLength(ZONES)
    const pads = new Float32Array(16 * PAD_FIELDS)
    pads[PadField.Sample] = 3
    send({ t: 'pads', s: 1, values: pads })
    expect(mod.padsOf(1)[0]?.sample).toBe(3)
    const zones = new Float32Array(ZONES * ZONE_FIELDS).fill(-1)
    send({ t: 'zones', s: 1, values: zones })
    expect(mod.zonesOf(1)).toHaveLength(ZONES)
  })

  it('ready: a stale engine, a version mismatch, a match', async () => {
    const known = Object.keys(Model).length
    const [major, minor, patch] = page.version.split('.').map(Number)
    const code = (major ?? 0) * 10000 + (minor ?? 0) * 100 + (patch ?? 0)

    const stale = await boot()
    stale.send({ t: 'ready', models: known - 1, version: code, build: 0 })
    expect(stale.mod.status.error).toBe(stale.mod.staleEngine(known - 1, known))
    expect(stale.mod.status.error).toContain(`it knows ${known - 1} models`)
    expect(stale.mod.staleEngine(0, known)).not.toContain('it knows')

    const old = await boot()
    old.send({ t: 'ready', models: known, version: 0, build: 0 })
    expect(old.mod.engineBuild.version).toBe('')
    expect(old.mod.status.error).toContain('older than the page')

    // The page's commit is `dev` under plain vitest and the checkout's under make (ALGO_BUILD_SHA).
    const id = /^[0-9a-f]{8}/.test(page.build) ? parseInt(page.build.slice(0, 8), 16) : 0x1234abcd
    const same = await boot()
    same.send({ t: 'ready', models: known, version: code, build: id })
    expect(same.mod.engineBuild).toEqual({ version: page.version, build: id.toString(16).padStart(8, '0') })
    expect(same.mod.status.error).toBe('')
  })

  it('imported: the notice, or why it failed', async () => {
    const { mod, send } = await boot()
    mod.files.fileName = 'tune.mid'
    send({ t: 'imported', code: 3 })
    expect(mod.files.notice).toContain('Imported tune.mid as the song: 3 tracks')
    send({ t: 'imported', code: -7 })
    expect(mod.files.notice).toBe('tune.mid: the file has no notes')
    send({ t: 'imported', code: -1 })
    expect(mod.files.notice).toBe('tune.mid: not a MIDI file')
    send({ t: 'imported', code: -99 })
    expect(mod.files.notice).toBe('tune.mid: import failed (-99)')
  })

  it('sysex: the voice names, or why it failed', async () => {
    const { mod, send } = await boot()
    send({ t: 'sysex', code: 2, names: [enc('BRASS 1'), enc('E.PIANO')] })
    expect(mod.sysex.names).toEqual(['BRASS 1', 'E.PIANO'])
    send({ t: 'sysex', code: -2 })
    expect(mod.sysex.error).toBe('not a Yamaha SysEx file')
    send({ t: 'sysex', code: -42 })
    expect(mod.sysex.error).toBe('load failed (-42)')
  })

  it('ignores a message it does not know', async () => {
    const { mod, send } = await boot()
    send({ t: 'nonsense' })
    expect(mod.files.notice).toBe('')
  })
})

describe('samples', () => {
  const loaded = (slot: number, extra: Record<string, unknown> = {}) => ({
    t: 'sample', slot, code: 100, frames: 100, root: 60, loopStart: 0, loopEnd: 100, peaks: new Float32Array(4), used: 200, cap: 1000, ...extra,
  })

  it('a load resolves with the engine`s code and fills the slot', async () => {
    const { mod, send, take } = await boot()
    take()
    const done = mod.loadSample(2, new ArrayBuffer(8), 'kick.wav')
    await Promise.resolve()
    await Promise.resolve()
    expect(take()).toEqual([{ t: 'sample', slot: 2, bytes: expect.any(ArrayBuffer) }])
    send(loaded(2))
    await expect(done).resolves.toBe(100)
    expect(mod.sampleStore.slots[2]).toMatchObject({ name: 'kick.wav', frames: 100, root: 60 })
    expect([mod.sampleStore.used, mod.sampleStore.cap]).toEqual([200, 1000])
  })

  it('a second load for a busy slot waits its turn; both resolve, the last fills the slot (#253)', async () => {
    const { mod, send } = await boot()
    const first = mod.loadSample(3, new ArrayBuffer(8), 'one.wav')
    const second = mod.loadSample(3, new ArrayBuffer(8), 'two.wav')
    await new Promise((r) => setTimeout(r))
    send(loaded(3, { code: 50 }))
    await expect(first).resolves.toBe(50)
    expect(mod.sampleStore.slots[3]?.name).toBe('one.wav')
    send(loaded(3, { code: 70 }))
    await expect(second).resolves.toBe(70)
    expect(mod.sampleStore.slots[3]?.name).toBe('two.wav')
  })

  it('a failed load says why and resolves with the error', async () => {
    const { mod, send } = await boot()
    const done = mod.loadSample(1, new ArrayBuffer(8), 'bad.wav')
    await new Promise((r) => setTimeout(r))
    send({ t: 'sample', slot: 1, code: -1 })
    await expect(done).resolves.toBe(-1)
    expect(mod.sampleStore.error).toBe('bad.wav: not a WAV file')
    expect(mod.sampleStore.slots[1]).toBeNull()
  })

  it('a sample the view did not ask for is named after its slot; a cleared one empties it', async () => {
    const { mod, send, take } = await boot()
    send(loaded(4))
    expect(mod.sampleStore.slots[4]?.name).toBe('sample 5')
    take()
    mod.clearSample(4)
    expect(take()).toEqual([{ t: 'sampleClear', slot: 4 }])
    send({ t: 'sampleCleared', slot: 4, used: 0 })
    expect(mod.sampleStore.slots[4]).toBeNull()
    expect(mod.sampleStore.used).toBe(0)
  })

  it('zone and pad edits post the edit, then ask for the result', async () => {
    const { mod, take } = await boot()
    take()
    mod.setZone(1, 2, 3, 4)
    mod.clearZones(1)
    mod.setPad(1, 5, PadField.Level, 0.5)
    mod.clearPads(1)
    expect(take()).toEqual([
      { t: 'zone', s: 1, zone: 2, field: 3, v: 4 },
      { t: 'zonesDump', s: 1 },
      { t: 'zonesClear', s: 1 },
      { t: 'zonesDump', s: 1 },
      { t: 'pad', s: 1, pad: 5, field: PadField.Level, v: 0.5 },
      { t: 'padsDump', s: 1 },
      { t: 'padsClear', s: 1 },
      { t: 'padsDump', s: 1 },
    ])
  })

  it('mapSample takes the first empty zone', async () => {
    const { mod, take } = await boot()
    take()
    mod.mapSample(0, 7)
    const posted = take()
    expect(posted.filter((m) => m.t === 'zone').every((m) => m.zone === 0)).toBe(true)
    expect(posted.at(-1)).toEqual({ t: 'zonesDump', s: 0 })
  })
})

describe('posting', () => {
  it('loadSong posts the text as bytes and keeps it as the draft', async () => {
    const { mod, take } = await boot()
    take()
    mod.loadSong('tempo 128')
    const [msg] = take()
    expect(msg?.t).toBe('song')
    expect(new TextDecoder().decode(msg?.bytes as ArrayBuffer)).toBe('tempo 128')
    expect(mod.song.draft).toBe('tempo 128')
  })

  it('song, transport and note edits', async () => {
    const { mod, take } = await boot()
    take()
    mod.routeTrack(2, 5)
    mod.setSongTempo(140)
    mod.setSongSwing(60)
    mod.playSong()
    mod.pauseSong()
    mod.stopSong()
    mod.requestSong()
    mod.setStep(1, 2, 3, 2)
    mod.addNote(0, 6, 60)
    mod.removeNote(0, 6, 60)
    mod.setNoteLength(0, 6, 60, 9)
    mod.freezeFrag(3)
    mod.applySysex(1, 4)
    expect(take()).toEqual([
      { t: 'songRoute', track: 2, s: 5 },
      { t: 'songTempo', v: 140 },
      { t: 'songSwing', v: 60 },
      { t: 'songPlay' },
      { t: 'songPause' },
      { t: 'songStop' },
      { t: 'songDump' },
      { t: 'step', f: 1, l: 2, s: 3, level: 2 },
      { t: 'note', f: 0, op: 0, tick: 6, note: 60, len: 0 },
      { t: 'note', f: 0, op: 1, tick: 6, note: 60, len: 0 },
      { t: 'note', f: 0, op: 2, tick: 6, note: 60, len: 9 },
      { t: 'freeze', f: 3 },
      { t: 'sysexApply', s: 1, i: 4 },
    ])
  })

  it('arrange ops', async () => {
    const { mod, take } = await boot()
    take()
    mod.arrange.toggle(1, 2, 3)
    mod.arrange.addSection()
    mod.arrange.addSection(8)
    mod.arrange.setBars(1, 2)
    mod.arrange.insert(0, 1)
    mod.arrange.remove(2)
    mod.arrange.move(1, 3)
    mod.arrange.loop(0, 2)
    mod.arrange.seekBar(5)
    mod.trackEdit.preset(1, Preset.MiniBass)
    mod.trackEdit.setting(0, 2)
    expect(take()).toEqual([
      { t: 'arr', op: 0, a: 1, b: 2, c: 3 },
      { t: 'arr', op: 1, a: 4 },
      { t: 'arr', op: 1, a: 8 },
      { t: 'arr', op: 2, a: 1, b: 2 },
      { t: 'arr', op: 3, a: 0, b: 1 },
      { t: 'arr', op: 4, a: 2 },
      { t: 'arr', op: 5, a: 1, b: 3 },
      { t: 'arr', op: 6, a: 0, b: 2 },
      { t: 'songSeek', bar: 5 },
      { t: 'track', op: 0, track: 1, a: Preset.MiniBass },
      { t: 'track', op: 1, track: 0, a: 2 },
    ])
  })

  it('engine methods keep the view`s values in step', async () => {
    const { mod, take } = await boot()
    take()
    const e = mod.getEngine()
    mod.params.values[0] = [0]
    e?.param(0, Param.Cutoff, 900)
    e?.preset(0, Preset[Object.keys(Preset)[0] as keyof typeof Preset])
    e?.noteOn(0, 60)
    e?.noteOff(0, 60)
    e?.panic()
    expect(mod.params.values[0]?.[Param.Cutoff]).toBe(900)
    expect(take().map((m) => m.t)).toEqual(['param', 'preset', 'on', 'off', 'panic'])
  })
})

describe('synths and groups', () => {
  it('addSynth shows the lowest free index, reset, named by family and on its first preset', async () => {
    const { mod, names, take } = await boot()
    take()
    expect(mod.addSynth()).toBe(true)
    expect(mod.synths.list).toEqual([0, 1])
    expect(mod.synths.selected).toBe(1)
    // Shown, then a song track for it (ADR-0027).
    expect(take().map((m) => [m.t, m.s])).toEqual([['reset', 1], ['trackAdd', 1]])
    expect(names.names.strips[1]).toMatch(/Synth/)
    for (let i = 2; i < mod.MAX_SYNTHS; i++) mod.addSynth()
    expect(mod.addSynth()).toBe(false)
  })

  it('removeSynth takes its track with it (ADR-0027)', async () => {
    const { mod, take } = await boot()
    mod.addSynth()
    take()
    mod.removeSynth(1)
    expect(take()).toEqual([{ t: 'trackRemove', s: 1 }])
  })

  it('removeSynth never removes the last', async () => {
    const { mod } = await boot()
    mod.removeSynth(0)
    expect(mod.synths.list).toEqual([0])
    mod.addSynth()
    mod.removeSynth(1)
    expect(mod.synths.list).toEqual([0])
    expect(mod.synths.selected).toBe(0)
  })

  it('addGroup and removeGroup; what fed a removed group goes to the master', async () => {
    const { mod, take } = await boot()
    take()
    expect(mod.addGroup()).toBe(true)
    expect(mod.layout.groups).toEqual([0])
    expect(take()).toEqual([{ t: 'reset', s: 16 }])
    mod.params.values[0] = []
    mod.setOut(0, 1)
    expect(take()).toEqual([{ t: 'param', s: 0, id: Param.Out, v: 1 }])
    mod.toggleCollapsed(16)
    mod.toggleHidden(16)
    mod.removeGroup(0)
    expect(mod.layout).toEqual({ groups: [], order: [], collapsed: [], hidden: [] })
    const posted = take()
    expect(posted).toContainEqual({ t: 'param', s: 0, id: Param.Out, v: 0 })
    expect(posted.at(-1)).toEqual({ t: 'reset', s: 16 })
    for (let g = 0; g < 8; g++) mod.addGroup()
    expect(mod.addGroup()).toBe(false)
  })
})

describe('MIDI files and setups', () => {
  const reg: Registry = {
    params: Param, global: GlobalParam, strip: StripParam,
    models: Model as unknown as Record<string, number>, maxSynths: 16,
  }

  it('a MIDI file is sent to be imported as the song (ADR-0022)', async () => {
    const { mod, send, take } = await boot()
    take()
    const bytes = new ArrayBuffer(4)
    await mod.loadMidi(bytes, 'tune.mid')
    expect(take()).toEqual([{ t: 'midi', bytes }])
    send({ t: 'imported', code: 2 })
    expect(mod.files.notice).toContain('Imported tune.mid as the song: 2 tracks')
  })

  it('a setup picked with its MIDI file applies once the file is imported', async () => {
    const { mod, send, take } = await boot()
    const setup = buildSetup({ synths: [0, 3], values: [[], [], [], []] }, reg)
    const picked = [new File([JSON.stringify(setup)], 'tune.synths.json'), new File([new Uint8Array(4)], 'tune.mid')]
    await mod.openFiles(picked)
    take()
    send({ t: 'imported', code: 1 })
    expect(mod.synths.list).toEqual([0, 3])
    expect(take().filter((m) => m.t === 'dump').map((m) => m.s)).toEqual([0, 3])
    // The setup round-trips through setupText.
    expect(JSON.parse(mod.setupText()).synths.map((s: { index: number }) => s.index)).toEqual([0, 3])
  })

  it('a setup waiting for a file that does not import is dropped', async () => {
    const { mod, send } = await boot()
    const setup = buildSetup({ synths: [0, 3], values: [[], [], [], []] }, reg)
    await mod.openFiles([new File([JSON.stringify(setup)], 'tune.synths.json'), new File([new Uint8Array(4)], 'tune.mid')])
    send({ t: 'imported', code: -7 })
    expect(mod.synths.list).toEqual([0])
    send({ t: 'imported', code: 1 })
    expect(mod.synths.list).toEqual([0])
  })

  it('a setup alone applies to the synths on screen; a bad one says so', async () => {
    const { mod } = await boot()
    await mod.openFiles([new File(['{not json'], 'x.synths.json')])
    expect(mod.files.notice).toMatch(/^x\.synths\.json: .*; nothing applied$/)
    const setup = buildSetup({ synths: [1], values: [[], []] }, reg)
    await mod.openFiles([new File([JSON.stringify(setup)], 'y.synths.json')])
    expect(mod.synths.list).toEqual([1])
    expect(mod.synths.selected).toBe(1)
  })

  it('a setup keeps a song track\'s synth on screen', async () => {
    const { mod } = await boot()
    mod.applySong({ ...ok, ok: true, text: enc('x'), error: null, tracks: [track('bass', 4, 1)] })
    const setup = buildSetup({ synths: [1], values: [[], []] }, reg)
    await mod.openFiles([new File([JSON.stringify(setup)], 'y.synths.json')])
    expect(mod.synths.list).toEqual([1, 4])
  })

  it('a song file goes to the composer', async () => {
    const { mod, take } = await boot()
    take()
    await mod.openFiles([new File(['tempo 77'], 'tune.song')])
    expect(mod.view.main).toBe('composer')
    expect(mod.song.draft).toBe('tempo 77')
    expect(take().map((m) => m.t)).toEqual(['song'])
  })

  it('the demo imports its MIDI file and no setup: the song picks its synths (#327)', async () => {
    const fetched: string[] = []
    const { mod, send, take } = await boot({
      fetch: async (url) => {
        fetched.push(url)
        return new Response(new Uint8Array(4))
      },
    })
    take()
    await mod.loadDemo()
    expect(fetched.filter((u) => u.includes('demo'))).toEqual([expect.stringMatching(/demo\.mid$/)])
    expect(mod.files.fileName).toBe('Canon in D (demo)')
    expect(take().map((m) => m.t)).toEqual(['midi'])
    send({ t: 'imported', code: 4 })
    expect(take().map((m) => m.t)).toEqual([])
  })
})

describe('user presets', () => {
  it('capture then apply posts defaults, the values and a dump', async () => {
    const { mod, take } = await boot()
    mod.params.values[0] = Array.from({ length: 400 }, () => 0)
    mod.params.values[0][Param.Cutoff] = 1000
    const preset = mod.capturePreset('bright', { kind: 'synth', s: 0 })
    expect(preset.name).toBe('bright')
    expect(mod.presetModified(preset, { kind: 'synth', s: 0 })).toBe(false)
    take()
    mod.params.values[1] = Array.from({ length: 400 }, () => 0)
    expect(mod.applyPreset(preset, { kind: 'synth', s: 1 })).toBe(true)
    const posted = take()
    expect(posted[0]?.t).toBe('defaults')
    expect(posted).toContainEqual({ t: 'param', s: 1, id: Param.Cutoff, v: 1000 })
    expect(posted.at(-1)).toEqual({ t: 'dump', s: 1 })
  })

  it('applyPreset is refused before power', async () => {
    vi.resetModules()
    const mod = await import('./engine')
    expect(mod.applyPreset({ id: 'x', kind: 'synth', name: 'x', params: {} } as never, { kind: 'synth', s: 0 })).toBe(false)
  })
})

describe('helpers', () => {
  it('outText names a group as the console does', async () => {
    const { mod } = await boot()
    expect(mod.outText('Master')).toBe('Master')
    const { names } = await import('./names')
    names.strips[16] = 'Drums bus'
    expect(mod.outText('Group 1')).toBe('Drums bus')
    expect(mod.synthColour(0)).toBe('hsl(12 68% 62%)')
  })

  it('renameSynth: an empty name puts a synth back to its family`s', async () => {
    const { mod, names } = await boot()
    mod.renameSynth(0, 'Lead')
    expect(names.names.strips[0]).toBe('Lead')
    mod.renameSynth(0, '  ')
    expect(names.names.strips[0]).toMatch(/Synth/)
  })

  it('moveStrip reorders the console', async () => {
    const { mod } = await boot()
    mod.addSynth()
    mod.addSynth()
    mod.moveStrip(2, 0)
    expect(mod.layout.order.slice(0, 3)).toEqual([2, 0, 1])
  })
})
