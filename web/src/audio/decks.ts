// Decks (ADR-0029): deck A is the engine on the audio thread, the song the
// rest of the app edits; decks B–D each run their own engine in a Web Worker
// (public/deck-worker.js) that renders ahead into a ring the worklet reads.
// This file only makes the rings and workers and sends messages; the deck
// mixer is Rust (deck.rs).

import { reactive } from 'vue'
import { record, trail, type Ahead } from './decktrail'
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
  /** Where Play starts the deck: the master's next bar, next 8-bar phrase, or now. */
  start: Start
  /** Play was pressed and the deck waits for its bar. */
  cued: boolean
  /** Sync lock: the deck's bar lines are pulled onto deck A's at every bar. */
  sync: boolean
  /** How far the last sync found the deck off deck A's bar, in ms (positive: ahead), or null before one. */
  syncMs: number | null
  /** The arrangement entry playing, −1 without one, and the entries of the bars ahead (#449). */
  entry: number
  ahead: Ahead | null
  /** A worker deck's song: its scenes' names and bars, and their order (deck A's are the app's song). */
  scenes: { name: string; bars: number }[]
  arrange: number[]
}

/** Steps to the start a deck is cued to: a bar is 16 steps (ADR-0029). */
export const STARTS = { bar: 16, phrase: 128, now: 0 } as const
export type Start = keyof typeof STARTS

const fresh = (name: string): Deck => ({
  name, loaded: false, playing: false, step: 0, error: '', level: 1, side: DeckSide.Thru, peak: 0, dropped: 0, start: 'bar', cued: false,
  sync: true, syncMs: null, entry: -1, ahead: null, scenes: [], arrange: [],
})

export const decks = reactive({
  list: [fresh('This song'), fresh(''), fresh(''), fresh('')] as Deck[],
  crossfade: 0,
  /** The master's tempo, which every worker deck follows. */
  bpm: 0,
  /** SharedArrayBuffer needs a cross-origin isolated page; without it there is deck A only. */
  isolated: typeof crossOriginIsolated === 'boolean' && crossOriginIsolated,
})

const workers: (Worker | null)[] = [null, null, null, null]

/** Each deck's levels by step, for its lane (#449); not reactive, the lane redraws from it. */
export const trails = [trail(), trail(), trail(), trail()]

/** The worklet's peaks and dropped blocks for every deck, and the master's tempo it sends the decks. */
export function onDecks(peaks: number[], dropped: number[], bpm: number) {
  decks.list.forEach((d, i) => {
    d.peak = peaks[i] ?? 0
    d.dropped = dropped[i] ?? 0
    const t = trails[i]
    if (t && d.playing) record(t, d.step, d.peak, d.entry)
  })
  if (bpm) decks.bpm = bpm
}

/** Where deck `deck` is: its step, whether it plays, its entry and the bars ahead. */
export function onDeckPos(deck: number, step: number, playing: boolean, entry: number, ahead: Ahead | null) {
  const d = decks.list[deck]
  if (!d) return
  d.step = step
  d.playing = playing
  d.entry = entry
  d.ahead = ahead
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
  // The worklet and the worker talk directly: cues, tempo and bar lines (ADR-0029).
  const channel = new MessageChannel()
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
    engine.post({ t: 'deckAttach', deck, ring, port: channel.port1 }, [channel.port1])
  })
  const worker = new Worker(`${base}deck-worker.js`)
  worker.onmessage = ({ data }) => onWorker(deck, data)
  worker.postMessage({ t: 'init', module: engine.module, sampleRate: engine.ctx.sampleRate, ring, port: channel.port2 }, [channel.port2])
  const d = decks.list[deck]
  if (d && !d.sync) engine.post({ t: 'deckSync', deck, on: false })
  workers[deck] = worker
  return worker
}

function onWorker(deck: number, data: { t: string } & Record<string, unknown>) {
  const d = decks.list[deck]
  if (!d) return
  if (data.t === 'song') {
    d.loaded = data.ok as boolean
    d.error = data.ok ? '' : `The song did not parse (line ${data.line}, column ${data.col}).`
    d.scenes = (data.scenes as Deck['scenes'] | undefined) ?? []
    d.arrange = (data.arrange as number[] | undefined) ?? []
  } else if (data.t === 'pos') {
    onDeckPos(deck, data.step as number, data.playing as boolean, (data.entry as number | undefined) ?? -1, (data.ahead as Ahead | undefined) ?? null)
    if (d.playing) d.cued = false
  } else if (data.t === 'sync') {
    const rate = getEngine()?.ctx.sampleRate ?? 48_000
    d.syncMs = (1000 * (data.frames as number)) / rate
  } else if (data.t === 'late') {
    const rate = getEngine()?.ctx.sampleRate ?? 48_000
    d.error = `The cue came ${Math.round((1000 * (data.frames as number)) / rate)} ms late; the deck started at once.`
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

/** Start deck `deck` on the master's next bar or phrase, or now (its `start`). */
export function playDeck(deck: number) {
  const d = decks.list[deck]
  if (!d || !workers[deck]) return
  d.cued = true
  d.error = ''
  getEngine()?.post({ t: 'deckCue', deck, every: STARTS[d.start] })
}

export function stopDeck(deck: number) {
  const d = decks.list[deck]
  if (d) d.cued = false
  workers[deck]?.postMessage({ t: 'stop' })
}

export function setDeck(deck: number, field: 'level' | 'side', v: number) {
  const d = decks.list[deck]
  if (!d) return
  d[field] = v
  getEngine()?.post({ t: 'deckSet', deck, field: field === 'level' ? DeckField.Level : DeckField.Side, v })
}

/** Sync lock on or off for deck `deck` (1–3). */
export function setSync(deck: number, on: boolean) {
  const d = decks.list[deck]
  if (!d) return
  d.sync = on
  if (!on) d.syncMs = null
  getEngine()?.post({ t: 'deckSync', deck, on })
}

export function setCrossfade(x: number) {
  decks.crossfade = x
  getEngine()?.post({ t: 'deckXfade', x })
}
