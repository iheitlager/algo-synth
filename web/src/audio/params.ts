// Mirror of crates/dsp/src/params.rs, mono/osc.rs, mono/noise.rs,
// mono/preset.rs, fx/drive.rs, mono/voice.rs and mono/patch.rs (ADR-0004).
// Rust is the source of truth; `cargo test` fails if a line here drifts.
// Keep the `Name: id,` shape: the test greps for it.

export const Param = {
  MasterGain: 0,
  Vco1Wave: 1,
  Vco1Coarse: 2,
  Vco1Fine: 3,
  Vco1Level: 4,
  Vco2Wave: 5,
  Vco2Coarse: 6,
  Vco2Fine: 7,
  Vco2Level: 8,
  Vco3Wave: 9,
  Vco3Coarse: 10,
  Vco3Fine: 11,
  Vco3Level: 12,
  PulseWidth: 13,
  Vco2Sync: 14,
  Vco3Sync: 15,
  NoiseLevel: 16,
  NoiseColour: 17,
  Cutoff: 18,
  Resonance: 19,
  Drive: 20,
  AdsrAttack: 21,
  AdsrDecay: 22,
  AdsrSustain: 23,
  AdsrRelease: 24,
  ArAttack: 25,
  ArRelease: 26,
  LfoRate: 27,
  LfoWave: 28,
  Priority: 29,
  Legato: 30,
  Glide: 31,
  Patch1Source: 32,
  Patch1Dest: 33,
  Patch1Amount: 34,
  Patch2Source: 35,
  Patch2Dest: 36,
  Patch2Amount: 37,
  Patch3Source: 38,
  Patch3Dest: 39,
  Patch3Amount: 40,
  Patch4Source: 41,
  Patch4Dest: 42,
  Patch4Amount: 43,
  Patch5Source: 44,
  Patch5Dest: 45,
  Patch5Amount: 46,
  Patch6Source: 47,
  Patch6Dest: 48,
  Patch6Amount: 49,
  Patch7Source: 50,
  Patch7Dest: 51,
  Patch7Amount: 52,
  Patch8Source: 53,
  Patch8Dest: 54,
  Patch8Amount: 55,
  EnvCutoff: 56,
  KeyTrack: 57,
  Vibrato: 58,
  ModWheel: 59,
  Level: 60,
  Pan: 61,
  EchoSend: 62,
  ReverbSend: 63,
  Mute: 64,
  Solo: 65,
  DriveMode: 66,
  DriveAmount: 67,
  DriveTone: 68,
  DriveLevel: 69,
  EchoTime: 70,
  EchoFeedback: 71,
  EchoTone: 72,
  EchoPingPong: 73,
  EchoReturn: 74,
  ReverbSize: 75,
  ReverbDamping: 76,
  ReverbPreDelay: 77,
  ReverbReturn: 78,
} as const
export type ParamId = (typeof Param)[keyof typeof Param]

export const Waveform = {
  Saw: 0,
  Pulse: 1,
  Triangle: 2,
  Sine: 3,
} as const
export type WaveformId = (typeof Waveform)[keyof typeof Waveform]

export const DriveMode = {
  Off: 0,
  Overdrive: 1,
  Distortion: 2,
  Fuzz: 3,
} as const
export type DriveModeId = (typeof DriveMode)[keyof typeof DriveMode]

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
