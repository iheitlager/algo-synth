// Main-thread side of the audio graph (ADR-0001, ADR-0003):
// AudioWorkletNode (dsp.wasm) -> AnalyserNode (scope) -> speakers.
// This file only sends messages; every musical decision is made in Rust.

import { reactive } from 'vue'
import type { ParamId, SourceId } from './params'

const base = import.meta.env.BASE_URL

class AudioEngine {
  constructor(
    readonly ctx: AudioContext,
    readonly node: AudioWorkletNode,
    readonly analyser: AnalyserNode,
  ) {}

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

  param(id: ParamId, v: number) { this.node.port.postMessage({ t: 'param', id, v }) }
  noteOn(s: SourceId, n: number, v = 0.8) { this.node.port.postMessage({ t: 'on', s, n, v }) }
  noteOff(s: SourceId, n: number) { this.node.port.postMessage({ t: 'off', s, n }) }
  panic() { this.node.port.postMessage({ t: 'panic' }) }
}

// One engine for the app. `status` is reactive so the UI can follow it.
export const status = reactive({ running: false, error: '', sampleRate: 0 })
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
