// Mirror of crates/dsp/src/params.rs, source.rs, mono/osc.rs, mono/noise.rs
// and mono/preset.rs (ADR-0004).
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
