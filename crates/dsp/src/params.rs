//! The one parameter registry (ADR-0004).
//!
//! Every knob in the UI maps to `set_param(id, value)`. The ids here are the
//! source of truth; `web/src/audio/params.ts` mirrors them and a test below
//! fails if the two drift apart.

/// A parameter id as it crosses the C ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Param {
    /// Master output gain, 0..=1.
    MasterGain = 0,
    /// VCO 1 waveform id (`Waveform`), 0..=3.
    Vco1Wave = 1,
    /// VCO 1 coarse tune in semitones, −24..=24.
    Vco1Coarse = 2,
    /// VCO 1 fine tune in cents, −50..=50.
    Vco1Fine = 3,
    /// VCO 1 level into the mixer, 0..=1.
    Vco1Level = 4,
    /// VCO 2 waveform id (`Waveform`), 0..=3.
    Vco2Wave = 5,
    /// VCO 2 coarse tune in semitones, −24..=24.
    Vco2Coarse = 6,
    /// VCO 2 fine tune in cents, −50..=50.
    Vco2Fine = 7,
    /// VCO 2 level into the mixer, 0..=1.
    Vco2Level = 8,
    /// VCO 3 waveform id (`Waveform`), 0..=3.
    Vco3Wave = 9,
    /// VCO 3 coarse tune in semitones, −24..=24.
    Vco3Coarse = 10,
    /// VCO 3 fine tune in cents, −50..=50.
    Vco3Fine = 11,
    /// VCO 3 level into the mixer, 0..=1.
    Vco3Level = 12,
    /// Pulse width of every VCO, 0.05..=0.95.
    PulseWidth = 13,
    /// VCO 2 hard-syncs to VCO 1 when ≥ 0.5.
    Vco2Sync = 14,
    /// VCO 3 hard-syncs to VCO 1 when ≥ 0.5.
    Vco3Sync = 15,
    /// Noise level into the mixer, 0..=1.
    NoiseLevel = 16,
    /// Noise colour id (`NoiseColour`), 0..=1.
    NoiseColour = 17,
    /// Ladder cutoff in Hz, 20..=20000.
    Cutoff = 18,
    /// Ladder resonance, 0..=1; it self-oscillates from 0.8.
    Resonance = 19,
    /// Ladder drive into the saturator, 0..=1 (0 to +18 dB).
    Drive = 20,
    /// ADSR attack, decay and release in seconds, 0.001..=10.
    AdsrAttack = 21,
    AdsrDecay = 22,
    /// ADSR sustain level, 0..=1.
    AdsrSustain = 23,
    AdsrRelease = 24,
    /// AR attack and release in seconds, 0.001..=10.
    ArAttack = 25,
    ArRelease = 26,
    /// LFO rate in Hz, 0.01..=50.
    LfoRate = 27,
    /// LFO waveform id (`Waveform`; pulse is the square), 0..=3.
    LfoWave = 28,
    /// Mono note priority id (`NotePriority`: last, low, high), 0..=2.
    Priority = 29,
    /// Mono legato when ≥ 0.5: a new key while one is held keeps the envelope.
    Legato = 30,
    /// Mono glide time in seconds, 0..=5; 0 is off.
    Glide = 31,
    /// Patch slot 1 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch1Source = 32,
    /// Patch slot 1 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch1Dest = 33,
    /// Patch slot 1 amount, −1..=1.
    Patch1Amount = 34,
    /// Patch slot 2 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch2Source = 35,
    /// Patch slot 2 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch2Dest = 36,
    /// Patch slot 2 amount, −1..=1.
    Patch2Amount = 37,
    /// Patch slot 3 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch3Source = 38,
    /// Patch slot 3 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch3Dest = 39,
    /// Patch slot 3 amount, −1..=1.
    Patch3Amount = 40,
    /// Patch slot 4 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch4Source = 41,
    /// Patch slot 4 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch4Dest = 42,
    /// Patch slot 4 amount, −1..=1.
    Patch4Amount = 43,
    /// Patch slot 5 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch5Source = 44,
    /// Patch slot 5 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch5Dest = 45,
    /// Patch slot 5 amount, −1..=1.
    Patch5Amount = 46,
    /// Patch slot 6 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch6Source = 47,
    /// Patch slot 6 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch6Dest = 48,
    /// Patch slot 6 amount, −1..=1.
    Patch6Amount = 49,
    /// Patch slot 7 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch7Source = 50,
    /// Patch slot 7 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch7Dest = 51,
    /// Patch slot 7 amount, −1..=1.
    Patch7Amount = 52,
    /// Patch slot 8 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch8Source = 53,
    /// Patch slot 8 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch8Dest = 54,
    /// Patch slot 8 amount, −1..=1.
    Patch8Amount = 55,
    /// Normalled ADSR → cutoff, −1..=1 (±4 octaves).
    EnvCutoff = 56,
    /// Normalled key → cutoff, 0..=1 (1 follows the key exactly).
    KeyTrack = 57,
    /// Normalled LFO → VCO pitch at full mod wheel, 0..=1 (±2 semitones).
    Vibrato = 58,
    /// The mod wheel, 0..=1, until MIDI input sends CC 1 (#10).
    ModWheel = 59,
    /// Mixer fader of a synth, 0..=1.
    Level = 60,
    /// Mixer pan of a synth, −1 (left)..=1 (right), equal power.
    Pan = 61,
    /// Post-fader send 1 (processor P1), 0..=1.
    Send1 = 62,
    /// Post-fader send 2 (processor P2), 0..=1.
    Send2 = 63,
    /// Silences the synth when ≥ 0.5.
    Mute = 64,
    /// When any synth is soloed (≥ 0.5), only soloed synths sound.
    Solo = 65,
    /// Insert slot 1's effect id (`InsertType`: off, overdrive, distortion, fuzz, EQ, compressor), 0..=5.
    I1Type = 66,
    /// Insert slot 1's knob A, 0..=1; what it does depends on the type (see `fx::insert`).
    I1A = 67,
    /// Insert slot 1's knob B, 0..=1; what it does depends on the type (see `fx::insert`).
    I1B = 68,
    /// Insert slot 1's knob C, 0..=1; what it does depends on the type (see `fx::insert`).
    I1C = 69,
    /// Insert slot 1's knob D, 0..=1; what it does depends on the type (see `fx::insert`).
    I1D = 70,
    /// Insert slot 1's knob E, 0..=1; what it does depends on the type (see `fx::insert`).
    I1E = 71,
    /// Insert slot 2's effect id (`InsertType`: off, overdrive, distortion, fuzz, EQ, compressor), 0..=5.
    I2Type = 72,
    /// Insert slot 2's knob A, 0..=1; what it does depends on the type (see `fx::insert`).
    I2A = 73,
    /// Insert slot 2's knob B, 0..=1; what it does depends on the type (see `fx::insert`).
    I2B = 74,
    /// Insert slot 2's knob C, 0..=1; what it does depends on the type (see `fx::insert`).
    I2C = 75,
    /// Insert slot 2's knob D, 0..=1; what it does depends on the type (see `fx::insert`).
    I2D = 76,
    /// Insert slot 2's knob E, 0..=1; what it does depends on the type (see `fx::insert`).
    I2E = 77,
    /// Insert slot 3's effect id (`InsertType`: off, overdrive, distortion, fuzz, EQ, compressor), 0..=5.
    I3Type = 78,
    /// Insert slot 3's knob A, 0..=1; what it does depends on the type (see `fx::insert`).
    I3A = 79,
    /// Insert slot 3's knob B, 0..=1; what it does depends on the type (see `fx::insert`).
    I3B = 80,
    /// Insert slot 3's knob C, 0..=1; what it does depends on the type (see `fx::insert`).
    I3C = 81,
    /// Insert slot 3's knob D, 0..=1; what it does depends on the type (see `fx::insert`).
    I3D = 82,
    /// Insert slot 3's knob E, 0..=1; what it does depends on the type (see `fx::insert`).
    I3E = 83,
    /// Processor P1's effect id (`ProcType`: off, echo, reverb), 0..=2.
    P1Type = 84,
    /// Processor P1's return level into the master, 0..=1; 0 is silent.
    P1Return = 85,
    /// Processor P1's knob A, 0..=1; what it does depends on the type (see `fx::processor`).
    P1A = 86,
    /// Processor P1's knob B, 0..=1; what it does depends on the type (see `fx::processor`).
    P1B = 87,
    /// Processor P1's knob C, 0..=1; what it does depends on the type (see `fx::processor`).
    P1C = 88,
    /// Processor P1's knob D, 0..=1; what it does depends on the type (see `fx::processor`).
    P1D = 89,
    /// Processor P1's knob E, 0..=1; what it does depends on the type (see `fx::processor`).
    P1E = 90,
    /// Processor P2's effect id (`ProcType`: off, echo, reverb), 0..=2.
    P2Type = 91,
    /// Processor P2's return level into the master, 0..=1; 0 is silent.
    P2Return = 92,
    /// Processor P2's knob A, 0..=1; what it does depends on the type (see `fx::processor`).
    P2A = 93,
    /// Processor P2's knob B, 0..=1; what it does depends on the type (see `fx::processor`).
    P2B = 94,
    /// Processor P2's knob C, 0..=1; what it does depends on the type (see `fx::processor`).
    P2C = 95,
    /// Processor P2's knob D, 0..=1; what it does depends on the type (see `fx::processor`).
    P2D = 96,
    /// Processor P2's knob E, 0..=1; what it does depends on the type (see `fx::processor`).
    P2E = 97,
    /// Processor P3's effect id (`ProcType`: off, echo, reverb), 0..=2.
    P3Type = 98,
    /// Processor P3's return level into the master, 0..=1; 0 is silent.
    P3Return = 99,
    /// Processor P3's knob A, 0..=1; what it does depends on the type (see `fx::processor`).
    P3A = 100,
    /// Processor P3's knob B, 0..=1; what it does depends on the type (see `fx::processor`).
    P3B = 101,
    /// Processor P3's knob C, 0..=1; what it does depends on the type (see `fx::processor`).
    P3C = 102,
    /// Processor P3's knob D, 0..=1; what it does depends on the type (see `fx::processor`).
    P3D = 103,
    /// Processor P3's knob E, 0..=1; what it does depends on the type (see `fx::processor`).
    P3E = 104,
    /// Processor P4's effect id (`ProcType`: off, echo, reverb), 0..=2.
    P4Type = 105,
    /// Processor P4's return level into the master, 0..=1; 0 is silent.
    P4Return = 106,
    /// Processor P4's knob A, 0..=1; what it does depends on the type (see `fx::processor`).
    P4A = 107,
    /// Processor P4's knob B, 0..=1; what it does depends on the type (see `fx::processor`).
    P4B = 108,
    /// Processor P4's knob C, 0..=1; what it does depends on the type (see `fx::processor`).
    P4C = 109,
    /// Processor P4's knob D, 0..=1; what it does depends on the type (see `fx::processor`).
    P4D = 110,
    /// Processor P4's knob E, 0..=1; what it does depends on the type (see `fx::processor`).
    P4E = 111,
    /// The synth's model id (`Model`), 0..=6; see spec 005.
    Model = 112,
    /// Filter ADSR attack, decay and release in seconds, 0.001..=10.
    FenvAttack = 113,
    FenvDecay = 114,
    /// Filter ADSR sustain level, 0..=1.
    FenvSustain = 115,
    FenvRelease = 116,
    /// High-pass cutoff in Hz, 20..=20000; 20 is out of the way.
    HpCutoff = 117,
    /// High-pass resonance, 0..=1 (the 12 dB high-pass of the MS-20 and CS-15).
    HpResonance = 118,
    /// Normalled envelope → high-pass cutoff, −1..=1 (±4 octaves).
    EnvHpCutoff = 119,
    /// Ring modulator (VCO 1 × VCO 2) level into the mixer, 0..=1.
    RingLevel = 120,
    /// Sub-oscillator level into the mixer, 0..=1.
    SubLevel = 121,
    /// Sub-oscillator octaves below VCO 1: 0 is one, 1 is two.
    SubOctave = 122,
    /// VCO 3 follows the key when ≥ 0.5; off holds its pitch.
    Vco3KeyFollow = 123,
    /// VCO 3 sounds five octaves lower, in the low-frequency range, when ≥ 0.5.
    Vco3Low = 124,
    /// Normalled LFO → cutoff, 0..=1 (±24 semitones at full LFO).
    LfoCutoff = 125,
    /// Normalled LFO → pulse width, 0..=1 (±0.45 at full LFO).
    LfoPw = 126,
    /// Poly-mod: filter envelope → VCO 2 pitch, −1..=1 (±24 semitones).
    EnvFreq2 = 127,
    /// Poly-mod: VCO 1 → VCO 2 pitch, −1..=1 (±24 semitones).
    OscFreq2 = 128,
    /// Poly-mod: filter envelope → pulse width, −1..=1 (±0.45).
    EnvPw = 129,
    /// Poly-mod: VCO 1 → pulse width, −1..=1 (±0.45).
    OscPw = 130,
    /// Poly-mod: VCO 1 → cutoff, −1..=1 (±48 semitones).
    OscCutoff = 131,
    /// Post-fader send 3 (processor P3), 0..=1.
    Send3 = 132,
    /// Post-fader send 4 (processor P4), 0..=1.
    Send4 = 133,
    /// Master compressor threshold in dB, −60..=0.
    CompThreshold = 134,
    /// Master compressor ratio, 1..=20; 1 is off.
    CompRatio = 135,
    /// Master compressor attack in ms, 0.1..=100.
    CompAttack = 136,
    /// Master compressor release in ms, 10..=1000.
    CompRelease = 137,
    /// Master compressor make-up gain in dB, 0..=24.
    CompMakeup = 138,
    /// Master EQ: Low shelf corner in Hz, 20..=500.
    EqLowFreq = 139,
    /// Master EQ: Low shelf gain in dB, −15..=15.
    EqLowGain = 140,
    /// Master EQ: Mid band 1 centre in Hz, 100..=8000.
    EqMid1Freq = 141,
    /// Master EQ: Mid band 1 gain in dB, −15..=15.
    EqMid1Gain = 142,
    /// Master EQ: Mid band 1 Q, 0.3..=8.
    EqMid1Q = 143,
    /// Master EQ: Mid band 2 centre in Hz, 500..=12000.
    EqMid2Freq = 144,
    /// Master EQ: Mid band 2 gain in dB, −15..=15.
    EqMid2Gain = 145,
    /// Master EQ: Mid band 2 Q, 0.3..=8.
    EqMid2Q = 146,
    /// Master EQ: High shelf corner in Hz, 2000..=18000.
    EqHighFreq = 147,
    /// Master EQ: High shelf gain in dB, −15..=15.
    EqHighGain = 148,
    /// Where a strip or group goes after its fader: 0 is the master, 1–8 a group.
    /// A group may only go to a higher-numbered group; other routes are ignored.
    Out = 149,
    /// Voices the synth plays at once, 1..=16: 1 is monophonic (one voice per
    /// owner, with note priority and glide), more press each note on a voice.
    Polyphony = 150,
    /// How a poly synth assigns notes: 0 poly (a voice per note), 1 unison (every
    /// voice plays the last note).
    Assign = 151,
    /// Unison detune spread, 0..=1: 0 to ±50 cents across the voices.
    UnisonDetune = 152,
    /// Analog variance, 0..=1: each voice's own detune, drift and cutoff offset.
    Analog = 153,
    /// The synth's stereo chorus: 0 off, 1 I, 2 II, 3 I+II (spec 006 Req 7).
    ChorusMode = 154,
    /// Cross-modulation of VCO 1's pitch by VCO 2, 0..=1 (up to ±24 semitones).
    XMod = 155,
    /// Low-pass slope on the models with the switch: 0 is 12 dB, 1 is 24 dB per octave.
    Slope = 156,
    /// The second LFO's rate in Hz, 0.01..=50 (the Matrix-12).
    Lfo2Rate = 157,
    /// The second LFO's waveform id (`Waveform`), 0..=3.
    Lfo2Wave = 158,
    /// The ramp's time to full in seconds, 0.01..=30; it restarts with each note.
    RampTime = 159,
    /// Patch slot 9 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch9Source = 160,
    /// Patch slot 9 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch9Dest = 161,
    /// Patch slot 9 amount, −1..=1.
    Patch9Amount = 162,
    /// Patch slot 10 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch10Source = 163,
    /// Patch slot 10 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch10Dest = 164,
    /// Patch slot 10 amount, −1..=1.
    Patch10Amount = 165,
    /// Patch slot 11 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch11Source = 166,
    /// Patch slot 11 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch11Dest = 167,
    /// Patch slot 11 amount, −1..=1.
    Patch11Amount = 168,
    /// Patch slot 12 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch12Source = 169,
    /// Patch slot 12 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch12Dest = 170,
    /// Patch slot 12 amount, −1..=1.
    Patch12Amount = 171,
    /// Patch slot 13 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch13Source = 172,
    /// Patch slot 13 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch13Dest = 173,
    /// Patch slot 13 amount, −1..=1.
    Patch13Amount = 174,
    /// Patch slot 14 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch14Source = 175,
    /// Patch slot 14 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch14Dest = 176,
    /// Patch slot 14 amount, −1..=1.
    Patch14Amount = 177,
    /// Patch slot 15 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch15Source = 178,
    /// Patch slot 15 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch15Dest = 179,
    /// Patch slot 15 amount, −1..=1.
    Patch15Amount = 180,
    /// Patch slot 16 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch16Source = 181,
    /// Patch slot 16 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch16Dest = 182,
    /// Patch slot 16 amount, −1..=1.
    Patch16Amount = 183,
    /// Patch slot 17 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch17Source = 184,
    /// Patch slot 17 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch17Dest = 185,
    /// Patch slot 17 amount, −1..=1.
    Patch17Amount = 186,
    /// Patch slot 18 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch18Source = 187,
    /// Patch slot 18 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch18Dest = 188,
    /// Patch slot 18 amount, −1..=1.
    Patch18Amount = 189,
    /// Patch slot 19 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch19Source = 190,
    /// Patch slot 19 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch19Dest = 191,
    /// Patch slot 19 amount, −1..=1.
    Patch19Amount = 192,
    /// Patch slot 20 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch20Source = 193,
    /// Patch slot 20 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch20Dest = 194,
    /// Patch slot 20 amount, −1..=1.
    Patch20Amount = 195,
    /// Wavetable of the first table oscillator, 0..=7 (the PPG Wave).
    Wt1Table = 196,
    /// Wave position of the first oscillator in its table, 0..=1.
    Wt1Pos = 197,
    /// Wavetable of the second oscillator, 0..=7.
    Wt2Table = 198,
    /// Wave position of the second oscillator, 0..=1.
    Wt2Pos = 199,
    /// Stepped wave positions when ≥ 0.5, as the PPG's, instead of a crossfade.
    WtSteps = 200,
    /// Filter envelope → wave position of both oscillators, −1..=1.
    EnvWt = 201,
    /// LFO → wave position of both oscillators, 0..=1.
    LfoWt = 202,
}

/// Where the global parameters start: P1 an echo and P2 a reverb, silent until
/// a return goes up; P3 and P4 are off.
pub const GLOBAL_DEFAULTS: [(Param, f32); 44] = [
    (Param::EqLowFreq, 100.0),
    (Param::EqLowGain, 0.0),
    (Param::EqMid1Freq, 500.0),
    (Param::EqMid1Gain, 0.0),
    (Param::EqMid1Q, 1.0),
    (Param::EqMid2Freq, 3000.0),
    (Param::EqMid2Gain, 0.0),
    (Param::EqMid2Q, 1.0),
    (Param::EqHighFreq, 8000.0),
    (Param::EqHighGain, 0.0),
    (Param::MasterGain, 0.5),
    (Param::CompThreshold, -12.0),
    (Param::CompRatio, 1.0),
    (Param::CompAttack, 10.0),
    (Param::CompRelease, 120.0),
    (Param::CompMakeup, 0.0),
    (Param::P1Type, 1.0),
    (Param::P1Return, 0.0),
    (Param::P1A, 0.75),
    (Param::P1B, 0.4),
    (Param::P1C, 0.7),
    (Param::P1D, 0.0),
    (Param::P1E, 0.0),
    (Param::P2Type, 2.0),
    (Param::P2Return, 0.0),
    (Param::P2A, 0.65),
    (Param::P2B, 0.3),
    (Param::P2C, 0.1),
    (Param::P2D, 0.0),
    (Param::P2E, 0.0),
    (Param::P3Type, 0.0),
    (Param::P3Return, 0.0),
    (Param::P3A, 0.0),
    (Param::P3B, 0.0),
    (Param::P3C, 0.0),
    (Param::P3D, 0.0),
    (Param::P3E, 0.0),
    (Param::P4Type, 0.0),
    (Param::P4Return, 0.0),
    (Param::P4A, 0.0),
    (Param::P4B, 0.0),
    (Param::P4C, 0.0),
    (Param::P4D, 0.0),
    (Param::P4E, 0.0),
];

/// The first processor parameter id, and how many each slot has: type,
/// return and five knobs.
const PROC_BASE: usize = Param::P1Type as usize;
const PROC_FIELDS: usize = 7;

/// The first insert parameter id, and how many each slot has: type and five
/// knobs. A strip has three slots.
const INSERT_BASE: usize = Param::I1Type as usize;
const INSERT_FIELDS: usize = 6;
pub const INSERT_SLOTS: usize = 3;

/// What an insert parameter sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertField {
    Type,
    /// Knob 0..=4 (A..E).
    Knob(usize),
}

/// What a processor parameter sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcField {
    Type,
    Return,
    /// Knob 0..=4 (A..E).
    Knob(usize),
}

impl Param {
    /// Every parameter with the name the TypeScript mirror uses.
    pub const ALL: [(Param, &'static str); 203] = [
        (Param::MasterGain, "MasterGain"),
        (Param::Vco1Wave, "Vco1Wave"),
        (Param::Vco1Coarse, "Vco1Coarse"),
        (Param::Vco1Fine, "Vco1Fine"),
        (Param::Vco1Level, "Vco1Level"),
        (Param::Vco2Wave, "Vco2Wave"),
        (Param::Vco2Coarse, "Vco2Coarse"),
        (Param::Vco2Fine, "Vco2Fine"),
        (Param::Vco2Level, "Vco2Level"),
        (Param::Vco3Wave, "Vco3Wave"),
        (Param::Vco3Coarse, "Vco3Coarse"),
        (Param::Vco3Fine, "Vco3Fine"),
        (Param::Vco3Level, "Vco3Level"),
        (Param::PulseWidth, "PulseWidth"),
        (Param::Vco2Sync, "Vco2Sync"),
        (Param::Vco3Sync, "Vco3Sync"),
        (Param::NoiseLevel, "NoiseLevel"),
        (Param::NoiseColour, "NoiseColour"),
        (Param::Cutoff, "Cutoff"),
        (Param::Resonance, "Resonance"),
        (Param::Drive, "Drive"),
        (Param::AdsrAttack, "AdsrAttack"),
        (Param::AdsrDecay, "AdsrDecay"),
        (Param::AdsrSustain, "AdsrSustain"),
        (Param::AdsrRelease, "AdsrRelease"),
        (Param::ArAttack, "ArAttack"),
        (Param::ArRelease, "ArRelease"),
        (Param::LfoRate, "LfoRate"),
        (Param::LfoWave, "LfoWave"),
        (Param::Priority, "Priority"),
        (Param::Legato, "Legato"),
        (Param::Glide, "Glide"),
        (Param::Patch1Source, "Patch1Source"),
        (Param::Patch1Dest, "Patch1Dest"),
        (Param::Patch1Amount, "Patch1Amount"),
        (Param::Patch2Source, "Patch2Source"),
        (Param::Patch2Dest, "Patch2Dest"),
        (Param::Patch2Amount, "Patch2Amount"),
        (Param::Patch3Source, "Patch3Source"),
        (Param::Patch3Dest, "Patch3Dest"),
        (Param::Patch3Amount, "Patch3Amount"),
        (Param::Patch4Source, "Patch4Source"),
        (Param::Patch4Dest, "Patch4Dest"),
        (Param::Patch4Amount, "Patch4Amount"),
        (Param::Patch5Source, "Patch5Source"),
        (Param::Patch5Dest, "Patch5Dest"),
        (Param::Patch5Amount, "Patch5Amount"),
        (Param::Patch6Source, "Patch6Source"),
        (Param::Patch6Dest, "Patch6Dest"),
        (Param::Patch6Amount, "Patch6Amount"),
        (Param::Patch7Source, "Patch7Source"),
        (Param::Patch7Dest, "Patch7Dest"),
        (Param::Patch7Amount, "Patch7Amount"),
        (Param::Patch8Source, "Patch8Source"),
        (Param::Patch8Dest, "Patch8Dest"),
        (Param::Patch8Amount, "Patch8Amount"),
        (Param::EnvCutoff, "EnvCutoff"),
        (Param::KeyTrack, "KeyTrack"),
        (Param::Vibrato, "Vibrato"),
        (Param::ModWheel, "ModWheel"),
        (Param::Level, "Level"),
        (Param::Pan, "Pan"),
        (Param::Send1, "Send1"),
        (Param::Send2, "Send2"),
        (Param::Mute, "Mute"),
        (Param::Solo, "Solo"),
        (Param::I1Type, "I1Type"),
        (Param::I1A, "I1A"),
        (Param::I1B, "I1B"),
        (Param::I1C, "I1C"),
        (Param::I1D, "I1D"),
        (Param::I1E, "I1E"),
        (Param::I2Type, "I2Type"),
        (Param::I2A, "I2A"),
        (Param::I2B, "I2B"),
        (Param::I2C, "I2C"),
        (Param::I2D, "I2D"),
        (Param::I2E, "I2E"),
        (Param::I3Type, "I3Type"),
        (Param::I3A, "I3A"),
        (Param::I3B, "I3B"),
        (Param::I3C, "I3C"),
        (Param::I3D, "I3D"),
        (Param::I3E, "I3E"),
        (Param::P1Type, "P1Type"),
        (Param::P1Return, "P1Return"),
        (Param::P1A, "P1A"),
        (Param::P1B, "P1B"),
        (Param::P1C, "P1C"),
        (Param::P1D, "P1D"),
        (Param::P1E, "P1E"),
        (Param::P2Type, "P2Type"),
        (Param::P2Return, "P2Return"),
        (Param::P2A, "P2A"),
        (Param::P2B, "P2B"),
        (Param::P2C, "P2C"),
        (Param::P2D, "P2D"),
        (Param::P2E, "P2E"),
        (Param::P3Type, "P3Type"),
        (Param::P3Return, "P3Return"),
        (Param::P3A, "P3A"),
        (Param::P3B, "P3B"),
        (Param::P3C, "P3C"),
        (Param::P3D, "P3D"),
        (Param::P3E, "P3E"),
        (Param::P4Type, "P4Type"),
        (Param::P4Return, "P4Return"),
        (Param::P4A, "P4A"),
        (Param::P4B, "P4B"),
        (Param::P4C, "P4C"),
        (Param::P4D, "P4D"),
        (Param::P4E, "P4E"),
        (Param::Model, "Model"),
        (Param::FenvAttack, "FenvAttack"),
        (Param::FenvDecay, "FenvDecay"),
        (Param::FenvSustain, "FenvSustain"),
        (Param::FenvRelease, "FenvRelease"),
        (Param::HpCutoff, "HpCutoff"),
        (Param::HpResonance, "HpResonance"),
        (Param::EnvHpCutoff, "EnvHpCutoff"),
        (Param::RingLevel, "RingLevel"),
        (Param::SubLevel, "SubLevel"),
        (Param::SubOctave, "SubOctave"),
        (Param::Vco3KeyFollow, "Vco3KeyFollow"),
        (Param::Vco3Low, "Vco3Low"),
        (Param::LfoCutoff, "LfoCutoff"),
        (Param::LfoPw, "LfoPw"),
        (Param::EnvFreq2, "EnvFreq2"),
        (Param::OscFreq2, "OscFreq2"),
        (Param::EnvPw, "EnvPw"),
        (Param::OscPw, "OscPw"),
        (Param::OscCutoff, "OscCutoff"),
        (Param::Send3, "Send3"),
        (Param::Send4, "Send4"),
        (Param::CompThreshold, "CompThreshold"),
        (Param::CompRatio, "CompRatio"),
        (Param::CompAttack, "CompAttack"),
        (Param::CompRelease, "CompRelease"),
        (Param::CompMakeup, "CompMakeup"),
        (Param::EqLowFreq, "EqLowFreq"),
        (Param::EqLowGain, "EqLowGain"),
        (Param::EqMid1Freq, "EqMid1Freq"),
        (Param::EqMid1Gain, "EqMid1Gain"),
        (Param::EqMid1Q, "EqMid1Q"),
        (Param::EqMid2Freq, "EqMid2Freq"),
        (Param::EqMid2Gain, "EqMid2Gain"),
        (Param::EqMid2Q, "EqMid2Q"),
        (Param::EqHighFreq, "EqHighFreq"),
        (Param::EqHighGain, "EqHighGain"),
        (Param::Out, "Out"),
        (Param::Polyphony, "Polyphony"),
        (Param::Assign, "Assign"),
        (Param::UnisonDetune, "UnisonDetune"),
        (Param::Analog, "Analog"),
        (Param::ChorusMode, "ChorusMode"),
        (Param::XMod, "XMod"),
        (Param::Slope, "Slope"),
        (Param::Lfo2Rate, "Lfo2Rate"),
        (Param::Lfo2Wave, "Lfo2Wave"),
        (Param::RampTime, "RampTime"),
        (Param::Patch9Source, "Patch9Source"),
        (Param::Patch9Dest, "Patch9Dest"),
        (Param::Patch9Amount, "Patch9Amount"),
        (Param::Patch10Source, "Patch10Source"),
        (Param::Patch10Dest, "Patch10Dest"),
        (Param::Patch10Amount, "Patch10Amount"),
        (Param::Patch11Source, "Patch11Source"),
        (Param::Patch11Dest, "Patch11Dest"),
        (Param::Patch11Amount, "Patch11Amount"),
        (Param::Patch12Source, "Patch12Source"),
        (Param::Patch12Dest, "Patch12Dest"),
        (Param::Patch12Amount, "Patch12Amount"),
        (Param::Patch13Source, "Patch13Source"),
        (Param::Patch13Dest, "Patch13Dest"),
        (Param::Patch13Amount, "Patch13Amount"),
        (Param::Patch14Source, "Patch14Source"),
        (Param::Patch14Dest, "Patch14Dest"),
        (Param::Patch14Amount, "Patch14Amount"),
        (Param::Patch15Source, "Patch15Source"),
        (Param::Patch15Dest, "Patch15Dest"),
        (Param::Patch15Amount, "Patch15Amount"),
        (Param::Patch16Source, "Patch16Source"),
        (Param::Patch16Dest, "Patch16Dest"),
        (Param::Patch16Amount, "Patch16Amount"),
        (Param::Patch17Source, "Patch17Source"),
        (Param::Patch17Dest, "Patch17Dest"),
        (Param::Patch17Amount, "Patch17Amount"),
        (Param::Patch18Source, "Patch18Source"),
        (Param::Patch18Dest, "Patch18Dest"),
        (Param::Patch18Amount, "Patch18Amount"),
        (Param::Patch19Source, "Patch19Source"),
        (Param::Patch19Dest, "Patch19Dest"),
        (Param::Patch19Amount, "Patch19Amount"),
        (Param::Patch20Source, "Patch20Source"),
        (Param::Patch20Dest, "Patch20Dest"),
        (Param::Patch20Amount, "Patch20Amount"),
        (Param::Wt1Table, "Wt1Table"),
        (Param::Wt1Pos, "Wt1Pos"),
        (Param::Wt2Table, "Wt2Table"),
        (Param::Wt2Pos, "Wt2Pos"),
        (Param::WtSteps, "WtSteps"),
        (Param::EnvWt, "EnvWt"),
        (Param::LfoWt, "LfoWt"),
    ];

    /// A mixer strip's parameters: the fader, pan, sends, mute and solo. The
    /// mixer owns them; the synth never sees them.
    pub fn is_strip(self) -> bool {
        matches!(
            self,
            Param::Level
                | Param::Pan
                | Param::Send1
                | Param::Send2
                | Param::Send3
                | Param::Send4
                | Param::Mute
                | Param::Solo
                | Param::Out
        ) || self.insert().is_some()
    }

    /// The insert slot (0–2) and field of an I1–I3 parameter.
    pub fn insert(self) -> Option<(usize, InsertField)> {
        let i = (self as usize).checked_sub(INSERT_BASE)?;
        if i >= INSERT_SLOTS * INSERT_FIELDS {
            return None;
        }
        let field = match i % INSERT_FIELDS {
            0 => InsertField::Type,
            k => InsertField::Knob(k - 1),
        };
        Some((i / INSERT_FIELDS, field))
    }

    /// Parameters of the whole engine, not of one synth: the master gain and
    /// the send effects. Whichever synth they are sent to, they are set once.
    pub fn is_global(self) -> bool {
        matches!(
            self,
            Param::MasterGain
                | Param::CompThreshold
                | Param::CompRatio
                | Param::CompAttack
                | Param::CompRelease
                | Param::CompMakeup
                | Param::EqLowFreq
                | Param::EqLowGain
                | Param::EqMid1Freq
                | Param::EqMid1Gain
                | Param::EqMid1Q
                | Param::EqMid2Freq
                | Param::EqMid2Gain
                | Param::EqMid2Q
                | Param::EqHighFreq
                | Param::EqHighGain
        ) || self.processor().is_some()
    }

    /// The processor slot (0–3) and field of a P1–P4 parameter.
    pub fn processor(self) -> Option<(usize, ProcField)> {
        let i = (self as usize).checked_sub(PROC_BASE)?;
        if i >= 4 * PROC_FIELDS {
            return None;
        }
        let field = match i % PROC_FIELDS {
            0 => ProcField::Type,
            1 => ProcField::Return,
            k => ProcField::Knob(k - 2),
        };
        Some((i / PROC_FIELDS, field))
    }

    /// The parameter for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Param> {
        Self::ALL
            .iter()
            .find(|(p, _)| *p as u32 == id)
            .map(|(p, _)| *p)
    }

    /// Clamp a value into this parameter's range.
    pub fn clamp(self, v: f32) -> f32 {
        let (lo, hi) = match self {
            Param::MasterGain => (0.0, 1.0),
            Param::Vco1Wave | Param::Vco2Wave | Param::Vco3Wave => (0.0, 3.0),
            Param::Vco1Coarse | Param::Vco2Coarse | Param::Vco3Coarse => (-24.0, 24.0),
            Param::Vco1Fine | Param::Vco2Fine | Param::Vco3Fine => (-50.0, 50.0),
            Param::Vco1Level | Param::Vco2Level | Param::Vco3Level => (0.0, 1.0),
            Param::PulseWidth => (0.05, 0.95),
            Param::Vco2Sync | Param::Vco3Sync => (0.0, 1.0),
            Param::RingLevel | Param::SubLevel | Param::SubOctave => (0.0, 1.0),
            Param::Vco3KeyFollow | Param::Vco3Low => (0.0, 1.0),
            Param::NoiseLevel | Param::NoiseColour => (0.0, 1.0),
            Param::Cutoff | Param::HpCutoff => (20.0, 20_000.0),
            Param::Resonance
            | Param::Drive
            | Param::AdsrSustain
            | Param::FenvSustain
            | Param::HpResonance => (0.0, 1.0),
            Param::AdsrAttack
            | Param::AdsrDecay
            | Param::AdsrRelease
            | Param::ArAttack
            | Param::ArRelease
            | Param::FenvAttack
            | Param::FenvDecay
            | Param::FenvRelease => (0.001, 10.0),
            Param::LfoRate => (0.01, 50.0),
            Param::LfoWave => (0.0, 3.0),
            Param::Priority => (0.0, 2.0),
            Param::Legato => (0.0, 1.0),
            Param::Glide => (0.0, 5.0),
            Param::Patch1Source | Param::Patch1Dest => (0.0, 255.0),
            Param::Patch2Source | Param::Patch2Dest => (0.0, 255.0),
            Param::Patch3Source | Param::Patch3Dest => (0.0, 255.0),
            Param::Patch4Source | Param::Patch4Dest => (0.0, 255.0),
            Param::Patch5Source | Param::Patch5Dest => (0.0, 255.0),
            Param::Patch6Source | Param::Patch6Dest => (0.0, 255.0),
            Param::Patch7Source | Param::Patch7Dest => (0.0, 255.0),
            Param::Patch8Source | Param::Patch8Dest => (0.0, 255.0),
            Param::Patch1Amount
            | Param::Patch2Amount
            | Param::Patch3Amount
            | Param::Patch4Amount
            | Param::Patch5Amount
            | Param::Patch6Amount
            | Param::Patch7Amount
            | Param::Patch8Amount
            | Param::EnvCutoff
            | Param::EnvHpCutoff
            | Param::EnvFreq2
            | Param::OscFreq2
            | Param::EnvPw
            | Param::OscPw
            | Param::OscCutoff => (-1.0, 1.0),
            Param::KeyTrack
            | Param::Vibrato
            | Param::ModWheel
            | Param::LfoCutoff
            | Param::LfoPw => (0.0, 1.0),
            Param::Model => (0.0, (crate::mono::model::Model::ALL.len() - 1) as f32),
            Param::Level | Param::Send1 | Param::Send2 | Param::Send3 | Param::Send4 => (0.0, 1.0),
            Param::Pan => (-1.0, 1.0),
            Param::Mute | Param::Solo => (0.0, 1.0),
            Param::Out => (0.0, 8.0),
            Param::Polyphony => (1.0, 16.0),
            Param::Assign => (0.0, 1.0),
            Param::ChorusMode => (0.0, 3.0),
            Param::XMod | Param::Slope => (0.0, 1.0),
            Param::Wt1Table | Param::Wt2Table => (0.0, 7.0),
            Param::Wt1Pos | Param::Wt2Pos | Param::LfoWt => (0.0, 1.0),
            Param::WtSteps => (0.0, 1.0),
            Param::EnvWt => (-1.0, 1.0),
            Param::Lfo2Rate => (0.01, 50.0),
            Param::Lfo2Wave => (0.0, 3.0),
            Param::RampTime => (0.01, 30.0),
            Param::Patch9Source
            | Param::Patch9Dest
            | Param::Patch10Source
            | Param::Patch10Dest
            | Param::Patch11Source
            | Param::Patch11Dest
            | Param::Patch12Source
            | Param::Patch12Dest
            | Param::Patch13Source
            | Param::Patch13Dest
            | Param::Patch14Source
            | Param::Patch14Dest
            | Param::Patch15Source
            | Param::Patch15Dest
            | Param::Patch16Source
            | Param::Patch16Dest
            | Param::Patch17Source
            | Param::Patch17Dest
            | Param::Patch18Source
            | Param::Patch18Dest
            | Param::Patch19Source
            | Param::Patch19Dest
            | Param::Patch20Source
            | Param::Patch20Dest => (0.0, 255.0),
            Param::Patch9Amount
            | Param::Patch10Amount
            | Param::Patch11Amount
            | Param::Patch12Amount
            | Param::Patch13Amount
            | Param::Patch14Amount
            | Param::Patch15Amount
            | Param::Patch16Amount
            | Param::Patch17Amount
            | Param::Patch18Amount
            | Param::Patch19Amount
            | Param::Patch20Amount => (-1.0, 1.0),
            Param::UnisonDetune | Param::Analog => (0.0, 1.0),
            Param::I1Type | Param::I2Type | Param::I3Type => (0.0, 5.0),
            Param::I1A => (0.0, 1.0),
            Param::I1B => (0.0, 1.0),
            Param::I1C => (0.0, 1.0),
            Param::I1D => (0.0, 1.0),
            Param::I1E => (0.0, 1.0),
            Param::I2A => (0.0, 1.0),
            Param::I2B => (0.0, 1.0),
            Param::I2C => (0.0, 1.0),
            Param::I2D => (0.0, 1.0),
            Param::I2E => (0.0, 1.0),
            Param::I3A => (0.0, 1.0),
            Param::I3B => (0.0, 1.0),
            Param::I3C => (0.0, 1.0),
            Param::I3D => (0.0, 1.0),
            Param::I3E => (0.0, 1.0),
            Param::CompThreshold => (-60.0, 0.0),
            Param::CompRatio => (1.0, 20.0),
            Param::CompAttack => (0.1, 100.0),
            Param::CompRelease => (10.0, 1_000.0),
            Param::CompMakeup => (0.0, 24.0),
            Param::EqLowFreq => (20.0, 500.0),
            Param::EqLowGain => (-15.0, 15.0),
            Param::EqMid1Freq => (100.0, 8000.0),
            Param::EqMid1Gain => (-15.0, 15.0),
            Param::EqMid1Q => (0.3, 8.0),
            Param::EqMid2Freq => (500.0, 12000.0),
            Param::EqMid2Gain => (-15.0, 15.0),
            Param::EqMid2Q => (0.3, 8.0),
            Param::EqHighFreq => (2000.0, 18000.0),
            Param::EqHighGain => (-15.0, 15.0),
            Param::P1Type | Param::P2Type | Param::P3Type | Param::P4Type => (0.0, 2.0),
            Param::P1Return | Param::P2Return | Param::P3Return | Param::P4Return => (0.0, 1.0),
            Param::P1A => (0.0, 1.0),
            Param::P1B => (0.0, 1.0),
            Param::P1C => (0.0, 1.0),
            Param::P1D => (0.0, 1.0),
            Param::P1E => (0.0, 1.0),
            Param::P2A => (0.0, 1.0),
            Param::P2B => (0.0, 1.0),
            Param::P2C => (0.0, 1.0),
            Param::P2D => (0.0, 1.0),
            Param::P2E => (0.0, 1.0),
            Param::P3A => (0.0, 1.0),
            Param::P3B => (0.0, 1.0),
            Param::P3C => (0.0, 1.0),
            Param::P3D => (0.0, 1.0),
            Param::P3E => (0.0, 1.0),
            Param::P4A => (0.0, 1.0),
            Param::P4B => (0.0, 1.0),
            Param::P4C => (0.0, 1.0),
            Param::P4D => (0.0, 1.0),
            Param::P4E => (0.0, 1.0),
        };
        if v.is_nan() { lo } else { v.clamp(lo, hi) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for (i, (p, _)) in Param::ALL.iter().enumerate() {
            assert_eq!(Param::from_id(*p as u32), Some(*p));
            // Ids run from 0 without gaps: `param_count` and the engine's
            // value table rely on it.
            assert_eq!(*p as usize, i);
        }
        assert_eq!(Param::from_id(999), None);
    }

    #[test]
    fn clamp_rejects_nan_and_out_of_range() {
        assert_eq!(Param::MasterGain.clamp(f32::NAN), 0.0);
        assert_eq!(Param::MasterGain.clamp(7.0), 1.0);
        assert_eq!(Param::Cutoff.clamp(0.0), 20.0);
    }

    /// The `Name: id` entries of `export const {name} = { ... }`.
    fn ts_block(ts: &str, name: &str) -> Vec<(String, u32)> {
        let open = format!("export const {name} = {{");
        let body = ts
            .split(&open)
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .unwrap_or_else(|| panic!("params.ts has no `{open}`"));
        let mut entries: Vec<(String, u32)> = body
            .lines()
            .filter_map(|line| {
                let (k, v) = line.trim().trim_end_matches(',').split_once(": ")?;
                Some((k.to_string(), v.parse().ok()?))
            })
            .collect();
        entries.sort();
        entries
    }

    /// ADR-0004: every id list in Rust and its block in
    /// `web/src/audio/params.ts` hold exactly the same names and ids.
    #[test]
    fn typescript_mirror_matches() {
        use crate::fx::insert::InsertType;
        use crate::fx::processor::ProcType;
        use crate::mono::model::Model;
        use crate::mono::noise::NoiseColour;
        use crate::mono::osc::Waveform;
        use crate::mono::patch::{ModDest, ModSource};
        use crate::mono::preset::Preset;
        use crate::mono::voice::NotePriority;
        fn rust<T: Copy>(all: &[(T, &str)], id: impl Fn(T) -> u32) -> Vec<(String, u32)> {
            let mut v: Vec<_> = all.iter().map(|(x, n)| (n.to_string(), id(*x))).collect();
            v.sort();
            v
        }
        let ts = include_str!("../../../web/src/audio/params.ts");
        // `GlobalParam`: what `is_global` marks, which a setup stores once.
        let global: Vec<(Param, &str)> = Param::ALL
            .iter()
            .copied()
            .filter(|(p, _)| p.is_global())
            .collect();
        // `StripParam`: what `is_strip` marks, which belongs to a mixer strip.
        let strip: Vec<(Param, &str)> = Param::ALL
            .iter()
            .copied()
            .filter(|(p, _)| p.is_strip())
            .collect();
        let lists = [
            ("Param", rust(&Param::ALL, |p| p as u32)),
            ("GlobalParam", rust(&global, |p| p as u32)),
            ("StripParam", rust(&strip, |p| p as u32)),
            ("Waveform", rust(&Waveform::ALL, |w| w as u32)),
            ("NoiseColour", rust(&NoiseColour::ALL, |c| c as u32)),
            ("Model", rust(&Model::ALL, |m| m as u32)),
            ("InsertType", rust(&InsertType::ALL, |t| t as u32)),
            ("ProcType", rust(&ProcType::ALL, |t| t as u32)),
            ("Preset", rust(&Preset::ALL, |p| p as u32)),
            ("NotePriority", rust(&NotePriority::ALL, |p| p as u32)),
            ("ModSource", rust(&ModSource::ALL, |s| s as u32)),
            ("ModDest", rust(&ModDest::ALL, |d| d as u32)),
        ];
        for (name, want) in lists {
            assert_eq!(
                ts_block(ts, name),
                want,
                "params.ts `{name}` differs from Rust"
            );
        }
    }
}
