// Export WAV (#705): the song bounced offline by its own engine in a worker
// (public/bounce-worker.js), rendered and encoded in Rust. This file only
// starts the worker, sends it the song and what the song text doesn't hold
// (resources.ts), shows how far it got and downloads the file.

import { reactive } from 'vue'
import { download, files, getEngine, opened, song } from './engine'
import { resources } from './resources'

const base = import.meta.env.BASE_URL

/** The choices the export offers: a rate and bits per sample (32 is float). */
export const FORMATS = [
  { label: '48 kHz · 24-bit', rate: 48_000, bits: 24 },
  { label: '96 kHz · 24-bit', rate: 96_000, bits: 24 },
  { label: '48 kHz · 32-bit float', rate: 48_000, bits: 32 },
  { label: '44.1 kHz · 16-bit (CD)', rate: 44_100, bits: 16 },
] as const
export type ExportFormat = (typeof FORMATS)[number]

/** The bounce under way: how far (0–1, the tail past 1 shown as 1), and why the last failed. */
export const bounce = reactive({ busy: false, progress: 0, error: '', peakDb: 0 })

// Ends the bounce under way.
let stop = () => {}

/** The `.wav` name for the song: after its file, or the MIDI file opened. */
export const wavName = (stem: string) => `${stem.replace(/\.(midi?|song)$/i, '') || 'algo-synth'}.wav`

/** Bounce the song as it is now and download it; resolves when done, failed or cancelled. */
export function exportWav(format: ExportFormat, bars = 0): Promise<void> {
  const engine = getEngine()
  if (!engine || bounce.busy || !song.text) return Promise.resolve()
  Object.assign(bounce, { busy: true, progress: 0, error: '' })
  const name = wavName(opened.name || files.fileName || 'algo-synth')
  const text = new TextEncoder().encode(song.text).buffer
  const { msgs, transfer } = resources()
  const w = new Worker(`${base}bounce-worker.js`)
  return new Promise((done) => {
    const finish = (error = '') => {
      w.terminate()
      stop = () => {}
      Object.assign(bounce, { busy: false, error })
      if (error) files.notice = `${name} was not exported: ${error}`
      done()
    }
    w.onerror = (e) => finish(e.message || 'the export worker failed')
    w.onmessage = ({ data }) => {
      switch (data.t) {
        case 'progress': bounce.progress = Math.min(1, data.frames / Math.max(1, data.end)); break
        case 'error': finish(data.why); break
        case 'done':
          bounce.peakDb = 20 * Math.log10(Math.max(1e-10, data.peak))
          download(data.wav, 'audio/wav', name)
          finish()
          break
      }
    }
    w.postMessage({
      t: 'bounce', module: engine.module, rate: format.rate, appRate: engine.ctx.sampleRate,
      song: text, resources: msgs, bars, bits: format.bits,
    }, [text, ...transfer])
    stop = () => finish()
  })
}

/** Stop the bounce under way; nothing is downloaded. */
export const cancelExport = () => stop()
