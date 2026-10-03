// The synth models as data (spec 005, ADR-0009): for each instrument its
// panel (sections, controls, the names the real machine uses) and its colours.
// The view only draws this and sends parameter changes; which control belongs
// on which panel is layout, and every musical decision stays in Rust.

import { Model, ModDest, ModSource, NoiseColour, NotePriority, Param, Preset, Waveform } from './params'
import type { ModelId, ParamId } from './params'

type Options = readonly (readonly [string, number])[]

export type Control =
  | { kind: 'range'; label: string; param: ParamId; min: number; max: number; step: number; map?: 'cutoff' | 'lfo' }
  | { kind: 'select'; label: string; param: ParamId; options: Options }
  | { kind: 'switch'; label: string; param: ParamId }
  | { kind: 'note'; text: string }

export interface Section {
  title: string
  controls: Control[]
  /** The eight patch slots instead of controls (spec 004 Req 7). */
  patch?: boolean
}

/** CSS colours of a panel: surface, lettering, quieter lettering, trim, accent. */
export interface Theme {
  panel: string
  ink: string
  soft: string
  trim: string
  accent: string
}

export interface ModelDef {
  id: ModelId
  name: string
  maker: string
  tagline: string
  theme: Theme
  /** The model's presets, the first loaded when the model is chosen. */
  presets: (keyof typeof Preset)[]
  sections: Section[]
}

const range = (label: string, param: ParamId, min: number, max: number, step: number, map?: 'cutoff' | 'lfo'): Control =>
  ({ kind: 'range', label, param, min, max, step, map })
const select = (label: string, param: ParamId, options: Options): Control => ({ kind: 'select', label, param, options })
const sw = (label: string, param: ParamId): Control => ({ kind: 'switch', label, param })

const entries = (o: Record<string, number>): Options => Object.entries(o)
export const WAVES = entries(Waveform)
export const LFO_WAVES: Options = WAVES.map(([n, id]) => [n === 'Pulse' ? 'Square' : n, id])
export const NOISES = entries(NoiseColour)
export const PRIORITIES = entries(NotePriority)
export const MOD_SOURCES = entries(ModSource)
export const MOD_DESTS = entries(ModDest)

const level = (p: ParamId) => range('Level', p, 0, 1, 0.01)
const coarse = (p: ParamId) => range('Coarse', p, -24, 24, 1)
const fine = (p: ParamId) => range('Fine', p, -50, 50, 1)

const arp2600: ModelDef = {
  id: Model.Arp2600,
  name: 'ARP 2600',
  maker: 'semi-modular, normalled',
  tagline: 'Three VCOs, ladder, ADSR and AR, S&H, a patch behind the normals',
  theme: { panel: '#2f3236', ink: '#ece6d6', soft: '#b3ada0', trim: '#1b1c1e', accent: '#f08a24' },
  presets: ['Bass', 'Lead', 'SyncLead', 'BowedString'],
  sections: [
    {
      title: 'ADSR',
      controls: [
        range('Attack', Param.AdsrAttack, 0.001, 2, 0.001),
        range('Decay', Param.AdsrDecay, 0.001, 4, 0.001),
        range('Sustain', Param.AdsrSustain, 0, 1, 0.01),
        range('Release', Param.AdsrRelease, 0.001, 4, 0.001),
      ],
    },
    {
      title: 'VCO 1',
      controls: [select('Wave', Param.Vco1Wave, WAVES), coarse(Param.Vco1Coarse), fine(Param.Vco1Fine), level(Param.Vco1Level)],
    },
    {
      title: 'VCO 2',
      controls: [
        select('Wave', Param.Vco2Wave, WAVES), coarse(Param.Vco2Coarse), fine(Param.Vco2Fine), level(Param.Vco2Level),
        sw('Sync to 1', Param.Vco2Sync),
      ],
    },
    {
      title: 'VCO 3',
      controls: [
        select('Wave', Param.Vco3Wave, WAVES), coarse(Param.Vco3Coarse), fine(Param.Vco3Fine), level(Param.Vco3Level),
        sw('Sync to 1', Param.Vco3Sync),
      ],
    },
    { title: 'Pulse', controls: [range('Width', Param.PulseWidth, 0.05, 0.95, 0.01)] },
    { title: 'Noise', controls: [select('Colour', Param.NoiseColour, NOISES), level(Param.NoiseLevel)] },
    {
      title: 'Keys',
      controls: [select('Priority', Param.Priority, PRIORITIES), sw('Legato', Param.Legato), range('Glide', Param.Glide, 0, 2, 0.01)],
    },
    {
      title: 'Ladder',
      controls: [
        range('Cutoff', Param.Cutoff, 0, 1, 0.001, 'cutoff'),
        range('Resonance', Param.Resonance, 0, 1, 0.01),
        range('Drive', Param.Drive, 0, 1, 0.01),
      ],
    },
    { title: 'LFO', controls: [select('Wave', Param.LfoWave, LFO_WAVES), range('Rate', Param.LfoRate, 0, 1, 0.001, 'lfo')] },
    {
      title: 'AR',
      controls: [range('Attack', Param.ArAttack, 0.001, 2, 0.001), range('Release', Param.ArRelease, 0.001, 4, 0.001)],
    },
    {
      title: 'Normal',
      controls: [
        range('Env → cutoff', Param.EnvCutoff, -1, 1, 0.01),
        range('Key track', Param.KeyTrack, 0, 1, 0.01),
        range('Vibrato', Param.Vibrato, 0, 1, 0.01),
        range('Mod wheel', Param.ModWheel, 0, 1, 0.01),
        {
          kind: 'note',
          text: 'Normalled: VCO 1–3 + noise → ladder → VCA · ADSR → cutoff, VCA · key → pitch, cutoff · LFO × wheel → pitch. A patch slot replaces the normals to its destination.',
        },
      ],
    },
    { title: 'Patch', controls: [], patch: true },
  ],
}

/** The models the view offers, in the order of the picker. */
export const MODELS: ModelDef[] = [arp2600]

/** The definition of a model id; an unknown one draws as the ARP 2600. */
export const modelDef = (id: number): ModelDef => MODELS.find((m) => m.id === id) ?? arp2600
