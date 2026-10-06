import { describe, expect, it } from 'vitest'
import * as tables from './params'
import { GlobalParam, InsertType, Param, ProcType, StripParam } from './params'
import { shortF32 } from './setup'
import { capture, libraryText, merge, modified, parseLibrary, plan, synthNames, type PresetRegistry, type UserPreset } from './presets'

const reg: PresetRegistry = {
  params: Param,
  global: GlobalParam,
  strip: StripParam,
  models: (tables as unknown as Record<string, Record<string, number>>).Model,
  maxSynths: 16,
  insertTypes: InsertType,
  procTypes: ProcType,
}

/** Values by strip and id: every parameter of strip `s` at `s + id / 1000`, so each is distinct. */
function values(): number[][] {
  return Array.from({ length: 24 }, (_, s) => {
    const row: number[] = []
    for (const id of Object.values(Param)) row[id] = s + id / 1000
    return row
  })
}
const vals = values()
vals[2]![Param.Model] = 8 // Juno-106
vals[3]![Param.I2Type] = InsertType.Comp
vals[0]![Param.P3Type] = ProcType.Reverb

/** Apply a plan to a copy of `vals`, as the engine would (without clamping). */
function applied(p: ReturnType<typeof plan>, from = values()): number[][] {
  for (const o of p?.ops ?? []) from[o.s]![o.id] = o.v
  return from
}

describe('user presets (ADR-0014)', () => {
  it('a synth preset holds the model by name and every synth parameter, nothing of the strip', () => {
    const p = capture('Warm pad', { kind: 'synth', s: 2 }, vals, reg)
    expect(p.model).toBe('Juno106')
    expect(Object.keys(p.params).sort()).toEqual(synthNames(reg).sort())
    for (const n of ['Level', 'Send1', 'Mute', 'Out', 'I1Type', 'MasterGain', 'Model']) expect(p.params).not.toHaveProperty(n)
  })

  it('a synth preset applies on the defaults to another synth, model first, bit-exact', () => {
    const p = capture('Warm pad', { kind: 'synth', s: 2 }, vals, reg)
    const pl = plan(p, { kind: 'synth', s: 5 }, reg)
    expect(pl?.defaults).toBe(5)
    expect(pl?.ops[0]).toEqual({ s: 5, id: Param.Model, v: 8 })
    const after = applied(pl)
    for (const n of synthNames(reg)) expect(Math.fround(after[5]![Param[n as keyof typeof Param]]!)).toBe(Math.fround(vals[2]![Param[n as keyof typeof Param]]!))
    expect(after[5]![Param.Level]).toBe(values()[5]![Param.Level])
    expect(modified(p, { kind: 'synth', s: 5 }, after, reg)).toBe(false)
    after[5]![Param.Cutoff] = 99
    expect(modified(p, { kind: 'synth', s: 5 }, after, reg)).toBe(true)
  })

  it('an insert preset moves between slots and strips, touching only that slot', () => {
    const p = capture('Glue', { kind: 'insert', s: 3, slot: 1 }, vals, reg)
    expect(p.type).toBe('Comp')
    expect(Object.keys(p.params)).toEqual(['A', 'B', 'C', 'D', 'E'])
    const pl = plan(p, { kind: 'insert', s: 18, slot: 0 }, reg)
    expect(pl?.ops.map((o) => o.id)).toEqual([Param.I1Type, Param.I1A, Param.I1B, Param.I1C, Param.I1D, Param.I1E])
    expect(pl?.ops.every((o) => o.s === 18)).toBe(true)
    expect(pl?.ops[0]?.v).toBe(InsertType.Comp)
    expect(pl?.ops[1]?.v).toBe(shortF32(vals[3]![Param.I2A]!))
  })

  it('a processor preset carries type, knobs and return into another processor', () => {
    const p = capture('Hall', { kind: 'processor', n: 2 }, vals, reg)
    expect(p.type).toBe('Reverb')
    expect(Object.keys(p.params)).toEqual(['A', 'B', 'C', 'D', 'E', 'Return'])
    const pl = plan(p, { kind: 'processor', n: 0 }, reg)
    expect(pl?.ops.map((o) => o.id)).toEqual([Param.P1Type, Param.P1A, Param.P1B, Param.P1C, Param.P1D, Param.P1E, Param.P1Return])
    expect(pl?.ops.every((o) => o.s === 0)).toBe(true)
  })

  it('a strip preset holds pan, sends and inserts, never fader, mute, solo or routing', () => {
    const p = capture('Vocal-ish', { kind: 'strip', s: 3 }, vals, reg)
    expect(Object.keys(p.params)).toContain('I2Type')
    expect(Object.keys(p.params)).toContain('Send4')
    for (const n of ['Level', 'Mute', 'Solo', 'Out', 'Cutoff']) expect(p.params).not.toHaveProperty(n)
    const pl = plan(p, { kind: 'strip', s: 20 }, reg)
    expect(pl?.defaults).toBeUndefined()
    expect(pl?.ops.every((o) => o.s === 20)).toBe(true)
  })

  it('refuses a preset of another kind', () => {
    const p = capture('Glue', { kind: 'insert', s: 3, slot: 1 }, vals, reg)
    expect(plan(p, { kind: 'strip', s: 3 }, reg)).toBeNull()
    expect(modified(p, { kind: 'strip', s: 3 }, vals, reg)).toBe(true)
  })
})

describe('the library file', () => {
  const presets = [
    capture('Warm pad', { kind: 'synth', s: 2 }, vals, reg, 'a'),
    capture('Glue', { kind: 'insert', s: 3, slot: 1 }, vals, reg, 'b'),
    capture('Hall', { kind: 'processor', n: 2 }, vals, reg, 'c'),
    capture('Bus', { kind: 'strip', s: 3 }, vals, reg, 'd'),
  ]

  it('round-trips every kind', () => {
    const parsed = parseLibrary(libraryText(presets), reg)
    expect(parsed.ok).toBe(true)
    if (!parsed.ok) return
    expect(parsed.warnings).toEqual([])
    expect(parsed.library.presets).toEqual(presets)
  })

  it('rejects what is not a library and skips what it cannot place, with warnings', () => {
    expect(parseLibrary('nope', reg)).toEqual({ ok: false, error: 'not a JSON file' })
    expect(parseLibrary('{"version":1}', reg)).toEqual({ ok: false, error: 'not a preset library' })
    expect(parseLibrary('{"version":9,"presets":[]}', reg)).toEqual({ ok: false, error: 'unknown library version 9' })
    const raw = {
      version: 1,
      presets: [
        { kind: 'drums', name: 'x', params: {} },
        { kind: 'synth', name: '', params: {} },
        { kind: 'synth', name: 'Old', model: 'Moog55', params: {} },
        { kind: 'insert', name: 'Odd', type: 'Phaser', params: {} },
        { kind: 'insert', name: '  Warm   drive ', type: 'Overdrive', params: { A: 0.5, Level: 1, B: 'x' } },
      ],
    }
    const parsed = parseLibrary(JSON.stringify(raw), reg)
    if (!parsed.ok) throw new Error(parsed.error)
    expect(parsed.library.presets).toHaveLength(1)
    expect(parsed.library.presets[0]).toMatchObject({ kind: 'insert', name: 'Warm drive', type: 'Overdrive', params: { A: 0.5 } })
    expect(parsed.warnings).toEqual([
      'skipped a preset of unknown kind drums',
      'skipped a synth preset without a name',
      'skipped Old: unknown model Moog55',
      'skipped Odd: unknown insert type Phaser',
      'ignored unknown parameters: B, Level',
    ])
  })

  it('merges by id, and numbers a name taken for its kind', () => {
    const glue2: UserPreset = { ...presets[1]!, id: 'z' }
    const renamed: UserPreset = { ...presets[0]!, name: 'Pad v2' }
    const out = merge(presets, [glue2, renamed])
    expect(out.map((p) => p.name)).toEqual(['Pad v2', 'Glue', 'Hall', 'Bus', 'Glue 2'])
  })
})
