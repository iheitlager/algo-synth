// The Spectral Lab (ADR-0017, spec 009 Req 7): its own engine to play the
// A/B, a worker with a second dsp.wasm instance for the analysis, and a
// BroadcastChannel to hand a resynthesis to the main window. Everything here
// sends, copies and draws; the analysis and the resynthesis are Rust.

import { Preset, ZoneField } from './params'

export const LAB_CHANNEL = 'algo-synth-spectral'

/** The part of a BroadcastChannel used here, so a test can pass a fake. */
export interface Port {
  postMessage(msg: unknown): void
  onmessage: ((e: MessageEvent) => void) | null
  close(): void
}

export type ToMain = { t: 'hello' } | { t: 'sample'; id: number; name: string; bytes: ArrayBuffer }
export type ToLab = { t: 'here' } | { t: 'loaded'; id: number; code: number } | { t: 'gone' }

/**
 * The main window's side: answers a lab's hello, loads a sample it sends with
 * `load` (the frame count or a negative code goes back), says when it goes.
 */
export function serveLab(port: Port, load: (name: string, bytes: ArrayBuffer) => Promise<number>) {
  const send = (m: ToLab) => port.postMessage(m)
  port.onmessage = (e: MessageEvent) => {
    const m = e.data as ToMain
    if (m?.t === 'hello') send({ t: 'here' })
    else if (m?.t === 'sample' && m.bytes instanceof ArrayBuffer) {
      void load(m.name, m.bytes).then((code) => send({ t: 'loaded', id: m.id, code }))
    }
  }
  // A lab opened before this page (a reload) learns it is back.
  send({ t: 'here' })
  return {
    close() {
      send({ t: 'gone' })
      port.close()
    },
  }
}

/** The lab's side: whether a main window answers, and sending it a sample. */
export function connectMain(port: Port, onMain: (here: boolean) => void, waitMs = 1500) {
  let next = 1
  const pending = new Map<number, (code: number) => void>()
  let heard = false
  port.onmessage = (e: MessageEvent) => {
    const m = e.data as ToLab
    if (m?.t === 'here') {
      heard = true
      onMain(true)
    } else if (m?.t === 'gone') onMain(false)
    else if (m?.t === 'loaded') {
      pending.get(m.id)?.(m.code)
      pending.delete(m.id)
    }
  }
  port.postMessage({ t: 'hello' } satisfies ToMain)
  const silent = setTimeout(() => { if (!heard) onMain(false) }, waitMs)
  return {
    /** Load `bytes` into the main window's sample store: its frames, or a negative code (−100: no answer). */
    send(name: string, bytes: ArrayBuffer): Promise<number> {
      const id = next++
      return new Promise((resolve) => {
        const timer = setTimeout(() => {
          pending.delete(id)
          resolve(-100)
        }, waitMs)
        pending.set(id, (code) => {
          clearTimeout(timer)
          resolve(code)
        })
        port.postMessage({ t: 'sample', id, name, bytes } satisfies ToMain)
      })
    },
    close() {
      clearTimeout(silent)
      port.close()
    },
  }
}

/** A spectrogram from the engine: `bands` byte levels a frame, log frequency from 30 Hz to Nyquist. */
export interface Spectrogram { levels: Uint8Array; bands: number }

/** One partial track: its first frame and a frequency and amplitude per frame. */
export interface Track { start: number; freq: Float32Array; amp: Float32Array }

/** The worker's flat tracks (start, n, n frequencies, n amplitudes, …) as tracks; a short tail is dropped. */
export function parseTracks(flat: Float32Array): Track[] {
  const out: Track[] = []
  let i = 0
  while (i + 2 <= flat.length) {
    const start = flat[i]
    const n = flat[i + 1]
    if (!(n >= 0) || i + 2 + 2 * n > flat.length) break
    out.push({ start, freq: flat.subarray(i + 2, i + 2 + n), amp: flat.subarray(i + 2 + n, i + 2 + 2 * n) })
    i += 2 + 2 * n
  }
  return out
}

/** What a negative code from analysing or loading means. */
export function codeMessage(code: number): string {
  const messages: Record<number, string> = {
    [-1]: 'not a WAV file',
    [-2]: 'the file is truncated',
    [-3]: 'unsupported format (PCM 16/24-bit or 32-bit float, mono or stereo)',
    [-4]: 'no audio in the file',
    [-6]: 'the file is too large',
    [-7]: 'no free sample slot',
    [-11]: 'no audio in the file',
    [-12]: 'longer than 60 seconds',
    [-13]: 'bad analysis settings',
    [-100]: 'the main window did not answer',
  }
  return messages[code] ?? `error ${code}`
}

/** The analysis worker: one request at a time, answered in order. */
export class Analyser {
  private waiting: ((data: any) => void)[] = []
  private bands = 0
  private constructor(private worker: Worker) {
    worker.onmessage = ({ data }) => this.waiting.shift()?.(data)
  }

  static async start(module: WebAssembly.Module): Promise<Analyser> {
    const a = new Analyser(new Worker(`${import.meta.env.BASE_URL}spectral-worker.js`))
    await a.ask({ t: 'init', module })
    return a
  }

  private ask(msg: object, transfer: Transferable[] = []): Promise<any> {
    return new Promise((resolve) => {
      this.waiting.push(resolve)
      this.worker.postMessage(msg, transfer)
    })
  }

  /**
   * Analyse a copy of a WAV at `rate`: the frame count (or a negative code),
   * the tracks, and its spectrogram (`bands` byte levels a frame).
   */
  async analyse(bytes: ArrayBuffer, rate: number, window: number, hop: number): Promise<{ code: number; tracks: Track[]; gram: Spectrogram }> {
    const copy = bytes.slice(0)
    const r = await this.ask({ t: 'analyse', bytes: copy, rate, window, hop }, [copy])
    this.bands = r.bands ?? 0
    return { code: r.code, tracks: parseTracks(r.tracks), gram: { levels: r.gram, bands: this.bands } }
  }

  /** The resynthesis of the `top` loudest tracks (0 all), shifted and stretched, as WAV bytes, with its spectrogram. */
  async render(top: number, shift: number, stretch: number): Promise<{ wav: ArrayBuffer; gram: Spectrogram }> {
    const r = await this.ask({ t: 'render', top, shift, stretch })
    return { wav: r.wav, gram: { levels: r.gram, bands: this.bands } }
  }
}

/** The lab's own engine: the same worklet and dsp.wasm as the app, two Samplers for the A/B. */
export class LabEngine {
  private loads = new Map<number, (r: { code: number; peaks?: Float32Array }) => void>()
  private constructor(readonly ctx: AudioContext, readonly node: AudioWorkletNode, readonly module: WebAssembly.Module) {
    node.port.onmessage = ({ data }) => {
      if (data.t === 'sample') {
        this.loads.get(data.slot)?.(data)
        this.loads.delete(data.slot)
      }
    }
  }

  static async start(): Promise<LabEngine> {
    const base = import.meta.env.BASE_URL
    const ctx = new AudioContext({ latencyHint: 'interactive' })
    const module = await WebAssembly.compileStreaming(fetch(`${base}dsp.wasm`))
    await ctx.audioWorklet.addModule(`${base}worklet.js`)
    const node = new AudioWorkletNode(ctx, 'algo-synth', {
      numberOfInputs: 0,
      outputChannelCount: [2],
      processorOptions: { module },
    })
    node.connect(ctx.destination)
    return new LabEngine(ctx, node, module)
  }

  private post(msg: object, transfer: Transferable[] = []) { this.node.port.postMessage(msg, transfer) }

  /**
   * Load a copy of a WAV into `slot` and play it on synth `s` across the keys:
   * the frames (or a negative code) and the waveform's (min, max) peaks.
   */
  async play(s: number, slot: number, bytes: ArrayBuffer): Promise<{ code: number; peaks: Float32Array }> {
    const copy = bytes.slice(0)
    const r = await new Promise<{ code: number; peaks?: Float32Array }>((resolve) => {
      this.loads.set(slot, resolve)
      this.post({ t: 'sample', slot, bytes: copy }, [copy])
    })
    const loaded = { code: r.code, peaks: r.peaks ?? new Float32Array(0) }
    if (r.code < 0) return loaded
    this.post({ t: 'preset', s, id: Preset.SamplerKeys })
    this.post({ t: 'zonesClear', s })
    for (const [field, v] of [[ZoneField.Sample, slot], [ZoneField.KeyLo, 0], [ZoneField.KeyHi, 127], [ZoneField.Root, -1]]) {
      this.post({ t: 'zone', s, zone: 0, field, v })
    }
    return loaded
  }

  noteOn(s: number, n: number, v = 0.8) { this.post({ t: 'on', s, n, v }) }
  noteOff(s: number, n: number) { this.post({ t: 'off', s, n }) }
  panic() { this.post({ t: 'panic' }) }
}
