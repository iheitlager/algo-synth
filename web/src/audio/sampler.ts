// The sampler's view helpers (#125): the zones as the engine reports them, the pack manifest
// that `make samples` writes (#129), and the drawing maths. Nothing here decides anything
// musical: zones are set through `zone_set`, and the engine clamps them (sampler.rs).

import { LoopMode, ZoneField } from './params'

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

/**
 * The slots a new pack on synth `s` may free: those that came from a pack (`fromPacks`, file to
 * slot), that no other synth's zones use and that the new pack does not reuse. The store is
 * shared by every pack and has a cap, so loading a pack replaces the one it follows.
 */
export function evictable(fromPacks: ReadonlyMap<string, number>, s: number, zonesByS: readonly (readonly Zone[] | undefined)[], reuse: ReadonlySet<string>): number[] {
  const used = new Set<number>()
  zonesByS.forEach((zones, synth) => {
    if (synth !== s) for (const z of zones ?? []) if (z.sample >= 0) used.add(z.sample)
  })
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
