import { describe, expect, it } from 'vitest'
import { LoopMode, PadField, ZoneField } from './params'
import {
  EMPTY_PAD, EMPTY_ZONE, PADS, PAD_FIELDS, PAD_ROWS, ZONES, evictable, slotsUsedElsewhere, ZONE_FIELDS, decodePads, kitFiles, padSets, parseKits, decodeZones, freeSlot, freeZone, keyName, loopOf, packFiles, parseManifest, peakPath,
  zoneRect, zoneSets, type Kit, type Pack,
} from './sampler'

const zone = (o: Record<string, unknown> = {}) => ({
  sample: 'instruments/a/C2.wav', keyLo: 33, keyHi: 38, velLo: 1, velHi: 127, root: 36, tune: 0, loop: 1, seqLen: 1, seqPos: 1,
  release: false, ...o,
})

describe('zone dumps', () => {
  it('decodes every field of every zone, and knows how many fields there are', () => {
    expect(ZONE_FIELDS).toBe(14)
    const v = new Float32Array(ZONES * ZONE_FIELDS)
    v[2 * ZONE_FIELDS + ZoneField.Sample] = 5
    v[2 * ZONE_FIELDS + ZoneField.KeyHi] = 60
    v[2 * ZONE_FIELDS + ZoneField.Release] = 1
    v[2 * ZONE_FIELDS + ZoneField.Loop] = LoopMode.Sustain
    const zones = decodeZones(v)
    expect(zones).toHaveLength(ZONES)
    expect(zones[2]).toMatchObject({ sample: 5, keyHi: 60, release: true, loop: LoopMode.Sustain })
    expect(zones[3]?.release).toBe(false)
  })

  it('finds the first empty zone and slot', () => {
    const zones = Array.from({ length: ZONES }, () => EMPTY_ZONE)
    expect(freeZone(zones)).toBe(0)
    expect(freeZone(zones.map((z) => ({ ...z, sample: 1 })))).toBe(-1)
    expect(freeZone([{ ...EMPTY_ZONE, sample: 2 }, EMPTY_ZONE])).toBe(1)
    expect(freeSlot([{}, {}, null])).toBe(2)
    expect(freeSlot(Array.from({ length: 64 }, () => ({})))).toBe(-1)
  })
})

describe('the pack manifest', () => {
  const manifest = {
    version: 1,
    instruments: [
      { id: 'a', name: 'A', license: 'CC0-1.0', credit: 'x', zones: [zone(), zone({ sample: 'instruments/a/F2.wav', keyLo: 39 })] },
      { id: 'bad-zone-only', name: 'B', zones: [zone({ keyLo: 300 })] },
      { id: 'escape', name: 'C', zones: [zone({ sample: '../etc/passwd' })] },
      { name: 'no id', zones: [zone()] },
    ],
  }

  it('keeps the well-formed instruments and drops the rest', () => {
    const packs = parseManifest(manifest)
    expect(packs.map((p) => p.id)).toEqual(['a'])
    expect(packs[0]?.zones).toHaveLength(2)
    expect(parseManifest(null)).toEqual([])
    expect(parseManifest({ instruments: 'no' })).toEqual([])
  })

  it('lists each file once and turns zones into zone_set calls', () => {
    const [pack] = parseManifest({ instruments: [{ ...manifest.instruments[0], zones: [zone(), zone({ keyLo: 39, keyHi: 44 }), zone({ sample: 'instruments/a/F2.wav' })] }] }) as [Pack]
    expect(packFiles(pack)).toEqual(['instruments/a/C2.wav', 'instruments/a/F2.wav'])
    const slots: Record<string, number> = { 'instruments/a/C2.wav': 4, 'instruments/a/F2.wav': 9 }
    const sets = zoneSets(pack, (f) => slots[f])
    expect(sets).toHaveLength(3 * 11)
    expect(sets.filter(([z, f]) => z === 2 && f === ZoneField.Sample)).toEqual([[2, ZoneField.Sample, 9]])
    expect(sets).toContainEqual([1, ZoneField.KeyLo, 39])
    expect(sets).toContainEqual([0, ZoneField.Root, 36])
    expect(sets).toContainEqual([0, ZoneField.Loop, 1])
    // A file that did not load leaves its zones out.
    expect(zoneSets(pack, (f) => (f.endsWith('C2.wav') ? 4 : undefined)).every(([z]) => z !== 2)).toBe(true)
  })
})

describe('replacing a pack or kit', () => {
  const z = (sample: number) => ({ ...EMPTY_ZONE, sample })
  const pad = (sample: number) => ({ ...EMPTY_PAD, sample })
  const packs = new Map([['a/C2.wav', 0], ['a/F2.wav', 1], ['b/C2.wav', 2], ['b/F2.wav', 3]])

  it('finds what other synths play, zones and pads alike, but not the synth itself', () => {
    const zones = [[z(0), z(1)], [z(2)]]
    const pads = [undefined, undefined, [pad(3), EMPTY_PAD]]
    expect([...slotsUsedElsewhere(0, zones, pads)].sort()).toEqual([2, 3])
    expect([...slotsUsedElsewhere(1, zones, pads)].sort()).toEqual([0, 1, 3])
    expect([...slotsUsedElsewhere(9, [], [])]).toEqual([])
  })

  it("frees the pack's own slots, but not what is in use or what the new one reuses", () => {
    expect(evictable(packs, new Set([2]), new Set(['b/F2.wav']))).toEqual([0, 1])
    expect(evictable(packs, new Set([2]), new Set())).toEqual([0, 1, 3])
    expect(evictable(new Map(), new Set(), new Set())).toEqual([])
  })
})

describe('pads', () => {
  it('decodes a dump and knows the grid', () => {
    expect(PAD_FIELDS).toBe(10)
    const v = new Float32Array(PADS * PAD_FIELDS)
    v[3 * PAD_FIELDS + PadField.Sample] = 4
    v[3 * PAD_FIELDS + PadField.Pan] = -0.5
    v[3 * PAD_FIELDS + PadField.OneShot] = 1
    v[3 * PAD_FIELDS + PadField.Out] = 3
    const pads = decodePads(v)
    expect(pads).toHaveLength(PADS)
    expect(pads[3]).toMatchObject({ sample: 4, pan: -0.5, oneShot: true, out: 3 })
    expect(pads[2]?.out).toBe(0)
    expect(pads[2]?.oneShot).toBe(false)
    expect(PAD_ROWS.flat().sort((a, b) => a - b)).toEqual(Array.from({ length: PADS }, (_, i) => i))
    expect(PAD_ROWS[3]).toEqual([0, 1, 2, 3])
  })

  const kitPad = (o: Record<string, unknown> = {}) => ({
    pad: 0, sample: 'kits/k/bd.wav', tune: 0, level: 0.8, pan: 0, decay: 0, choke: 0, velLevel: 1, velStart: 0, oneShot: true, ...o,
  })

  it('keeps well-formed kits and turns pads into pad_set calls', () => {
    const json = {
      kits: [
        { id: 'k', name: 'K', license: 'CC0-1.0', credit: 'x', pads: [kitPad(), kitPad({ pad: 6, sample: 'kits/k/ch.wav', choke: 1 }), kitPad({ pad: 99 })] },
        { id: 'bad', name: 'B', pads: [kitPad({ sample: '../x' })] },
        { name: 'no id', pads: [kitPad()] },
      ],
    }
    const kits = parseKits(json)
    expect(kits.map((k) => k.id)).toEqual(['k'])
    expect(kits[0]?.pads).toHaveLength(2)
    expect(parseKits(null)).toEqual([])
    const kit = kits[0] as Kit
    expect(kitFiles(kit)).toEqual(['kits/k/bd.wav', 'kits/k/ch.wav'])
    const sets = padSets(kit, (f) => ({ 'kits/k/bd.wav': 3, 'kits/k/ch.wav': 8 })[f])
    // A kit lays out the nine sound fields, not Out: loading one does not undo the routing (#220).
    expect(sets).toHaveLength(2 * (PAD_FIELDS - 1))
    expect(sets.some(([, f]) => f === PadField.Out)).toBe(false)
    expect(sets).toContainEqual([6, PadField.Sample, 8])
    expect(sets).toContainEqual([6, PadField.Choke, 1])
    expect(padSets(kit, () => undefined)).toEqual([])
  })
})

describe('drawing', () => {
  it('names keys with middle C as C4', () => {
    expect([keyName(60), keyName(21), keyName(61), keyName(0)]).toEqual(['C4', 'A0', 'C♯4', 'C-1'])
  })

  it('puts a zone in the key and velocity plane', () => {
    const r = zoneRect({ ...EMPTY_ZONE, keyLo: 0, keyHi: 127, velLo: 1, velHi: 127 }, 640, 127)
    expect(r.x).toBe(0)
    expect(r.w).toBe(640)
    expect(r.y).toBeCloseTo(1, 5)
    const low = zoneRect({ ...EMPTY_ZONE, keyLo: 64, keyHi: 64, velLo: 1, velHi: 63 }, 128, 127)
    expect(low.x).toBe(64)
    expect(low.w).toBe(1)
    expect(low.y).toBeGreaterThan(r.y)
  })

  it('draws peaks as a closed shape within the box, empty for no peaks', () => {
    expect(peakPath([], 100, 40)).toBe('')
    const d = peakPath([-0.5, 1, 0, 0.25], 100, 40)
    expect(d.startsWith('M25.0 0.0')).toBe(true)
    expect(d.endsWith('Z')).toBe(true)
    const ys = [...d.matchAll(/[ML][\d.]+ ([\d.]+)/g)].map((m) => Number(m[1]))
    expect(ys.every((y) => y >= 0 && y <= 40)).toBe(true)
  })

  it("takes a zone's own loop, else the sample's, else none", () => {
    const sample = { loopStart: 10, loopEnd: 90 }
    expect(loopOf(EMPTY_ZONE, sample)).toEqual([10, 90])
    expect(loopOf({ ...EMPTY_ZONE, loopStart: 20, loopEnd: 40 }, sample)).toEqual([20, 40])
    expect(loopOf(EMPTY_ZONE, { loopStart: 0, loopEnd: 0 })).toBeNull()
    expect(loopOf(EMPTY_ZONE, null)).toBeNull()
  })
})
