import { describe, expect, it } from 'vitest'
import { LoopMode, ZoneField } from './params'
import {
  EMPTY_ZONE, ZONES, ZONE_FIELDS, decodeZones, freeSlot, freeZone, keyName, loopOf, packFiles, parseManifest, peakPath,
  zoneRect, zoneSets, type Pack,
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
