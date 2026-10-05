// The composer's track picker (#213): which models a track may play and which
// presets each offers, from what the engine sent. The rule a track line is
// checked by and the patch itself live in the engine (ADR-0001); a choice is
// sent as `trackEdit`, and the engine prints the song back.
import type { SongSetting, SongTrack } from './engine'
import { MODELS, type ModelDef } from './models'
import { Preset } from './params'

const KINDS = ['drums', 'synth', 'sampler'] as const

/** The models that play a track of `kind`, by the engine's table. */
export const modelsFor = (kind: SongTrack['kind'], fits: boolean[][]): ModelDef[] =>
  MODELS.filter((m) => fits[KINDS.indexOf(kind)]?.[m.id] === true)

/** The model a factory preset belongs to (models.ts lists them, checked against the engine). */
export const modelOfPreset = (preset: number): ModelDef | undefined =>
  MODELS.find((m) => m.presets.some((name) => Preset[name] === preset))

export interface Choice { value: string; label: string }

/** A track's preset choices on `model`: the model's factory presets, then the song's settings on it. */
export function presetChoices(model: ModelDef, settings: SongSetting[]): Choice[] {
  const factory = model.presets.map((name) => ({ value: `p${Preset[name]}`, label: name }))
  const own = settings
    .map((st, i) => ({ st, i }))
    .filter(({ st }) => modelOfPreset(st.preset)?.id === model.id)
    .map(({ st, i }) => ({ value: `s${i}`, label: `${st.name} (song)` }))
  return [...factory, ...own]
}

/** The choice a track plays now: its setting if it has one, else its preset; '' for neither. */
export const choiceOf = (t: SongTrack): string => (t.setting >= 0 ? `s${t.setting}` : t.preset >= 0 ? `p${t.preset}` : '')

/** A chosen value as the edit to send: a factory preset or a song setting. */
export function parseChoice(value: string): { kind: 'preset' | 'setting'; id: number } | null {
  if (!/^[ps]\d+$/.test(value)) return null
  const id = Number(value.slice(1))
  if (value.startsWith('p')) return { kind: 'preset', id }
  if (value.startsWith('s')) return { kind: 'setting', id }
  return null
}
