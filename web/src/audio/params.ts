// Mirror of crates/dsp/src/params.rs, source.rs, mono/osc.rs, mono/noise.rs,
// mono/preset.rs, mono/voice.rs and mono/patch.rs (ADR-0004).
// Rust is the source of truth; `cargo test` fails if a line here drifts.
// Keep the `Name: id,` shape: the test greps for it.

export const Param = {
  MasterGain: 0,
  Attack: 1,
  Release: 2,
  Vco1Wave: 3,
  Vco1Coarse: 4,
  Vco1Fine: 5,
  Vco1Level: 6,
  Vco2Wave: 7,
  Vco2Coarse: 8,
  Vco2Fine: 9,
  Vco2Level: 10,
  Vco3Wave: 11,
  Vco3Coarse: 12,
  Vco3Fine: 13,
  Vco3Level: 14,
  PulseWidth: 15,
  Vco2Sync: 16,
  Vco3Sync: 17,
  NoiseLevel: 18,
  NoiseColour: 19,
  Cutoff: 20,
  Resonance: 21,
  Drive: 22,
  AdsrAttack: 23,
  AdsrDecay: 24,
  AdsrSustain: 25,
  AdsrRelease: 26,
  ArAttack: 27,
  ArRelease: 28,
  LfoRate: 29,
  LfoWave: 30,
  Priority: 31,
  Legato: 32,
  Glide: 33,
  Patch1Source: 34,
  Patch1Dest: 35,
  Patch1Amount: 36,
  Patch2Source: 37,
  Patch2Dest: 38,
  Patch2Amount: 39,
  Patch3Source: 40,
  Patch3Dest: 41,
  Patch3Amount: 42,
  Patch4Source: 43,
  Patch4Dest: 44,
  Patch4Amount: 45,
  Patch5Source: 46,
  Patch5Dest: 47,
  Patch5Amount: 48,
  Patch6Source: 49,
  Patch6Dest: 50,
  Patch6Amount: 51,
  Patch7Source: 52,
  Patch7Dest: 53,
  Patch7Amount: 54,
  Patch8Source: 55,
  Patch8Dest: 56,
  Patch8Amount: 57,
  EnvCutoff: 58,
  KeyTrack: 59,
  Vibrato: 60,
  ModWheel: 61,
} as const
export type ParamId = (typeof Param)[keyof typeof Param]

export const Source = {
  Mono: 0,
  Wave: 1,
  Drums: 2,
} as const
export type SourceId = (typeof Source)[keyof typeof Source]

export const Waveform = {
  Saw: 0,
  Pulse: 1,
  Triangle: 2,
  Sine: 3,
} as const
export type WaveformId = (typeof Waveform)[keyof typeof Waveform]

export const NoiseColour = {
  White: 0,
  Pink: 1,
} as const
export type NoiseColourId = (typeof NoiseColour)[keyof typeof NoiseColour]

export const Preset = {
  Bass: 0,
  Lead: 1,
  SyncLead: 2,
  BowedString: 3,
} as const
export type PresetId = (typeof Preset)[keyof typeof Preset]

export const NotePriority = {
  Last: 0,
  Low: 1,
  High: 2,
} as const
export type NotePriorityId = (typeof NotePriority)[keyof typeof NotePriority]

export const ModSource = {
  None: 0,
  Vco1: 1,
  Vco2: 2,
  Vco3: 3,
  Noise: 4,
  Adsr: 5,
  Ar: 6,
  Lfo: 7,
  SampleHold: 8,
  ModWheel: 9,
  Velocity: 10,
  Key: 11,
} as const
export type ModSourceId = (typeof ModSource)[keyof typeof ModSource]

export const ModDest = {
  None: 0,
  Vco1Pitch: 1,
  Vco2Pitch: 2,
  Vco3Pitch: 3,
  PulseWidth: 4,
  Cutoff: 5,
  Resonance: 6,
  Vca: 7,
  LfoRate: 8,
} as const
export type ModDestId = (typeof ModDest)[keyof typeof ModDest]
