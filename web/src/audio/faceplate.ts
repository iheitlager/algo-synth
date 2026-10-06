// The maths behind the synth faceplates (spec 003 Req 9): the curve an envelope
// is drawn as, waveform icons, which option a stepped selector shows, how a
// value is printed, and the cells of the patch bay. Pure functions, no Vue and
// no engine: the engine stays the source of truth for every value, and these
// only turn a value into a picture, a label or an edit.

import type { Scale } from './console'
import { clamp, clamp01, hzText } from './console'

// --- envelope ----------------------------------------------------------------------

export interface EnvTimes {
  /** Attack, decay and release in seconds; sustain is a level 0..1. */
  a: number
  d: number
  s: number
  r: number
}

/** A sustain segment has the width of a segment this long, whatever the times. */
const SUSTAIN_SECONDS = 0.25
/** No segment is drawn narrower than this share of the curve. */
const MIN_SHARE = 0.07
/** How sharply a segment bends: the exponential's rate over its length. */
const BEND = 4
const POINTS = 14

/**
 * The widths of the four segments in `width` px. They grow with the square root
 * of their time, so 5 ms and 4 s both show, and none vanishes.
 */
export function envWidths(t: EnvTimes, width: number): [number, number, number, number] {
  const weight = [t.a, t.d, SUSTAIN_SECONDS, t.r].map((x) => Math.sqrt(Math.max(x, 0.001)))
  const total = weight.reduce((a, b) => a + b, 0)
  const share = weight.map((x) => Math.max(MIN_SHARE, x / total))
  const sum = share.reduce((a, b) => a + b, 0)
  const [a, d, s, r] = share.map((x) => (x / sum) * width) as [number, number, number, number]
  return [a, d, s, r]
}

/** An exponential from 0 to 1 over x in 0..1 that arrives exactly at 1. */
const bend = (x: number) => (1 - Math.exp(-BEND * x)) / (1 - Math.exp(-BEND))

/** The curve as points in a `w` × `h` box with `pad` px around it, level 1 at the top. */
export function envPoints(t: EnvTimes, w: number, h: number, pad = 3): [number, number][] {
  const inner = w - 2 * pad
  const top = pad
  const base = h - pad
  const y = (level: number) => base - clamp01(level) * (base - top)
  const [wa, wd, ws, wr] = envWidths(t, inner)
  const s = clamp01(t.s)
  const pts: [number, number][] = []
  let x = pad
  for (let i = 0; i <= POINTS; i++) pts.push([x + (wa * i) / POINTS, y(bend(i / POINTS))])
  x += wa
  for (let i = 1; i <= POINTS; i++) pts.push([x + (wd * i) / POINTS, y(1 - (1 - s) * bend(i / POINTS))])
  x += wd
  pts.push([x + ws, y(s)])
  x += ws
  for (let i = 1; i <= POINTS; i++) pts.push([x + (wr * i) / POINTS, y(s * (1 - bend(i / POINTS)))])
  return pts
}

/** The curve as an SVG path. */
export const envPath = (t: EnvTimes, w: number, h: number, pad = 3) =>
  envPoints(t, w, h, pad)
    .map(([x, y], i) => `${i ? 'L' : 'M'}${x.toFixed(1)} ${y.toFixed(1)}`)
    .join('')

// --- waveform icons -----------------------------------------------------------------

const SINE_POINTS = 24

/** An SVG path for a waveform's name (as the `Waveform` ids are named), or null. */
export function wavePath(name: string, w: number, h: number, pad = 2): string | null {
  const x0 = pad
  const x1 = w - pad
  const lo = h - pad
  const hi = pad
  const mid = h / 2
  const f = (n: number) => n.toFixed(1)
  switch (name) {
    case 'Saw':
      return `M${f(x0)} ${f(lo)}L${f(x1 - (x1 - x0) / 2)} ${f(hi)}L${f(x1 - (x1 - x0) / 2)} ${f(lo)}L${f(x1)} ${f(hi)}`
    case 'Pulse':
    case 'Square': {
      const a = x0 + (x1 - x0) * 0.25
      const b = x0 + (x1 - x0) * 0.75
      return `M${f(x0)} ${f(lo)}L${f(x0)} ${f(hi)}L${f(a + (b - a) / 2)} ${f(hi)}L${f(a + (b - a) / 2)} ${f(lo)}L${f(x1)} ${f(lo)}`
    }
    case 'Triangle':
      return `M${f(x0)} ${f(mid)}L${f(x0 + (x1 - x0) / 4)} ${f(hi)}L${f(x0 + (3 * (x1 - x0)) / 4)} ${f(lo)}L${f(x1)} ${f(mid)}`
    case 'Sine': {
      const pts = Array.from({ length: SINE_POINTS + 1 }, (_, i) => {
        const t = i / SINE_POINTS
        return `${i ? 'L' : 'M'}${f(x0 + (x1 - x0) * t)} ${f(mid - Math.sin(2 * Math.PI * t) * (mid - pad))}`
      })
      return pts.join('')
    }
    default:
      return null
  }
}

// --- stepped selectors --------------------------------------------------------------

/** The index of the option whose value is nearest `v` (an f32 reported by the engine is not exact). */
export function nearestStep(options: readonly (readonly [string, number])[], v: number): number {
  let best = 0
  let dist = Infinity
  options.forEach(([, value], i) => {
    const d = Math.abs(value - v)
    if (d < dist) {
      dist = d
      best = i
    }
  })
  return best
}

/** The index after a key press: arrows step and stay in range, Home and End jump. */
export function stepIndex(i: number, n: number, key: string): number | null {
  if (key === 'ArrowRight' || key === 'ArrowDown') return Math.min(n - 1, i + 1)
  if (key === 'ArrowLeft' || key === 'ArrowUp') return Math.max(0, i - 1)
  if (key === 'Home') return 0
  if (key === 'End') return n - 1
  return null
}

// --- knob scales --------------------------------------------------------------------

/** An even scale in whole steps of `step` (coarse and fine tune): the knob lands on a step. */
export const stepped = (lo: number, hi: number, step: number): Scale => ({
  toValue: (t) => clamp(lo + Math.round((clamp01(t) * (hi - lo)) / step) * step, lo, hi),
  toPos: (v) => clamp01((v - lo) / (hi - lo)),
})

// --- value readouts -----------------------------------------------------------------

/** How a knob prints its value. */
export type Unit = 'pct' | 'bip' | 'sec' | 'hz' | 'rate' | 'st' | 'ct' | 'width' | 'int'

const signed = (x: number, text: string) => (x > 0 ? `+${text}` : text)

/** A value in its own unit, as a knob's readout. */
export function fmtUnit(unit: Unit, v: number): string {
  switch (unit) {
    case 'pct':
      return `${Math.round(v * 100)}%`
    case 'bip':
      return signed(v, `${Math.round(v * 100)}%`)
    case 'width':
      return `${Math.round(v * 100)}%`
    case 'sec':
      return v < 1 ? `${Math.round(v * 1000)} ms` : `${v.toFixed(2)} s`
    case 'hz':
      return hzText(v)
    case 'rate':
      return v < 10 ? `${v.toFixed(2)} Hz` : `${v.toFixed(1)} Hz`
    case 'st':
      return signed(v, `${Math.round(v)} st`)
    case 'ct':
      return signed(v, `${Math.round(v)} ct`)
    case 'int':
      return `${Math.round(v)}`
  }
}

// --- patch bay ----------------------------------------------------------------------

/** The names the bay prints for the engine's ids: `Vco1Pitch` is `VCO 1 PITCH`, `SampleHold` is `S&H`. */
export function jackName(id: string): string {
  if (id === 'SampleHold') return 'S&H'
  if (id === 'Fenv') return 'Filter env'
  if (id === 'Adsr') return 'ADSR'
  if (id === 'Ar') return 'AR'
  if (id === 'Lfo') return 'LFO'
  if (id === 'Vca') return 'VCA'
  if (id === 'LfoRate') return 'LFO rate'
  if (id === 'Lfo2') return 'LFO 2'
  if (id === 'Lfo2Rate') return 'LFO 2 rate'
  if (id === 'HpCutoff') return 'HP cutoff'
  if (id === 'PulseWidth') return 'Pulse width'
  return id.replace(/Vco(\d)/, 'VCO $1 ').replace(/([a-z])([A-Z])/g, '$1 $2').replace(/\s+/g, ' ').trim()
}

/** One patch slot, as the engine reports it (spec 004 Req 7); source 0 is an empty slot. */
export interface PatchSlot {
  source: number
  dest: number
  amount: number
}

/** The slot joining `source` to `dest`, or −1. */
export const findSlot = (slots: readonly PatchSlot[], source: number, dest: number) =>
  slots.findIndex((s) => s.source === source && s.dest === dest)

/** The first empty slot, or −1 when all are used. */
export const freeSlot = (slots: readonly PatchSlot[]) => slots.findIndex((s) => s.source === 0)

/** A new connection starts at a middle amount. */
export const NEW_AMOUNT = 0.5

/**
 * What pressing the cell (`source`, `dest`) does: take its slot away, or join
 * the two in the first free slot. `null` when there is no free slot.
 */
export function pressCell(slots: readonly PatchSlot[], source: number, dest: number): { slot: number; value: PatchSlot } | null {
  const joined = findSlot(slots, source, dest)
  if (joined >= 0) return { slot: joined, value: { source: 0, dest: 0, amount: 0 } }
  const free = freeSlot(slots)
  return free >= 0 ? { slot: free, value: { source, dest, amount: NEW_AMOUNT } } : null
}

/** A slot's amount −1..1 as a knob position 0..1, and back. */
export const amountToPos = (a: number) => clamp01((a + 1) / 2)
export const posToAmount = (p: number) => clamp(p * 2 - 1, -1, 1)

// --- modular voice -----------------------------------------------------------------

/** The song voice the first track on synth `s` plays (#292), or −1 when it plays a factory voice. */
export const playedVoice = (tracks: readonly { synth: number; voice: number }[], s: number): number =>
  tracks.find((t) => t.synth === s && t.voice >= 0)?.voice ?? -1

// --- the Minimoog's keyboard control -------------------------------------------------

const THIRD = Math.fround(1 / 3)
const TWO_THIRDS = Math.fround(2 / 3)

/**
 * The Minimoog's two keyboard control switches for a key-track amount (#308):
 * switch 1 adds a third, switch 2 two thirds, both together full tracking.
 */
export function keySwitches(v: number): [boolean, boolean] {
  const n = Math.round(clamp01(v) * 3)
  return [n === 1 || n === 3, n >= 2]
}

/** The key-track amount the two keyboard control switches give. */
export const keyTrackOf = (one: boolean, two: boolean): number =>
  one && two ? 1 : two ? TWO_THIRDS : one ? THIRD : 0
