import { describe, expect, it } from 'vitest'
import { FAMILIES, MODELS, familyModels, modelDef, scaleOf, type Control } from './models'
import { Model, ModDest, ModSource, Param, Preset } from './params'

const paramIds = new Set<number>(Object.values(Param))
const controls = (m: (typeof MODELS)[number]) => m.sections.flatMap((s) => s.controls)
const paramsOf = (c: Control): number[] => {
  switch (c.kind) {
    case 'knob':
    case 'select':
    case 'switch':
      return [c.param]
    case 'env':
      return [c.a, c.d, c.s, c.r].filter((p) => p !== undefined) as number[]
    case 'eg4':
      return [...c.rates, ...c.levels]
    case 'algo':
      return [c.param]
    case 'sysex':
    case 'sampler':
      return []
    default:
      return []
  }
}

describe('model descriptions', () => {
  it('describes every model, once each, in the Model order', () => {
    expect(MODELS.map((m) => m.id)).toEqual(Object.values(Model))
    expect(modelDef(Model.Cs15).name).toBe('CS-15')
    expect(modelDef(99).id).toBe(Model.Arp2600)
  })

  it('names a real parameter for every control, and never the same twice on one panel', () => {
    for (const m of MODELS) {
      const seen = new Set<number>()
      for (const c of controls(m)) {
        for (const p of paramsOf(c)) {
          expect(paramIds.has(p), `${m.name}: parameter ${p}`).toBe(true)
          expect(seen.has(p), `${m.name}: parameter ${p} twice`).toBe(false)
          seen.add(p)
        }
      }
    }
  })

  it('gives each knob a sound range, scale and reset value', () => {
    for (const m of MODELS) {
      for (const c of controls(m)) {
        if (c.kind !== 'knob') continue
        const name = `${m.name} ${c.label}`
        expect(c.lo, name).toBeLessThan(c.hi)
        expect(c.def, name).toBeGreaterThanOrEqual(c.lo)
        expect(c.def, name).toBeLessThanOrEqual(c.hi)
        if (c.scale === 'exp') expect(c.lo, name).toBeGreaterThan(0)
        const scale = scaleOf(c)
        expect(scale.toValue(0), name).toBeCloseTo(c.lo)
        expect(scale.toValue(1), name).toBeCloseTo(c.hi)
        expect(scale.toValue(scale.toPos(c.def)), name).toBeCloseTo(c.def, c.step ? 0 : 2)
      }
    }
  })

  it('has options with distinct values for every selector, and a title for every section', () => {
    for (const m of MODELS) {
      for (const s of m.sections) expect(s.title, m.name).not.toBe('')
      for (const c of controls(m)) {
        if (c.kind !== 'select') continue
        expect(c.options.length, `${m.name} ${c.label}`).toBeGreaterThan(1)
        expect(new Set(c.options.map(([, v]) => v)).size).toBe(c.options.length)
      }
    }
  })

  it('draws every envelope with an attack, and a patch bay only on the modular models', () => {
    for (const m of MODELS) {
      for (const c of controls(m)) if (c.kind === 'env') expect(c.a, m.name).toBeDefined()
      const patch = m.sections.some((s) => s.patch)
      expect(patch, m.name).toBe([Model.Arp2600, Model.Ms20, Model.Cs15, Model.Matrix12].some((id) => id === m.id))
      // The matrix has twenty slots and the engine has parameters for all of them.
      if (m.patchSlots) expect(Param[`Patch${m.patchSlots}Amount` as keyof typeof Param], m.name).toBeDefined()
    }
  })

  it('has five colours per palette, and presets that exist', () => {
    for (const m of MODELS) {
      for (const k of ['panel', 'ink', 'soft', 'trim', 'accent'] as const) expect(m.theme[k], `${m.name} ${k}`).toMatch(/^#[0-9a-f]{6}$/i)
      expect(m.presets.length, m.name).toBeGreaterThanOrEqual(2)
      for (const p of m.presets) expect(Preset[p], `${m.name} preset ${p}`).toBeDefined()
    }
  })

  it('offers a patch bay every source and destination the engine has', () => {
    expect(Object.keys(ModSource).length).toBeGreaterThan(10)
    expect(Object.keys(ModDest).length).toBeGreaterThan(7)
  })
})

describe('families (#132)', () => {
  it('puts every model in exactly one listed family, in MODELS order', () => {
    expect(FAMILIES.flatMap((f) => familyModels(f.id))).toHaveLength(MODELS.length)
    for (const f of FAMILIES) {
      const ms = familyModels(f.id)
      expect(ms.length, f.label).toBeGreaterThan(0)
      expect(ms.map((m) => MODELS.indexOf(m))).toEqual([...ms.map((m) => MODELS.indexOf(m))].sort((a, b) => a - b))
    }
    expect(familyModels('mono')[0]?.name).toBe('ARP 2600')
    expect(familyModels('drums').map((m) => m.id)).toEqual([Model.Tr808, Model.PadSampler])
    expect(familyModels('samplers').map((m) => m.id)).toEqual([Model.Sampler])
    expect(familyModels('poly').map((m) => m.id)).toEqual([
      Model.Prophet5, Model.Juno106, Model.Jupiter8, Model.Matrix12, Model.PpgWave, Model.D50, Model.Dx7, Model.PolyMoog,
    ])
  })

  it('gives every model a first preset to start a new synth on', () => {
    for (const m of MODELS) expect(m.presets.length, m.name).toBeGreaterThan(0)
  })
})
