// The deck lane (#449): what each deck has played, a level for every step,
// kept so the lane can draw it rising above the playhead. Only what the
// engine reported, stored for drawing; no music logic.

/** Steps to a bar and bars to an 8-bar phrase, as the clock counts them. */
export const STEPS_PER_BAR = 16
export const PHRASE = 8 * STEPS_PER_BAR
/** Steps of history a trail keeps: more than the lane shows. */
export const ROWS = 16 * STEPS_PER_BAR

export type Trail = {
  /** The highest level heard on each step, linear, by step modulo `ROWS`. */
  peaks: Float32Array
  /** The arrangement entry each step fell in, −1 without one. */
  entries: Int16Array
  /** The first and last step recorded, −1 before any. */
  first: number
  last: number
}

/** The bars ahead of the playhead: the entry of bar `from` and each after it. */
export type Ahead = { from: number; entries: number[] }

export const trail = (): Trail => ({ peaks: new Float32Array(ROWS), entries: new Int16Array(ROWS).fill(-1), first: -1, last: -1 })

function clear(t: Trail) {
  t.peaks.fill(0)
  t.entries.fill(-1)
  t.first = -1
  t.last = -1
}

/**
 * A level report: `peak` was heard up to step `step`, in entry `entry`. Steps
 * passed since the last report take it too; a step that went back (a stop,
 * a seek) starts the trail over.
 */
export function record(t: Trail, step: number, peak: number, entry: number) {
  if (step < 0) return
  if (step < t.last || (t.last >= 0 && step - t.last > ROWS)) clear(t)
  if (t.first < 0) t.first = step
  const from = t.last < 0 ? step : t.last + 1
  for (let k = from; k <= step; k++) {
    t.peaks[k % ROWS] = peak
    t.entries[k % ROWS] = entry
  }
  if (from > step) t.peaks[step % ROWS] = Math.max(t.peaks[step % ROWS] ?? 0, peak)
  t.last = step
}

/** Step `k`'s level and entry, or null when the trail doesn't hold it. */
export function at(t: Trail, k: number): { peak: number; entry: number } | null {
  if (k < t.first || k > t.last || k <= t.last - ROWS) return null
  return { peak: t.peaks[k % ROWS] ?? 0, entry: t.entries[k % ROWS] ?? -1 }
}

/** The entry bar `bar` falls in, as the engine last named it ahead; −1 when unknown. */
export const aheadEntry = (a: Ahead | null, bar: number) => a?.entries[bar - a.from] ?? -1

/** A linear level as the lane's width, 0..1, over 48 dB. */
export const width = (peak: number) => (peak > 0 ? Math.min(1, Math.max(0, 1 + (20 * Math.log10(peak)) / 48)) : 0)

/** A section's band colour, by its index. */
export const sectionColour = (s: number) => `hsl(${(160 + 67 * s) % 360} 45% 50%)`
