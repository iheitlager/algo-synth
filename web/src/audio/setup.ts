// Synth setups (#41): a JSON file next to a MIDI file holding which synths
// are shown, each one's model and parameters, the channel routing and the
// engine-wide settings. Pure functions over the parameter registry: no Vue,
// no engine. The engine still clamps every value it receives.
//
// Parameters are stored by name, never by id: ids shift when parameters are
// added (ADR-0004), names don't. Anything the registry gains later (mixer,
// effects, models) is saved and restored without changes here.

export const SETUP_VERSION = 1
/** A route to no synth: the channel is muted. Matches `MUTE` in engine.ts. */
export const MUTE = 255

export interface SynthSetup {
  index: number
  kind: 'mono'
  /** The synth model by name, once the registry has models (epic #28). */
  model?: string
  params: Record<string, number>
}

export interface Setup {
  version: typeof SETUP_VERSION
  /** The MIDI file the setup was made for, to warn on a mismatch. */
  midi?: { name: string; parts: number }
  global: Record<string, number>
  synths: SynthSetup[]
  /** MIDI channel (0-15) to a synth index, or 'mute'. */
  routes: Record<string, number | 'mute'>
}

/** The registry a setup is built from and applied to, by name. */
export interface Registry {
  /** Every parameter, name to id (`Param`). */
  params: Record<string, number>
  /** The engine-wide ones (`GlobalParam`). */
  global: Record<string, number>
  /** Model names to ids (`Model`), when synth models exist. */
  models?: Record<string, number>
  maxSynths: number
  channels: number
}

/** What the view holds: shown synths, values by synth and id, routes. */
export interface State {
  synths: number[]
  values: number[][]
  routes: { channel: number; synth: number }[]
  midi?: { name: string; parts: number }
}

export type Op =
  | { t: 'show'; synths: number[] }
  | { t: 'reset'; s: number }
  | { t: 'param'; s: number; id: number; v: number }
  | { t: 'route'; channel: number; synth: number }

/** The parameter that selects a synth's model; stored as `model` by name. */
const MODEL = 'Model'

const isObject = (x: unknown): x is Record<string, unknown> =>
  typeof x === 'object' && x !== null && !Array.isArray(x)
const finite = (x: unknown): x is number => typeof x === 'number' && Number.isFinite(x)
/**
 * The shortest decimal that is the same 32-bit float: the engine stores f32,
 * so 0.6999999284744263 is written as 0.7 and still restores bit-exact.
 */
export function shortF32(v: number): number {
  const target = Math.fround(v)
  for (let digits = 1; digits <= 9; digits++) {
    const x = Number(v.toPrecision(digits))
    if (Math.fround(x) === target) return x
  }
  return v
}
const nameOf = (table: Record<string, number>, id: number) =>
  Object.entries(table).find(([, v]) => v === id)?.[0]

/** Build a setup from the view's state. */
export function buildSetup(state: State, reg: Registry): Setup {
  const first = state.values[state.synths[0] ?? 0] ?? []
  const global: Record<string, number> = {}
  for (const [name, id] of Object.entries(reg.global)) {
    const v = first[id]
    if (finite(v)) global[name] = shortF32(v)
  }
  const modelId = reg.models ? reg.params[MODEL] : undefined
  const synths = state.synths.map((index): SynthSetup => {
    const values = state.values[index] ?? []
    const params: Record<string, number> = {}
    let model: string | undefined
    for (const [name, id] of Object.entries(reg.params)) {
      const v = values[id]
      if (!finite(v) || name in reg.global) continue
      if (id === modelId && reg.models) model = nameOf(reg.models, v)
      else params[name] = shortF32(v)
    }
    return { index, kind: 'mono', ...(model !== undefined && { model }), params }
  })
  const routes: Setup['routes'] = {}
  for (const r of state.routes) routes[String(r.channel)] = r.synth === MUTE ? 'mute' : r.synth
  return { version: SETUP_VERSION, ...(state.midi && { midi: state.midi }), global, synths, routes }
}

// Setups written before the mixer was central (#50) used other names: the
// per-synth `EchoSend` and `ReverbSend`, and the global echo and reverb
// parameters in physical units. They become `Send1`, `Send2` and the knobs of
// processors P1 (echo) and P2 (reverb), which are 0..1 (spec 002 Req 2).
const logPos = (v: number, lo: number, hi: number) =>
  Math.min(1, Math.max(0, Math.log(v / lo) / Math.log(hi / lo)))
const SEND_ALIASES: Record<string, string> = { EchoSend: 'Send1', ReverbSend: 'Send2' }
// The drive insert became insert slot 1 (ADR-0010): mode 0..3 is the type
// (off, overdrive, distortion, fuzz), and amount, tone and level are A, B, C.
const DRIVE_ALIASES: Record<string, string> = {
  DriveMode: 'I1Type', DriveAmount: 'I1A', DriveTone: 'I1B', DriveLevel: 'I1C',
}
const GLOBAL_ALIASES: Record<string, (v: number) => [string, number]> = {
  EchoTime: (v) => ['P1A', logPos(v, 1, 2000)],
  EchoFeedback: (v) => ['P1B', Math.min(1, v / 0.95)],
  EchoTone: (v) => ['P1C', v],
  EchoPingPong: (v) => ['P1D', v],
  EchoReturn: (v) => ['P1Return', v],
  ReverbSize: (v) => ['P2A', logPos(v, 0.1, 10)],
  ReverbDamping: (v) => ['P2B', v],
  ReverbPreDelay: (v) => ['P2C', v / 100],
  ReverbReturn: (v) => ['P2Return', v],
}

/** Rewrite old names to current ones; `renamed` collects the old names seen. */
function migrate(from: unknown, aliases: Record<string, string>, renamed: Set<string>): unknown {
  if (!isObject(from)) return from
  const out: Record<string, unknown> = {}
  for (const [name, v] of Object.entries(from)) {
    const to = aliases[name]
    if (to === undefined) out[name] = v
    else {
      renamed.add(name)
      if (!(to in from)) out[to] = v
    }
  }
  return out
}

function migrateGlobal(from: unknown, renamed: Set<string>): unknown {
  if (!isObject(from)) return from
  const out: Record<string, unknown> = {}
  for (const [name, v] of Object.entries(from)) {
    const convert = GLOBAL_ALIASES[name]
    if (convert === undefined) out[name] = v
    else if (finite(v)) {
      renamed.add(name)
      const [to, value] = convert(v)
      if (!(to in from)) out[to] = value
    } else renamed.add(name)
  }
  // An old setup had an echo and a reverb: keep them as P1 and P2.
  if ([...renamed].some((n) => n.startsWith('Echo') && n in GLOBAL_ALIASES)) out.P1Type ??= 1
  if ([...renamed].some((n) => n.startsWith('Reverb') && n in GLOBAL_ALIASES)) out.P2Type ??= 2
  return out
}

export type Parsed = { ok: true; setup: Setup; warnings: string[] } | { ok: false; error: string }

/**
 * Read a setup file. A file that isn't JSON, isn't a setup or has an unknown
 * version is rejected; anything unknown inside a valid one is dropped and
 * listed in `warnings`, so what is left always applies.
 */
export function parseSetup(text: string, reg: Registry): Parsed {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch {
    return { ok: false, error: 'not a JSON file' }
  }
  if (!isObject(raw) || !Array.isArray(raw.synths)) return { ok: false, error: 'not a synth setup' }
  if (raw.version !== SETUP_VERSION) return { ok: false, error: `unknown setup version ${String(raw.version)}` }

  const warnings: string[] = []
  const unknown = new Set<string>()
  const values = (from: unknown, allowed: (name: string) => boolean): Record<string, number> => {
    const out: Record<string, number> = {}
    if (!isObject(from)) return out
    for (const [name, v] of Object.entries(from)) {
      if (!allowed(name) || !finite(v)) unknown.add(name)
      else out[name] = v
    }
    return out
  }

  const renamed = new Set<string>()
  const global = values(migrateGlobal(raw.global, renamed), (n) => n in reg.global)
  const synths: SynthSetup[] = []
  for (const entry of raw.synths) {
    if (!isObject(entry) || !Number.isInteger(entry.index)) {
      warnings.push('skipped a synth without an index')
      continue
    }
    const index = entry.index as number
    if (index < 0 || index >= reg.maxSynths) {
      warnings.push(`skipped synth ${index + 1}: there are ${reg.maxSynths}`)
      continue
    }
    if (synths.some((s) => s.index === index)) {
      warnings.push(`skipped a second synth ${index + 1}`)
      continue
    }
    if (entry.kind !== undefined && entry.kind !== 'mono') {
      warnings.push(`skipped synth ${index + 1}: unknown kind ${String(entry.kind)}`)
      continue
    }
    const synth: SynthSetup = {
      index,
      kind: 'mono',
      params: values(migrate(entry.params, { ...SEND_ALIASES, ...DRIVE_ALIASES }, renamed), (n) => n in reg.params && !(n in reg.global) && !(n === MODEL && reg.models)),
    }
    if (typeof entry.model === 'string') {
      if (reg.models && entry.model in reg.models) synth.model = entry.model
      else warnings.push(`synth ${index + 1}: unknown model ${entry.model}`)
    }
    synths.push(synth)
  }
  if (renamed.size) warnings.push(`migrated an older setup: ${[...renamed].sort().join(', ')} now live on the mixer, the insert slots and processors P1–P2`)
  if (unknown.size) warnings.push(`ignored unknown parameters: ${[...unknown].sort().join(', ')}`)

  const routes: Setup['routes'] = {}
  if (isObject(raw.routes)) {
    for (const [ch, target] of Object.entries(raw.routes)) {
      const channel = Number(ch)
      if (!Number.isInteger(channel) || channel < 0 || channel >= reg.channels) {
        warnings.push(`ignored a route for channel ${ch}`)
      } else if (target === 'mute') {
        routes[ch] = 'mute'
      } else if (Number.isInteger(target) && synths.some((s) => s.index === target)) {
        routes[ch] = target as number
      } else {
        warnings.push(`channel ${channel + 1}: no synth ${String(target)} in the setup, muted`)
        routes[ch] = 'mute'
      }
    }
  }

  const midi =
    isObject(raw.midi) && typeof raw.midi.name === 'string' && Number.isInteger(raw.midi.parts)
      ? { name: raw.midi.name, parts: raw.midi.parts as number }
      : undefined
  return { ok: true, setup: { version: SETUP_VERSION, ...(midi && { midi }), global, synths, routes }, warnings }
}

/**
 * The messages that apply a setup, in order: show its synths, then per synth
 * a reset (so anything the file leaves out is the default), the model before
 * the other parameters, then the routes and the global parameters. Warns when
 * the setup was made for a file with a different number of parts.
 */
export function applyPlan(
  setup: Setup,
  reg: Registry,
  midi?: { name: string; parts: number },
): { ops: Op[]; warnings: string[] } {
  const warnings: string[] = []
  if (setup.midi && midi && setup.midi.parts !== midi.parts) {
    warnings.push(`the setup is for ${setup.midi.parts} parts (${setup.midi.name}); this file has ${midi.parts}`)
  }
  const ops: Op[] = []
  if (setup.synths.length) ops.push({ t: 'show', synths: setup.synths.map((s) => s.index).sort((a, b) => a - b) })
  const modelId = reg.params[MODEL]
  for (const synth of setup.synths) {
    const s = synth.index
    ops.push({ t: 'reset', s })
    const model = synth.model !== undefined ? reg.models?.[synth.model] : undefined
    if (modelId !== undefined && model !== undefined) ops.push({ t: 'param', s, id: modelId, v: model })
    for (const [name, v] of Object.entries(synth.params)) {
      const id = reg.params[name]
      if (id !== undefined) ops.push({ t: 'param', s, id, v })
    }
  }
  for (const [ch, target] of Object.entries(setup.routes)) {
    ops.push({ t: 'route', channel: Number(ch), synth: target === 'mute' ? MUTE : target })
  }
  for (const [name, v] of Object.entries(setup.global)) {
    const id = reg.global[name]
    if (id !== undefined) ops.push({ t: 'param', s: 0, id, v })
  }
  return { ops, warnings }
}
