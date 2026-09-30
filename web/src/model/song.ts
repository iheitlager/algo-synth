// The composition model as the UI sees it (ADR-0005).
//
// The engine owns the real model: it will receive the song as data and play
// it on the audio thread. Until the sequencer lands (plan.md MVP 3-4) this
// is a static demo, so the panes have something true-to-shape to draw.

export type SourceKind = 'Mono' | 'Wave' | 'Drums'
export type EffectKind = 'Drive' | 'Filter' | 'Crush' | 'Chorus'
export type GeneratorKind = 'Euclid' | 'Markov' | 'Walk' | 'Arp' | 'Mutate'
/** Where a clip's pattern came from. */
export type Origin = 'hand' | 'algo' | 'score'

/** A track owns exactly one source, up to four inserts, and two send levels. */
export interface Track {
  id: string
  name: string
  source: SourceKind
  inserts: EffectKind[]
  /** Send levels to the two buses: [delay, reverb], 0..1. */
  sends: [number, number]
}

/** A pattern placed on a track at a bar. */
export interface Clip {
  track: string
  /** First bar, 1-based. */
  bar: number
  bars: number
  pattern: string
  origin: Origin
}

/** An algo loop: a seeded generator writing patterns into a track. */
export interface AlgoLoop {
  id: string
  generator: GeneratorKind
  params: Record<string, number>
  scale: string
  target: string
  /** live: a new pattern every cycle; frozen: committed to a clip. */
  mode: 'live' | 'frozen'
  seed: number
  /** Preview only (step velocities 0..1); the engine will send the real ones. */
  preview: number[]
}

export interface Song {
  bpm: number
  bars: number
  tracks: Track[]
  clips: Clip[]
  loops: AlgoLoop[]
}

const e = (k: number, n: number): number[] =>
  Array.from({ length: n }, (_, i) => ((i * k) % n < k ? 1 : 0))

export const demoSong: Song = {
  bpm: 112,
  bars: 32,
  tracks: [
    { id: 'kit', name: 'Kit', source: 'Drums', inserts: ['Drive'], sends: [0, 0.15] },
    { id: 'bass', name: 'Bass', source: 'Mono', inserts: ['Filter'], sends: [0.1, 0] },
    { id: 'lead', name: 'Lead', source: 'Mono', inserts: [], sends: [0.35, 0.3] },
    { id: 'pad', name: 'Pad', source: 'Wave', inserts: ['Chorus'], sends: [0.2, 0.6] },
    { id: 'viv', name: 'Violins I-VI', source: 'Mono', inserts: [], sends: [0, 0.4] },
  ],
  clips: [
    { track: 'kit', bar: 1, bars: 8, pattern: 'E(5,16)', origin: 'algo' },
    { track: 'kit', bar: 9, bars: 16, pattern: 'E(7,16)', origin: 'algo' },
    { track: 'bass', bar: 5, bars: 12, pattern: 'walk', origin: 'algo' },
    { track: 'lead', bar: 9, bars: 4, pattern: 'riff A', origin: 'hand' },
    { track: 'lead', bar: 17, bars: 8, pattern: 'arp', origin: 'algo' },
    { track: 'pad', bar: 1, bars: 24, pattern: 'sweep', origin: 'hand' },
    { track: 'viv', bar: 25, bars: 8, pattern: 'RV 269 i', origin: 'score' },
  ],
  loops: [
    { id: 'L1', generator: 'Euclid', params: { k: 5, n: 16, rot: 0 }, scale: '—', target: 'kit', mode: 'live', seed: 1, preview: e(5, 16) },
    { id: 'L2', generator: 'Walk', params: { step: 2, range: 12 }, scale: 'D dorian', target: 'bass', mode: 'live', seed: 7, preview: [1, 0, 0.6, 0, 1, 0, 0, 0.8, 1, 0, 0.5, 0, 1, 0.4, 0, 0.7] },
    { id: 'L3', generator: 'Arp', params: { rate: 16, oct: 2 }, scale: 'D dorian', target: 'lead', mode: 'frozen', seed: 3, preview: Array(16).fill(0.7) },
    { id: 'L4', generator: 'Markov', params: { order: 1, temp: 0.6 }, scale: 'D dorian', target: 'lead', mode: 'live', seed: 42, preview: [1, 0, 0, 0.5, 0, 0.9, 0, 0, 1, 0, 0.3, 0, 0, 0.8, 0, 0] },
  ],
}
