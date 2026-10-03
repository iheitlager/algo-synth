// Main-thread side of the audio graph (ADR-0001, ADR-0003):
// AudioWorkletNode (dsp.wasm) -> AnalyserNode (scope) -> speakers.
// This file only sends messages; every musical decision is made in Rust.

import { reactive, watch } from 'vue'
import * as registryTables from './params'
import { GlobalParam, Param, type ParamId, type PresetId } from './params'
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
export const meter = reactive({ load: 0, peak: null as number | null, voices: 0, seen: false })

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

/** One hue per synth, so a part's notes match its synth's card. */
export const synthColour = (s: number) => `hsl(${(12 + 47 * s) % 360} 68% 62%)`

/** Show synth `s`, reset to the default patch unless it is already shown. */
function show(s: number) {
  if (synths.list.includes(s)) return
  engine?.reset(s)
  synths.list = [...synths.list, s].sort((a, b) => a - b)
}

/** Add a synth on the lowest free index and select it; false when all 16 are shown. */
export function addSynth(): boolean {
  const free = Array.from({ length: MAX_SYNTHS }, (_, i) => i).find((i) => !synths.list.includes(i))
  if (free === undefined) return false
  show(free)
  synths.selected = free
  return true
}

/** Remove synth `s` (never the last one); parts playing on it are muted. */
export function removeSynth(s: number) {
  if (synths.list.length <= 1) return
  synths.list = synths.list.filter((i) => i !== s)
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

function onMessage(data: { t: string } & Record<string, unknown>) {
  if (data.t === 'pos') {
    player.position = data.sec as number
    player.playing = data.playing as boolean
  } else if (data.t === 'load') {
    meter.load = data.load as number
    meter.peak = data.peak as number | null
    meter.voices = data.voices as number
    meter.seen = true
  } else if (data.t === 'params') {
    params.values[data.s as number] = Array.from(data.values as Float32Array)
  } else if (data.t === 'midi') {
    onMidi(data as unknown as MidiSummary)
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
  models: (registryTables as unknown as Record<string, Record<string, number> | undefined>).Model,
  maxSynths: MAX_SYNTHS,
  channels: CHANNELS,
}

/** A setup waiting for its MIDI file to load. */
let pending: { setup: Setup; warnings: string[]; from: 'file' | 'shipped' | 'session' } | null = null
/** The MIDI file whose setup the last session is kept under. */
let loadedName = ''

const sessionKey = (name: string) => `algo-synth:setup:${name}`

function state(): State {
  return {
    synths: synths.list,
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
  for (const s of new Set([0, ...synths.list])) engine.post({ t: 'dump', s })
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
  () => [params.values, synths.list, player.parts.map((p) => p.synth)],
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
