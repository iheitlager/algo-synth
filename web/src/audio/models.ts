// The synth models as data (spec 005, ADR-0009): for each instrument its
// panel (sections, controls, the names the real machine uses) and its colours.
// The view only draws this and sends parameter changes; which control belongs
// on which panel is layout, and every musical decision stays in Rust.

import { Model, ModDest, ModSource, NoiseColour, NotePriority, Param, Preset, Waveform } from './params'
import type { ModelId, ParamId } from './params'
import type { Scale } from './console'
import { exp, lin } from './console'
import type { Unit } from './faceplate'
import { stepped } from './faceplate'

type Options = readonly (readonly [string, number])[]

/**
 * A control and how it is drawn (spec 003 Req 9). A knob keeps its range, its
 * scale and its unit here, so the view only draws and sends; `def` is where a
 * double-click puts it, in the engine's units.
 */
export type Control =
  | {
      kind: 'knob'; label: string; param: ParamId; lo: number; hi: number; scale: 'lin' | 'exp'; unit: Unit
      /** Whole steps of this size (coarse and fine tune). */
      step?: number; bipolar?: boolean; def: number; size?: number
    }
  | { kind: 'select'; label: string; param: ParamId; options: Options }
  | { kind: 'switch'; label: string; param: ParamId }
  /** An envelope drawn as its curve with a knob for each time and level it has. */
  | { kind: 'env'; label: string; a: ParamId; d?: ParamId; s?: ParamId; r?: ParamId; decayIsRelease?: boolean }
  | { kind: 'note'; text: string }

export interface Section {
  title: string
  controls: Control[]
  /** The eight patch slots instead of controls (spec 004 Req 7). */
  patch?: boolean
}

/**
 * CSS colours of a faceplate: surface, lettering, quieter lettering, trim,
 * accent, and the wood of the cheeks for the instruments that have it.
 */
export interface Theme {
  panel: string
  ink: string
  soft: string
  trim: string
  accent: string
  wood?: string
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

/** Where a double-click puts a knob, in engine units, when it is not the low end (or 0 for a bipolar one). */
const RESET: Partial<Record<ParamId, number>> = {
  [Param.Cutoff]: 4_000, [Param.PulseWidth]: 0.5, [Param.Vco1Level]: 1, [Param.LfoRate]: 4,
  [Param.AdsrAttack]: 0.005, [Param.AdsrDecay]: 0.3, [Param.AdsrSustain]: 0.7, [Param.AdsrRelease]: 0.3,
  [Param.FenvAttack]: 0.005, [Param.FenvDecay]: 0.3, [Param.FenvSustain]: 0.7, [Param.FenvRelease]: 0.3,
  [Param.ArAttack]: 0.005, [Param.ArRelease]: 0.3,
}
const BIG = 46

/**
 * A knob for the range a panel gives a parameter: the old slider's minimum,
 * maximum and step, with a scale and a unit worked out from what it is (an
 * exponential cutoff in Hz, envelope times in seconds, tune in semitones or
 * cents, a bipolar amount, otherwise a share of its range).
 */
export function range(label: string, param: ParamId, min: number, max: number, step: number, map?: 'cutoff' | 'lfo'): Control {
  const knob = { kind: 'knob' as const, label, param }
  if (map === 'cutoff') return { ...knob, lo: 20, hi: 20_000, scale: 'exp', unit: 'hz', def: RESET[param] ?? 20, size: BIG }
  if (map === 'lfo') return { ...knob, lo: 0.01, hi: 50, scale: 'exp', unit: 'rate', def: RESET[param] ?? 4 }
  if (min === 0.001) return { ...knob, lo: min, hi: max, scale: 'exp', unit: 'sec', def: RESET[param] ?? 0.3 }
  if (param === Param.Glide) return { ...knob, lo: min, hi: max, scale: 'lin', unit: 'sec', def: 0 }
  if (step === 1 && max === 24) return { ...knob, lo: min, hi: max, scale: 'lin', unit: 'st', step: 1, bipolar: true, def: 0 }
  if (step === 1 && max === 50) return { ...knob, lo: min, hi: max, scale: 'lin', unit: 'ct', step: 1, bipolar: true, def: 0 }
  if (min < 0) return { ...knob, lo: min, hi: max, scale: 'lin', unit: 'bip', bipolar: true, def: 0 }
  if (param === Param.PulseWidth) return { ...knob, lo: min, hi: max, scale: 'lin', unit: 'width', def: RESET[param] ?? 0.5 }
  return { ...knob, lo: min, hi: max, scale: 'lin', unit: 'pct', def: RESET[param] ?? min, ...(param === Param.Resonance ? { size: BIG } : {}) }
}
/** The scale a knob control turns a position into a value with. */
export function scaleOf(c: Extract<Control, { kind: 'knob' }>): Scale {
  if (c.scale === 'exp') return exp(c.lo, c.hi)
  return c.step ? stepped(c.lo, c.hi, c.step) : lin(c.lo, c.hi)
}

/** An envelope with its attack, decay, sustain and release; leave out what it has not. */
const envelope = (label: string, a: ParamId, d?: ParamId, s?: ParamId, r?: ParamId, decayIsRelease = false): Control =>
  ({ kind: 'env', label, a, d, s, r, decayIsRelease })
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
  presets: ['Bass', 'Lead', 'SyncLead', 'BowedString', 'R2D2', 'ShArp', 'SolinaStrings'],
  sections: [
    {
      title: 'ADSR',
      controls: [envelope('', Param.AdsrAttack, Param.AdsrDecay, Param.AdsrSustain, Param.AdsrRelease)],
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
      controls: [envelope('', Param.ArAttack, undefined, undefined, Param.ArRelease)],
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

/** Key tracking in steps, as f32 values so the engine's report selects the step. */
const KEY_STEPS: Options = [['Off', 0], ['⅓', Math.fround(1 / 3)], ['⅔', Math.fround(2 / 3)], ['Full', 1]]

const minimoog: ModelDef = {
  id: Model.Minimoog,
  name: 'Minimoog',
  maker: 'Model D · three oscillators, one ladder',
  tagline: 'Three oscillators, the ladder, two contours; Osc 3 is the modulator; low note priority',
  theme: { panel: '#1a1816', ink: '#efe9dc', soft: '#b6af9f', trim: '#6e4a2c', accent: '#f1ead8', wood: '#5a3a22' },
  presets: ['MiniBass', 'MiniLead', 'LuckyMan', 'FunkBass', 'MoogStrings'],
  sections: [
    {
      title: 'Controllers',
      controls: [
        range('Glide', Param.Glide, 0, 2, 0.01),
        select('Note priority', Param.Priority, PRIORITIES),
        sw('Legato', Param.Legato),
      ],
    },
    {
      title: 'Oscillator 1',
      controls: [range('Range', Param.Vco1Coarse, -24, 24, 1), select('Waveform', Param.Vco1Wave, WAVES)],
    },
    {
      title: 'Oscillator 2',
      controls: [
        range('Range', Param.Vco2Coarse, -24, 24, 1), fine(Param.Vco2Fine), select('Waveform', Param.Vco2Wave, WAVES),
      ],
    },
    {
      title: 'Oscillator 3',
      controls: [
        range('Range', Param.Vco3Coarse, -24, 24, 1), fine(Param.Vco3Fine), select('Waveform', Param.Vco3Wave, WAVES),
        sw('Low frequency', Param.Vco3Low), sw('Keyboard control', Param.Vco3KeyFollow),
      ],
    },
    { title: 'Pulse', controls: [range('Width', Param.PulseWidth, 0.05, 0.95, 0.01)] },
    {
      title: 'Mixer',
      controls: [
        range('Osc 1', Param.Vco1Level, 0, 1, 0.01), range('Osc 2', Param.Vco2Level, 0, 1, 0.01),
        range('Osc 3', Param.Vco3Level, 0, 1, 0.01), range('Noise', Param.NoiseLevel, 0, 1, 0.01),
        select('Noise colour', Param.NoiseColour, NOISES), range('Overdrive', Param.Drive, 0, 1, 0.01),
      ],
    },
    {
      title: 'Filter',
      controls: [
        range('Cutoff', Param.Cutoff, 0, 1, 0.001, 'cutoff'), range('Emphasis', Param.Resonance, 0, 1, 0.01),
        range('Contour amount', Param.EnvCutoff, -1, 1, 0.01), select('Keyboard control', Param.KeyTrack, KEY_STEPS),
        range('Filter modulation', Param.LfoCutoff, 0, 1, 0.01),
      ],
    },
    {
      title: 'Filter contour',
      controls: [envelope('', Param.FenvAttack, Param.FenvDecay, Param.FenvSustain, undefined, true)],
    },
    {
      title: 'Loudness contour',
      controls: [envelope('', Param.AdsrAttack, Param.AdsrDecay, Param.AdsrSustain, undefined, true)],
    },
    {
      title: 'Modulation',
      controls: [
        range('Oscillator modulation', Param.Vibrato, 0, 1, 0.01), range('Mod wheel', Param.ModWheel, 0, 1, 0.01),
        { kind: 'note', text: 'Osc 3 is the modulation source; decay is also the release.' },
      ],
    },
  ],
}

const HALF_FULL: Options = [['Off', 0], ['Half', 0.5], ['Full', 1]]
const adsrControls = (a: ParamId, d: ParamId, su: ParamId, r: ParamId): Control[] => [envelope('', a, d, su, r)]

// Oscillator A is VCO 2 and B is VCO 1, so A can be synced to B and poly-mod
// runs from B into A, in the direction the instrument has (spec 005 Req 4).
const proOne: ModelDef = {
  id: Model.ProOne,
  name: 'Pro-One',
  maker: 'Sequential · two oscillators, poly-mod',
  tagline: 'Oscillator A synced to B, poly-mod from the filter envelope and B, 4-pole filter',
  theme: { panel: '#241f1c', ink: '#f1e6d2', soft: '#bcae98', trim: '#8c2f1f', accent: '#e8482b', wood: '#6b4a2e' },
  presets: ['ProLead', 'ProBass', 'SyncSweep', 'PolyModBell', 'ProStrings'],
  sections: [
    {
      title: 'Oscillator A',
      controls: [
        range('Frequency', Param.Vco2Coarse, -24, 24, 1), fine(Param.Vco2Fine), select('Wave', Param.Vco2Wave, WAVES),
        range('Pulse width', Param.PulseWidth, 0.05, 0.95, 0.01), sw('Sync to B', Param.Vco2Sync),
      ],
    },
    {
      title: 'Oscillator B',
      controls: [range('Frequency', Param.Vco1Coarse, -24, 24, 1), fine(Param.Vco1Fine), select('Wave', Param.Vco1Wave, WAVES)],
    },
    {
      title: 'Mixer',
      controls: [
        range('Osc A', Param.Vco2Level, 0, 1, 0.01), range('Osc B', Param.Vco1Level, 0, 1, 0.01),
        range('Noise', Param.NoiseLevel, 0, 1, 0.01), select('Noise colour', Param.NoiseColour, NOISES),
      ],
    },
    {
      title: 'Poly-mod',
      controls: [
        range('Filter env → Freq A', Param.EnvFreq2, -1, 1, 0.01), range('Osc B → Freq A', Param.OscFreq2, -1, 1, 0.01),
        range('Filter env → PW A', Param.EnvPw, -1, 1, 0.01), range('Osc B → PW A', Param.OscPw, -1, 1, 0.01),
        range('Osc B → Filter', Param.OscCutoff, -1, 1, 0.01),
      ],
    },
    {
      title: 'Filter',
      controls: [
        range('Cutoff', Param.Cutoff, 0, 1, 0.001, 'cutoff'), range('Resonance', Param.Resonance, 0, 1, 0.01),
        range('Envelope amount', Param.EnvCutoff, -1, 1, 0.01), select('Key track', Param.KeyTrack, HALF_FULL),
        range('Drive', Param.Drive, 0, 1, 0.01),
      ],
    },
    { title: 'Filter envelope', controls: adsrControls(Param.FenvAttack, Param.FenvDecay, Param.FenvSustain, Param.FenvRelease) },
    { title: 'Amplifier envelope', controls: adsrControls(Param.AdsrAttack, Param.AdsrDecay, Param.AdsrSustain, Param.AdsrRelease) },
    {
      title: 'LFO',
      controls: [
        select('Wave', Param.LfoWave, LFO_WAVES), range('Rate', Param.LfoRate, 0, 1, 0.001, 'lfo'),
        range('→ Freq', Param.Vibrato, 0, 1, 0.01), range('→ Filter', Param.LfoCutoff, 0, 1, 0.01),
        range('→ PW A', Param.LfoPw, 0, 1, 0.01), range('Mod wheel', Param.ModWheel, 0, 1, 0.01),
      ],
    },
    {
      title: 'Keys',
      controls: [select('Priority', Param.Priority, PRIORITIES), sw('Legato', Param.Legato), range('Glide', Param.Glide, 0, 2, 0.01)],
    },
  ],
}

/** What every polyphonic model has: how notes are assigned, the unison spread and the vintage drift. */
const ASSIGN: Options = [['Poly', 0], ['Unison', 1]]
const voicesSection = (): Section => ({
  title: 'Voices',
  controls: [select('Assign', Param.Assign, ASSIGN), range('Unison detune', Param.UnisonDetune, 0, 1, 0.01), range('Vintage', Param.Analog, 0, 1, 0.01)],
})

// Five voices of the Pro-One's two-oscillator voice (spec 006 Req 6): A is VCO 2,
// B is VCO 1, poly-mod from the filter envelope and B, unison and vintage drift.
const prophet5: ModelDef = {
  id: Model.Prophet5,
  name: 'Prophet-5',
  maker: 'Sequential · five voices, poly-mod',
  tagline: 'Five voices: two oscillators, poly-mod, a 4-pole filter, two envelopes, unison, vintage drift',
  theme: { panel: '#1c1b1a', ink: '#f4ead8', soft: '#bfae94', trim: '#3a2a1c', accent: '#f0a73a', wood: '#6e4426' },
  presets: ['P5Brass', 'P5Strings', 'P5Bass', 'P5SyncLead', 'P5Bell', 'P5Pad'],
  sections: [
    {
      title: 'Oscillator A',
      controls: [
        range('Frequency', Param.Vco2Coarse, -24, 24, 1), fine(Param.Vco2Fine), select('Wave', Param.Vco2Wave, WAVES),
        range('Pulse width', Param.PulseWidth, 0.05, 0.95, 0.01), sw('Sync to B', Param.Vco2Sync),
      ],
    },
    {
      title: 'Oscillator B',
      controls: [range('Frequency', Param.Vco1Coarse, -24, 24, 1), fine(Param.Vco1Fine), select('Wave', Param.Vco1Wave, WAVES)],
    },
    {
      title: 'Mixer',
      controls: [
        range('Osc A', Param.Vco2Level, 0, 1, 0.01), range('Osc B', Param.Vco1Level, 0, 1, 0.01),
        range('Noise', Param.NoiseLevel, 0, 1, 0.01), select('Noise colour', Param.NoiseColour, NOISES),
      ],
    },
    {
      title: 'Poly-mod',
      controls: [
        range('Filter env → Freq A', Param.EnvFreq2, -1, 1, 0.01), range('Osc B → Freq A', Param.OscFreq2, -1, 1, 0.01),
        range('Filter env → PW A', Param.EnvPw, -1, 1, 0.01), range('Osc B → PW A', Param.OscPw, -1, 1, 0.01),
        range('Osc B → Filter', Param.OscCutoff, -1, 1, 0.01),
      ],
    },
    {
      title: 'Filter',
      controls: [
        range('Cutoff', Param.Cutoff, 0, 1, 0.001, 'cutoff'), range('Resonance', Param.Resonance, 0, 1, 0.01),
        range('Envelope amount', Param.EnvCutoff, -1, 1, 0.01), select('Key track', Param.KeyTrack, HALF_FULL),
      ],
    },
    { title: 'Filter envelope', controls: adsrControls(Param.FenvAttack, Param.FenvDecay, Param.FenvSustain, Param.FenvRelease) },
    { title: 'Amplifier envelope', controls: adsrControls(Param.AdsrAttack, Param.AdsrDecay, Param.AdsrSustain, Param.AdsrRelease) },
    {
      title: 'LFO',
      controls: [
        select('Wave', Param.LfoWave, LFO_WAVES), range('Rate', Param.LfoRate, 0, 1, 0.001, 'lfo'),
        range('→ Freq', Param.Vibrato, 0, 1, 0.01), range('→ Filter', Param.LfoCutoff, 0, 1, 0.01),
        range('→ PW A', Param.LfoPw, 0, 1, 0.01), range('Mod wheel', Param.ModWheel, 0, 1, 0.01),
      ],
    },
    voicesSection(),
  ],
}

/** The Juno's high-pass in its four steps, and its chorus modes. */
const HPF_STEPS: Options = [['0', 20], ['1', 240], ['2', 720], ['3', 1600]]
const CHORUS: Options = [['Off', 0], ['I', 1], ['II', 2], ['I+II', 3]]

// Six voices of one DCO (saw and a locked pulse, a sub), an IR3109-voiced
// low-pass, a high-pass in four steps, one envelope for filter and loudness,
// and the stereo chorus (spec 006 Req 7). The pulse is VCO 2, locked by the model.
const juno106: ModelDef = {
  id: Model.Juno106,
  name: 'Juno-106',
  maker: 'Roland · six voices, DCO, chorus',
  tagline: 'Six voices: a DCO with sub, a 24 dB low-pass, one envelope, and the stereo chorus',
  theme: { panel: '#262b30', ink: '#eaeef2', soft: '#a3adb8', trim: '#14171a', accent: '#3fb6c9' },
  presets: ['JunoPad', 'JunoStrings', 'JunoBrass', 'JunoBass', 'JunoPluck', 'JunoPoly'],
  sections: [
    {
      title: 'LFO',
      controls: [
        select('Wave', Param.LfoWave, LFO_WAVES), range('Rate', Param.LfoRate, 0, 1, 0.001, 'lfo'),
        range('→ Pitch', Param.Vibrato, 0, 1, 0.01), range('→ Filter', Param.LfoCutoff, 0, 1, 0.01),
        range('Mod wheel', Param.ModWheel, 0, 1, 0.01),
      ],
    },
    {
      title: 'DCO',
      controls: [
        range('Pulse width', Param.PulseWidth, 0.05, 0.95, 0.01), range('PWM · LFO', Param.LfoPw, 0, 1, 0.01),
        select('Range', Param.Vco1Coarse, [['16′', -12], ['8′', 0], ['4′', 12]]),
      ],
    },
    {
      title: 'Mixer',
      controls: [
        range('Saw', Param.Vco1Level, 0, 1, 0.01), range('Pulse', Param.Vco2Level, 0, 1, 0.01),
        range('Sub', Param.SubLevel, 0, 1, 0.01), range('Noise', Param.NoiseLevel, 0, 1, 0.01),
      ],
    },
    { title: 'HPF', controls: [select('Cutoff', Param.HpCutoff, HPF_STEPS)] },
    {
      title: 'VCF',
      controls: [
        range('Cutoff', Param.Cutoff, 0, 1, 0.001, 'cutoff'), range('Resonance', Param.Resonance, 0, 1, 0.01),
        range('Envelope', Param.EnvCutoff, -1, 1, 0.01), range('Key follow', Param.KeyTrack, 0, 1, 0.01),
      ],
    },
    { title: 'Envelope', controls: adsrControls(Param.AdsrAttack, Param.AdsrDecay, Param.AdsrSustain, Param.AdsrRelease) },
    { title: 'Chorus', controls: [select('Mode', Param.ChorusMode, CHORUS)] },
    voicesSection(),
  ],
}

const ms20: ModelDef = {
  id: Model.Ms20,
  name: 'MS-20',
  maker: 'Korg · high-pass and low-pass, patch panel',
  tagline: 'Two oscillators, ring mod, a high-pass and a low-pass that both scream, a patch panel',
  theme: { panel: '#18191c', ink: '#f6efdc', soft: '#b9b4a4', trim: '#6e5a12', accent: '#f2c230' },
  presets: ['Ms20Lead', 'Ms20Wobble', 'Ms20Squelch', 'JetSweep', 'Ms20Strings'],
  sections: [
    {
      title: 'VCO 1',
      controls: [
        select('Wave', Param.Vco1Wave, WAVES), range('Pitch', Param.Vco1Coarse, -24, 24, 1),
        fine(Param.Vco1Fine), range('Pulse width', Param.PulseWidth, 0.05, 0.95, 0.01),
      ],
    },
    {
      title: 'VCO 2',
      controls: [select('Wave', Param.Vco2Wave, WAVES), range('Pitch', Param.Vco2Coarse, -24, 24, 1), fine(Param.Vco2Fine)],
    },
    {
      title: 'Mixer',
      controls: [
        range('VCO 1', Param.Vco1Level, 0, 1, 0.01), range('VCO 2', Param.Vco2Level, 0, 1, 0.01),
        range('Ring mod', Param.RingLevel, 0, 1, 0.01), range('Noise', Param.NoiseLevel, 0, 1, 0.01),
        select('Noise colour', Param.NoiseColour, NOISES),
      ],
    },
    {
      title: 'High-pass',
      controls: [range('Cutoff', Param.HpCutoff, 0, 1, 0.001, 'cutoff'), range('Peak', Param.HpResonance, 0, 1, 0.01)],
    },
    {
      title: 'Low-pass',
      controls: [
        range('Cutoff', Param.Cutoff, 0, 1, 0.001, 'cutoff'), range('Peak', Param.Resonance, 0, 1, 0.01),
        range('Drive', Param.Drive, 0, 1, 0.01),
      ],
    },
    {
      title: 'Filter modulation',
      controls: [
        range('EG 2 → LPF', Param.EnvCutoff, -1, 1, 0.01), range('EG 2 → HPF', Param.EnvHpCutoff, -1, 1, 0.01),
        range('MG → LPF', Param.LfoCutoff, 0, 1, 0.01), range('Key track', Param.KeyTrack, 0, 1, 0.01),
      ],
    },
    {
      title: 'MG',
      controls: [
        select('Wave', Param.LfoWave, LFO_WAVES), range('Frequency', Param.LfoRate, 0, 1, 0.001, 'lfo'),
        range('→ Pitch', Param.Vibrato, 0, 1, 0.01), range('Mod wheel', Param.ModWheel, 0, 1, 0.01),
      ],
    },
    { title: 'EG 1 (amplifier)', controls: adsrControls(Param.AdsrAttack, Param.AdsrDecay, Param.AdsrSustain, Param.AdsrRelease) },
    { title: 'EG 2 (filters)', controls: adsrControls(Param.FenvAttack, Param.FenvDecay, Param.FenvSustain, Param.FenvRelease) },
    {
      title: 'Keys',
      controls: [range('Portamento', Param.Glide, 0, 2, 0.01), select('Priority', Param.Priority, PRIORITIES), sw('Legato', Param.Legato)],
    },
    { title: 'Patch panel', controls: [], patch: true },
  ],
}

const cs15: ModelDef = {
  id: Model.Cs15,
  name: 'CS-15',
  maker: 'Yamaha · two filters, an envelope for each',
  tagline: 'Two oscillators, ring mod, a high-pass and a low-pass each with an envelope of its own',
  theme: { panel: '#2c1f19', ink: '#f2e4ca', soft: '#bba98b', trim: '#6b4b2f', accent: '#d9b27c' },
  presets: ['Cs15Brass', 'Cs15Lead', 'BladeBrass', 'Cs15Strings'],
  sections: [
    {
      title: 'VCO 1',
      controls: [
        select('Wave', Param.Vco1Wave, WAVES), range('Octave', Param.Vco1Coarse, -24, 24, 1),
        fine(Param.Vco1Fine), range('Pulse width', Param.PulseWidth, 0.05, 0.95, 0.01),
      ],
    },
    {
      title: 'VCO 2',
      controls: [select('Wave', Param.Vco2Wave, WAVES), range('Octave', Param.Vco2Coarse, -24, 24, 1), fine(Param.Vco2Fine)],
    },
    {
      title: 'Mixer',
      controls: [
        range('VCO 1', Param.Vco1Level, 0, 1, 0.01), range('VCO 2', Param.Vco2Level, 0, 1, 0.01),
        range('Ring mod', Param.RingLevel, 0, 1, 0.01), range('Noise', Param.NoiseLevel, 0, 1, 0.01),
        select('Noise colour', Param.NoiseColour, NOISES),
      ],
    },
    {
      title: 'HPF',
      controls: [
        range('Cutoff', Param.HpCutoff, 0, 1, 0.001, 'cutoff'), range('Resonance', Param.HpResonance, 0, 1, 0.01),
        range('Envelope (AR)', Param.EnvHpCutoff, -1, 1, 0.01),
      ],
    },
    {
      title: 'LPF',
      controls: [
        range('Cutoff', Param.Cutoff, 0, 1, 0.001, 'cutoff'), range('Resonance', Param.Resonance, 0, 1, 0.01),
        range('Envelope (ADSR)', Param.EnvCutoff, -1, 1, 0.01), range('Key track', Param.KeyTrack, 0, 1, 0.01),
        range('Drive', Param.Drive, 0, 1, 0.01),
      ],
    },
    { title: 'Amplifier envelope', controls: adsrControls(Param.AdsrAttack, Param.AdsrDecay, Param.AdsrSustain, Param.AdsrRelease) },
    { title: 'LPF envelope', controls: adsrControls(Param.FenvAttack, Param.FenvDecay, Param.FenvSustain, Param.FenvRelease) },
    {
      title: 'HPF envelope',
      controls: [envelope('', Param.ArAttack, undefined, undefined, Param.ArRelease)],
    },
    {
      title: 'LFO',
      controls: [
        select('Wave', Param.LfoWave, LFO_WAVES), range('Speed', Param.LfoRate, 0, 1, 0.001, 'lfo'),
        range('→ Pitch', Param.Vibrato, 0, 1, 0.01), range('→ LPF', Param.LfoCutoff, 0, 1, 0.01),
        range('→ PW', Param.LfoPw, 0, 1, 0.01), range('Mod wheel', Param.ModWheel, 0, 1, 0.01),
      ],
    },
    {
      title: 'Keys',
      controls: [range('Portamento', Param.Glide, 0, 2, 0.01), select('Priority', Param.Priority, PRIORITIES), sw('Legato', Param.Legato)],
    },
    { title: 'Sample & hold, patch', controls: [], patch: true },
  ],
}

const OCTAVES: Options = [['16′', -12], ['8′', 0], ['4′', 12], ['2′', 24]]
const SUB: Options = [['−1 oct', 0], ['−2 oct', 1]]

// One oscillator gives saw and pulse together: the pulse is VCO 2 locked to
// VCO 1 by the engine (spec 005 Req 7), so the panel has one Range.
const sh101: ModelDef = {
  id: Model.Sh101,
  name: 'SH-101',
  maker: 'Roland · one oscillator, sub, one envelope',
  tagline: 'Saw, pulse and sub from one oscillator, a resonant low-pass, one envelope for filter and loudness',
  theme: { panel: '#34373b', ink: '#eef1f3', soft: '#adb3b9', trim: '#1c1e21', accent: '#3b8fd6' },
  presets: ['Sh101Bass', 'Sh101Lead', 'AcidBass', 'SubPluck', 'Sh101Strings'],
  sections: [
    {
      title: 'Controller',
      controls: [range('Portamento', Param.Glide, 0, 2, 0.01), select('Priority', Param.Priority, PRIORITIES), sw('Legato', Param.Legato)],
    },
    {
      title: 'LFO',
      controls: [
        select('Wave', Param.LfoWave, LFO_WAVES), range('Rate', Param.LfoRate, 0, 1, 0.001, 'lfo'),
        range('Pitch', Param.Vibrato, 0, 1, 0.01), range('Mod wheel', Param.ModWheel, 0, 1, 0.01),
      ],
    },
    {
      title: 'VCO',
      controls: [
        select('Range', Param.Vco1Coarse, OCTAVES), fine(Param.Vco1Fine),
        range('Pulse width', Param.PulseWidth, 0.05, 0.95, 0.01),
        range('PWM · LFO', Param.LfoPw, 0, 1, 0.01), range('PWM · envelope', Param.EnvPw, -1, 1, 0.01),
      ],
    },
    {
      title: 'Source mixer',
      controls: [
        range('Saw', Param.Vco1Level, 0, 1, 0.01), range('Pulse', Param.Vco2Level, 0, 1, 0.01),
        range('Sub', Param.SubLevel, 0, 1, 0.01), select('Sub octave', Param.SubOctave, SUB),
        range('Noise', Param.NoiseLevel, 0, 1, 0.01),
      ],
    },
    {
      title: 'HPF',
      controls: [range('Cutoff', Param.HpCutoff, 0, 1, 0.001, 'cutoff')],
    },
    {
      title: 'VCF',
      controls: [
        range('Cutoff', Param.Cutoff, 0, 1, 0.001, 'cutoff'), range('Resonance', Param.Resonance, 0, 1, 0.01),
        range('Envelope', Param.EnvCutoff, -1, 1, 0.01), range('LFO', Param.LfoCutoff, 0, 1, 0.01),
        range('Key follow', Param.KeyTrack, 0, 1, 0.01),
      ],
    },
    { title: 'Envelope', controls: adsrControls(Param.AdsrAttack, Param.AdsrDecay, Param.AdsrSustain, Param.AdsrRelease) },
  ],
}

// The ARP Odyssey Mk II (spec 005 Req 10): two VCOs with sync, ring mod and
// noise, a 24 dB ladder and a 6 dB high-pass after it, one ADSR to filter and
// VCA. Not modular, so no patch bay: the presets' patch slots still play.
// Black and gold.
const odyssey: ModelDef = {
  id: Model.Odyssey,
  name: 'Odyssey',
  maker: 'ARP · two oscillators, sync, high-pass after the ladder',
  tagline: 'Two oscillators with hard sync and ring mod, a bright 24 dB ladder, a high-pass after it, glide',
  theme: { panel: '#121212', ink: '#f0e3b8', soft: '#a6996f', trim: '#8a6d24', accent: '#c9a227' },
  presets: ['CurrieLead', 'OdysseySync'],
  sections: [
    {
      title: 'VCO 1',
      controls: [
        select('Wave', Param.Vco1Wave, WAVES), coarse(Param.Vco1Coarse), fine(Param.Vco1Fine),
        range('Pulse width', Param.PulseWidth, 0.05, 0.95, 0.01),
      ],
    },
    {
      title: 'VCO 2',
      controls: [select('Wave', Param.Vco2Wave, WAVES), coarse(Param.Vco2Coarse), fine(Param.Vco2Fine), sw('Sync', Param.Vco2Sync)],
    },
    {
      title: 'Mixer',
      controls: [
        range('VCO 1', Param.Vco1Level, 0, 1, 0.01), range('VCO 2', Param.Vco2Level, 0, 1, 0.01),
        range('Ring mod', Param.RingLevel, 0, 1, 0.01), range('Noise', Param.NoiseLevel, 0, 1, 0.01),
        select('Noise colour', Param.NoiseColour, NOISES),
      ],
    },
    { title: 'HPF', controls: [range('Cutoff', Param.HpCutoff, 0, 1, 0.001, 'cutoff')] },
    {
      title: 'VCF',
      controls: [
        range('Cutoff', Param.Cutoff, 0, 1, 0.001, 'cutoff'), range('Resonance', Param.Resonance, 0, 1, 0.01),
        range('ADSR', Param.EnvCutoff, -1, 1, 0.01), range('Key track', Param.KeyTrack, 0, 1, 0.01),
        range('LFO', Param.LfoCutoff, 0, 1, 0.01),
      ],
    },
    { title: 'ADSR', controls: adsrControls(Param.AdsrAttack, Param.AdsrDecay, Param.AdsrSustain, Param.AdsrRelease) },
    {
      title: 'LFO',
      controls: [
        select('Wave', Param.LfoWave, LFO_WAVES), range('Speed', Param.LfoRate, 0, 1, 0.001, 'lfo'),
        range('→ Pitch', Param.Vibrato, 0, 1, 0.01), range('→ PW', Param.LfoPw, 0, 1, 0.01),
        range('Mod wheel', Param.ModWheel, 0, 1, 0.01),
      ],
    },
    {
      title: 'Keys',
      controls: [range('Portamento', Param.Glide, 0, 2, 0.01), select('Priority', Param.Priority, PRIORITIES), sw('Legato', Param.Legato)],
    },
  ],
}

/** The models the view offers, in the order of the picker. */
export const MODELS: ModelDef[] = [arp2600, minimoog, proOne, ms20, cs15, sh101, odyssey, prophet5, juno106]

/** The definition of a model id; an unknown one draws as the ARP 2600. */
export const modelDef = (id: number): ModelDef => MODELS.find((m) => m.id === id) ?? (MODELS[0] as ModelDef)
