import { describe, expect, it } from 'vitest'
import demoText from '../../public/demo.synths.json?raw'
import { GlobalParam, Param, StripParam } from './params'
import { applyPlan, buildSetup, parseSetup, shortF32, type Registry, type Setup, type State } from './setup'

const reg: Registry = { params: Param, global: GlobalParam, strip: StripParam, maxSynths: 16 }
// The registry once synth models exist (epic #28): a `Model` parameter and names.
const withModels: Registry = {
  ...reg,
  params: { ...Param, Model: 999 },
  models: { Arp2600: 0, Minimoog: 1 },
}

/** Values by id for one synth: every parameter at `base + id / 10`. */
function values(base: number, extra: Record<number, number> = {}): number[] {
  const out: number[] = []
  for (const id of Object.values(Param)) out[id] = base + id / 10
  for (const [id, v] of Object.entries(extra)) out[Number(id)] = v
  return out
}

const state: State = {
  synths: [0, 2],
  values: [values(1), values(9), values(2, { [Param.Cutoff]: 1234.5678 })],
}

describe('buildSetup', () => {
  it('stores shown synths, params by name and globals once', () => {
    const s = buildSetup(state, reg)
    expect(s.version).toBe(1)
    expect(s.synths.map((x) => x.index)).toEqual([0, 2])
    expect(s.synths[1]?.params.Cutoff).toBe(shortF32(1234.5678))
    expect(s.synths[0]?.params).not.toHaveProperty('MasterGain')
    const want = Object.fromEntries(Object.entries(GlobalParam).map(([n, id]) => [n, shortF32(values(1)[id] ?? 0)]))
    expect(s.global).toEqual(want)
    expect(s.global).toHaveProperty('P1Return')
    expect(s.synths[0]?.params).not.toHaveProperty('P1Return')
    // A MIDI file is the song now: no channel routes, no file (ADR-0022).
    expect(s).not.toHaveProperty('routes')
    expect(s).not.toHaveProperty('midi')
  })

  it('writes the model by name, not as a parameter', () => {
    const s = buildSetup({ ...state, values: [[], [], { ...values(2), 999: 1 }] }, withModels)
    expect(s.synths[1]?.model).toBe('Minimoog')
    expect(s.synths[1]?.params).not.toHaveProperty('Model')
  })
})

describe('round trip', () => {
  it('build, stringify, parse and apply gives back every value as the same f32', () => {
    const text = JSON.stringify(buildSetup(state, reg))
    const parsed = parseSetup(text, reg)
    expect(parsed.ok).toBe(true)
    if (!parsed.ok) return
    expect(parsed.warnings).toEqual([])
    const { ops } = applyPlan(parsed.setup, reg)
    const got: number[][] = [[], [], []]
    for (const op of ops) if (op.t === 'param') (got[op.s] ??= [])[op.id] = op.v
    const perSynth = Object.values(Param).filter((id) => !Object.values(GlobalParam).includes(id as never))
    for (const s of state.synths) {
      for (const id of perSynth) {
        expect(Math.fround(got[s]?.[id] ?? NaN), `synth ${s} param ${id}`).toBe(Math.fround(state.values[s]?.[id] ?? 0))
      }
    }
    // Global parameters once, from the first shown synth.
    expect(Math.fround(got[0]?.[Param.MasterGain] ?? NaN)).toBe(Math.fround(state.values[0]?.[Param.MasterGain] ?? 0))
  })

  it('writes an f32 the engine reported as its short decimal', () => {
    const f32 = Math.fround(0.7)
    const text = JSON.stringify(buildSetup({ ...state, values: [values(0, { [Param.Drive]: f32 })], synths: [0] }, reg))
    const parsed = parseSetup(text, reg)
    expect(text).toContain('"Drive":0.7,')
    expect(parsed.ok && Math.fround(parsed.setup.synths[0]?.params.Drive ?? NaN)).toBe(f32)
  })

  it('shortF32 is the same float for awkward values', () => {
    for (const v of [0, -0.5, 1 / 3, 19999.99, 1e-7, Math.fround(Math.PI), 440, -24]) {
      expect(Math.fround(shortF32(v))).toBe(Math.fround(v))
    }
    expect(shortF32(Math.fround(0.35))).toBe(0.35)
  })
})

describe('parseSetup', () => {
  it('rejects what is not a setup', () => {
    expect(parseSetup('{nope', reg)).toEqual({ ok: false, error: 'not a JSON file' })
    expect(parseSetup('[]', reg)).toEqual({ ok: false, error: 'not a synth setup' })
    expect(parseSetup('{"version":2,"synths":[]}', reg)).toEqual({ ok: false, error: 'unknown setup version 2' })
  })

  it('drops unknown names, synths, kinds and models with one warning each', () => {
    const parsed = parseSetup(
      JSON.stringify({
        version: 1,
        global: { MasterGain: 0.3, Wobble: 1 },
        synths: [
          { index: 0, kind: 'mono', model: 'Minimoog', params: { Cutoff: 500, Sparkle: 2, Glide: 'fast' } },
          { index: 0, params: {} },
          { index: 16, params: {} },
          { index: 3, kind: 'poly', params: {} },
          { params: {} },
        ],
        routes: { 0: 0, 1: 5, 20: 0, 2: 'mute' },
      }),
      reg,
    )
    expect(parsed.ok).toBe(true)
    if (!parsed.ok) return
    expect(parsed.setup.synths).toEqual([{ index: 0, kind: 'mono', params: { Cutoff: 500 } }])
    expect(parsed.setup.global).toEqual({ MasterGain: 0.3 })
    expect(parsed.warnings).toEqual([
      'synth 1: unknown model Minimoog',
      'skipped a second synth 1',
      'skipped synth 17: there are 16',
      'skipped synth 4: unknown kind poly',
      'skipped a synth without an index',
      'ignored unknown parameters: Glide, Sparkle, Wobble',
      'ignored the MIDI channel routes: a MIDI file plays as the song',
    ])
  })

  it('keeps a parameter it does not know only as a warning, so newer files still load', () => {
    const parsed = parseSetup('{"version":1,"synths":[{"index":1,"params":{"FutureKnob":0.4,"Cutoff":800}}]}', reg)
    expect(parsed.ok && parsed.setup.synths[0]?.params).toEqual({ Cutoff: 800 })
    expect(parsed.ok && parsed.warnings).toEqual(['ignored unknown parameters: FutureKnob'])
  })
})

describe('applyPlan', () => {
  it('shows, resets, sets the model first, then params and globals', () => {
    const parsed = parseSetup(
      JSON.stringify({
        version: 1,
        global: { MasterGain: 0.4 },
        synths: [{ index: 3, model: 'Minimoog', params: { Cutoff: 700 } }, { index: 1, params: {} }],
      }),
      withModels,
    )
    expect(parsed.ok).toBe(true)
    if (!parsed.ok) return
    expect(applyPlan(parsed.setup, withModels).ops).toEqual([
      { t: 'show', synths: [1, 3] },
      { t: 'reset', s: 3 },
      { t: 'param', s: 3, id: 999, v: 1 },
      { t: 'param', s: 3, id: Param.Cutoff, v: 700 },
      { t: 'reset', s: 1 },
      { t: 'param', s: 0, id: Param.MasterGain, v: 0.4 },
      { t: 'names', names: {} },
    ])
  })
})

describe('the mixer in a setup (#49)', () => {
  const mixerNames = [
    'Level', 'Pan', 'Send1', 'Send2', 'Send3', 'Send4', 'Mute', 'Solo', 'I1Type', 'I1A', 'I2Type', 'I3E',
  ]
  const globalNames = [
    'P1Type', 'P1Return', 'P1A', 'P4E', 'P2In', 'P4In', 'CompThreshold', 'CompRatio', 'EqLowGain', 'EqHighFreq',
  ]

  it('saves strips per synth and the processors, compressor and EQ once', () => {
    const s = buildSetup(state, reg)
    for (const n of mixerNames) {
      expect(s.synths[0]?.params, n).toHaveProperty(n)
      expect(s.global, n).not.toHaveProperty(n)
    }
    for (const n of globalNames) {
      expect(s.global, n).toHaveProperty(n)
      expect(s.synths[0]?.params, n).not.toHaveProperty(n)
    }
  })

  it('restores every mixer value exactly', () => {
    const parsed = parseSetup(JSON.stringify(buildSetup(state, reg)), reg)
    expect(parsed.ok && parsed.warnings).toEqual([])
    if (!parsed.ok) return
    const got: Record<string, number> = {}
    for (const op of applyPlan(parsed.setup, reg).ops) {
      if (op.t === 'param' && op.s === 2) got[String(op.id)] = op.v
    }
    for (const n of mixerNames) {
      const id = Param[n as keyof typeof Param]
      expect(Math.fround(got[String(id)] ?? NaN), n).toBe(Math.fround(state.values[2]?.[id] ?? 0))
    }
  })

  it('loads a version 1 file written before the mixer was central', () => {
    const parsed = parseSetup(
      JSON.stringify({
        version: 1,
        global: {
          MasterGain: 0.5, EchoTime: 300, EchoFeedback: 0.475, EchoTone: 0.7, EchoPingPong: 1, EchoReturn: 0.3,
          ReverbSize: 1, ReverbDamping: 0.3, ReverbPreDelay: 50, ReverbReturn: 0.2,
        },
        synths: [{ index: 0, params: { Level: 0.8, EchoSend: 0.4, ReverbSend: 0.6 } }],
        routes: {},
      }),
      reg,
    )
    expect(parsed.ok).toBe(true)
    if (!parsed.ok) return
    const g = parsed.setup.global
    expect(g.P1Type).toBe(1)
    expect(g.P2Type).toBe(2)
    expect(g.P1A).toBeCloseTo(Math.log(300) / Math.log(2000), 6)
    expect(g.P1B).toBeCloseTo(0.5, 6)
    expect(g.P1D).toBe(1)
    expect(g.P1Return).toBe(0.3)
    expect(g.P2A).toBeCloseTo(Math.log(10) / Math.log(100), 6)
    expect(g.P2C).toBeCloseTo(0.5, 6)
    expect(g.P2Return).toBe(0.2)
    expect(g).not.toHaveProperty('EchoTime')
    expect(parsed.setup.synths[0]?.params).toEqual({ Level: 0.8, Send1: 0.4, Send2: 0.6 })
    expect(parsed.warnings).toHaveLength(1)
    expect(parsed.warnings[0]).toMatch(/^migrated an older setup: EchoFeedback, EchoPingPong, .*EchoSend/)
  })

  it('does not let an old name overwrite a new one in the same file', () => {
    const parsed = parseSetup(
      JSON.stringify({ version: 1, synths: [{ index: 0, params: { EchoSend: 0.9, Send1: 0.2 } }] }),
      reg,
    )
    expect(parsed.ok && parsed.setup.synths[0]?.params).toEqual({ Send1: 0.2 })
  })

  it('moves an old drive insert into insert slot 1', () => {
    const parsed = parseSetup(
      JSON.stringify({
        version: 1,
        synths: [{ index: 0, params: { DriveMode: 2, DriveAmount: 0.8, DriveTone: 0.6, DriveLevel: 0.7, Level: 0.9 } }],
      }),
      reg,
    )
    expect(parsed.ok && parsed.setup.synths[0]?.params).toEqual({ I1Type: 2, I1A: 0.8, I1B: 0.6, I1C: 0.7, Level: 0.9 })
    expect(parsed.ok && parsed.warnings[0]).toMatch(/DriveAmount, DriveLevel, DriveMode, DriveTone/)
  })

  it('opens the shipped demo setup without a warning', () => {
    const parsed = parseSetup(demoText, { ...reg, models: { Arp2600: 0 } })
    expect(parsed.ok && parsed.warnings).toEqual([])
  })
})

describe('groups and layout in a setup (#61)', () => {
  const grouped: State = {
    ...state,
    groups: [0, 2],
    layout: { order: [2, 16, 0], collapsed: [16], hidden: [5] },
    values: [...state.values, ...Array.from({ length: 13 }, () => []), values(3), [], values(7, { [Param.Out]: 3, [Param.Level]: 0.4 })],
  }

  it('saves each group with its strip parameters only, and the layout', () => {
    const s = buildSetup(grouped, reg)
    expect(s.groups?.map((g) => g.index)).toEqual([0, 2])
    const params = s.groups?.[1]?.params ?? {}
    expect(Object.keys(params).sort()).toEqual(Object.keys(StripParam).sort())
    expect(params.Out).toBe(3)
    expect(params.Level).toBe(0.4)
    expect(params).not.toHaveProperty('Cutoff')
    expect(s.layout).toEqual({ order: [2, 16, 0], collapsed: [16], hidden: [5] })
  })

  it('restores groups and layout exactly through a file', () => {
    const parsed = parseSetup(JSON.stringify(buildSetup(grouped, reg)), reg)
    expect(parsed.ok && parsed.warnings).toEqual([])
    if (!parsed.ok) return
    const { ops } = applyPlan(parsed.setup, reg)
    expect(ops.find((o) => o.t === 'groups')).toEqual({ t: 'groups', groups: [0, 2] })
    expect(ops.find((o) => o.t === 'layout')).toEqual({ t: 'layout', layout: { order: [2, 16, 0], collapsed: [16], hidden: [5] } })
    // Group 3 is strip 18: reset first, then its parameters, with the route and level exact.
    const at = ops.findIndex((o) => o.t === 'reset' && o.s === 18)
    expect(at).toBeGreaterThan(0)
    const mine = ops.filter((o) => o.t === 'param' && o.s === 18)
    const out = mine.find((o) => o.t === 'param' && o.id === Param.Out)
    expect(out && out.t === 'param' && out.v).toBe(3)
  })

  it('drops what is not a group or a strip, with one warning each', () => {
    const parsed = parseSetup(
      JSON.stringify({
        version: 1,
        synths: [],
        groups: [{ index: 0, params: { Level: 0.5, Cutoff: 300 } }, { index: 0, params: {} }, { index: 9, params: {} }, { params: {} }],
        layout: { order: [0, 99, 16], collapsed: 'no', hidden: [1, 1, -2] },
      }),
      reg,
    )
    expect(parsed.ok).toBe(true)
    if (!parsed.ok) return
    expect(parsed.setup.groups).toEqual([{ index: 0, params: { Level: 0.5 } }])
    expect(parsed.setup.layout).toEqual({ order: [0, 16], collapsed: [], hidden: [1] })
    expect(parsed.warnings).toEqual([
      'skipped a second group 1',
      'skipped group 10: there are 8',
      'skipped a group without an index',
      'ignored unknown group parameters: Cutoff',
      'ignored layout entries that are not strips',
    ])
  })

  it('leaves groups and layout alone when an older file has none', () => {
    const parsed = parseSetup('{"version":1,"synths":[{"index":0,"params":{}}]}', reg)
    expect(parsed.ok && parsed.setup.groups).toBeUndefined()
    if (!parsed.ok) return
    expect(applyPlan(parsed.setup, reg).ops.some((o) => o.t === 'groups' || o.t === 'layout')).toBe(false)
  })
})

describe('names in a setup (#127)', () => {
  const named: State = { ...state, names: { strips: { 0: 'Violin I', 17: 'Strings bus' } } }

  it('saves only the names given, keyed by strip', () => {
    expect(buildSetup(named, reg).names).toEqual({ strips: { 0: 'Violin I', 17: 'Strings bus' } })
    expect(buildSetup({ ...state, names: { strips: {} } }, reg)).not.toHaveProperty('names')
  })

  it('restores them through a file and applies them as one step', () => {
    const parsed = parseSetup(JSON.stringify(buildSetup(named, reg)), reg)
    expect(parsed.ok).toBe(true)
    if (!parsed.ok) return
    expect(parsed.warnings).toEqual([])
    const ops = applyPlan(parsed.setup, reg).ops.filter((o) => o.t === 'names')
    expect(ops).toEqual([{ t: 'names', names: { strips: { 0: 'Violin I', 17: 'Strings bus' } } }])
  })

  it('puts the defaults back for a setup without names', () => {
    const parsed = parseSetup(JSON.stringify(buildSetup(state, reg)), reg)
    if (!parsed.ok) throw new Error(parsed.error)
    expect(applyPlan(parsed.setup, reg).ops.filter((o) => o.t === 'names')).toEqual([{ t: 'names', names: {} }])
  })

  it('cleans names and drops what is not a strip, with one warning; lane names are gone with the player', () => {
    const raw = {
      ...buildSetup(state, reg),
      names: { strips: { 0: '  Lead   synth  ', 24: 'Nope', x: 'Bad', 3: '', 4: 7, 5: 'A'.repeat(40) }, parts: { 16: 'Too far', 2: 'Bass' } },
    }
    const parsed = parseSetup(JSON.stringify(raw), reg)
    if (!parsed.ok) throw new Error(parsed.error)
    expect(parsed.setup.names).toEqual({ strips: { 0: 'Lead synth', 5: 'A'.repeat(24) } })
    expect(parsed.warnings).toEqual(['ignored names that are not strips'])
  })
})

describe('an Out to a group the setup does not have (#218)', () => {
  const setup = (groups: number[]): Setup => ({
    version: 1,
    global: {},
    synths: [
      { index: 0, kind: 'mono', params: { Out: 1, Level: 0.5 } },
      { index: 3, kind: 'mono', params: { Out: 3, BdOut: 3, SnOut: 1, CpOut: 9 } },
    ],
    groups: groups.map((index) => ({ index, params: { Out: 5 } })),
  })
  const outs = (ops: ReturnType<typeof applyPlan>['ops'], s: number, name: keyof typeof Param) =>
    ops.flatMap((o) => (o.t === 'param' && o.s === s && o.id === Param[name] ? [o.v] : []))

  it('sends it to the master, with a warning for each', () => {
    const { ops, warnings } = applyPlan(setup([0]), reg)
    expect(outs(ops, 0, 'Out')).toEqual([1]) // group 1 is there
    expect(outs(ops, 3, 'Out')).toEqual([0])
    expect(outs(ops, 3, 'BdOut')).toEqual([0])
    expect(outs(ops, 3, 'SnOut')).toEqual([1])
    expect(outs(ops, 3, 'CpOut')).toEqual([9]) // None is not a group
    expect(outs(ops, 16, 'Out')).toEqual([0]) // group 1's own Out, to a group 5 that is not there
    expect(warnings).toHaveLength(3)
    expect(warnings[0]).toMatch(/synth 4: Out went to group 3, which the setup does not have/)
  })

  it('keeps every Out when the groups are there', () => {
    const { ops, warnings } = applyPlan(setup([0, 2, 4]), reg)
    expect(outs(ops, 3, 'Out')).toEqual([3])
    expect(outs(ops, 3, 'BdOut')).toEqual([3])
    expect(outs(ops, 16, 'Out')).toEqual([5])
    expect(warnings).toEqual([])
  })
})

describe('Modular code (ADR-0024)', () => {
  const modular: Registry = { ...reg, params: { ...Param, Model: 999 }, models: { Minimoog: 1, Modular: 19 } }
  const code = 'SynthDef(\\a, { Saw.ar(440) }).add;'
  const row = (model: number) => ({ ...values(1), 999: model })

  it('saves a Modular synth with its code, and only a Modular one', () => {
    const s = buildSetup({ synths: [0, 1], values: [row(19), row(1)], codes: { 0: code, 1: code } }, modular)
    expect(s.synths[0]?.code).toBe(code)
    expect(s.synths[1]).not.toHaveProperty('code')
  })

  it('reads the code back and builds it after the parameters', () => {
    const text = JSON.stringify(buildSetup({ synths: [0], values: [row(19)], codes: { 0: code } }, modular))
    const parsed = parseSetup(text, modular)
    if (!parsed.ok) throw new Error(parsed.error)
    expect(parsed.setup.synths[0]?.code).toBe(code)
    const { ops } = applyPlan(parsed.setup, modular)
    const at = ops.findIndex((o) => o.t === 'code')
    expect(ops[at]).toEqual({ t: 'code', s: 0, text: code })
    expect(ops.slice(0, at).filter((o) => o.t === 'param').length).toBeGreaterThan(1)
  })

  it('ignores code on a synth that is not Modular', () => {
    const text = JSON.stringify({ version: 1, global: {}, synths: [{ index: 0, model: 'Minimoog', params: {}, code }] })
    const parsed = parseSetup(text, modular)
    if (!parsed.ok) throw new Error(parsed.error)
    expect(parsed.setup.synths[0]).not.toHaveProperty('code')
    expect(parsed.warnings).toContain('synth 1: ignored code on a synth that is not Modular')
  })
})
