// Main-thread side of the audio graph (ADR-0001, ADR-0003):
// AudioWorkletNode (dsp.wasm) -> AnalyserNode (scope) -> speakers.
// This file only sends messages; every musical decision is made in Rust.

import { reactive } from 'vue'
import type { ParamId, PresetId, SourceId } from './params'

const base = import.meta.env.BASE_URL

/** A routing choice for a MIDI part: a source id, or MUTE. */
export const MUTE = 255
export type Route = SourceId | typeof MUTE

/** One MIDI channel with notes, as the engine summarised it. */
export interface Part {
  channel: number
  name: string
  notes: number
  start: number
  end: number
  source: Route
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
  param(id: ParamId, v: number) {
    params.values[id] = v
    this.post({ t: 'param', id, v })
  }
  preset(id: PresetId) { this.post({ t: 'preset', id }) }
  noteOn(s: SourceId, n: number, v = 0.8) { this.post({ t: 'on', s, n, v }) }
  noteOff(s: SourceId, n: number) { this.post({ t: 'off', s, n }) }
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
})
/** DSP load as a share of real time (peak is null without a precise clock). */
export const meter = reactive({ load: 0, peak: null as number | null, voices: 0, seen: false })

/** Parameter values by id, as the engine last reported them (clamped). */
export const params = reactive({ values: [] as number[] })
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
  const res = await fetch(`${base}demo.mid`)
  await loadMidi(await res.arrayBuffer(), 'Canon in D (demo)')
}

export const play = () => engine?.post({ t: 'play' })
export const stop = () => engine?.post({ t: 'stop' })
export const seek = (sec: number) => engine?.post({ t: 'seek', sec })
export function route(part: Part, source: Route) {
  part.source = source
  engine?.post({ t: 'route', ch: part.channel, s: source })
}

interface MidiSummary {
  t: 'midi'
  code: number
  parts?: { channel: number; name: Uint8Array; notes: number; start: number; end: number; source: number }[]
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
    params.values = Array.from(data.values as Float32Array)
  } else if (data.t === 'midi') {
    onMidi(data as unknown as MidiSummary)
  }
}

function onMidi(m: MidiSummary) {
  if (m.code < 0 || !m.parts || !m.packed || !m.times) {
    player.error = LOAD_ERRORS[m.code] ?? `load failed (${m.code})`
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
    source: p.source as Route,
    roll: rolls.get(p.channel) ?? [],
  }))
  player.length = m.length ?? 0
  player.bar = m.bar || 2
  player.position = 0
  player.playing = false
  player.loaded = true
}

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
