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
  3: [
    { label: 'Rate', def: 0.4, text: (t) => `${logMap(0.1, 8)(t).toFixed(2)} Hz` },
    { label: 'Depth', def: 0.5, text: (t) => `${Math.round(t * 100)}%` },
    { label: 'Delay', def: 0.4, text: (t) => `${(5 + 25 * t).toFixed(1)} ms` },
    { label: 'Spread', def: 0.5, text: (t) => `${Math.round(t * 180)}°` },
    { label: 'Tone', def: 0.8, text: (t) => hzText(logMap(500, 20_000)(t)) },
  ],
  4: [
    { label: 'Rate', def: 0.35, text: (t) => `${logMap(0.05, 5)(t).toFixed(2)} Hz` },
    { label: 'Depth', def: 0.7, text: (t) => `${Math.round(t * 100)}%` },
    { label: 'Manual', def: 0.35, text: (t) => `${logMap(0.5, 10)(t).toFixed(1)} ms` },
    { label: 'Fdbk', def: 0.7, text: (t) => `${Math.round((t - 0.5) * 190)}%` },
    { label: 'Tone', def: 0.8, text: (t) => hzText(logMap(500, 20_000)(t)) },
  ],
}

// --- insert slots -----------------------------------------------------------------------

/** The short name a strip shows for an insert type id (`InsertType`). */
export const INSERT_SHORT = ['—', 'OVR', 'DST', 'FZZ', 'EQ', 'CMP', 'VOC']

const driveKnobs: ProcKnob[] = [
  { label: 'Amount', def: 0.5, text: (t) => `+${Math.round(t * 40)} dB` },
  { label: 'Tone', def: 0.5, text: (t) => hzText(logMap(200, 20_000)(t)) },
  { label: 'Level', def: 0.5 },
]
const dbAround = (t: number) => `${(t - 0.5) * 30 > 0 ? '+' : ''}${((t - 0.5) * 30).toFixed(1)} dB`

/**
 * What knob A..E of an insert slot is, by type id. At the default positions
 * (A–D 0.5, E 0) every type is neutral: an EQ is flat, a compressor adds no
 * make-up.
 */
export const INSERT_KNOBS: Record<number, ProcKnob[]> = {
  0: [],
  1: driveKnobs,
  2: driveKnobs,
  3: driveKnobs,
  4: [
    { label: 'Low', def: 0.5, text: dbAround },
    { label: 'Mid Hz', def: 0.5, text: (t) => hzText(logMap(200, 8000)(t)) },
    { label: 'Mid', def: 0.5, text: dbAround },
    { label: 'High', def: 0.5, text: dbAround },
    { label: 'Mid Q', def: 0, text: (t) => (8 ** t).toFixed(1) },
  ],
  5: [
    { label: 'Thresh', def: 0.5, text: (t) => `${Math.round(-60 * (1 - t))} dB` },
    { label: 'Ratio', def: 0.5, text: (t) => `${logMap(1, 20)(t).toFixed(1)}:1` },
    { label: 'Attack', def: 0.5, text: (t) => `${logMap(0.1, 100)(t).toFixed(1)} ms` },
    { label: 'Release', def: 0.5, text: (t) => `${Math.round(logMap(10, 1000)(t))} ms` },
    { label: 'Make-up', def: 0, text: (t) => `+${(t * 24).toFixed(1)} dB` },
  ],
  // The vocoder (#161), following the synth its strip's Key names.
  6: [
    { label: 'Shift', def: 0.5, text: (t) => `${Math.round((t - 0.5) * 24)} st` },
    { label: 'Release', def: 0.5, text: (t) => `${Math.round(logMap(20, 500)(t))} ms` },
    { label: 'Unvoiced', def: 0, text: (t) => `${Math.round(t * 100)}%` },
    { label: 'Width', def: 0.5, text: (t) => `Q ${(12 * 0.25 ** t).toFixed(1)}` },
    { label: 'Dry', def: 0, text: (t) => `${Math.round(t * 100)}%` },
  ],
}

// --- groups, routing and layout (ADR-0010) -----------------------------------------------

/** Synth strips 0–15, then eight group buses 16–23. */
export const SYNTH_STRIPS = 16
export const GROUPS = 8
export const STRIPS = SYNTH_STRIPS + GROUPS
/** The strip index of group `g` (0–7). */
export const groupStrip = (g: number) => SYNTH_STRIPS + g

/** The `Out` that goes nowhere (#161): the strip still feeds its sends and any vocoder keyed to it. */
export const OUT_NONE = GROUPS + 1

/** Whether `strip` may go to `out` (0 master, 1–8 a group, 9 nowhere): a group only to a higher one. */
export const routeOk = (strip: number, out: number) =>
  out === 0 || out === OUT_NONE || (out >= 1 && out <= GROUPS && (strip < SYNTH_STRIPS || out - 1 > strip - SYNTH_STRIPS))

/** The destinations a strip can pick among the groups on screen, master first. */
export const outChoices = (strip: number, shownGroups: number[]) => [
  { out: 0, label: 'Master' },
  ...shownGroups.filter((g) => routeOk(strip, g + 1)).map((g) => ({ out: g + 1, label: `Group ${g + 1}` })),
  { out: OUT_NONE, label: 'None' },
]

/** A pad's Out choices: Master and the groups on screen, and the one it has now so the pull-down never goes blank (#218). */
export const visibleOuts = <T extends readonly [string, number]>(options: readonly T[], shownGroups: readonly number[], current: number) =>
  options.filter(([, v]) => v === 0 || v === current || shownGroups.includes(v - 1))

/**
 * What feeds group `g` (0–7), for when it is removed and has to go back to the master: each of `strips` whose
 * `Out` (parameter `stripOut`) is the group, and each of `outIds` (a kit's pad outs) on the `kits`, as
 * `[strip, parameter id]`. `value` reads a strip's parameter.
 */
export function feedsOf(
  g: number,
  strips: readonly number[],
  kits: readonly number[],
  outIds: readonly number[],
  stripOut: number,
  value: (strip: number, id: number) => number | undefined,
) {
  const found: [number, number][] = []
  for (const s of strips) if (s !== groupStrip(g) && value(s, stripOut) === g + 1) found.push([s, stripOut])
  for (const s of kits) for (const id of outIds) if (value(s, id) === g + 1) found.push([s, id])
  return found
}

/** What a strip's routing and solo state is, as the engine reports it. */
export interface StripState {
  mute: boolean
  solo: boolean
  out: number
  /** The groups it feeds directly (0–7): a drum kit's individual outs (#162). */
  feeds?: number[]
}

/** The groups a strip passes through, nearest first. */
function chain(strips: StripState[], i: number): number[] {
  const groups: number[] = []
  let out = strips[i]?.out ?? 0
  while (out !== 0 && out <= GROUPS && groups.length < GROUPS) {
    const g = SYNTH_STRIPS + out - 1
    groups.push(g)
    out = strips[g]?.out ?? 0
  }
  return groups
}

/**
 * Which strips are heard, as the engine's mixer decides it: a muted strip is
 * not; with a solo, only soloed strips, the groups they pass through, and what
 * feeds a soloed group.
 */
export function heardStrips(strips: StripState[]): boolean[] {
  const soloed = strips.map((s, i) => (s.solo ? i : -1)).filter((i) => i >= 0)
  const heard = strips.map(() => soloed.length === 0)
  for (const j of soloed) {
    heard[j] = true
    for (const g of chain(strips, j)) heard[g] = true
    // A kit's individual outs keep the groups they feed heard, and what those feed.
    for (const f of strips[j]?.feeds ?? []) {
      const g = groupStrip(f)
      heard[g] = true
      for (const h of chain(strips, g)) heard[h] = true
    }
  }
  strips.forEach((_, i) => {
    if (chain(strips, i).some((g) => strips[g]?.solo)) heard[i] = true
  })
  return heard.map((h, i) => h && !strips[i]?.mute)
}

/**
 * The strips in the order the console shows them: those of `saved` that are on
 * screen, then the rest, synths before groups.
 */
export function orderStrips(saved: number[], shown: number[]): number[] {
  const on = new Set(shown)
  const kept = saved.filter((id, i) => on.has(id) && saved.indexOf(id) === i)
  const rest = [...on].filter((id) => !kept.includes(id)).sort((a, b) => a - b)
  return [...kept, ...rest]
}

/** `order` with `id` moved to just before `target` (or to the end when `target` is -1). */
export function moveBefore(order: number[], id: number, target: number): number[] {
  const without = order.filter((x) => x !== id)
  const at = target === -1 ? without.length : without.indexOf(target)
  if (at < 0 || !order.includes(id)) return order
  without.splice(at, 0, id)
  return without
}

/** The colour of group `g`, apart from the synth hues. */
export const groupColour = (g: number) => `hsl(${(205 + 47 * g) % 360} 52% 58%)`
