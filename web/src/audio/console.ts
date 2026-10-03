// The maths behind the mixer console (#52): knob and fader geometry, value
// formats, and the curves drawn for the EQ and the compressor. Pure functions,
// no Vue and no engine: the engine stays the source of truth for every value,
// and these only turn a value into a picture or a label.

/** A knob turns this many degrees either side of straight up. */
export const SWEEP = 135

export const clamp = (x: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, x))
export const clamp01 = (x: number) => clamp(x, 0, 1)

/** The point at angle `a` degrees clockwise from straight up. */
export const polar = (cx: number, cy: number, r: number, a: number): [number, number] => {
  const rad = (a * Math.PI) / 180
  return [cx + r * Math.sin(rad), cy - r * Math.cos(rad)]
}

/** An SVG arc path from angle `a0` to `a1`; empty when they are the same. */
export function arcPath(cx: number, cy: number, r: number, a0: number, a1: number): string {
  if (Math.abs(a1 - a0) < 0.01) return ''
  const [x0, y0] = polar(cx, cy, r, a0)
  const [x1, y1] = polar(cx, cy, r, a1)
  const large = Math.abs(a1 - a0) > 180 ? 1 : 0
  const sweep = a1 > a0 ? 1 : 0
  return `M${x0.toFixed(2)} ${y0.toFixed(2)}A${r} ${r} 0 ${large} ${sweep} ${x1.toFixed(2)} ${y1.toFixed(2)}`
}

/** The knob angle for a value 0..1. */
export const knobAngle = (v: number) => -SWEEP + 2 * SWEEP * clamp01(v)

/** The arc a knob draws: from the start, or from the centre when bipolar. */
export const knobArc = (cx: number, cy: number, r: number, v: number, bipolar: boolean) =>
  arcPath(cx, cy, r, bipolar ? 0 : -SWEEP, knobAngle(v))

/** A value after dragging `dy` pixels up from `start` (`fine` slows it down). */
export const dragValue = (start: number, dy: number, fine: boolean) => clamp01(start + dy / (fine ? 700 : 170))

// --- exponential and linear ranges ---------------------------------------------

/** Position 0..1 to a value in `lo..hi`, evenly in octaves. */
export const logMap = (lo: number, hi: number) => (t: number) => lo * (hi / lo) ** clamp01(t)
/** The inverse of `logMap`. */
export const logPos = (lo: number, hi: number) => (v: number) =>
  v > lo ? clamp01(Math.log(v / lo) / Math.log(hi / lo)) : 0

// --- fader ----------------------------------------------------------------------

/** The quietest fader position that is not off. */
const FLOOR = 0.01
/** The taper: 0 dB at the top, about −6 dB at three quarters, −50 dB near the bottom. */
export const posToDb = (p: number) => (p < FLOOR ? -Infinity : -50 * (1 - clamp01(p)) ** 1.5)
export const dbToPos = (db: number) => (db <= -50 ? 0 : 1 - (Math.min(50, -db) / 50) ** (1 / 1.5))
/** The engine's linear `Level` (0..1) for a fader position, and back. */
export const posToLevel = (p: number) => (p < FLOOR ? 0 : 10 ** (posToDb(p) / 20))
export const levelToPos = (level: number) => (level <= 0 ? 0 : dbToPos(20 * Math.log10(level)))

/** A dB figure as the console prints it: `−6.0`, `+3.5`, `0.0`, `−∞`. */
export const dbText = (db: number, digits = 1) =>
  db === -Infinity || db < -59.5 ? '−∞' : `${db > 0 ? '+' : db < 0 ? '−' : ''}${Math.abs(db).toFixed(digits)}`

/** Peak level 0..1 as dB, floored at −90 for silence. */
export const levelDb = (level: number) => (level > 1e-4 ? 20 * Math.log10(level) : -90)
/** How many of `segs` LED segments light for a level: −54 dB is none, 0 dB all. */
export const ledSegments = (level: number, segs: number) => clamp01((levelDb(level) + 54) / 54) * segs

// --- equalizer --------------------------------------------------------------------

export interface EqBand {
  type: 'low' | 'peak' | 'high'
  freq: number
  gainDb: number
  /** Only for `peak`. */
  q: number
}

/** The response in dB of one band at `f` Hz (RBJ cookbook, as the engine does). */
export function bandDb(b: EqBand, f: number, sampleRate = 48_000): number {
  if (b.gainDb === 0) return 0
  const A = 10 ** (b.gainDb / 40)
  const w0 = (2 * Math.PI * clamp(b.freq, 10, 0.45 * sampleRate)) / sampleRate
  const cos = Math.cos(w0)
  const sin = Math.sin(w0)
  let num: number[]
  let den: number[]
  if (b.type === 'peak') {
    const alpha = sin / (2 * b.q)
    num = [1 + alpha * A, -2 * cos, 1 - alpha * A]
    den = [1 + alpha / A, -2 * cos, 1 - alpha / A]
  } else {
    const alpha = (sin / 2) * Math.SQRT2
    const k = 2 * Math.sqrt(A) * alpha
    if (b.type === 'low') {
      num = [A * (A + 1 - (A - 1) * cos + k), 2 * A * (A - 1 - (A + 1) * cos), A * (A + 1 - (A - 1) * cos - k)]
      den = [A + 1 + (A - 1) * cos + k, -2 * (A - 1 + (A + 1) * cos), A + 1 + (A - 1) * cos - k]
    } else {
      num = [A * (A + 1 + (A - 1) * cos + k), -2 * A * (A - 1 + (A + 1) * cos), A * (A + 1 + (A - 1) * cos - k)]
      den = [A + 1 - (A - 1) * cos + k, 2 * (A - 1 - (A + 1) * cos), A + 1 - (A - 1) * cos - k]
    }
  }
  const w = (2 * Math.PI * f) / sampleRate
  const mag = (c: number[]) => {
    const re = (c[0] ?? 0) + (c[1] ?? 0) * Math.cos(w) + (c[2] ?? 0) * Math.cos(2 * w)
    const im = -((c[1] ?? 0) * Math.sin(w) + (c[2] ?? 0) * Math.sin(2 * w))
    return re * re + im * im
  }
  return 10 * Math.log10(mag(num) / mag(den))
}

/** The summed response of all bands at `f` Hz. */
export const eqDb = (bands: EqBand[], f: number) => bands.reduce((sum, b) => sum + bandDb(b, f), 0)

// --- compressor ---------------------------------------------------------------------

/** The output level in dB for an input level, with a hard knee and make-up gain. */
export const compOutDb = (inDb: number, thresholdDb: number, ratio: number, makeupDb: number) =>
  (inDb <= thresholdDb ? inDb : thresholdDb + (inDb - thresholdDb) / Math.max(1, ratio)) + makeupDb

// --- parameter scales ----------------------------------------------------------------

/** How a knob position 0..1 maps to a parameter's value and back. */
export interface Scale {
  toValue: (t: number) => number
  toPos: (v: number) => number
}

/** An even scale from `lo` to `hi`. */
export const lin = (lo: number, hi: number): Scale => ({
  toValue: (t) => lo + (hi - lo) * clamp01(t),
  toPos: (v) => clamp01((v - lo) / (hi - lo)),
})
/** An even-in-octaves scale from `lo` to `hi` (both above 0). */
export const exp = (lo: number, hi: number): Scale => ({ toValue: logMap(lo, hi), toPos: logPos(lo, hi) })

/** A frequency as the console prints it. */
export const hzText = (hz: number) => (hz >= 1000 ? `${(hz / 1000).toFixed(1)} k` : `${Math.round(hz)} Hz`)

// --- processor knobs ------------------------------------------------------------------

/** What knob A..E of an effect processor is, by type id (`ProcType`). */
export interface ProcKnob {
  label: string
  /** The readout for a position 0..1. */
  text?: (t: number) => string
  /** A button, not a knob: on from 0.5. */
  toggle?: boolean
  /** Where double-click puts the knob, as the engine starts it. */
  def: number
}

export const PROC_KNOBS: Record<number, ProcKnob[]> = {
  0: [],
  1: [
    { label: 'Time', def: 0.75, text: (t) => `${Math.round(logMap(1, 2000)(t))} ms` },
    { label: 'Fdbk', def: 0.4, text: (t) => `${Math.round(t * 95)}%` },
    { label: 'Tone', def: 0.7, text: (t) => hzText(logMap(500, 20_000)(t)) },
    { label: 'Ping-pong', def: 0, toggle: true },
  ],
  2: [
    { label: 'Size', def: 0.65, text: (t) => `${logMap(0.1, 10)(t).toFixed(1)} s` },
    { label: 'Damp', def: 0.3, text: (t) => `${Math.round(t * 100)}%` },
    { label: 'Pre', def: 0.1, text: (t) => `${Math.round(t * 100)} ms` },
  ],
}
