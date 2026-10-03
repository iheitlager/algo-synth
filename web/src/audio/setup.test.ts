import { describe, expect, it } from 'vitest'
import demoText from '../../public/demo.synths.json?raw'
import { GlobalParam, Param } from './params'
import { MUTE, applyPlan, buildSetup, parseSetup, shortF32, type Registry, type State } from './setup'

const reg: Registry = { params: Param, global: GlobalParam, maxSynths: 16, channels: 16 }
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
  routes: [
    { channel: 0, synth: 0 },
    { channel: 1, synth: 2 },
    { channel: 9, synth: MUTE },
  ],
  midi: { name: 'canon.mid', parts: 3 },
}

describe('buildSetup', () => {
  it('stores shown synths, params by name, globals once and routes', () => {
    const s = buildSetup(state, reg)
    expect(s.version).toBe(1)
    expect(s.synths.map((x) => x.index)).toEqual([0, 2])
    expect(s.synths[1]?.params.Cutoff).toBe(shortF32(1234.5678))
    expect(s.synths[0]?.params).not.toHaveProperty('MasterGain')
    const want = Object.fromEntries(Object.entries(GlobalParam).map(([n, id]) => [n, shortF32(values(1)[id] ?? 0)]))
    expect(s.global).toEqual(want)
    expect(s.global).toHaveProperty('P1Return')
    expect(s.synths[0]?.params).not.toHaveProperty('P1Return')
    expect(s.routes).toEqual({ 0: 0, 1: 2, 9: 'mute' })
    expect(s.midi).toEqual({ name: 'canon.mid', parts: 3 })
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
    expect(parsed.setup.routes).toEqual({ 0: 0, 1: 'mute', 2: 'mute' })
    expect(parsed.warnings).toEqual([
      'synth 1: unknown model Minimoog',
      'skipped a second synth 1',
      'skipped synth 17: there are 16',
      'skipped synth 4: unknown kind poly',
      'skipped a synth without an index',
      'ignored unknown parameters: Glide, Sparkle, Wobble',
      'channel 2: no synth 5 in the setup, muted',
      'ignored a route for channel 20',
    ])
  })

  it('keeps a parameter it does not know only as a warning, so newer files still load', () => {
    const parsed = parseSetup('{"version":1,"synths":[{"index":1,"params":{"FutureKnob":0.4,"Cutoff":800}}]}', reg)
    expect(parsed.ok && parsed.setup.synths[0]?.params).toEqual({ Cutoff: 800 })
    expect(parsed.ok && parsed.warnings).toEqual(['ignored unknown parameters: FutureKnob'])
  })
})

describe('applyPlan', () => {
  it('shows, resets, sets the model first, then params, routes and globals', () => {
    const parsed = parseSetup(
      JSON.stringify({
        version: 1,
        global: { MasterGain: 0.4 },
        synths: [{ index: 3, model: 'Minimoog', params: { Cutoff: 700 } }, { index: 1, params: {} }],
        routes: { 0: 3, 1: 'mute' },
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
      { t: 'route', channel: 0, synth: 3 },
      { t: 'route', channel: 1, synth: MUTE },
      { t: 'param', s: 0, id: Param.MasterGain, v: 0.4 },
    ])
  })

  it('warns when the file has a different number of parts', () => {
    const setup = { version: 1 as const, midi: { name: 'a.mid', parts: 9 }, global: {}, synths: [], routes: {} }
    expect(applyPlan(setup, reg, { name: 'b.mid', parts: 4 }).warnings).toEqual([
      'the setup is for 9 parts (a.mid); this file has 4',
    ])
    expect(applyPlan(setup, reg, { name: 'a.mid', parts: 9 }).warnings).toEqual([])
  })
})

describe('the mixer in a setup (#49)', () => {
  const mixerNames = [
    'Level', 'Pan', 'Send1', 'Send2', 'Send3', 'Send4', 'Mute', 'Solo',
  ]
  const globalNames = [
    'P1Type', 'P1Return', 'P1A', 'P4E', 'CompThreshold', 'CompRatio', 'EqLowGain', 'EqHighFreq',
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

  it('opens the shipped demo setup without a warning', () => {
    const parsed = parseSetup(demoText, { ...reg, models: { Arp2600: 0 } })
    expect(parsed.ok && parsed.warnings).toEqual([])
  })
})
