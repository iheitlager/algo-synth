// User presets (ADR-0014, epic #153): a named set of parameter values, stored
// by name like a setup (ADR-0004), in four kinds that each touch one scope:
//
// - synth: the model and every synth parameter, applied on the synth defaults
// - insert: one insert slot's type (by name) and knobs A–E, into any slot
// - processor: one processor's type (by name), knobs A–E and return, into any
// - strip: a strip's or group's pan, four sends and three insert slots; never
//   its fader, mute, solo or routing
//
// Pure functions over the registry: no Vue, no storage, no engine.

import { cleanName, shortF32, type Registry } from './setup'

export const LIBRARY_VERSION = 1

export type Kind = 'synth' | 'insert' | 'processor' | 'strip'
export const KINDS: readonly Kind[] = ['synth', 'insert', 'processor', 'strip']

export interface UserPreset {
  /** Stable while the preset lives in a library; renaming keeps it. */
  id: string
  kind: Kind
  name: string
  /** A synth preset's model, by name. */
  model?: string
  /** An insert or processor preset's type, by name (`InsertType`, `ProcType`). */
  type?: string
  params: Record<string, number>
}

export interface Library {
  version: typeof LIBRARY_VERSION
  presets: UserPreset[]
}

/** The registry presets are built from: a setup's, plus the effect type names. */
export interface PresetRegistry extends Registry {
  insertTypes: Record<string, number>
  procTypes: Record<string, number>
}

/** Where a preset is applied: a synth, an insert slot (0–2) of a strip, a processor (0–3), a strip. */
export type Target =
  | { kind: 'synth'; s: number }
  | { kind: 'insert'; s: number; slot: number }
  | { kind: 'processor'; n: number }
  | { kind: 'strip'; s: number }

/** What applying a preset sends: optionally the synth defaults first, then values. */
export interface Plan {
  defaults?: number
  ops: { s: number; id: number; v: number }[]
}

const MODEL = 'Model'
const KNOBS = ['A', 'B', 'C', 'D', 'E'] as const
const PROC_FIELDS = [...KNOBS, 'Return']
const STRIP_FIELDS = [
  'Pan', 'Send1', 'Send2', 'Send3', 'Send4',
  ...[1, 2, 3].flatMap((n) => ['Type', ...KNOBS].map((f) => `I${n}${f}`)),
]

const isObject = (x: unknown): x is Record<string, unknown> => typeof x === 'object' && x !== null && !Array.isArray(x)
const finite = (x: unknown): x is number => typeof x === 'number' && Number.isFinite(x)
const nameOf = (table: Record<string, number>, id: number) => Object.entries(table).find(([, v]) => v === Math.round(id))?.[0]

/** The parameters a synth preset holds: neither global nor a strip's, and not the model. */
export const synthNames = (reg: Registry) =>
  Object.keys(reg.params).filter((n) => !(n in reg.global) && !(n in (reg.strip ?? {})) && n !== MODEL)

/** The names a preset of `kind` may hold. */
function allowed(kind: Kind, reg: Registry): Set<string> {
  if (kind === 'synth') return new Set(synthNames(reg))
  if (kind === 'insert') return new Set(KNOBS)
  if (kind === 'processor') return new Set(PROC_FIELDS)
  return new Set(STRIP_FIELDS.filter((n) => n in reg.params))
}

/** Every value of `names`, by name, from a row of values by id. */
function pick(values: readonly number[], names: Iterable<[string, string]>, reg: Registry): Record<string, number> {
  const out: Record<string, number> = {}
  for (const [key, param] of names) {
    const id = reg.params[param]
    const v = id === undefined ? undefined : values[id]
    if (finite(v)) out[key] = shortF32(v)
  }
  return out
}

/** A new id for a preset; random enough for one person's library. */
export const newId = () => `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`

/**
 * A preset of what `target` holds now. `values` are the engine's values by
 * strip and id; a processor reads the globals from synth 0.
 */
export function capture(name: string, target: Target, values: readonly (readonly number[])[], reg: PresetRegistry, id = newId()): UserPreset {
  const clean = cleanName(name) || 'Preset'
  if (target.kind === 'synth') {
    const row = values[target.s] ?? []
    const modelId = reg.params[MODEL]
    const model = modelId !== undefined && reg.models ? nameOf(reg.models, row[modelId] ?? 0) : undefined
    return { id, kind: 'synth', name: clean, ...(model && { model }), params: pick(row, synthNames(reg).map((n) => [n, n]), reg) }
  }
  if (target.kind === 'insert') {
    const row = values[target.s] ?? []
    const field = (f: string) => `I${target.slot + 1}${f}`
    const typeId = reg.params[field('Type')]
    const type = typeId === undefined ? undefined : nameOf(reg.insertTypes, row[typeId] ?? 0)
    return { id, kind: 'insert', name: clean, ...(type && { type }), params: pick(row, KNOBS.map((k) => [k, field(k)]), reg) }
  }
  if (target.kind === 'processor') {
    const row = values[0] ?? []
    const field = (f: string) => `P${target.n + 1}${f}`
    const typeId = reg.params[field('Type')]
    const type = typeId === undefined ? undefined : nameOf(reg.procTypes, row[typeId] ?? 0)
    return { id, kind: 'processor', name: clean, ...(type && { type }), params: pick(row, PROC_FIELDS.map((k) => [k, field(k)]), reg) }
  }
  const row = values[target.s] ?? []
  return { id, kind: 'strip', name: clean, params: pick(row, [...allowed('strip', reg)].map((n) => [n, n]), reg) }
}

/**
 * The values that put `preset` on `target`, by id. A synth preset starts from
 * the synth defaults, so a parameter added after it was saved is the default,
 * not what the synth held before. Null when the kinds don't match.
 */
export function plan(preset: UserPreset, target: Target, reg: PresetRegistry): Plan | null {
  if (preset.kind !== target.kind) return null
  const ops: Plan['ops'] = []
  const put = (s: number, name: string, v: number | undefined) => {
    const id = reg.params[name]
    if (id !== undefined && finite(v)) ops.push({ s, id, v })
  }
  if (target.kind === 'synth') {
    const model = preset.model !== undefined ? reg.models?.[preset.model] : undefined
    if (model !== undefined) put(target.s, MODEL, model)
    for (const [n, v] of Object.entries(preset.params)) put(target.s, n, v)
    return { defaults: target.s, ops }
  }
  if (target.kind === 'insert' || target.kind === 'processor') {
    const [s, field, types] =
      target.kind === 'insert'
        ? [target.s, (f: string) => `I${target.slot + 1}${f}`, reg.insertTypes]
        : [0, (f: string) => `P${target.n + 1}${f}`, reg.procTypes]
    put(s, field('Type'), preset.type !== undefined ? types[preset.type] : undefined)
    for (const [n, v] of Object.entries(preset.params)) put(s, field(n), v)
    return { ops }
  }
  for (const [n, v] of Object.entries(preset.params)) put(target.s, n, v)
  return { ops }
}

/** Whether `target` holds something other than `preset` (the "modified" dot). */
export function modified(preset: UserPreset, target: Target, values: readonly (readonly number[])[], reg: PresetRegistry): boolean {
  const p = plan(preset, target, reg)
  if (!p) return true
  return p.ops.some((o) => Math.fround(values[o.s]?.[o.id] ?? NaN) !== Math.fround(o.v))
}

/** The library as the text of `algo-synth.presets.json`. */
export const libraryText = (presets: readonly UserPreset[]) =>
  `${JSON.stringify({ version: LIBRARY_VERSION, presets: [...presets] } satisfies Library, null, 2)}\n`

export type ParsedLibrary = { ok: true; library: Library; warnings: string[] } | { ok: false; error: string }

/**
 * Read a library file. One that isn't JSON or a library, or has an unknown
 * version, is rejected; a preset of an unknown kind, model or type is skipped,
 * and parameters a kind doesn't hold are dropped, each listed once.
 */
export function parseLibrary(text: string, reg: PresetRegistry): ParsedLibrary {
  let raw: unknown
  try {
    raw = JSON.parse(text)
  } catch {
    return { ok: false, error: 'not a JSON file' }
  }
  if (!isObject(raw) || !Array.isArray(raw.presets)) return { ok: false, error: 'not a preset library' }
  if (raw.version !== LIBRARY_VERSION) return { ok: false, error: `unknown library version ${String(raw.version)}` }
  const warnings: string[] = []
  const unknown = new Set<string>()
  const presets: UserPreset[] = []
  const ids = new Set<string>()
  for (const entry of raw.presets) {
    if (!isObject(entry) || !KINDS.includes(entry.kind as Kind)) {
      warnings.push(`skipped a preset of unknown kind ${isObject(entry) ? String(entry.kind) : ''}`.trim())
      continue
    }
    const kind = entry.kind as Kind
    const name = typeof entry.name === 'string' ? cleanName(entry.name) : ''
    if (!name) {
      warnings.push(`skipped a ${kind} preset without a name`)
      continue
    }
    const preset: UserPreset = { id: typeof entry.id === 'string' && entry.id && !ids.has(entry.id) ? entry.id : newId(), kind, name, params: {} }
    if (kind === 'synth' && typeof entry.model === 'string') {
      if (!reg.models || !(entry.model in reg.models)) {
        warnings.push(`skipped ${name}: unknown model ${entry.model}`)
        continue
      }
      preset.model = entry.model
    }
    if (kind === 'insert' || kind === 'processor') {
      const types = kind === 'insert' ? reg.insertTypes : reg.procTypes
      if (typeof entry.type !== 'string' || !(entry.type in types)) {
        warnings.push(`skipped ${name}: unknown ${kind} type ${String(entry.type)}`)
        continue
      }
      preset.type = entry.type
    }
    const ok = allowed(kind, reg)
    if (isObject(entry.params)) {
      for (const [n, v] of Object.entries(entry.params)) {
        if (ok.has(n) && finite(v)) preset.params[n] = v
        else unknown.add(n)
      }
    }
    ids.add(preset.id)
    presets.push(preset)
  }
  if (unknown.size) warnings.push(`ignored unknown parameters: ${[...unknown].sort().join(', ')}`)
  return { ok: true, library: { version: LIBRARY_VERSION, presets }, warnings }
}

/**
 * `incoming` added to `presets`: a preset with the same id replaces its
 * older copy, and a new name that is taken for its kind gets a number.
 */
export function merge(presets: readonly UserPreset[], incoming: readonly UserPreset[]): UserPreset[] {
  const out = [...presets]
  for (const p of incoming) {
    const at = out.findIndex((q) => q.id === p.id)
    if (at >= 0) {
      out[at] = p
      continue
    }
    let name = p.name
    for (let n = 2; out.some((q) => q.kind === p.kind && q.model === p.model && q.type === p.type && q.name === name); n++) {
      name = cleanName(`${p.name.slice(0, 20)} ${n}`)
    }
    out.push({ ...p, name })
  }
  return out
}
