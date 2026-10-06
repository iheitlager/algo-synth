// Main-thread side of the audio graph (ADR-0001, ADR-0003):
// AudioWorkletNode (dsp.wasm) -> AnalyserNode (scope) -> speakers.
// This file only sends messages; every musical decision is made in Rust.

import { computed, reactive, shallowReactive } from 'vue'
import * as registryTables from './params'
import { buildOf, mismatch, versionOf, type Build } from './buildinfo'
import { GROUPS, feedsOf, groupStrip, padsOnGroup, moveBefore, orderStrips, routeOk } from './console'
import { modelDef, type ModelDef } from './models'
import { GlobalParam, InsertType, Model, PadField, Param, Preset, ProcType, StripParam, ZoneField, type ParamId, type PresetId } from './params'
import { lexer, wasmLexer } from './lex'
import { loadLibrary } from './library'
import { capture, modified, plan, type PresetRegistry, type Target, type UserPreset } from './presets'
import { cleanName, familyName, isFamilyName, names, renameStrip, setNames, stripName } from './names'
import {
  EMPTY_PAD, EMPTY_ZONE, SAMPLE_SLOTS, ZONES, decodePads, decodeZones, evictable, freeSlot, kitFiles, packFiles, padSets, parseKits,
  parseManifest, slotsUsedElsewhere, zoneSets, type Kit, type Pack, type Pad, type Zone,
} from './sampler'
import { MUTE, applyPlan, buildSetup, parseSetup, type Registry, type Setup, type State } from './setup'
import { isSongFile, keepSong, lastSong, songFileName } from './songfile'

const base = import.meta.env.BASE_URL

/** Synth slots the engine holds, each any model (`SYNTHS` in engine.rs). */
export const MAX_SYNTHS = 16
/** A song track's synth: a synth index, or MUTE. */
export { MUTE }
export type Route = number

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
    lexer.value = wasmLexer(module)
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

// One engine for the app. `status` and `files` are reactive so the UI follows them.
export const status = reactive({ running: false, error: '', sampleRate: 0 })
/** Which build of dsp.wasm is running, once the worklet says (#197). */
export const engineBuild = reactive<Build>({ version: '', build: '' })
/** The MIDI file last opened, and what opening files reported: an import, a setup's skipped entries, or why either failed. */
export const files = reactive({ fileName: '', notice: '' })
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
 * The knobs a modulation of the song drives now (ADR-0019, #208), as
 * `strip * 1024 + id`; the engine sends the set when it changes.
 */
export const modulated = shallowReactive<{ keys: Set<number> }>({ keys: new Set() })
export const modKey = (strip: number, id: number) => strip * 1024 + id

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

/** The `Out` of each of a drum kit's pads (`BdOut`, `SnOut`, …; #162). */
const PAD_OUT_IDS = Object.entries(Param).filter(([name]) => /^[A-Z][a-z]Out$/.test(name)).map(([, id]) => id as number)

/** A pad's or strip's Out option named as the console names it: a group by its own name (#127). */
export const outText = (name: string) => {
  const g = /^Group (\d)$/.exec(name)
  return g ? stripName(16 + Number(g[1]) - 1) : name
}

/** Remove group `g`: what fed it goes to the master, and the group goes back to its defaults. */
export function removeGroup(g: number) {
  const strip = groupStrip(g)
  // A kit's pads have outs of their own (#162): they go back to the master too (#218).
  const feeds = feedsOf(g, [...synths.list, ...layout.groups.map(groupStrip)], synths.list, PAD_OUT_IDS, Param.Out, (s, id) => params.values[s]?.[id])
  for (const [s, id] of feeds) engine?.param(s, id as ParamId, 0)
  // A pad sampler's pads too (#220).
  for (const s of synths.list) for (const i of padsOnGroup(g, padsOf(s))) setPad(s, i, PadField.Out, 0)
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
  main: 'synths' as 'synths' | 'mixer' | 'composer' | 'sound',
})

/** One hue per synth, so a part's notes match its synth's card. */
export const synthColour = (s: number) => `hsl(${(12 + 47 * s) % 360} 68% 62%)`

/** Show synth `s`, reset to the default patch unless it is already shown or `keep` its patch. */
function show(s: number, keep = false) {
  if (synths.list.includes(s)) return
  if (!keep) engine?.reset(s)
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

const KIND_FAMILY = { drums: 'drums', synth: 'mono', sampler: 'samplers' } as const

/**
 * Name a song track's synth by its kind (#177), so a 909 on synth 0 is `Drum 1`
 * and not `Synth 1`. A name the user typed stays, and so does a family name
 * it was given that already fits.
 */
function nameByKind(s: number, kind: SongTrack['kind']) {
  const own = names.strips[s]
  const family = KIND_FAMILY[kind]
  if (own !== undefined && (!isFamilyName(own) || own.startsWith(familyName(family, []).split(' ')[0]))) return
  names.strips[s] = familyName(family, synths.list.filter((i) => i !== s).map((i) => stripName(i)))
}

/** Remove synth `s` (never the last one). */
export function removeSynth(s: number) {
  if (synths.list.length <= 1) return
  synths.list = synths.list.filter((i) => i !== s)
  delete names.strips[s]
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
    // The last session's song (#105); a song file opened at the same time follows and replaces it.
    const kept = lastSong()
    if (kept) loadSong(kept)
  } catch (e) {
    status.error = e instanceof Error ? e.message : String(e)
  }
}

export const getEngine = (): AudioEngine | null => engine

// --- MIDI files (ADR-0022): opening one imports it as the song ----------------

const LOAD_ERRORS: Record<number, string> = {
  [-1]: 'not a MIDI file',
  [-2]: 'the file is truncated',
  [-3]: 'SMPTE timing is not supported',
  [-4]: 'the file has a malformed event',
  [-6]: 'the file is larger than 16 MiB',
}

/** Send a MIDI file to the engine to import as the song; powers audio on first (a click is a gesture). */
export async function loadMidi(bytes: ArrayBuffer, fileName: string): Promise<void> {
  await power()
  if (!engine) return
  files.fileName = fileName
  engine.post({ t: 'midi', bytes }, [bytes])
}

export async function loadDemo(): Promise<void> {
  const [mid, setup] = await Promise.all([fetch(`${base}demo.mid`), fetch(`${base}demo.synths.json`)])
  const parsed = setup.ok ? parseSetup(await setup.text(), registry) : null
  pending = parsed?.ok ? { setup: parsed.setup, warnings: parsed.warnings } : null
  files.notice = ''
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

export { stripName }

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

// The loads waiting on each slot, oldest first: the worklet answers in the
// order it was sent, so a second load for a busy slot waits its turn (#253).
const loading = new Map<number, { name: string; done: (code: number) => void }[]>()
/** Send a WAV file to the engine for `slot`; resolves with its frame count or a negative error code. */
export async function loadSample(slot: number, bytes: ArrayBuffer, name: string): Promise<number> {
  await power()
  if (!engine) return -5
  return new Promise((done) => {
    loading.set(slot, [...(loading.get(slot) ?? []), { name, done }])
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
/** One note of a fragment: start and length in ticks (48 to a bar, 3 to a sixteenth), MIDI note, accent. */
export interface SongNote { start: number; len: number; note: number; accent: boolean }
/**
 * A note fragment: its line as the engine prints it, the bars before it
 * repeats, its notes, whether it is a generator call (frozen before it is
 * edited) and whether that call is live.
 */
export interface SongNotes { text: string; bars: number; events: SongNote[]; generated: boolean; live: boolean }
export interface SongFrag { name: string; track: number; lanes: SongLane[]; notes: SongNotes | null }
/** A song track (#210, #213): its synth, kind, factory preset and the song setting it plays (−1 for none). */
export interface SongTrack { name: string; synth: Route; kind: 'drums' | 'synth' | 'sampler'; preset: number; setting: number; voice: number }
/** A Modular voice of the song (ADR-0020): its text and controls, in their units. */
export interface SongVoice { name: string; text: string; ctls: { name: string; lo: number; hi: number; def: number; exp: boolean }[] }
/** A setting of the song (#210): a factory preset and changes, named. */
export interface SongSetting { name: string; preset: number }
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
  /** The transport, the only one (ADR-0022): playing, or paused or stopped. */
  playing: false,
  /** With an arrangement (ADR-0015): the entry playing and the steps into it, −1 without. */
  entry: -1,
  local: -1,
  /** The arrangement (ADR-0015): sections with what each holds (by index), their order, lanes, scenes, loop bars (0 0 none). */
  sections: [] as SongSection[],
  arrange: [] as number[],
  autos: [] as string[],
  scenes: [] as string[],
  settings: [] as SongSetting[],
  /** Per track kind (drums, synth, sampler), which models play it: the engine's rule (#213). */
  fits: [[], [], []] as boolean[][],
  loop: [0, 0] as [number, number],
  /** The song's Modular voices, and why the last voice edit did not play (in its own lines). */
  voices: [] as SongVoice[],
  voiceError: null as { line: number; col: number; msg: string } | null,
})

/** Replace voice `i` with `text`, its voice line and ctl lines (the Sound screen, ADR-0020). */
export function editVoice(i: number, text: string) {
  const bytes = new TextEncoder().encode(text)
  engine?.post({ t: 'voice', i, bytes: bytes.buffer }, [bytes.buffer])
}

/** Send `text` to the engine to parse and play. */
export function loadSong(text: string) {
  song.draft = text
  const bytes = new TextEncoder().encode(text)
  engine?.post({ t: 'song', bytes: bytes.buffer }, [bytes.buffer])
}

/** Set one step (0 off, 1 hit, 2 accent); the engine sends the song back. */
export const setStep = (f: number, l: number, s: number, level: number) =>
  engine?.post({ t: 'step', f, l, s, level })

/** Add a sixteenth note at `tick` of fragment `f`; the engine sends the song back. */
export const addNote = (f: number, tick: number, note: number) => engine?.post({ t: 'note', f, op: 0, tick, note, len: 0 })
/** Remove the note that starts at `tick`. */
export const removeNote = (f: number, tick: number, note: number) => engine?.post({ t: 'note', f, op: 1, tick, note, len: 0 })
/** Make the note at `tick` `len` ticks long (the engine keeps it inside its bar and clear of the next note). */
export const setNoteLength = (f: number, tick: number, note: number, len: number) =>
  engine?.post({ t: 'note', f, op: 2, tick, note, len })
/** Replace fragment `f`'s generator call with the notes it is playing. */
export const freezeFrag = (f: number) => engine?.post({ t: 'freeze', f })

/** Play song track `t` on synth `s` (255 mutes). */
export const routeTrack = (t: number, s: Route) => engine?.post({ t: 'songRoute', track: t, s })

/** The song's tempo (BPM) and swing (percent); the engine updates the text and the clock. */
export const setSongTempo = (v: number) => engine?.post({ t: 'songTempo', v })
export const setSongSwing = (v: number) => engine?.post({ t: 'songSwing', v })

/** The transport (ADR-0022): play goes on from where it paused, pause holds the place, stop goes back to the top. */
export const playSong = () => engine?.post({ t: 'songPlay' })
export const pauseSong = () => engine?.post({ t: 'songPause' })
export const stopSong = () => engine?.post({ t: 'songStop' })

/** Where the song is, in steps from the top (−1 before the first): in an arrangement it counts from the top of the arrangement, so it follows the loop. */
export const songPosition = computed(() => {
  if (song.entry < 0) return song.step
  const start = song.arrange.slice(0, song.entry).reduce((n, s) => n + (song.sections[s]?.bars ?? 0), 0)
  return song.local < 0 ? -1 : start * 16 + song.local
})

/** Ask the engine for the song it holds (when the composer opens). */
export const requestSong = () => engine?.post({ t: 'songDump' })
/** Print the mixer as it is into the song as `strip`, `group` and `master` lines (ADR-0018). */
export const writeMixerToSong = () => engine?.post({ t: 'mixWrite' })

// Opening a MIDI file turns it into the song (#173, ADR-0022): the engine
// converts it; the composer and the arranger show the result.
const IMPORT_ERRORS: Record<number, string> = {
  [-7]: 'the file has no notes',
  [-8]: 'the file has more notes, fragments or sections than a song holds',
  [-9]: 'the converted text did not parse (a bug)',
}
function onImported(code: number) {
  const next = pending
  pending = null
  if (code < 0) {
    files.notice = `${files.fileName}: ${IMPORT_ERRORS[code] ?? LOAD_ERRORS[code] ?? `import failed (${code})`}`
    return
  }
  files.notice = `Imported ${files.fileName} as the song: ${code} tracks in sections; edit it in the composer, chain it in the arranger.`
  if (next) applySetup(next.setup, next.warnings)
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
  if (data.ok) {
    song.draft = text
    keepSong(text)
  }
  song.text = text
  song.tempo = data.tempo as number
  song.swing = data.swing as number
  song.tracks = (data.tracks as { name: Uint8Array; synth: number; kind: number; preset?: number; setting?: number; voice?: number }[]).map((t) => ({
    name: decoder.decode(t.name),
    synth: t.synth,
    kind: (['drums', 'synth', 'sampler'] as const)[t.kind] ?? 'drums',
    preset: t.preset ?? -1,
    setting: t.setting ?? -1,
    voice: t.voice ?? -1,
  }))
  type RawVoice = { name: Uint8Array; text: Uint8Array; ctls: { name: Uint8Array; lo: number; hi: number; def: number; exp: boolean }[] }
  song.voices = ((data.voices as RawVoice[] | undefined) ?? []).map((v) => ({
    name: decoder.decode(v.name),
    text: decoder.decode(v.text),
    ctls: v.ctls.map((c) => ({ ...c, name: decoder.decode(c.name) })),
  }))
  song.settings = ((data.settings ?? []) as { name: Uint8Array; preset: number }[]).map((st) => ({
    name: decoder.decode(st.name),
    preset: st.preset,
  }))
  song.fits = (data.fits as boolean[][] | undefined) ?? [[], [], []]
  // The engine put each track on a synth with its preset (#210): show them as they are.
  if (data.ok) for (const t of song.tracks) if (t.synth !== MUTE) {
    show(t.synth, true)
    nameByKind(t.synth, t.kind)
  }
  song.frags = (data.frags as {
    name: Uint8Array; track: number; lanes: { pad: number; steps: Uint8Array }[]
    notes: { text: Uint8Array; bars: number; events: [number, number, number, number][]; generated: boolean; live: boolean } | null
  }[]).map((f) => ({
    name: decoder.decode(f.name),
    track: f.track,
    lanes: f.lanes.map((l) => ({ pad: l.pad, steps: Array.from(l.steps) })),
    notes: f.notes
      ? {
          text: decoder.decode(f.notes.text),
          bars: f.notes.bars,
          generated: f.notes.generated,
          live: f.notes.live,
          events: f.notes.events.map(([start, len, note, accent]) => ({ start, len, note, accent: accent === 1 })),
        }
      : null,
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

/**
 * A track's patch from the composer (#213): a factory preset, one of the song's
 * settings, or the synth's sound saved as a new setting. The engine rewrites
 * the track line, sets the synth and prints the song back.
 */
export const trackEdit = {
  preset: (t: number, preset: number) => engine?.post({ t: 'track', op: 0, track: t, a: preset }),
  setting: (t: number, i: number) => engine?.post({ t: 'track', op: 1, track: t, a: i }),
  save: (t: number) => engine?.post({ t: 'track', op: 2, track: t }),
}

/** What to tell the user when the engine knows fewer models than the view offers. */
export const staleEngine = (engine: number, view: number) =>
  `dsp.wasm is out of date${engine ? ` (it knows ${engine} models)` : ''}: this page offers ${view}, so a new model stays an ARP 2600. Run make wasm, then hard-refresh.`

function onMessage(data: { t: string } & Record<string, unknown>) {
  if (data.t === 'pos') {
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
  } else if (data.t === 'voice') {
    const e = data.error as { line: number; col: number; msg: Uint8Array } | null
    song.voiceError = e ? { line: e.line, col: e.col, msg: new TextDecoder('utf-8').decode(e.msg) } : null
  } else if (data.t === 'mods') {
    modulated.keys = new Set(data.keys as number[])
  } else if (data.t === 'imported') {
    onImported(data.code as number)
  } else if (data.t === 'sample') {
    const slot = data.slot as number
    const queue = loading.get(slot)
    const wait = queue?.shift()
    if (!queue?.length) loading.delete(slot)
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
    engineBuild.version = versionOf(data.version as number)
    engineBuild.build = buildOf(data.build as number)
    if (engineModels < known) status.error = staleEngine(engineModels, known)
    else status.error = mismatch(engineBuild) || status.error
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

// --- Setups (#41) -------------------------------------------------------------

/** The registry setups are built from; `Model` joins it with synth models (epic #28). */
const registry: Registry = {
  params: Param,
  global: GlobalParam,
  strip: StripParam,
  models: (registryTables as unknown as Record<string, Record<string, number> | undefined>).Model,
  maxSynths: MAX_SYNTHS,
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
let pending: { setup: Setup; warnings: string[] } | null = null
function state(): State {
  return {
    synths: synths.list,
    groups: layout.groups,
    layout: { order: layout.order, collapsed: layout.collapsed, hidden: layout.hidden },
    names: { strips: names.strips },
    values: params.values,
  }
}

/** The current setup as the text of a `.synths.json` file. */
export const setupText = () => `${JSON.stringify(buildSetup(state(), registry), null, 2)}\n`

function download(text: string, type: string, name: string) {
  const link = document.createElement('a')
  link.href = URL.createObjectURL(new Blob([text], { type }))
  link.download = name
  link.click()
  setTimeout(() => URL.revokeObjectURL(link.href), 1000)
}

/** Download the current setup, named after the loaded MIDI file. */
export function saveSetup() {
  const stem = files.fileName ? files.fileName.replace(/\.midi?$/i, '') : 'algo-synth'
  download(setupText(), 'application/json', `${stem}.synths.json`)
}

/** The song file last opened, so Save song writes it back under its name. */
let songName = ''

/** Download the song the engine plays as `.song` text (#105), named after the song or MIDI file opened. */
export function saveSong() {
  download(song.text, 'text/plain', songFileName(songName || files.fileName || 'algo-synth'))
}

/**
 * Open what the user picked: a MIDI file, a setup, a song, or any of them
 * together. With a MIDI file the setup waits until it is imported; alone it
 * applies to the synths on screen. A song goes to the composer; one that does
 * not parse shows its error there and the playing song plays on.
 */
export async function openFiles(picked: File[]): Promise<void> {
  const isSetup = (f: File) => /\.json$/i.test(f.name)
  const setupFile = picked.find(isSetup)
  const songFile = picked.find((f) => isSongFile(f.name))
  const midiFile = picked.find((f) => !isSetup(f) && !isSongFile(f.name))
  files.notice = ''
  if (songFile) {
    await power()
    songName = songFile.name
    loadSong(await songFile.text())
    view.main = 'composer'
  }
  let parsed: ReturnType<typeof parseSetup> | null = null
  if (setupFile) {
    parsed = parseSetup(await setupFile.text(), registry)
    if (!parsed.ok) files.notice = `${setupFile.name}: ${parsed.error}; nothing applied`
  }
  if (midiFile) {
    pending = parsed?.ok ? { setup: parsed.setup, warnings: parsed.warnings } : null
    await loadMidi(await midiFile.arrayBuffer(), midiFile.name)
  } else if (parsed?.ok) {
    await power()
    applySetup(parsed.setup, parsed.warnings)
  }
}

/** Apply a parsed setup to the engine and the view. */
function applySetup(setup: Setup, warnings: string[]) {
  if (!engine) return
  const plan = applyPlan(setup, registry)
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
    } else {
      engine.param(op.s, op.id as ParamId, op.v)
    }
  }
  // A song track on a synth the setup doesn't list keeps that synth on screen.
  for (const t of song.tracks) {
    if (t.synth !== MUTE && !synths.list.includes(t.synth)) synths.list = [...synths.list, t.synth].sort((a, b) => a - b)
  }
  // Ask for the values again: the replies to the resets above would
  // otherwise arrive last and put the sliders back to the defaults. Synth 0
  // too, shown or not: the master and returns read the globals from it.
  for (const s of new Set([0, ...synths.list, ...layout.groups.map(groupStrip)])) engine.post({ t: 'dump', s })
  const all = [...warnings, ...plan.warnings]
  if (all.length) files.notice = `Setup: ${all.join('; ')}`
}
