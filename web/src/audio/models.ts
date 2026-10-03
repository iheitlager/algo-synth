// The synth models as data (spec 005, ADR-0009): for each instrument its
// panel (sections, controls, the names the real machine uses) and its colours.
// The view only draws this and sends parameter changes; which control belongs
// on which panel is layout, and every musical decision stays in Rust.

import { DriveMode, Model, ModDest, ModSource, NoiseColour, NotePriority, Param, Preset, Waveform } from './params'
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
export const DRIVE_MODES = entries(DriveMode)
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

const attack = (p: ParamId) => range('Attack', p, 0.001, 2, 0.001)
const decay = (label: string, p: ParamId) => range(label, p, 0.001, 4, 0.001)
const sustain = (p: ParamId) => range('Sustain', p, 0, 1, 0.01)
/** Key tracking in steps, as f32 values so the engine's report selects the step. */
const KEY_STEPS: Options = [['Off', 0], ['⅓', Math.fround(1 / 3)], ['⅔', Math.fround(2 / 3)], ['Full', 1]]

const minimoog: ModelDef = {
  id: Model.Minimoog,
  name: 'Minimoog',
  maker: 'Model D · three oscillators, one ladder',
  tagline: 'Three oscillators, the ladder, two contours; Osc 3 is the modulator; low note priority',
  theme: { panel: '#1a1816', ink: '#efe9dc', soft: '#b6af9f', trim: '#6e4a2c', accent: '#f1ead8' },
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
      controls: [attack(Param.FenvAttack), decay('Decay', Param.FenvDecay), sustain(Param.FenvSustain)],
    },
    {
      title: 'Loudness contour',
      controls: [attack(Param.AdsrAttack), decay('Decay', Param.AdsrDecay), sustain(Param.AdsrSustain)],
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
const adsrControls = (a: ParamId, d: ParamId, su: ParamId, r: ParamId): Control[] => [
  attack(a), decay('Decay', d), sustain(su), decay('Release', r),
]

// Oscillator A is VCO 2 and B is VCO 1, so A can be synced to B and poly-mod
// runs from B into A, in the direction the instrument has (spec 005 Req 4).
const proOne: ModelDef = {
  id: Model.ProOne,
  name: 'Pro-One',
  maker: 'Sequential · two oscillators, poly-mod',
  tagline: 'Oscillator A synced to B, poly-mod from the filter envelope and B, 4-pole filter',
  theme: { panel: '#241f1c', ink: '#f1e6d2', soft: '#bcae98', trim: '#8c2f1f', accent: '#e8482b' },
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
      controls: [range('Attack', Param.ArAttack, 0.001, 2, 0.001), decay('Release', Param.ArRelease)],
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

// What every synth has, whatever its model: its mixer strip and its drive
// insert (spec 002, spec 004 Req 11). They come last on every panel.
const strip: Section[] = [
  {
    title: 'Channel',
    controls: [
      range('Level', Param.Level, 0, 1, 0.01), range('Pan', Param.Pan, -1, 1, 0.01),
      range('Echo', Param.EchoSend, 0, 1, 0.01), range('Reverb', Param.ReverbSend, 0, 1, 0.01),
      sw('Mute', Param.Mute), sw('Solo', Param.Solo),
    ],
  },
  {
    title: 'Drive insert',
    controls: [
      select('Mode', Param.DriveMode, DRIVE_MODES), range('Amount', Param.DriveAmount, 0, 1, 0.01),
      range('Tone', Param.DriveTone, 0, 1, 0.01), range('Level', Param.DriveLevel, 0, 1, 0.01),
    ],
  },
]

/** The models the view offers, in the order of the picker. */
export const MODELS: ModelDef[] = [arp2600, minimoog, proOne, ms20, cs15, sh101].map((m) => ({
  ...m,
  sections: [...m.sections, ...strip],
}))

/** The definition of a model id; an unknown one draws as the ARP 2600. */
export const modelDef = (id: number): ModelDef => MODELS.find((m) => m.id === id) ?? (MODELS[0] as ModelDef)
