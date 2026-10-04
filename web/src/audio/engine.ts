// Main-thread side of the audio graph (ADR-0001, ADR-0003):
// AudioWorkletNode (dsp.wasm) -> AnalyserNode (scope) -> speakers.
// This file only sends messages; every musical decision is made in Rust.

import { reactive, shallowReactive, watch } from 'vue'
import * as registryTables from './params'
import { GROUPS, groupStrip, moveBefore, orderStrips, routeOk } from './console'
import { modelDef, type ModelDef } from './models'
import { GlobalParam, InsertType, Model, Param, Preset, ProcType, StripParam, ZoneField, type ParamId, type PresetId } from './params'
import { loadLibrary } from './library'
import { capture, modified, plan, type PresetRegistry, type Target, type UserPreset } from './presets'
import { cleanName, familyName, names, partName as laneName, renameStrip, setNames, stripName as nameOfStrip } from './names'
import {
  EMPTY_PAD, EMPTY_ZONE, SAMPLE_SLOTS, ZONES, decodePads, decodeZones, evictable, freeSlot, kitFiles, packFiles, padSets, parseKits,
  parseManifest, slotsUsedElsewhere, zoneSets, type Kit, type Pack, type Pad, type Zone,
} from './sampler'
import { MUTE, applyPlan, buildSetup, parseSetup, type Registry, type Setup, type State } from './setup'

const base = import.meta.env.BASE_URL

/** Mono synths the engine holds (`SYNTHS` in engine.rs). */
export const MAX_SYNTHS = 16
/** A routing choice for a MIDI part: a synth index, or MUTE. */
export { MUTE }
export type Route = number
/** MIDI channels the player routes. */
const CHANNELS = 16

/** One MIDI channel with notes, as the engine summarised it. */
export interface Part {
  channel: number
  name: string
  notes: number
  start: number
  end: number
  synth: Route
  /** Notes for drawing: [start s, end s, pitch], paired on the main thread. */
  roll: [number, number, number][]
}

class AudioEngine {
  constructor(
    readonly ctx: AudioContext,
    readonly node: AudioWorkletNode,
    readonly analyser: AnalyserNode,
  ) {
    node.port.onmessage = ({ data }) => onMessage(data)
  }

  static async start(): Promise<AudioEngine> {
    const ctx = new AudioContext({ latencyHint: 'interactive' })
    const module = await WebAssembly.compileStreaming(fetch(`${base}dsp.wasm`))
    await ctx.audioWorklet.addModule(`${base}worklet.js`)
    const node = new AudioWorkletNode(ctx, 'algo-synth', {
      numberOfInputs: 0,
      outputChannelCount: [2],
      processorOptions: { module },
    })
    const analyser = new AnalyserNode(ctx, { fftSize: 2048 })
    node.connect(analyser).connect(ctx.destination)
    return new AudioEngine(ctx, node, analyser)
  }

  post(msg: object, transfer: Transferable[] = []) { this.node.port.postMessage(msg, transfer) }
  param(s: number, id: ParamId, v: number) {
    const values = params.values[s]
    if (values) values[id] = v
    this.post({ t: 'param', s, id, v })
  }
  preset(s: number, id: PresetId) { this.post({ t: 'preset', s, id }) }
  reset(s: number) { this.post({ t: 'reset', s }) }
  noteOn(s: number, n: number, v = 0.8) { this.post({ t: 'on', s, n, v }) }
  noteOff(s: number, n: number) { this.post({ t: 'off', s, n }) }
  panic() { this.post({ t: 'panic' }) }
}

// One engine for the app. `status` and `player` are reactive so the UI follows them.
export const status = reactive({ running: false, error: '', sampleRate: 0 })
export const player = reactive({
  loaded: false,
  fileName: '',
  parts: [] as Part[],
  length: 0,
  bar: 2,
  position: 0,
  playing: false,
  error: '',
  /** What applying a setup reported: skipped entries, a part-count mismatch, or why it failed. */
  notice: '',
})
/** DSP load as a share of real time (peak is null without a precise clock). */
export const meter = reactive({ load: 0, peak: null as number | null, voices: 0, reduction: 0, seen: false })

/**
 * The console's meters (#53), as linear peaks since the last update: one per
 * strip (16 synths, then 8 groups; after the fader), then master left and
 * right, then each processor's return. Replaced whole about 47 times a second.
 */
export const METER_STRIPS = 16 + 8
export const levels = shallowReactive<{ values: Float32Array }>({ values: new Float32Array(METER_STRIPS + 2 + 4) })

/**
 * Parameter values by synth and id: what the view last sent, replaced by the
 * engine's clamped values at start and after a preset or reset. The sliders'
 * ranges match Rust's, so the two only differ out of range.
 */
export const params = reactive({ values: [] as number[][] })

/**
 * The synths on screen, by engine index, and the one the keyboard plays. The
 * engine always holds `MAX_SYNTHS`; adding one shows a free index, reset to
 * the default patch.
 */
export const synths = reactive({ list: [0] as number[], selected: 0 })

/**
 * The group buses on screen (0–7) and how the console lays its strips out
 * (#61, ADR-0010): `order`, `collapsed` and `hidden` are strip indices, synths
 * 0–15 and groups 16–23. The engine never sees the layout; a setup keeps it.
 */
export const layout = reactive({ groups: [] as number[], order: [] as number[], collapsed: [] as number[], hidden: [] as number[] })

/** Show a group bus on the lowest free number, reset to its defaults; false when all 8 are shown. */
export function addGroup(): boolean {
  const free = Array.from({ length: GROUPS }, (_, g) => g).find((g) => !layout.groups.includes(g))
  if (free === undefined) return false
  engine?.reset(groupStrip(free))
  layout.groups = [...layout.groups, free].sort((a, b) => a - b)
  return true
}

/** Remove group `g`: what fed it goes to the master, and the group goes back to its defaults. */
export function removeGroup(g: number) {
  const strip = groupStrip(g)
  for (const s of [...synths.list, ...layout.groups.map(groupStrip)]) {
    if (s !== strip && params.values[s]?.[Param.Out] === g + 1) engine?.param(s, Param.Out, 0)
  }
  engine?.reset(strip)
  layout.groups = layout.groups.filter((x) => x !== g)
  layout.order = layout.order.filter((x) => x !== strip)
  layout.collapsed = layout.collapsed.filter((x) => x !== strip)
  layout.hidden = layout.hidden.filter((x) => x !== strip)
}

/** Send strip `s` to `out` (0 master, 1–8 a group); the engine refuses a route that would loop. */
export const setOut = (s: number, out: number) => { if (routeOk(s, out)) engine?.param(s, Param.Out, out) }

const toggle = (list: number[], id: number) => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id])
export const toggleCollapsed = (id: number) => { layout.collapsed = toggle(layout.collapsed, id) }
export const toggleHidden = (id: number) => { layout.hidden = toggle(layout.hidden, id) }
/** Move strip `id` to just before `target` in the console. */
export function moveStrip(id: number, target: number) {
  const shown = [...synths.list, ...layout.groups.map(groupStrip)]
  layout.order = moveBefore(orderStrips(layout.order, shown), id, target)
}

/** Which main view is shown: the synth panels or the mixer console. */
export const view = reactive({
  main: 'synths' as 'synths' | 'mixer' | 'composer',
  /** The bottom pane: the arranger (ADR-0015) or the MIDI file player, until MIDI import replaces it. */
  bottom: 'arranger' as 'arranger' | 'player',
})

/** One hue per synth, so a part's notes match its synth's card. */
export const synthColour = (s: number) => `hsl(${(12 + 47 * s) % 360} 68% 62%)`

/** Show synth `s`, reset to the default patch unless it is already shown. */
function show(s: number) {
  if (synths.list.includes(s)) return
  engine?.reset(s)
  synths.list = [...synths.list, s].sort((a, b) => a - b)
}

/**
 * Add a synth on the lowest free index and select it; false when all 16 are
 * shown. With a model it starts on that model's first preset (#132).
 */
export function addSynth(model?: ModelDef): boolean {
  const free = Array.from({ length: MAX_SYNTHS }, (_, i) => i).find((i) => !synths.list.includes(i))
  if (free === undefined) return false
  // Named once, by its family, so adding or removing others never renames it (#177).
  names.strips[free] = familyName(model?.family ?? 'mono', synths.list.map((s) => stripName(s)))
  show(free)
  const first = model?.presets[0]
  if (first !== undefined) engine?.preset(free, Preset[first])
  synths.selected = free
  return true
}

/**
 * Rename strip `s`. An empty name puts a synth back to its family's default
 * (`Drum N`, `Sampler N`, `Synth N`, #177) and a group to `Group N`.
 */
export function renameSynth(s: number, raw: string) {
  if (s >= MAX_SYNTHS || cleanName(raw)) {
    renameStrip(s, raw)
    return
  }
  const family = modelDef(params.values[s]?.[Param.Model] ?? 0).family
  const others = synths.list.filter((i) => i !== s).map((i) => stripName(i))
  names.strips[s] = familyName(family, others)
}

/** Remove synth `s` (never the last one); parts playing on it are muted. */
export function removeSynth(s: number) {
  if (synths.list.length <= 1) return
  synths.list = synths.list.filter((i) => i !== s)
  delete names.strips[s]
  for (const p of player.parts) if (p.synth === s) route(p, MUTE)
  if (synths.selected === s) synths.selected = synths.list[0] ?? 0
}
let engine: AudioEngine | null = null

export async function power(): Promise<void> {
  if (engine) {
    await engine.ctx.resume()
    return
  }
  try {
    engine = await AudioEngine.start()
    status.running = true
    status.sampleRate = engine.ctx.sampleRate
  } catch (e) {
    status.error = e instanceof Error ? e.message : String(e)
  }
}

export const getEngine = (): AudioEngine | null => engine

// --- MIDI player ------------------------------------------------------------

const LOAD_ERRORS: Record<number, string> = {
  [-1]: 'not a MIDI file',
  [-2]: 'the file is truncated',
  [-3]: 'SMPTE timing is not supported',
  [-4]: 'the file has a malformed event',
  [-6]: 'the file is larger than 16 MiB',
}

/** Send a MIDI file to the engine; powers audio on first (a click is a gesture). */
export async function loadMidi(bytes: ArrayBuffer, fileName: string): Promise<void> {
  await power()
  if (!engine) return
  player.fileName = fileName
  player.error = ''
  engine.post({ t: 'midi', bytes }, [bytes])
}

export async function loadDemo(): Promise<void> {
  const [mid, setup] = await Promise.all([fetch(`${base}demo.mid`), fetch(`${base}demo.synths.json`)])
  const parsed = setup.ok ? parseSetup(await setup.text(), registry) : null
  pending = parsed?.ok ? { setup: parsed.setup, warnings: parsed.warnings, from: 'shipped' } : null
  player.notice = ''
  await loadMidi(await mid.arrayBuffer(), 'Canon in D (demo)')
}

// DX7 SysEx: the engine parses the file; the view keeps the voice names it sends back.
export const sysex = reactive({ names: [] as string[], error: '', fileName: '' })
const SYSEX_ERRORS: Record<number, string> = {
  [-1]: 'the file is empty',
  [-2]: 'not a Yamaha SysEx file',
  [-3]: 'not a DX7 voice or bank',
  [-4]: 'the file is truncated',
  [-6]: 'the file is too large',
}
export async function loadSysex(bytes: ArrayBuffer, fileName: string): Promise<void> {
  await power()
  if (!engine) return
  sysex.fileName = fileName
  sysex.error = ''
  engine.post({ t: 'sysex', bytes }, [bytes])
}
export const applySysex = (s: number, i: number) => engine?.post({ t: 'sysexApply', s, i })

/** A strip's name, a synth's following the lane it plays when not renamed (#127). */
export const stripName = (s: number) => nameOfStrip(s, player.parts)
export const partName = (p: Part) => laneName(p)

// --- The sampler (#125): the sample store, each synth's zones and the packs on offer -----------
// The engine parses and holds everything; the view keeps what it reports (the waveform's peaks,
// the zones) and the names the files had.

export interface SampleInfo {
  name: string
  frames: number
  root: number
  loopStart: number
  loopEnd: number
  /** (min, max) pairs for drawing, computed by the engine. */
  peaks: Float32Array
}
const SAMPLE_ERRORS: Record<number, string> = {
  [-1]: 'not a WAV file',
  [-2]: 'the file is truncated',
  [-3]: 'only 16- and 24-bit PCM or 32-bit float WAV, mono or stereo',
  [-4]: 'the file has no audio',
  [-6]: 'too large, or the sample store is full: free a sample with ×, or load one pack at a time',
  [-7]: 'no such slot',
}
export const sampleStore = reactive({
  slots: Array.from({ length: SAMPLE_SLOTS }, () => null) as (SampleInfo | null)[],
  /** `f32` values held and the cap (`MAX_VALUES` in sample.rs; the engine reports it with each load). */
  used: 0,
  cap: 16 << 20,
  error: '',
  busy: '',
})
/** Each synth's zones by engine index, as the engine last reported them. */
export const zoneState = reactive({ zones: [] as Zone[][] })
export const zonesOf = (s: number): Zone[] => zoneState.zones[s] ?? Array.from({ length: ZONES }, () => EMPTY_ZONE)
/** Each pad sampler's pads by engine index, as the engine last reported them. */
export const padState = reactive({ pads: [] as Pad[][] })
export const padsOf = (s: number): Pad[] => padState.pads[s] ?? Array.from({ length: 16 }, () => EMPTY_PAD)

const loading = new Map<number, { name: string; done: (code: number) => void }>()
/** Send a WAV file to the engine for `slot`; resolves with its frame count or a negative error code. */
export async function loadSample(slot: number, bytes: ArrayBuffer, name: string): Promise<number> {
  await power()
  if (!engine) return -5
  return new Promise((done) => {
    loading.set(slot, { name, done })
    engine?.post({ t: 'sample', slot, bytes }, [bytes])
  })
}
export const clearSample = (slot: number) => engine?.post({ t: 'sampleClear', slot })
export const requestZones = (s: number) => engine?.post({ t: 'zonesDump', s })
export const requestPads = (s: number) => engine?.post({ t: 'padsDump', s })
/** Set one field of a zone (`ZoneField`); the engine clamps it and the view re-reads the zones. */
export function setZone(s: number, zone: number, field: number, v: number) {
  engine?.post({ t: 'zone', s, zone, field, v })
  requestZones(s)
}
export function clearZones(s: number) {
  engine?.post({ t: 'zonesClear', s })
  requestZones(s)
}

/** Set one field of a pad (`PadField`); the engine clamps it and the view re-reads the pads. */
export function setPad(s: number, pad: number, field: number, v: number) {
  engine?.post({ t: 'pad', s, pad, field, v })
  requestPads(s)
}
export function clearPads(s: number) {
  engine?.post({ t: 'padsClear', s })
  requestPads(s)
}

/** The packs and kits `make samples` fetched, from `samples/manifest.json`; empty when there are none. */
export const packs = reactive({ list: [] as Pack[], kits: [] as Kit[], loaded: false })
export async function fetchPacks(): Promise<void> {
  try {
    const r = await fetch(`${base}samples/manifest.json`)
    const json: unknown = r.ok ? await r.json() : null
    packs.list = parseManifest(json)
    packs.kits = parseKits(json)
  } catch {
    packs.list = []
    packs.kits = []
  }
  packs.loaded = true
}

/** Files already in the store, by their path under `samples/`, so a pack used twice is loaded once. */
const slotOfFile = new Map<string, number>()

/** Fetch `files` (paths under `samples/`) into free slots, the ones already in the store excepted; throws what went wrong. */
async function loadFiles(label: string, files: string[]) {
  for (const [i, file] of files.entries()) {
    sampleStore.busy = `${label}: ${i + 1} of ${files.length}`
    const known = slotOfFile.get(file)
    if (known !== undefined && sampleStore.slots[known]) continue
    const slot = freeSlot(sampleStore.slots)
    if (slot < 0) throw new Error('all sample slots are used; free some')
    const r = await fetch(`${base}samples/${file}`)
    if (!r.ok) throw new Error(`${file}: ${r.status}`)
    const code = await loadSample(slot, await r.arrayBuffer(), file.split('/').pop() ?? file)
    if (code < 0) throw new Error(`${file}: ${SAMPLE_ERRORS[code] ?? code}`)
    slotOfFile.set(file, slot)
  }
}

/** Load a pack's files into free slots and lay its zones out on synth `s`. */
export async function loadPack(s: number, pack: Pack): Promise<void> {
  sampleStore.error = ''
  const files = packFiles(pack)
  // The store is shared and capped: a pack replaces the one this synth had, keeping what other
  // synths play and what this pack shares with it.
  engine?.post({ t: 'zonesClear', s })
  for (const slot of evictable(slotOfFile, slotsUsedElsewhere(s, zoneState.zones, padState.pads), new Set(files))) {
    engine?.post({ t: 'sampleClear', slot })
    sampleStore.slots[slot] = null
    for (const [file, at] of slotOfFile) if (at === slot) slotOfFile.delete(file)
  }
  try {
    await loadFiles(pack.name, files)
    engine?.post({ t: 'zonesClear', s })
    for (const [zone, field, v] of zoneSets(pack, (f) => slotOfFile.get(f))) engine?.post({ t: 'zone', s, zone, field, v })
    requestZones(s)
  } catch (e) {
    sampleStore.error = e instanceof Error ? e.message : String(e)
  } finally {
    sampleStore.busy = ''
  }
}

/** Load a drum kit's files into free slots and lay its pads out on synth `s`; like `loadPack`, it replaces the kit it follows. */
export async function loadKit(s: number, kit: Kit): Promise<void> {
  sampleStore.error = ''
  const files = kitFiles(kit)
  engine?.post({ t: 'padsClear', s })
  for (const slot of evictable(slotOfFile, slotsUsedElsewhere(s, zoneState.zones, padState.pads), new Set(files))) {
    engine?.post({ t: 'sampleClear', slot })
    sampleStore.slots[slot] = null
    for (const [file, at] of slotOfFile) if (at === slot) slotOfFile.delete(file)
  }
  try {
    await loadFiles(kit.name, files)
    for (const [pad, field, v] of padSets(kit, (f) => slotOfFile.get(f))) engine?.post({ t: 'pad', s, pad, field, v })
    requestPads(s)
  } catch (e) {
    sampleStore.error = e instanceof Error ? e.message : String(e)
  } finally {
    sampleStore.busy = ''
  }
}

/** Put the sample in `slot` across the keys on the first empty zone of `s`, at its own root. */
export function mapSample(s: number, slot: number) {
  const zone = zonesOf(s).findIndex((z) => z.sample < 0)
  if (zone < 0) return
  for (const [field, v] of [[ZoneField.Sample, slot], [ZoneField.KeyLo, 0], [ZoneField.KeyHi, 127], [ZoneField.Root, -1]] as const) {
    engine?.post({ t: 'zone', s, zone, field, v })
  }
  requestZones(s)
}

/** A lane of a drum fragment: its pad (`Pad` id) and its steps, 0 off, 1 hit, 2 accent. */
export interface SongLane { pad: number; steps: number[] }
/** A note fragment's line as the engine prints it, and the bars before it repeats. */
export interface SongNotes { text: string; bars: number }
export interface SongFrag { name: string; track: number; lanes: SongLane[]; notes: SongNotes | null }
export interface SongTrack { name: string; synth: Route; kind: 'drums' | 'synth' | 'sampler' }
export interface SongSection { name: string; bars: number; frags: boolean[]; autos: boolean[]; scenes: boolean[] }

/**
 * The song (ADR-0012) as the engine holds it: the engine parses the text and
 * prints it back; the view draws the grid from `frags` and never parses.
 * `draft` is the text being edited, `error` why the last one did not play.
 */
export const song = reactive({
  text: '',
  draft: '',
  tracks: [] as SongTrack[],
  frags: [] as SongFrag[],
  error: null as { line: number; col: number; msg: string } | null,
  tempo: 120,
  swing: 50,
  /** The clock's last step, −1 before the first. */
  step: -1,
  /** The song's own transport (the composer's Play and Stop). */
  playing: false,
  /** With an arrangement (ADR-0015): the entry playing and the steps into it, −1 without. */
  entry: -1,
  local: -1,
  /** The arrangement (ADR-0015): sections with what each holds (by index), their order, lanes, scenes, loop bars (0 0 none). */
  sections: [] as SongSection[],
  arrange: [] as number[],
  autos: [] as string[],
  scenes: [] as string[],
  loop: [0, 0] as [number, number],
})

/** Send `text` to the engine to parse and play. */
export function loadSong(text: string) {
  song.draft = text
  const bytes = new TextEncoder().encode(text)
  engine?.post({ t: 'song', bytes: bytes.buffer }, [bytes.buffer])
}

/** Set one step (0 off, 1 hit, 2 accent); the engine sends the song back. */
export const setStep = (f: number, l: number, s: number, level: number) =>
  engine?.post({ t: 'step', f, l, s, level })

/** Play song track `t` on synth `s` (255 mutes). */
export const routeTrack = (t: number, s: Route) => engine?.post({ t: 'songRoute', track: t, s })

/** The song's tempo (BPM) and swing (percent); the engine updates the text and the clock. */
export const setSongTempo = (v: number) => engine?.post({ t: 'songTempo', v })
export const setSongSwing = (v: number) => engine?.post({ t: 'songSwing', v })

/** The song's transport, apart from the MIDI file's. */
export const playSong = () => engine?.post({ t: 'songPlay' })
export const stopSong = () => engine?.post({ t: 'songStop' })

/** Ask the engine for the song it holds (when the composer opens). */
export const requestSong = () => engine?.post({ t: 'songDump' })

// Turning the loaded MIDI file into the song (#173): the engine converts it;
// the composer and the arranger show the result.
const IMPORT_ERRORS: Record<number, string> = {
  [-7]: 'the file has no notes',
  [-8]: 'the file has more notes, fragments or sections than a song holds',
  [-9]: 'the converted text did not parse (a bug)',
}
export const importMidi = () => engine?.post({ t: 'midiImport' })
function onImported(code: number) {
  if (code < 0) {
    player.notice = `Import: ${IMPORT_ERRORS[code] ?? LOAD_ERRORS[code] ?? `failed (${code})`}`
    return
  }
  player.notice = `Imported ${player.fileName} as the song: ${code} tracks in sections; edit it in the composer, chain it in the arranger.`
  view.bottom = 'arranger'
}

export const play = () => engine?.post({ t: 'play' })
export const stop = () => engine?.post({ t: 'stop' })
export const seek = (sec: number) => engine?.post({ t: 'seek', sec })
export function route(part: Part, synth: Route) {
  part.synth = synth
  engine?.post({ t: 'route', ch: part.channel, s: synth })
}

interface MidiSummary {
  t: 'midi'
  code: number
  parts?: { channel: number; name: Uint8Array; notes: number; start: number; end: number; synth: number }[]
  packed?: Uint32Array
  times?: Float32Array
  length?: number
  bar?: number
}

/** Take the song the worklet sent: decode its text, names and grid into `song`. */
export function applySong(data: Record<string, unknown>) {
  const decoder = new TextDecoder('utf-8')
  if (data.tooLong) {
    song.error = { line: 1, col: 1, msg: 'the text is too long' }
    return
  }
  const text = decoder.decode(data.text as Uint8Array)
  const error = data.error as { line: number; col: number; msg: Uint8Array } | null
  song.error = error ? { line: error.line, col: error.col, msg: decoder.decode(error.msg) } : null
  // A song that played replaces the draft with its canonical text; a failed one leaves the draft alone.
  if (data.ok) song.draft = text
  song.text = text
  song.tempo = data.tempo as number
  song.swing = data.swing as number
  song.tracks = (data.tracks as { name: Uint8Array; synth: number; kind: number }[]).map((t) => ({
    name: decoder.decode(t.name),
    synth: t.synth,
    kind: (['drums', 'synth', 'sampler'] as const)[t.kind] ?? 'drums',
  }))
  song.frags = (data.frags as {
    name: Uint8Array; track: number; lanes: { pad: number; steps: Uint8Array }[]; notes: { text: Uint8Array; bars: number } | null
  }[]).map((f) => ({
    name: decoder.decode(f.name),
    track: f.track,
    lanes: f.lanes.map((l) => ({ pad: l.pad, steps: Array.from(l.steps) })),
    notes: f.notes ? { text: decoder.decode(f.notes.text), bars: f.notes.bars } : null,
  }))
  type RawSection = { name: Uint8Array; bars: number; frags: boolean[]; autos: boolean[]; scenes: boolean[] }
  song.sections = ((data.sections as RawSection[] | undefined) ?? []).map((s) => ({ ...s, name: decoder.decode(s.name) }))
  song.arrange = (data.arrange as number[] | undefined) ?? []
  song.autos = ((data.autos as Uint8Array[] | undefined) ?? []).map((n) => decoder.decode(n))
  song.scenes = ((data.scenes as Uint8Array[] | undefined) ?? []).map((n) => decoder.decode(n))
  song.loop = (data.loop as [number, number] | undefined) ?? [0, 0]
}

/**
 * Arranger edits (#171); the engine changes the song and sends it back as text.
 * 0 toggle (section, kind 0 frag 1 auto 2 scene, item), 1 add a section (bars),
 * 2 set bars (section, bars), 3 insert (place, section), 4 remove (place),
 * 5 move (from, to), 6 loop (first, last; 0 0 clears).
 */
export const arrange = {
  toggle: (s: number, kind: 0 | 1 | 2, item: number) => engine?.post({ t: 'arr', op: 0, a: s, b: kind, c: item }),
  addSection: (bars = 4) => engine?.post({ t: 'arr', op: 1, a: bars }),
  setBars: (s: number, bars: number) => engine?.post({ t: 'arr', op: 2, a: s, b: bars }),
  insert: (at: number, s: number) => engine?.post({ t: 'arr', op: 3, a: at, b: s }),
  remove: (at: number) => engine?.post({ t: 'arr', op: 4, a: at }),
  move: (from: number, to: number) => engine?.post({ t: 'arr', op: 5, a: from, b: to }),
  loop: (first: number, last: number) => engine?.post({ t: 'arr', op: 6, a: first, b: last }),
  seekBar: (bar: number) => engine?.post({ t: 'songSeek', bar }),
}

/** What to tell the user when the engine knows fewer models than the view offers. */
export const staleEngine = (engine: number, view: number) =>
  `dsp.wasm is out of date${engine ? ` (it knows ${engine} models)` : ''}: this page offers ${view}, so a new model stays an ARP 2600. Run make wasm, then hard-refresh.`

function onMessage(data: { t: string } & Record<string, unknown>) {
  if (data.t === 'pos') {
    player.position = data.sec as number
    player.playing = data.playing as boolean
    song.step = data.step as number
    song.playing = data.songPlaying as boolean
    song.entry = (data.entry as number | undefined) ?? -1
    song.local = (data.local as number | undefined) ?? -1
  } else if (data.t === 'song') {
    applySong(data)
  } else if (data.t === 'load') {
    meter.load = data.load as number
    meter.peak = data.peak as number | null
    meter.voices = data.voices as number
    meter.reduction = data.reduction as number
    meter.seen = true
  } else if (data.t === 'meters') {
    levels.values = data.levels as Float32Array
  } else if (data.t === 'params') {
    params.values[data.s as number] = Array.from(data.values as Float32Array)
  } else if (data.t === 'imported') {
    onImported(data.code as number)
  } else if (data.t === 'midi') {
    onMidi(data as unknown as MidiSummary)
  } else if (data.t === 'sample') {
    const slot = data.slot as number
    const wait = loading.get(slot)
    loading.delete(slot)
    const code = data.code as number
    if (code < 0) {
      sampleStore.error = `${wait?.name ?? 'sample'}: ${SAMPLE_ERRORS[code] ?? `load failed (${code})`}`
    } else {
      sampleStore.slots[slot] = {
        name: wait?.name ?? `sample ${slot + 1}`, frames: data.frames as number, root: data.root as number,
        loopStart: data.loopStart as number, loopEnd: data.loopEnd as number, peaks: data.peaks as Float32Array,
      }
      sampleStore.used = data.used as number
      sampleStore.cap = data.cap as number
    }
    wait?.done(code)
  } else if (data.t === 'sampleCleared') {
    const slot = data.slot as number
    sampleStore.slots[slot] = null
    sampleStore.used = data.used as number
    for (const [file, s] of slotOfFile) if (s === slot) slotOfFile.delete(file)
  } else if (data.t === 'ready') {
    const known = Object.keys(Model).length
    const engineModels = data.models as number
    if (engineModels < known) status.error = staleEngine(engineModels, known)
  } else if (data.t === 'pads') {
    padState.pads[data.s as number] = decodePads(data.values as Float32Array)
  } else if (data.t === 'zones') {
    zoneState.zones[data.s as number] = decodeZones(data.values as Float32Array)
  } else if (data.t === 'sysex') {
    const code = data.code as number
    if (code < 0) {
      sysex.error = SYSEX_ERRORS[code] ?? `load failed (${code})`
      return
    }
    const decoder = new TextDecoder('utf-8')
    sysex.names = (data.names as Uint8Array[]).map((n) => decoder.decode(n))
  }
}

function onMidi(m: MidiSummary) {
  if (m.code < 0 || !m.parts || !m.packed || !m.times) {
    player.error = LOAD_ERRORS[m.code] ?? `load failed (${m.code})`
    pending = null
    return
  }
  const decoder = new TextDecoder('utf-8')
  const rolls = pairNotes(m.packed, m.times)
  player.parts = m.parts.map((p) => ({
    channel: p.channel,
    name: decoder.decode(p.name),
    notes: p.notes,
    start: p.start,
    end: p.end,
    synth: p.synth,
    roll: rolls.get(p.channel) ?? [],
  }))
  // The engine puts the parts on synths 0, 1, 2…: show each of them.
  for (const p of player.parts) if (p.synth !== MUTE) show(p.synth)
  player.length = m.length ?? 0
  player.bar = m.bar || 2
  player.position = 0
  player.playing = false
  player.loaded = true
  loadedName = player.fileName
  // A picked setup file beats the last session for this file, which beats a
  // shipped one (the demo's).
  const next = pending?.from === 'file' ? pending : (storedSetup(loadedName) ?? pending)
  pending = null
  if (next) applySetup(next.setup, next.warnings)
}

// --- Setups (#41) -------------------------------------------------------------

/** The registry setups are built from; `Model` joins it with synth models (epic #28). */
const registry: Registry = {
  params: Param,
  global: GlobalParam,
  strip: StripParam,
  models: (registryTables as unknown as Record<string, Record<string, number> | undefined>).Model,
  maxSynths: MAX_SYNTHS,
  channels: CHANNELS,
}

// --- User presets (ADR-0014, epic #153) -----------------------------------------

/** The registry user presets are built from: the setup's and the effect type names. */
export const presetRegistry: PresetRegistry = { ...registry, insertTypes: InsertType, procTypes: ProcType }
if (typeof indexedDB !== 'undefined') void loadLibrary(presetRegistry)

/** A preset of what `target` holds now, named `name`. */
export const capturePreset = (name: string, target: Target, id?: string) => capture(name, target, params.values, presetRegistry, id)

/** Put `preset` on `target`; false when it is of another kind. */
export function applyPreset(preset: UserPreset, target: Target): boolean {
  const p = plan(preset, target, presetRegistry)
  if (!p || !engine) return false
  if (p.defaults !== undefined) engine.post({ t: 'defaults', s: p.defaults })
  for (const op of p.ops) engine.param(op.s, op.id as ParamId, op.v)
  // The values after clamping, and the defaults the plan didn't name.
  engine.post({ t: 'dump', s: target.kind === 'processor' ? 0 : target.s })
  return true
}

/** Whether `target` differs from `preset`. */
export const presetModified = (preset: UserPreset, target: Target) => modified(preset, target, params.values, presetRegistry)

/** A setup waiting for its MIDI file to load. */
let pending: { setup: Setup; warnings: string[]; from: 'file' | 'shipped' | 'session' } | null = null
/** The MIDI file whose setup the last session is kept under. */
let loadedName = ''

const sessionKey = (name: string) => `algo-synth:setup:${name}`

function state(): State {
  return {
    synths: synths.list,
    groups: layout.groups,
    layout: { order: layout.order, collapsed: layout.collapsed, hidden: layout.hidden },
    names: { strips: names.strips, parts: names.parts },
    values: params.values,
    routes: player.parts.map((p) => ({ channel: p.channel, synth: p.synth })),
    ...(player.loaded && { midi: { name: player.fileName, parts: player.parts.length } }),
  }
}

/** The current setup as the text of a `.synths.json` file. */
export const setupText = () => `${JSON.stringify(buildSetup(state(), registry), null, 2)}\n`

/** Download the current setup, named after the loaded MIDI file. */
export function saveSetup() {
  const stem = player.loaded ? player.fileName.replace(/\.midi?$/i, '') : 'algo-synth'
  const link = document.createElement('a')
  link.href = URL.createObjectURL(new Blob([setupText()], { type: 'application/json' }))
  link.download = `${stem}.synths.json`
  link.click()
  setTimeout(() => URL.revokeObjectURL(link.href), 1000)
}

/**
 * Open what the user picked: a MIDI file, a setup, or both in either order.
 * With a MIDI file the setup waits until its parts arrive; alone it applies
 * to the synths on screen.
 */
export async function openFiles(files: File[]): Promise<void> {
  const isSetup = (f: File) => /\.json$/i.test(f.name)
  const setupFile = files.find(isSetup)
  const midiFile = files.find((f) => !isSetup(f))
  player.notice = ''
  let parsed: ReturnType<typeof parseSetup> | null = null
  if (setupFile) {
    parsed = parseSetup(await setupFile.text(), registry)
    if (!parsed.ok) player.notice = `${setupFile.name}: ${parsed.error}; nothing applied`
  }
  if (midiFile) {
    pending = parsed?.ok ? { setup: parsed.setup, warnings: parsed.warnings, from: 'file' } : null
    await loadMidi(await midiFile.arrayBuffer(), midiFile.name)
  } else if (parsed?.ok) {
    await power()
    applySetup(parsed.setup, parsed.warnings)
  }
}

/** Apply a parsed setup to the engine and the view. */
function applySetup(setup: Setup, warnings: string[]) {
  if (!engine) return
  const midi = player.loaded ? { name: player.fileName, parts: player.parts.length } : undefined
  const plan = applyPlan(setup, registry, midi)
  const before = synths.list
  for (const op of plan.ops) {
    if (op.t === 'show') {
      synths.list = op.synths
      if (!op.synths.includes(synths.selected)) synths.selected = op.synths[0] ?? 0
    } else if (op.t === 'groups') {
      layout.groups = op.groups
    } else if (op.t === 'layout') {
      Object.assign(layout, op.layout)
    } else if (op.t === 'names') {
      setNames(op.names)
    } else if (op.t === 'reset') {
      engine.reset(op.s)
    } else if (op.t === 'param') {
      engine.param(op.s, op.id as ParamId, op.v)
    } else {
      const part = player.parts.find((p) => p.channel === op.channel)
      if (part) route(part, op.synth)
      else engine.post({ t: 'route', ch: op.channel, s: op.synth })
    }
  }
  // A part on a synth the setup doesn't list keeps that synth: as it was if
  // it was on screen, at the default patch if not.
  for (const p of player.parts) {
    if (p.synth === MUTE || synths.list.includes(p.synth)) continue
    if (before.includes(p.synth)) synths.list = [...synths.list, p.synth].sort((a, b) => a - b)
    else show(p.synth)
  }
  // Ask for the values again: the replies to the resets above would
  // otherwise arrive last and put the sliders back to the defaults. Synth 0
  // too, shown or not: the master and returns read the globals from it.
  for (const s of new Set([0, ...synths.list, ...layout.groups.map(groupStrip)])) engine.post({ t: 'dump', s })
  const all = [...warnings, ...plan.warnings]
  if (all.length) player.notice = `Setup: ${all.join('; ')}`
}

/** The last session's setup for this MIDI file, if any. */
function storedSetup(name: string): typeof pending {
  try {
    const text = localStorage.getItem(sessionKey(name))
    const parsed = text ? parseSetup(text, registry) : null
    return parsed?.ok ? { setup: parsed.setup, warnings: parsed.warnings, from: 'session' } : null
  } catch {
    return null
  }
}

// Keep the last session per MIDI file, a moment after anything changes. A
// convenience only: storage can be unavailable, and the file is the real save.
let saveTimer: ReturnType<typeof setTimeout> | undefined
watch(
  () => [params.values, synths.list, player.parts.map((p) => p.synth), names.strips, names.parts],
  () => {
    if (!player.loaded || !loadedName) return
    clearTimeout(saveTimer)
    saveTimer = setTimeout(() => {
      try {
        localStorage.setItem(sessionKey(loadedName), setupText())
      } catch {
        // Private window or storage full: keep playing.
      }
    }, 500)
  },
  { deep: true },
)

/** Pair note-ons with their offs per channel, for drawing only. */
function pairNotes(packed: Uint32Array, times: Float32Array) {
  const open = new Map<number, number>()
  const rolls = new Map<number, [number, number, number][]>()
  packed.forEach((p, i) => {
    const on = (p >> 15) & 1
    const ch = (p >> 8) & 0x0f
    const note = p & 0x7f
    const key = (ch << 7) | note
    const t = times[i] ?? 0
    if (on) {
      open.set(key, t)
    } else if (open.has(key)) {
      const list = rolls.get(ch) ?? []
      list.push([open.get(key) ?? t, t, note])
      rolls.set(ch, list)
      open.delete(key)
    }
  })
  return rolls
}
