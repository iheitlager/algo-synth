// Decks (ADR-0029): deck A is the engine on the audio thread, the song the
// rest of the app edits; decks B–D each run their own engine in a Web Worker
// (public/deck-worker.js) that renders ahead into a ring the worklet reads.
// This file only makes the rings and workers and sends messages; the deck
// mixer is Rust (deck.rs).

import { reactive } from 'vue'
import { getEngine } from './engine'
import { DeckField, DeckSide } from './params'

const base = import.meta.env.BASE_URL

/** Decks A–D; A is index 0 (`DECKS` in deck.rs). */
export const DECKS = 4
/** Samples per block (`BLOCK` in engine.rs). */
const BLOCK = 128
/** Blocks a ring holds; the worker keeps 4 ahead (deck-worker.js). */
const SLOTS = 16

export type Deck = {
  /** The song file's name; deck A plays the app's own song. */
  name: string
  /** A worker deck has a song that parsed. */
  loaded: boolean
  playing: boolean
  /** The worker deck's clock step, for its position. */
  step: number
  /** Why the last song didn't load, or ''. */
  error: string
  level: number
  side: number
  /** Linear peak after the deck's gain, as last reported. */
  peak: number
  /** Blocks that came too late and were dropped since the deck started. */
  dropped: number
}

const fresh = (name: string): Deck => ({ name, loaded: false, playing: false, step: 0, error: '', level: 1, side: DeckSide.Thru, peak: 0, dropped: 0 })

export const decks = reactive({
  list: [fresh('This song'), fresh(''), fresh(''), fresh('')] as Deck[],
  crossfade: 0,
  /** SharedArrayBuffer needs a cross-origin isolated page; without it there is deck A only. */
  isolated: typeof crossOriginIsolated === 'boolean' && crossOriginIsolated,
})

const workers: (Worker | null)[] = [null, null, null, null]

/** The worklet's peaks and dropped blocks for every deck. */
export function onDecks(peaks: number[], dropped: number[]) {
  decks.list.forEach((d, i) => {
    d.peak = peaks[i] ?? 0
    d.dropped = dropped[i] ?? 0
  })
}

/** Start deck `deck`'s worker (1–3) the first time it is used: its ring, then its engine. */
async function ensureWorker(deck: number): Promise<Worker | null> {
  const engine = getEngine()
  if (!engine || !decks.isolated) return null
  const existing = workers[deck]
  if (existing) return existing
  const ring = {
    ctrl: new SharedArrayBuffer(2 * 4),
    audio: new SharedArrayBuffer(SLOTS * 2 * BLOCK * 4),
    seq: new SharedArrayBuffer(SLOTS * 4),
  }
  new Int32Array(ring.seq).fill(-1)
  // The worklet counts blocks: it stamps the ring with its block number first,
  // so the worker starts there and not at zero.
  await new Promise<void>((resolve) => {
    const port = engine.node.port
    const done = ({ data }: MessageEvent) => {
      if (data.t === 'deckAttached' && data.deck === deck) {
        port.removeEventListener('message', done)
        resolve()
      }
    }
    port.addEventListener('message', done)
    engine.post({ t: 'deckAttach', deck, ring })
  })
  const worker = new Worker(`${base}deck-worker.js`)
  worker.onmessage = ({ data }) => onWorker(deck, data)
  worker.postMessage({ t: 'init', module: engine.module, sampleRate: engine.ctx.sampleRate, ring })
  workers[deck] = worker
  return worker
}

function onWorker(deck: number, data: { t: string } & Record<string, unknown>) {
  const d = decks.list[deck]
  if (!d) return
  if (data.t === 'song') {
    d.loaded = data.ok as boolean
    d.error = data.ok ? '' : `The song did not parse (line ${data.line}, column ${data.col}).`
  } else if (data.t === 'pos') {
    d.step = data.step as number
    d.playing = data.playing as boolean
  }
}

/** Load a .song file into deck `deck` (1–3); it waits, stopped. */
export async function loadDeck(deck: number, file: File) {
  const d = decks.list[deck]
  if (!d || deck < 1) return
  const worker = await ensureWorker(deck)
  if (!worker) {
    d.error = decks.isolated ? 'Power on first.' : 'Decks need a cross-origin isolated page.'
    return
  }
  const bytes = await file.arrayBuffer()
  d.name = file.name
  worker.postMessage({ t: 'stop' })
  worker.postMessage({ t: 'song', bytes }, [bytes])
}

export function playDeck(deck: number) { workers[deck]?.postMessage({ t: 'play' }) }
export function stopDeck(deck: number) { workers[deck]?.postMessage({ t: 'stop' }) }

export function setDeck(deck: number, field: 'level' | 'side', v: number) {
  const d = decks.list[deck]
  if (!d) return
  d[field] = v
  getEngine()?.post({ t: 'deckSet', deck, field: field === 'level' ? DeckField.Level : DeckField.Side, v })
}

export function setCrossfade(x: number) {
  decks.crossfade = x
  getEngine()?.post({ t: 'deckXfade', x })
}
