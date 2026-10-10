// The note view's geometry (#168): where a note sits in a clip's grid and
// what a click or a drag on the grid means. Pure, so the component only draws
// and the engine keeps the music (ADR-0001).
import type { SongNote } from './engine'

/** Ticks in a bar, and in a sixteenth: the engine's note grid (ADR-0016). */
export const TICKS_PER_BAR = 48
export const TICKS_PER_STEP = 3

const NAMES = ['c', 'c#', 'd', 'd#', 'e', 'f', 'f#', 'g', 'g#', 'a', 'a#', 'b']

/** `c4` is 60. */
export const noteName = (n: number) => `${NAMES[n % 12]}${Math.floor(n / 12) - 1}`
export const isBlack = (n: number) => [1, 3, 6, 8, 10].includes(n % 12)

/** The rows to draw, lowest to highest pitch: the notes' range with a row of room, never under an octave. */
export function rollRange(events: SongNote[], minRows = 12): { lo: number; hi: number } {
  if (!events.length) return { lo: 55, hi: 55 + minRows - 1 }
  let lo = Math.min(...events.map((e) => e.note)) - 1
  let hi = Math.max(...events.map((e) => e.note)) + 1
  while (hi - lo + 1 < minRows) {
    if ((hi - lo) % 2 === 0) hi++
    else lo--
  }
  return { lo: Math.max(lo, 0), hi: Math.min(hi, 127) }
}

/** The tick (snapped to a sixteenth) and pitch under a point `fx`, `fy` (0 to 1 from the top left) of the grid. */
export function cellAt(fx: number, fy: number, bars: number, lo: number, hi: number): { tick: number; note: number } {
  const steps = bars * (TICKS_PER_BAR / TICKS_PER_STEP)
  const step = Math.min(Math.max(Math.floor(fx * steps), 0), steps - 1)
  const row = Math.min(Math.max(Math.floor(fy * (hi - lo + 1)), 0), hi - lo)
  return { tick: step * TICKS_PER_STEP, note: hi - row }
}

/** A note's box in percent of the grid. */
export function noteBox(e: SongNote, bars: number, lo: number, hi: number) {
  const total = bars * TICKS_PER_BAR
  const rows = hi - lo + 1
  return { left: (100 * e.start) / total, width: (100 * e.len) / total, top: (100 * (hi - e.note)) / rows, height: 100 / rows }
}

/** The note sounding at `tick` on `note`, if any. */
export const noteAt = (events: SongNote[], tick: number, note: number) =>
  events.find((e) => e.note === note && tick >= e.start && tick < e.start + e.len)

/** The length a drag to `fx` gives a note that starts at `start`: whole sixteenths, at least one. */
export function dragLength(start: number, fx: number, bars: number): number {
  const end = Math.round((fx * bars * TICKS_PER_BAR) / TICKS_PER_STEP) * TICKS_PER_STEP
  return Math.max(TICKS_PER_STEP, end - start)
}

/** Which sixteenth of the clip the song's step counter is on (it counts from the top of the song). */
export const stepIn = (step: number, bars: number) => (step < 0 ? -1 : step % (bars * (TICKS_PER_BAR / TICKS_PER_STEP)))
