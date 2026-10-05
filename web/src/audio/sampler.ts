// The sampler's view helpers (#125): the zones as the engine reports them, the pack manifest
// that `make samples` writes (#129), and the drawing maths. Nothing here decides anything
// musical: zones are set through `zone_set`, and the engine clamps them (sampler.rs).

import { LoopMode, PadField, ZoneField } from './params'

/** Zones a synth holds (`ZONES` in sampler.rs) and the sample slots of the store (`SLOTS` in sample.rs). */
export const ZONES = 64
export const SAMPLE_SLOTS = 64
/** Fields in a zone, the length of `ZoneField`; the worklet sends this many per zone. */
export const ZONE_FIELDS = Object.keys(ZoneField).length

/** One zone as the engine holds it: `sample` and `root` are -1 when empty and unset. */
export interface Zone {
  sample: number
  keyLo: number
  keyHi: number
  velLo: number
  velHi: number
  root: number
  tune: number
  level: number
  loop: number
  loopStart: number
  loopEnd: number
  seqLen: number
  seqPos: number
  release: boolean
}

/** What the engine holds for a synth that has no zones yet. */
export const EMPTY_ZONE: Zone = {
  sample: -1, keyLo: 0, keyHi: 127, velLo: 1, velHi: 127, root: -1, tune: 0, level: 1,
  loop: LoopMode.Off, loopStart: 0, loopEnd: 0, seqLen: 1, seqPos: 1, release: false,
}

/** The zones from a dump of `ZONES * ZONE_FIELDS` values, zone by zone. */
export function decodeZones(values: ArrayLike<number>): Zone[] {
  return Array.from({ length: ZONES }, (_, z) => {
    const at = (f: number) => values[z * ZONE_FIELDS + f] ?? 0
    return {
      sample: at(ZoneField.Sample), keyLo: at(ZoneField.KeyLo), keyHi: at(ZoneField.KeyHi),
      velLo: at(ZoneField.VelLo), velHi: at(ZoneField.VelHi), root: at(ZoneField.Root),
      tune: at(ZoneField.Tune), level: at(ZoneField.Level), loop: at(ZoneField.Loop),
      loopStart: at(ZoneField.LoopStart), loopEnd: at(ZoneField.LoopEnd),
      seqLen: at(ZoneField.SeqLen), seqPos: at(ZoneField.SeqPos), release: at(ZoneField.Release) >= 0.5,
    }
  })
}

/** The lowest empty zone, or -1 when all are taken. */
export const freeZone = (zones: readonly Zone[]) => zones.findIndex((z) => z.sample < 0)
/** The lowest empty sample slot, or -1 when the store is full. */
export const freeSlot = (slots: readonly unknown[]) => {
  for (let i = 0; i < SAMPLE_SLOTS; i++) if (!slots[i]) return i
  return -1
}

/** The sample slots that the zones or pads of every synth but `except` play. */
export function slotsUsedElsewhere(
  except: number, zonesByS: readonly (readonly Zone[] | undefined)[], padsByS: readonly (readonly Pad[] | undefined)[],
): Set<number> {
  const used = new Set<number>()
  zonesByS.forEach((zones, synth) => {
    if (synth !== except) for (const z of zones ?? []) if (z.sample >= 0) used.add(z.sample)
  })
  padsByS.forEach((pads, synth) => {
    if (synth !== except) for (const p of pads ?? []) if (p.sample >= 0) used.add(p.sample)
  })
  return used
}

/**
 * The slots a new pack or kit may free: those that came from a pack (`fromPacks`, file to slot),
 * that nothing else plays (`used`) and that the new one does not reuse. The store is shared by
 * every pack and has a cap, so loading one replaces the one it follows.
 */
export function evictable(fromPacks: ReadonlyMap<string, number>, used: ReadonlySet<number>, reuse: ReadonlySet<string>): number[] {
  return [...fromPacks].filter(([file, slot]) => !used.has(slot) && !reuse.has(file)).map(([, slot]) => slot)
}

const NAMES = ['C', 'C♯', 'D', 'D♯', 'E', 'F', 'F♯', 'G', 'G♯', 'A', 'A♯', 'B']
/** A MIDI note as a name, middle C being C4. */
export const keyName = (n: number) => `${NAMES[((n % 12) + 12) % 12]}${Math.floor(n / 12) - 1}`

// --- The pack manifest (tools/fetch_samples.py) --------------------------------

/** A zone of a pack, with the field names of `ZoneField`; `sample` is a path under `samples/`. */
export interface PackZone {
  sample: string
  keyLo: number
  keyHi: number
  velLo: number
  velHi: number
  root: number
  tune: number
  loop: number
  seqLen: number
  seqPos: number
  release: boolean
}

export interface Pack {
  id: string
  name: string
  license: string
  credit: string
  zones: PackZone[]
}

const num = (v: unknown, lo: number, hi: number) => typeof v === 'number' && Number.isFinite(v) && v >= lo && v <= hi

function validZone(z: unknown): z is PackZone {
  if (typeof z !== 'object' || z === null) return false
  const o = z as Record<string, unknown>
  return typeof o.sample === 'string' && o.sample !== '' && !o.sample.startsWith('/') && !o.sample.includes('..')
    && num(o.keyLo, 0, 127) && num(o.keyHi, 0, 127) && num(o.velLo, 1, 127) && num(o.velHi, 1, 127)
    && num(o.root, 0, 127) && num(o.tune, -1200, 1200) && num(o.loop, 0, 2)
    && num(o.seqLen, 1, 16) && num(o.seqPos, 1, 16) && typeof o.release === 'boolean'
}

/** The instruments of a manifest, leaving out an entry or a zone that is malformed or has too many zones. */
export function parseManifest(json: unknown): Pack[] {
  const list = (json as { instruments?: unknown } | null)?.instruments
  if (!Array.isArray(list)) return []
  return list.flatMap((p: Record<string, unknown>) => {
    const zones = Array.isArray(p?.zones) ? p.zones.filter(validZone) : []
    if (typeof p?.id !== 'string' || typeof p.name !== 'string' || zones.length === 0 || zones.length > ZONES) return []
    return [{ id: p.id, name: p.name, license: String(p.license ?? ''), credit: String(p.credit ?? ''), zones }]
  })
}

/** The distinct sample files of a pack, in first-use order. */
export const packFiles = (pack: Pack) => [...new Set(pack.zones.map((z) => z.sample))]

/** The `zone_set` calls that lay a pack's zones out, as [zone, field, value]; `slotOf` finds a file's slot. */
export function zoneSets(pack: Pack, slotOf: (file: string) => number | undefined): [number, number, number][] {
  const sets: [number, number, number][] = []
  pack.zones.forEach((z, i) => {
    const slot = slotOf(z.sample)
    if (slot === undefined) return
    sets.push(
      [i, ZoneField.Sample, slot], [i, ZoneField.KeyLo, z.keyLo], [i, ZoneField.KeyHi, z.keyHi],
      [i, ZoneField.VelLo, z.velLo], [i, ZoneField.VelHi, z.velHi], [i, ZoneField.Root, z.root],
      [i, ZoneField.Tune, z.tune], [i, ZoneField.Loop, z.loop], [i, ZoneField.SeqLen, z.seqLen],
      [i, ZoneField.SeqPos, z.seqPos], [i, ZoneField.Release, Number(z.release)],
    )
  })
  return sets
}

// --- Drawing -----------------------------------------------------------------

/** The rectangle of a zone in a `w` x `h` map with the key across and the velocity up. */
export function zoneRect(z: Zone, w: number, h: number) {
  const x0 = (Math.min(z.keyLo, z.keyHi) / 128) * w
  const x1 = ((Math.max(z.keyLo, z.keyHi) + 1) / 128) * w
  const y0 = ((128 - Math.max(z.velLo, z.velHi)) / 127) * h
  const y1 = ((128 - Math.min(z.velLo, z.velHi) + 1) / 127) * h
  return { x: x0, y: y0, w: Math.max(1, x1 - x0), h: Math.max(1, Math.min(h, y1) - y0) }
}

/** An SVG path of a waveform from (min, max) pairs, in a `w` x `h` box centred on the middle. */
export function peakPath(peaks: ArrayLike<number>, w: number, h: number): string {
  const bins = Math.floor(peaks.length / 2)
  if (bins === 0) return ''
  const y = (v: number) => (h / 2 - v * (h / 2)).toFixed(1)
  let top = ''
  let bottom = ''
  for (let b = 0; b < bins; b++) {
    const x = (((b + 0.5) / bins) * w).toFixed(1)
    top += `${b === 0 ? 'M' : 'L'}${x} ${y(peaks[2 * b + 1] ?? 0)}`
    bottom = `L${x} ${y(peaks[2 * b] ?? 0)}${bottom}`
  }
  return `${top}${bottom}Z`
}

/** The loop markers of a zone in frames: its own points, else the sample's, else none. */
export function loopOf(z: Zone, sample: { loopStart: number; loopEnd: number } | null): [number, number] | null {
  const [a, b] = z.loopStart === 0 && z.loopEnd === 0 ? [sample?.loopStart ?? 0, sample?.loopEnd ?? 0] : [z.loopStart, z.loopEnd]
  return b > a ? [a, b] : null
}

// --- Pads (the drum/pad sampler, #124) ----------------------------------------------

/** Pads in a kit (`PADS` in padsampler.rs) and the note of the first; pad i answers `PAD_FIRST_NOTE + i`. */
export const PADS = 16
export const PAD_FIRST_NOTE = 36
/** Fields in a pad, the length of `PadField`. */
export const PAD_FIELDS = Object.keys(PadField).length

/** One pad as the engine holds it: `sample` is -1 when it has none. */
export interface Pad {
  sample: number
  tune: number
  level: number
  pan: number
  decay: number
  choke: number
  velLevel: number
  velStart: number
  oneShot: boolean
  /** 0 the sampler's own strip, 1–8 a group (#220). */
  out: number
}
export const EMPTY_PAD: Pad = { sample: -1, tune: 0, level: 0.8, pan: 0, decay: 0, choke: 0, velLevel: 1, velStart: 0, oneShot: true, out: 0 }

/** The pads from a dump of `PADS * PAD_FIELDS` values, pad by pad. */
export function decodePads(values: ArrayLike<number>): Pad[] {
  return Array.from({ length: PADS }, (_, p) => {
    const at = (f: number) => values[p * PAD_FIELDS + f] ?? 0
    return {
      sample: at(PadField.Sample), tune: at(PadField.Tune), level: at(PadField.Level), pan: at(PadField.Pan),
      decay: at(PadField.Decay), choke: at(PadField.Choke), velLevel: at(PadField.VelLevel),
      velStart: at(PadField.VelStart), oneShot: at(PadField.OneShot) >= 0.5, out: at(PadField.Out),
    }
  })
}

/** The pads as a 4 x 4 grid, top row first, pad 1 at the bottom left as on an MPC. */
export const PAD_ROWS: readonly (readonly number[])[] = [[12, 13, 14, 15], [8, 9, 10, 11], [4, 5, 6, 7], [0, 1, 2, 3]]

/** A drum kit of the manifest: which sample each pad plays and how. */
export interface KitPad {
  pad: number
  sample: string
  tune: number
  level: number
  pan: number
  decay: number
  choke: number
  velLevel: number
  velStart: number
  oneShot: boolean
}
export interface Kit {
  id: string
  name: string
  license: string
  credit: string
  pads: KitPad[]
}

function validPad(p: unknown): p is KitPad {
  if (typeof p !== 'object' || p === null) return false
  const o = p as Record<string, unknown>
  return typeof o.sample === 'string' && o.sample !== '' && !o.sample.startsWith('/') && !o.sample.includes('..')
    && num(o.pad, 0, PADS - 1) && Number.isInteger(o.pad) && num(o.tune, -24, 24) && num(o.level, 0, 2) && num(o.pan, -1, 1)
    && num(o.decay, 0, 10) && num(o.choke, 0, 8) && num(o.velLevel, 0, 1) && num(o.velStart, 0, 1) && typeof o.oneShot === 'boolean'
}

/** The kits of a manifest (`kits`), leaving out one that is malformed or has no usable pad. */
export function parseKits(json: unknown): Kit[] {
  const list = (json as { kits?: unknown } | null)?.kits
  if (!Array.isArray(list)) return []
  return list.flatMap((k: Record<string, unknown>) => {
    const pads = Array.isArray(k?.pads) ? k.pads.filter(validPad) : []
    if (typeof k?.id !== 'string' || typeof k.name !== 'string' || pads.length === 0) return []
    return [{ id: k.id, name: k.name, license: String(k.license ?? ''), credit: String(k.credit ?? ''), pads }]
  })
}

export const kitFiles = (kit: Kit) => [...new Set(kit.pads.map((p) => p.sample))]

/** The `pad_set` calls that lay a kit out, as [pad, field, value]; `slotOf` finds a file's slot. */
export function padSets(kit: Kit, slotOf: (file: string) => number | undefined): [number, number, number][] {
  return kit.pads.flatMap((p) => {
    const slot = slotOf(p.sample)
    if (slot === undefined) return []
    return [
      [p.pad, PadField.Sample, slot], [p.pad, PadField.Tune, p.tune], [p.pad, PadField.Level, p.level],
      [p.pad, PadField.Pan, p.pan], [p.pad, PadField.Decay, p.decay], [p.pad, PadField.Choke, p.choke],
      [p.pad, PadField.VelLevel, p.velLevel], [p.pad, PadField.VelStart, p.velStart], [p.pad, PadField.OneShot, Number(p.oneShot)],
    ] as [number, number, number][]
  })
}
