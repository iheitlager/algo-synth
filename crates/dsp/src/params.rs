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
    /// Insert slot 1's effect id (`InsertType`: off, overdrive, distortion, fuzz, EQ, compressor, vocoder, bitcrush), 0..=7.
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
    /// Insert slot 2's effect id (`InsertType`: off, overdrive, distortion, fuzz, EQ, compressor, vocoder, bitcrush), 0..=7.
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
    /// Insert slot 3's effect id (`InsertType`: off, overdrive, distortion, fuzz, EQ, compressor, vocoder, bitcrush), 0..=7.
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
    /// Processor P1's effect id (`ProcType`: off, echo, reverb, chorus, flanger), 0..=4.
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
    /// Processor P2's effect id (`ProcType`: off, echo, reverb, chorus, flanger), 0..=4.
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
    /// Processor P3's effect id (`ProcType`: off, echo, reverb, chorus, flanger), 0..=4.
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
    /// Processor P4's effect id (`ProcType`: off, echo, reverb, chorus, flanger), 0..=4.
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
    /// Where a strip or group goes after its fader: 0 is the master, 1–8 a group,
    /// 9 nowhere (it still feeds its sends and any vocoder keyed to it).
    /// A group may only go to a higher-numbered group; other routes are ignored.
    Out = 149,
    /// Processor P2 takes its input from P1's output as well as its sends when ≥ 0.5.
    P2In = 150,
    /// Processor P3 takes its input from P2's output as well as its sends when ≥ 0.5.
    P3In = 151,
    /// Processor P4 takes its input from P3's output as well as its sends when ≥ 0.5.
    P4In = 152,
    /// Voices the synth plays at once, 1..=16: 1 is monophonic (one voice per
    /// owner, with note priority and glide), more press each note on a voice.
    Polyphony = 153,
    /// How a poly synth assigns notes: 0 poly (a voice per note), 1 unison (every
    /// voice plays the last note).
    Assign = 154,
    /// Unison detune spread, 0..=1: 0 to ±50 cents across the voices.
    UnisonDetune = 155,
    /// Analog variance, 0..=1: each voice's own detune, drift and cutoff offset.
    Analog = 156,
    /// The synth's stereo chorus: 0 off, 1 I, 2 II, 3 I+II (spec 006 Req 7).
    ChorusMode = 157,
    /// Cross-modulation of VCO 1's pitch by VCO 2, 0..=1 (up to ±24 semitones).
    XMod = 158,
    /// Low-pass slope on the models with the switch: 0 is 12 dB, 1 is 24 dB per octave.
    Slope = 159,
    /// The second LFO's rate in Hz, 0.01..=50 (the Matrix-12).
    Lfo2Rate = 160,
    /// The second LFO's waveform id (`Waveform`), 0..=3.
    Lfo2Wave = 161,
    /// The ramp's time to full in seconds, 0.01..=30; it restarts with each note.
    RampTime = 162,
    /// Patch slot 9 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch9Source = 163,
    /// Patch slot 9 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch9Dest = 164,
    /// Patch slot 9 amount, −1..=1.
    Patch9Amount = 165,
    /// Patch slot 10 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch10Source = 166,
    /// Patch slot 10 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch10Dest = 167,
    /// Patch slot 10 amount, −1..=1.
    Patch10Amount = 168,
    /// Patch slot 11 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch11Source = 169,
    /// Patch slot 11 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch11Dest = 170,
    /// Patch slot 11 amount, −1..=1.
    Patch11Amount = 171,
    /// Patch slot 12 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch12Source = 172,
    /// Patch slot 12 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch12Dest = 173,
    /// Patch slot 12 amount, −1..=1.
    Patch12Amount = 174,
    /// Patch slot 13 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch13Source = 175,
    /// Patch slot 13 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch13Dest = 176,
    /// Patch slot 13 amount, −1..=1.
    Patch13Amount = 177,
    /// Patch slot 14 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch14Source = 178,
    /// Patch slot 14 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch14Dest = 179,
    /// Patch slot 14 amount, −1..=1.
    Patch14Amount = 180,
    /// Patch slot 15 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch15Source = 181,
    /// Patch slot 15 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch15Dest = 182,
    /// Patch slot 15 amount, −1..=1.
    Patch15Amount = 183,
    /// Patch slot 16 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch16Source = 184,
    /// Patch slot 16 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch16Dest = 185,
    /// Patch slot 16 amount, −1..=1.
    Patch16Amount = 186,
    /// Patch slot 17 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch17Source = 187,
    /// Patch slot 17 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch17Dest = 188,
    /// Patch slot 17 amount, −1..=1.
    Patch17Amount = 189,
    /// Patch slot 18 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch18Source = 190,
    /// Patch slot 18 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch18Dest = 191,
    /// Patch slot 18 amount, −1..=1.
    Patch18Amount = 192,
    /// Patch slot 19 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch19Source = 193,
    /// Patch slot 19 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch19Dest = 194,
    /// Patch slot 19 amount, −1..=1.
    Patch19Amount = 195,
    /// Patch slot 20 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch20Source = 196,
    /// Patch slot 20 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch20Dest = 197,
    /// Patch slot 20 amount, −1..=1.
    Patch20Amount = 198,
    /// Wavetable of the first table oscillator, 0..=7 (the PPG Wave), 8..=15 a user table.
    Wt1Table = 199,
    /// Wave position of the first oscillator in its table, 0..=1.
    Wt1Pos = 200,
    /// Wavetable of the second oscillator, 0..=7, 8..=15 a user table.
    Wt2Table = 201,
    /// Wave position of the second oscillator, 0..=1.
    Wt2Pos = 202,
    /// Stepped wave positions when ≥ 0.5, as the PPG's, instead of a crossfade.
    WtSteps = 203,
    /// Filter envelope → wave position of both oscillators, −1..=1.
    EnvWt = 204,
    /// LFO → wave position of both oscillators, 0..=1.
    LfoWt = 205,
    /// Partial 1's PCM attack: 0 is a synthesised oscillator, 1..=8 a sample (the D-50), 9..=16 a user attack.
    Pcm1Sample = 206,
    /// Partial 2's PCM attack: 0 is a synthesised oscillator, 1..=8 a sample, 9..=16 a user attack.
    Pcm2Sample = 207,
    /// How the two partials combine: 0 add, 1 sync (partial 2 to 1), 2 ring (1 × 2).
    Structure = 208,
    /// Partial 2's filter cutoff in Hz, 20..=20000.
    P2Cutoff = 209,
    /// Partial 2's filter resonance, 0..=1.
    P2Resonance = 210,
    /// Partial 2's filter envelope amount, −1..=1 (±4 octaves).
    P2EnvCutoff = 211,
    /// Partial 2's filter envelope attack, decay and release in seconds, 0.001..=10.
    P2FenvAttack = 212,
    P2FenvDecay = 213,
    /// Partial 2's filter envelope sustain level, 0..=1.
    P2FenvSustain = 214,
    P2FenvRelease = 215,
    /// Partial 2's amplifier envelope attack, decay and release in seconds, 0.001..=10.
    P2AdsrAttack = 216,
    P2AdsrDecay = 217,
    /// Partial 2's amplifier envelope sustain level, 0..=1.
    P2AdsrSustain = 218,
    P2AdsrRelease = 219,
    /// Operator 1 Rate 1, 0..=99.
    Op1R1 = 220,
    /// Operator 1 Rate 2, 0..=99.
    Op1R2 = 221,
    /// Operator 1 Rate 3, 0..=99.
    Op1R3 = 222,
    /// Operator 1 Rate 4, 0..=99.
    Op1R4 = 223,
    /// Operator 1 Level 1, 0..=99.
    Op1L1 = 224,
    /// Operator 1 Level 2, 0..=99.
    Op1L2 = 225,
    /// Operator 1 Level 3, 0..=99.
    Op1L3 = 226,
    /// Operator 1 Level 4, 0..=99.
    Op1L4 = 227,
    /// Operator 1 level scaling break point, 0..=99.
    Op1BreakPoint = 228,
    /// Operator 1 level scaling depth to the left, 0..=99.
    Op1LeftDepth = 229,
    /// Operator 1 level scaling depth to the right, 0..=99.
    Op1RightDepth = 230,
    /// Operator 1 level scaling curve to the left (−lin, −exp, +exp, +lin), 0..=3.
    Op1LeftCurve = 231,
    /// Operator 1 level scaling curve to the right, 0..=3.
    Op1RightCurve = 232,
    /// Operator 1 rate scaling, 0..=7.
    Op1RateScale = 233,
    /// Operator 1 amplitude modulation sensitivity, 0..=3.
    Op1AmpSens = 234,
    /// Operator 1 velocity sensitivity, 0..=7.
    Op1VelSens = 235,
    /// Operator 1 output level, 0..=99.
    Op1Level = 236,
    /// Operator 1 frequency mode (0 ratio, 1 fixed), 0..=1.
    Op1Mode = 237,
    /// Operator 1 coarse frequency, 0..=31.
    Op1Coarse = 238,
    /// Operator 1 fine frequency, 0..=99.
    Op1Fine = 239,
    /// Operator 1 detune (7 is centre), 0..=14.
    Op1Detune = 240,
    /// Operator 2 Rate 1, 0..=99.
    Op2R1 = 241,
    /// Operator 2 Rate 2, 0..=99.
    Op2R2 = 242,
    /// Operator 2 Rate 3, 0..=99.
    Op2R3 = 243,
    /// Operator 2 Rate 4, 0..=99.
    Op2R4 = 244,
    /// Operator 2 Level 1, 0..=99.
    Op2L1 = 245,
    /// Operator 2 Level 2, 0..=99.
    Op2L2 = 246,
    /// Operator 2 Level 3, 0..=99.
    Op2L3 = 247,
    /// Operator 2 Level 4, 0..=99.
    Op2L4 = 248,
    /// Operator 2 level scaling break point, 0..=99.
    Op2BreakPoint = 249,
    /// Operator 2 level scaling depth to the left, 0..=99.
    Op2LeftDepth = 250,
    /// Operator 2 level scaling depth to the right, 0..=99.
    Op2RightDepth = 251,
    /// Operator 2 level scaling curve to the left (−lin, −exp, +exp, +lin), 0..=3.
    Op2LeftCurve = 252,
    /// Operator 2 level scaling curve to the right, 0..=3.
    Op2RightCurve = 253,
    /// Operator 2 rate scaling, 0..=7.
    Op2RateScale = 254,
    /// Operator 2 amplitude modulation sensitivity, 0..=3.
    Op2AmpSens = 255,
    /// Operator 2 velocity sensitivity, 0..=7.
    Op2VelSens = 256,
    /// Operator 2 output level, 0..=99.
    Op2Level = 257,
    /// Operator 2 frequency mode (0 ratio, 1 fixed), 0..=1.
    Op2Mode = 258,
    /// Operator 2 coarse frequency, 0..=31.
    Op2Coarse = 259,
    /// Operator 2 fine frequency, 0..=99.
    Op2Fine = 260,
    /// Operator 2 detune (7 is centre), 0..=14.
    Op2Detune = 261,
    /// Operator 3 Rate 1, 0..=99.
    Op3R1 = 262,
    /// Operator 3 Rate 2, 0..=99.
    Op3R2 = 263,
    /// Operator 3 Rate 3, 0..=99.
    Op3R3 = 264,
    /// Operator 3 Rate 4, 0..=99.
    Op3R4 = 265,
    /// Operator 3 Level 1, 0..=99.
    Op3L1 = 266,
    /// Operator 3 Level 2, 0..=99.
    Op3L2 = 267,
    /// Operator 3 Level 3, 0..=99.
    Op3L3 = 268,
    /// Operator 3 Level 4, 0..=99.
    Op3L4 = 269,
    /// Operator 3 level scaling break point, 0..=99.
    Op3BreakPoint = 270,
    /// Operator 3 level scaling depth to the left, 0..=99.
    Op3LeftDepth = 271,
    /// Operator 3 level scaling depth to the right, 0..=99.
    Op3RightDepth = 272,
    /// Operator 3 level scaling curve to the left (−lin, −exp, +exp, +lin), 0..=3.
    Op3LeftCurve = 273,
    /// Operator 3 level scaling curve to the right, 0..=3.
    Op3RightCurve = 274,
    /// Operator 3 rate scaling, 0..=7.
    Op3RateScale = 275,
    /// Operator 3 amplitude modulation sensitivity, 0..=3.
    Op3AmpSens = 276,
    /// Operator 3 velocity sensitivity, 0..=7.
    Op3VelSens = 277,
    /// Operator 3 output level, 0..=99.
    Op3Level = 278,
    /// Operator 3 frequency mode (0 ratio, 1 fixed), 0..=1.
    Op3Mode = 279,
    /// Operator 3 coarse frequency, 0..=31.
    Op3Coarse = 280,
    /// Operator 3 fine frequency, 0..=99.
    Op3Fine = 281,
    /// Operator 3 detune (7 is centre), 0..=14.
    Op3Detune = 282,
    /// Operator 4 Rate 1, 0..=99.
    Op4R1 = 283,
    /// Operator 4 Rate 2, 0..=99.
    Op4R2 = 284,
    /// Operator 4 Rate 3, 0..=99.
    Op4R3 = 285,
    /// Operator 4 Rate 4, 0..=99.
    Op4R4 = 286,
    /// Operator 4 Level 1, 0..=99.
    Op4L1 = 287,
    /// Operator 4 Level 2, 0..=99.
    Op4L2 = 288,
    /// Operator 4 Level 3, 0..=99.
    Op4L3 = 289,
    /// Operator 4 Level 4, 0..=99.
    Op4L4 = 290,
    /// Operator 4 level scaling break point, 0..=99.
    Op4BreakPoint = 291,
    /// Operator 4 level scaling depth to the left, 0..=99.
    Op4LeftDepth = 292,
    /// Operator 4 level scaling depth to the right, 0..=99.
    Op4RightDepth = 293,
    /// Operator 4 level scaling curve to the left (−lin, −exp, +exp, +lin), 0..=3.
    Op4LeftCurve = 294,
    /// Operator 4 level scaling curve to the right, 0..=3.
    Op4RightCurve = 295,
    /// Operator 4 rate scaling, 0..=7.
    Op4RateScale = 296,
    /// Operator 4 amplitude modulation sensitivity, 0..=3.
    Op4AmpSens = 297,
    /// Operator 4 velocity sensitivity, 0..=7.
    Op4VelSens = 298,
    /// Operator 4 output level, 0..=99.
    Op4Level = 299,
    /// Operator 4 frequency mode (0 ratio, 1 fixed), 0..=1.
    Op4Mode = 300,
    /// Operator 4 coarse frequency, 0..=31.
    Op4Coarse = 301,
    /// Operator 4 fine frequency, 0..=99.
    Op4Fine = 302,
    /// Operator 4 detune (7 is centre), 0..=14.
    Op4Detune = 303,
    /// Operator 5 Rate 1, 0..=99.
    Op5R1 = 304,
    /// Operator 5 Rate 2, 0..=99.
    Op5R2 = 305,
    /// Operator 5 Rate 3, 0..=99.
    Op5R3 = 306,
    /// Operator 5 Rate 4, 0..=99.
    Op5R4 = 307,
    /// Operator 5 Level 1, 0..=99.
    Op5L1 = 308,
    /// Operator 5 Level 2, 0..=99.
    Op5L2 = 309,
    /// Operator 5 Level 3, 0..=99.
    Op5L3 = 310,
    /// Operator 5 Level 4, 0..=99.
    Op5L4 = 311,
    /// Operator 5 level scaling break point, 0..=99.
    Op5BreakPoint = 312,
    /// Operator 5 level scaling depth to the left, 0..=99.
    Op5LeftDepth = 313,
    /// Operator 5 level scaling depth to the right, 0..=99.
    Op5RightDepth = 314,
    /// Operator 5 level scaling curve to the left (−lin, −exp, +exp, +lin), 0..=3.
    Op5LeftCurve = 315,
    /// Operator 5 level scaling curve to the right, 0..=3.
    Op5RightCurve = 316,
    /// Operator 5 rate scaling, 0..=7.
    Op5RateScale = 317,
    /// Operator 5 amplitude modulation sensitivity, 0..=3.
    Op5AmpSens = 318,
    /// Operator 5 velocity sensitivity, 0..=7.
    Op5VelSens = 319,
    /// Operator 5 output level, 0..=99.
    Op5Level = 320,
    /// Operator 5 frequency mode (0 ratio, 1 fixed), 0..=1.
    Op5Mode = 321,
    /// Operator 5 coarse frequency, 0..=31.
    Op5Coarse = 322,
    /// Operator 5 fine frequency, 0..=99.
    Op5Fine = 323,
    /// Operator 5 detune (7 is centre), 0..=14.
    Op5Detune = 324,
    /// Operator 6 Rate 1, 0..=99.
    Op6R1 = 325,
    /// Operator 6 Rate 2, 0..=99.
    Op6R2 = 326,
    /// Operator 6 Rate 3, 0..=99.
    Op6R3 = 327,
    /// Operator 6 Rate 4, 0..=99.
    Op6R4 = 328,
    /// Operator 6 Level 1, 0..=99.
    Op6L1 = 329,
    /// Operator 6 Level 2, 0..=99.
    Op6L2 = 330,
    /// Operator 6 Level 3, 0..=99.
    Op6L3 = 331,
    /// Operator 6 Level 4, 0..=99.
    Op6L4 = 332,
    /// Operator 6 level scaling break point, 0..=99.
    Op6BreakPoint = 333,
    /// Operator 6 level scaling depth to the left, 0..=99.
    Op6LeftDepth = 334,
    /// Operator 6 level scaling depth to the right, 0..=99.
    Op6RightDepth = 335,
    /// Operator 6 level scaling curve to the left (−lin, −exp, +exp, +lin), 0..=3.
    Op6LeftCurve = 336,
    /// Operator 6 level scaling curve to the right, 0..=3.
    Op6RightCurve = 337,
    /// Operator 6 rate scaling, 0..=7.
    Op6RateScale = 338,
    /// Operator 6 amplitude modulation sensitivity, 0..=3.
    Op6AmpSens = 339,
    /// Operator 6 velocity sensitivity, 0..=7.
    Op6VelSens = 340,
    /// Operator 6 output level, 0..=99.
    Op6Level = 341,
    /// Operator 6 frequency mode (0 ratio, 1 fixed), 0..=1.
    Op6Mode = 342,
    /// Operator 6 coarse frequency, 0..=31.
    Op6Coarse = 343,
    /// Operator 6 fine frequency, 0..=99.
    Op6Fine = 344,
    /// Operator 6 detune (7 is centre), 0..=14.
    Op6Detune = 345,
    /// Pitch envelope rate 1, 0..=99.
    PitchR1 = 346,
    /// Pitch envelope rate 2, 0..=99.
    PitchR2 = 347,
    /// Pitch envelope rate 3, 0..=99.
    PitchR3 = 348,
    /// Pitch envelope rate 4, 0..=99.
    PitchR4 = 349,
    /// Pitch envelope level 1, 0..=99.
    PitchL1 = 350,
    /// Pitch envelope level 2, 0..=99.
    PitchL2 = 351,
    /// Pitch envelope level 3, 0..=99.
    PitchL3 = 352,
    /// Pitch envelope level 4, 0..=99.
    PitchL4 = 353,
    /// Algorithm (0 is algorithm 1), 0..=31.
    Algorithm = 354,
    /// Operator 6 feedback, 0..=7.
    Feedback = 355,
    /// Oscillator key sync, 0..=1.
    OscSync = 356,
    /// LFO speed, 0..=99.
    LfoSpeed = 357,
    /// LFO delay, 0..=99.
    LfoDelay = 358,
    /// LFO pitch modulation depth, 0..=99.
    LfoPitchDepth = 359,
    /// LFO amplitude modulation depth, 0..=99.
    LfoAmpDepth = 360,
    /// LFO key sync, 0..=1.
    LfoSync = 361,
    /// LFO waveform (triangle, saw down, saw up, square, sine, sample and hold), 0..=5.
    LfoShape = 362,
    /// Pitch modulation sensitivity, 0..=7.
    PitchSens = 363,
    /// Transpose, 24 is the middle C, 0..=48.
    Transpose = 364,
    /// Drum kit: the kick's tune in semitones, −12..=12.
    BdTune = 365,
    /// Drum kit: the kick's decay as a factor on its own, 0.25..=4.
    BdDecay = 366,
    /// Drum kit: the kick's tone, 0..=1.
    BdTone = 367,
    /// Drum kit: the kick's level, 0..=1.
    BdLevel = 368,
    /// Drum kit: the snare's tune in semitones, −12..=12.
    SnTune = 369,
    /// Drum kit: the snare's decay as a factor on its own, 0.25..=4.
    SnDecay = 370,
    /// Drum kit: the snare's tone, 0..=1.
    SnTone = 371,
    /// Drum kit: the snare's level, 0..=1.
    SnLevel = 372,
    /// Drum kit: the clap's tune in semitones, −12..=12.
    CpTune = 373,
    /// Drum kit: the clap's decay as a factor on its own, 0.25..=4.
    CpDecay = 374,
    /// Drum kit: the clap's tone, 0..=1.
    CpTone = 375,
    /// Drum kit: the clap's level, 0..=1.
    CpLevel = 376,
    /// Drum kit: the closed hat's tune in semitones, −12..=12.
    ChTune = 377,
    /// Drum kit: the closed hat's decay as a factor on its own, 0.25..=4.
    ChDecay = 378,
    /// Drum kit: the closed hat's tone, 0..=1.
    ChTone = 379,
    /// Drum kit: the closed hat's level, 0..=1.
    ChLevel = 380,
    /// Drum kit: the open hat's tune in semitones, −12..=12.
    OhTune = 381,
    /// Drum kit: the open hat's decay as a factor on its own, 0.25..=4.
    OhDecay = 382,
    /// Drum kit: the open hat's tone, 0..=1.
    OhTone = 383,
    /// Drum kit: the open hat's level, 0..=1.
    OhLevel = 384,
    /// Drum kit: the low tom's tune in semitones, −12..=12.
    LtTune = 385,
    /// Drum kit: the low tom's decay as a factor on its own, 0.25..=4.
    LtDecay = 386,
    /// Drum kit: the low tom's tone, 0..=1.
    LtTone = 387,
    /// Drum kit: the low tom's level, 0..=1.
    LtLevel = 388,
    /// Drum kit: the high tom's tune in semitones, −12..=12.
    HtTune = 389,
    /// Drum kit: the high tom's decay as a factor on its own, 0.25..=4.
    HtDecay = 390,
    /// Drum kit: the high tom's tone, 0..=1.
    HtTone = 391,
    /// Drum kit: the high tom's level, 0..=1.
    HtLevel = 392,
    /// Drum kit: the cowbell's tune in semitones, −12..=12.
    CbTune = 393,
    /// Drum kit: the cowbell's decay as a factor on its own, 0.25..=4.
    CbDecay = 394,
    /// Drum kit: the cowbell's tone, 0..=1.
    CbTone = 395,
    /// Drum kit: the cowbell's level, 0..=1.
    CbLevel = 396,
    /// Drum kit: how much louder an accented hit is, 0..=1.
    DrumAccent = 397,
    /// Drum kit: the rimshot's tune in semitones, −12..=12.
    RsTune = 398,
    /// Drum kit: the rimshot's decay as a factor on its own, 0.25..=4.
    RsDecay = 399,
    /// Drum kit: the rimshot's tone, 0..=1.
    RsTone = 400,
    /// Drum kit: the rimshot's level, 0..=1.
    RsLevel = 401,
    /// Drum kit: the claves's tune in semitones, −12..=12.
    ClTune = 402,
    /// Drum kit: the claves's decay as a factor on its own, 0.25..=4.
    ClDecay = 403,
    /// Drum kit: the claves's tone, 0..=1.
    ClTone = 404,
    /// Drum kit: the claves's level, 0..=1.
    ClLevel = 405,
    /// Drum kit: the maracas's tune in semitones, −12..=12.
    MaTune = 406,
    /// Drum kit: the maracas's decay as a factor on its own, 0.25..=4.
    MaDecay = 407,
    /// Drum kit: the maracas's tone, 0..=1.
    MaTone = 408,
    /// Drum kit: the maracas's level, 0..=1.
    MaLevel = 409,
    /// Drum kit: the cymbal's tune in semitones, −12..=12.
    CyTune = 410,
    /// Drum kit: the cymbal's decay as a factor on its own, 0.25..=4.
    CyDecay = 411,
    /// Drum kit: the cymbal's tone, 0..=1.
    CyTone = 412,
    /// Drum kit: the cymbal's level, 0..=1.
    CyLevel = 413,
    /// Drum kit: the mid tom's tune in semitones, −12..=12.
    MtTune = 414,
    /// Drum kit: the mid tom's decay as a factor on its own, 0.25..=4.
    MtDecay = 415,
    /// Drum kit: the mid tom's tone, 0..=1.
    MtTone = 416,
    /// Drum kit: the mid tom's level, 0..=1.
    MtLevel = 417,
    /// Drum kit: the low conga's tune in semitones, −12..=12.
    LcTune = 418,
    /// Drum kit: the low conga's decay as a factor on its own, 0.25..=4.
    LcDecay = 419,
    /// Drum kit: the low conga's tone, 0..=1.
    LcTone = 420,
    /// Drum kit: the low conga's level, 0..=1.
    LcLevel = 421,
    /// Drum kit: the mid conga's tune in semitones, −12..=12.
    McTune = 422,
    /// Drum kit: the mid conga's decay as a factor on its own, 0.25..=4.
    McDecay = 423,
    /// Drum kit: the mid conga's tone, 0..=1.
    McTone = 424,
    /// Drum kit: the mid conga's level, 0..=1.
    McLevel = 425,
    /// Drum kit: the high conga's tune in semitones, −12..=12.
    HcTune = 426,
    /// Drum kit: the high conga's decay as a factor on its own, 0.25..=4.
    HcDecay = 427,
    /// Drum kit: the high conga's tone, 0..=1.
    HcTone = 428,
    /// Drum kit: the high conga's level, 0..=1.
    HcLevel = 429,
    /// Send 1 taps before the fader (1) or after it (0, the default).
    Send1Pre = 430,
    /// Send 2 taps before the fader (1) or after it (0, the default).
    Send2Pre = 431,
    /// Send 3 taps before the fader (1) or after it (0, the default).
    Send3Pre = 432,
    /// Send 4 taps before the fader (1) or after it (0, the default).
    Send4Pre = 433,
    /// Send 1 is on (1, the default) or off: silent, keeping its level.
    Send1On = 434,
    /// Send 2 is on (1, the default) or off: silent, keeping its level.
    Send2On = 435,
    /// Send 3 is on (1, the default) or off: silent, keeping its level.
    Send3On = 436,
    /// Send 4 is on (1, the default) or off: silent, keeping its level.
    Send4On = 437,
    /// Drum kit, bd: where it goes: 0 the kit's strip, 1–8 a group.
    BdOut = 438,
    /// Drum kit, bd: pan into its group, −1..=1.
    BdPan = 439,
    /// Drum kit, sn: where it goes: 0 the kit's strip, 1–8 a group.
    SnOut = 440,
    /// Drum kit, sn: pan into its group, −1..=1.
    SnPan = 441,
    /// Drum kit, cp: where it goes: 0 the kit's strip, 1–8 a group.
    CpOut = 442,
    /// Drum kit, cp: pan into its group, −1..=1.
    CpPan = 443,
    /// Drum kit, ch: where it goes: 0 the kit's strip, 1–8 a group.
    ChOut = 444,
    /// Drum kit, ch: pan into its group, −1..=1.
    ChPan = 445,
    /// Drum kit, oh: where it goes: 0 the kit's strip, 1–8 a group.
    OhOut = 446,
    /// Drum kit, oh: pan into its group, −1..=1.
    OhPan = 447,
    /// Drum kit, lt: where it goes: 0 the kit's strip, 1–8 a group.
    LtOut = 448,
    /// Drum kit, lt: pan into its group, −1..=1.
    LtPan = 449,
    /// Drum kit, ht: where it goes: 0 the kit's strip, 1–8 a group.
    HtOut = 450,
    /// Drum kit, ht: pan into its group, −1..=1.
    HtPan = 451,
    /// Drum kit, cb: where it goes: 0 the kit's strip, 1–8 a group.
    CbOut = 452,
    /// Drum kit, cb: pan into its group, −1..=1.
    CbPan = 453,
    /// Drum kit, rs: where it goes: 0 the kit's strip, 1–8 a group.
    RsOut = 454,
    /// Drum kit, rs: pan into its group, −1..=1.
    RsPan = 455,
    /// Drum kit, cl: where it goes: 0 the kit's strip, 1–8 a group.
    ClOut = 456,
    /// Drum kit, cl: pan into its group, −1..=1.
    ClPan = 457,
    /// Drum kit, ma: where it goes: 0 the kit's strip, 1–8 a group.
    MaOut = 458,
    /// Drum kit, ma: pan into its group, −1..=1.
    MaPan = 459,
    /// Drum kit, cy: where it goes: 0 the kit's strip, 1–8 a group.
    CyOut = 460,
    /// Drum kit, cy: pan into its group, −1..=1.
    CyPan = 461,
    /// Drum kit, mt: where it goes: 0 the kit's strip, 1–8 a group.
    MtOut = 462,
    /// Drum kit, mt: pan into its group, −1..=1.
    MtPan = 463,
    /// Drum kit, lc: where it goes: 0 the kit's strip, 1–8 a group.
    LcOut = 464,
    /// Drum kit, lc: pan into its group, −1..=1.
    LcPan = 465,
    /// Drum kit, mc: where it goes: 0 the kit's strip, 1–8 a group.
    McOut = 466,
    /// Drum kit, mc: pan into its group, −1..=1.
    McPan = 467,
    /// Drum kit, hc: where it goes: 0 the kit's strip, 1–8 a group.
    HcOut = 468,
    /// Drum kit, hc: pan into its group, −1..=1.
    HcPan = 469,
    /// The synth a strip's vocoder listens to (#161): 0 none, 1–16 that synth's raw signal.
    Key = 470,
    /// Drum kit: the crash's tune in semitones, −12..=12.
    CrTune = 471,
    /// Drum kit: the crash's decay as a factor on its own, 0.25..=4.
    CrDecay = 472,
    /// Drum kit: the crash's tone, 0..=1.
    CrTone = 473,
    /// Drum kit: the crash's level, 0..=1.
    CrLevel = 474,
    /// Drum kit: the crash's where it goes: 0 the kit's strip, 1–8 a group.
    CrOut = 475,
    /// Drum kit: the crash's pan into its group, −1..=1.
    CrPan = 476,
    /// Drum kit: the ride's tune in semitones, −12..=12.
    RdTune = 477,
    /// Drum kit: the ride's decay as a factor on its own, 0.25..=4.
    RdDecay = 478,
    /// Drum kit: the ride's tone, 0..=1.
    RdTone = 479,
    /// Drum kit: the ride's level, 0..=1.
    RdLevel = 480,
    /// Drum kit: the ride's where it goes: 0 the kit's strip, 1–8 a group.
    RdOut = 481,
    /// Drum kit: the ride's pan into its group, −1..=1.
    RdPan = 482,
    /// Live arpeggiator on (0/1): held keys play as a pattern with the clock.
    ArpOn = 483,
    /// Arp order (`ArpMode`: up, down, up-down, as played, random), 0..=4.
    ArpMode = 484,
    /// Arp octaves, 1..=4.
    ArpOctaves = 485,
    /// Arp rate id (1/8, 1/16, 1/8T, 1/16T), 0..=3.
    ArpRate = 486,
    /// Arp note length as a fraction of the step, 0.05..=1.
    ArpGate = 487,
    /// Arp latch (0/1): the chord keeps playing after the keys are released.
    ArpLatch = 488,
    /// Arp free run (0/1): plays on its own grid while the song is stopped.
    ArpFree = 489,
    /// Seed of the random mode, 0..=9999.
    ArpSeed = 490,
    /// Drum kit: the kick's drive into a soft clip, 0..=1 (#264).
    BdDrive = 491,
    /// A Modular voice's controls (ADR-0020): `ctl` lines name them per
    /// voice; each holds its value in the control's own units.
    Ctl1 = 492,
    Ctl2 = 493,
    Ctl3 = 494,
    Ctl4 = 495,
    Ctl5 = 496,
    Ctl6 = 497,
    Ctl7 = 498,
    Ctl8 = 499,
    Ctl9 = 500,
    Ctl10 = 501,
    Ctl11 = 502,
    Ctl12 = 503,
    Ctl13 = 504,
    Ctl14 = 505,
    Ctl15 = 506,
    Ctl16 = 507,
    Ctl17 = 508,
    Ctl18 = 509,
    Ctl19 = 510,
    Ctl20 = 511,
    Ctl21 = 512,
    Ctl22 = 513,
    Ctl23 = 514,
    Ctl24 = 515,
    Ctl25 = 516,
    Ctl26 = 517,
    Ctl27 = 518,
    Ctl28 = 519,
    Ctl29 = 520,
    Ctl30 = 521,
    Ctl31 = 522,
    Ctl32 = 523,
    /// The Minimoog's mixer switches: VCO 1, 2, 3 and noise sound when ≥ 0.5,
    /// whatever their level (#308).
    Vco1On = 524,
    Vco2On = 525,
    Vco3On = 526,
    NoiseOn = 527,
    /// Glide sounds when ≥ 0.5; off keeps the time but plays no glide.
    GlideOn = 528,
    /// Where a model's contours have no release (the Minimoog), ≥ 0.5 makes
    /// the decay the release, as its Decay switch does; off releases at once.
    DecayRelease = 529,
    /// Oscillator modulation (vibrato) and filter modulation (`LfoCutoff`)
    /// reach their destinations when ≥ 0.5.
    OscModOn = 530,
    FilterModOn = 531,
    /// A 440 Hz reference tone at the synth's output when ≥ 0.5, key or not.
    A440 = 532,
    /// The filter revision on the models with the switch, 1..=3 (#321): the
    /// Prophet-5's SSM2040 at 1 and 2, its CEM3320 at 3; the Odyssey's
    /// 4023, 4035 and 4075.
    FilterRev = 533,
    /// The instrument's revision on the models with the Revision switch
    /// (#343): 1..=4 sets the VCO, filter and envelope switches and `Analog`
    /// as that revision had them; 0 is Custom, which a switch turned away
    /// from them leaves.
    Revision = 534,
    /// The VCO revision, 1..=3: the Prophet-5's SSM2030 at 1 and 2, its
    /// CEM3340 at 3 (#343).
    VcoRev = 535,
    /// The envelope revision, 1..=3: the Prophet-5's SSM2050 at 1 and 2, its
    /// CEM3310 at 3 (#343).
    EnvRev = 536,
    /// The pitch wheel, −1..=1, until MIDI input sends pitch bend (#10).
    PitchBend = 537,
    /// How far the pitch wheel bends, in semitones each way, 0..=24.
    BendRange = 538,
}

/// Controls a Modular voice may have (`Param::Ctl1`…).
pub const CTLS: usize = 32;

/// Where the global parameters start: P1 an echo and P2 a reverb, silent until
/// a return goes up; P3 and P4 are off.
pub const GLOBAL_DEFAULTS: [(Param, f32); 47] = [
    (Param::P2In, 0.0),
    (Param::P3In, 0.0),
    (Param::P4In, 0.0),
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
    pub const ALL: [(Param, &'static str); 539] = [
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
        (Param::P2In, "P2In"),
        (Param::P3In, "P3In"),
        (Param::P4In, "P4In"),
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
        (Param::Pcm1Sample, "Pcm1Sample"),
        (Param::Pcm2Sample, "Pcm2Sample"),
        (Param::Structure, "Structure"),
        (Param::P2Cutoff, "P2Cutoff"),
        (Param::P2Resonance, "P2Resonance"),
        (Param::P2EnvCutoff, "P2EnvCutoff"),
        (Param::P2FenvAttack, "P2FenvAttack"),
        (Param::P2FenvDecay, "P2FenvDecay"),
        (Param::P2FenvSustain, "P2FenvSustain"),
        (Param::P2FenvRelease, "P2FenvRelease"),
        (Param::P2AdsrAttack, "P2AdsrAttack"),
        (Param::P2AdsrDecay, "P2AdsrDecay"),
        (Param::P2AdsrSustain, "P2AdsrSustain"),
        (Param::P2AdsrRelease, "P2AdsrRelease"),
        (Param::Op1R1, "Op1R1"),
        (Param::Op1R2, "Op1R2"),
        (Param::Op1R3, "Op1R3"),
        (Param::Op1R4, "Op1R4"),
        (Param::Op1L1, "Op1L1"),
        (Param::Op1L2, "Op1L2"),
        (Param::Op1L3, "Op1L3"),
        (Param::Op1L4, "Op1L4"),
        (Param::Op1BreakPoint, "Op1BreakPoint"),
        (Param::Op1LeftDepth, "Op1LeftDepth"),
        (Param::Op1RightDepth, "Op1RightDepth"),
        (Param::Op1LeftCurve, "Op1LeftCurve"),
        (Param::Op1RightCurve, "Op1RightCurve"),
        (Param::Op1RateScale, "Op1RateScale"),
        (Param::Op1AmpSens, "Op1AmpSens"),
        (Param::Op1VelSens, "Op1VelSens"),
        (Param::Op1Level, "Op1Level"),
        (Param::Op1Mode, "Op1Mode"),
        (Param::Op1Coarse, "Op1Coarse"),
        (Param::Op1Fine, "Op1Fine"),
        (Param::Op1Detune, "Op1Detune"),
        (Param::Op2R1, "Op2R1"),
        (Param::Op2R2, "Op2R2"),
        (Param::Op2R3, "Op2R3"),
        (Param::Op2R4, "Op2R4"),
        (Param::Op2L1, "Op2L1"),
        (Param::Op2L2, "Op2L2"),
        (Param::Op2L3, "Op2L3"),
        (Param::Op2L4, "Op2L4"),
        (Param::Op2BreakPoint, "Op2BreakPoint"),
        (Param::Op2LeftDepth, "Op2LeftDepth"),
        (Param::Op2RightDepth, "Op2RightDepth"),
        (Param::Op2LeftCurve, "Op2LeftCurve"),
        (Param::Op2RightCurve, "Op2RightCurve"),
        (Param::Op2RateScale, "Op2RateScale"),
        (Param::Op2AmpSens, "Op2AmpSens"),
        (Param::Op2VelSens, "Op2VelSens"),
        (Param::Op2Level, "Op2Level"),
        (Param::Op2Mode, "Op2Mode"),
        (Param::Op2Coarse, "Op2Coarse"),
        (Param::Op2Fine, "Op2Fine"),
        (Param::Op2Detune, "Op2Detune"),
        (Param::Op3R1, "Op3R1"),
        (Param::Op3R2, "Op3R2"),
        (Param::Op3R3, "Op3R3"),
        (Param::Op3R4, "Op3R4"),
        (Param::Op3L1, "Op3L1"),
        (Param::Op3L2, "Op3L2"),
        (Param::Op3L3, "Op3L3"),
        (Param::Op3L4, "Op3L4"),
        (Param::Op3BreakPoint, "Op3BreakPoint"),
        (Param::Op3LeftDepth, "Op3LeftDepth"),
        (Param::Op3RightDepth, "Op3RightDepth"),
        (Param::Op3LeftCurve, "Op3LeftCurve"),
        (Param::Op3RightCurve, "Op3RightCurve"),
        (Param::Op3RateScale, "Op3RateScale"),
        (Param::Op3AmpSens, "Op3AmpSens"),
        (Param::Op3VelSens, "Op3VelSens"),
        (Param::Op3Level, "Op3Level"),
        (Param::Op3Mode, "Op3Mode"),
        (Param::Op3Coarse, "Op3Coarse"),
        (Param::Op3Fine, "Op3Fine"),
        (Param::Op3Detune, "Op3Detune"),
        (Param::Op4R1, "Op4R1"),
        (Param::Op4R2, "Op4R2"),
        (Param::Op4R3, "Op4R3"),
        (Param::Op4R4, "Op4R4"),
        (Param::Op4L1, "Op4L1"),
        (Param::Op4L2, "Op4L2"),
        (Param::Op4L3, "Op4L3"),
        (Param::Op4L4, "Op4L4"),
        (Param::Op4BreakPoint, "Op4BreakPoint"),
        (Param::Op4LeftDepth, "Op4LeftDepth"),
        (Param::Op4RightDepth, "Op4RightDepth"),
        (Param::Op4LeftCurve, "Op4LeftCurve"),
        (Param::Op4RightCurve, "Op4RightCurve"),
        (Param::Op4RateScale, "Op4RateScale"),
        (Param::Op4AmpSens, "Op4AmpSens"),
        (Param::Op4VelSens, "Op4VelSens"),
        (Param::Op4Level, "Op4Level"),
        (Param::Op4Mode, "Op4Mode"),
        (Param::Op4Coarse, "Op4Coarse"),
        (Param::Op4Fine, "Op4Fine"),
        (Param::Op4Detune, "Op4Detune"),
        (Param::Op5R1, "Op5R1"),
        (Param::Op5R2, "Op5R2"),
        (Param::Op5R3, "Op5R3"),
        (Param::Op5R4, "Op5R4"),
        (Param::Op5L1, "Op5L1"),
        (Param::Op5L2, "Op5L2"),
        (Param::Op5L3, "Op5L3"),
        (Param::Op5L4, "Op5L4"),
        (Param::Op5BreakPoint, "Op5BreakPoint"),
        (Param::Op5LeftDepth, "Op5LeftDepth"),
        (Param::Op5RightDepth, "Op5RightDepth"),
        (Param::Op5LeftCurve, "Op5LeftCurve"),
        (Param::Op5RightCurve, "Op5RightCurve"),
        (Param::Op5RateScale, "Op5RateScale"),
        (Param::Op5AmpSens, "Op5AmpSens"),
        (Param::Op5VelSens, "Op5VelSens"),
        (Param::Op5Level, "Op5Level"),
        (Param::Op5Mode, "Op5Mode"),
        (Param::Op5Coarse, "Op5Coarse"),
        (Param::Op5Fine, "Op5Fine"),
        (Param::Op5Detune, "Op5Detune"),
        (Param::Op6R1, "Op6R1"),
        (Param::Op6R2, "Op6R2"),
        (Param::Op6R3, "Op6R3"),
        (Param::Op6R4, "Op6R4"),
        (Param::Op6L1, "Op6L1"),
        (Param::Op6L2, "Op6L2"),
        (Param::Op6L3, "Op6L3"),
        (Param::Op6L4, "Op6L4"),
        (Param::Op6BreakPoint, "Op6BreakPoint"),
        (Param::Op6LeftDepth, "Op6LeftDepth"),
        (Param::Op6RightDepth, "Op6RightDepth"),
        (Param::Op6LeftCurve, "Op6LeftCurve"),
        (Param::Op6RightCurve, "Op6RightCurve"),
        (Param::Op6RateScale, "Op6RateScale"),
        (Param::Op6AmpSens, "Op6AmpSens"),
        (Param::Op6VelSens, "Op6VelSens"),
        (Param::Op6Level, "Op6Level"),
        (Param::Op6Mode, "Op6Mode"),
        (Param::Op6Coarse, "Op6Coarse"),
        (Param::Op6Fine, "Op6Fine"),
        (Param::Op6Detune, "Op6Detune"),
        (Param::PitchR1, "PitchR1"),
        (Param::PitchR2, "PitchR2"),
        (Param::PitchR3, "PitchR3"),
        (Param::PitchR4, "PitchR4"),
        (Param::PitchL1, "PitchL1"),
        (Param::PitchL2, "PitchL2"),
        (Param::PitchL3, "PitchL3"),
        (Param::PitchL4, "PitchL4"),
        (Param::Algorithm, "Algorithm"),
        (Param::Feedback, "Feedback"),
        (Param::OscSync, "OscSync"),
        (Param::LfoSpeed, "LfoSpeed"),
        (Param::LfoDelay, "LfoDelay"),
        (Param::LfoPitchDepth, "LfoPitchDepth"),
        (Param::LfoAmpDepth, "LfoAmpDepth"),
        (Param::LfoSync, "LfoSync"),
        (Param::LfoShape, "LfoShape"),
        (Param::PitchSens, "PitchSens"),
        (Param::Transpose, "Transpose"),
        (Param::BdTune, "BdTune"),
        (Param::BdDecay, "BdDecay"),
        (Param::BdTone, "BdTone"),
        (Param::BdLevel, "BdLevel"),
        (Param::SnTune, "SnTune"),
        (Param::SnDecay, "SnDecay"),
        (Param::SnTone, "SnTone"),
        (Param::SnLevel, "SnLevel"),
        (Param::CpTune, "CpTune"),
        (Param::CpDecay, "CpDecay"),
        (Param::CpTone, "CpTone"),
        (Param::CpLevel, "CpLevel"),
        (Param::ChTune, "ChTune"),
        (Param::ChDecay, "ChDecay"),
        (Param::ChTone, "ChTone"),
        (Param::ChLevel, "ChLevel"),
        (Param::OhTune, "OhTune"),
        (Param::OhDecay, "OhDecay"),
        (Param::OhTone, "OhTone"),
        (Param::OhLevel, "OhLevel"),
        (Param::LtTune, "LtTune"),
        (Param::LtDecay, "LtDecay"),
        (Param::LtTone, "LtTone"),
        (Param::LtLevel, "LtLevel"),
        (Param::HtTune, "HtTune"),
        (Param::HtDecay, "HtDecay"),
        (Param::HtTone, "HtTone"),
        (Param::HtLevel, "HtLevel"),
        (Param::CbTune, "CbTune"),
        (Param::CbDecay, "CbDecay"),
        (Param::CbTone, "CbTone"),
        (Param::CbLevel, "CbLevel"),
        (Param::DrumAccent, "DrumAccent"),
        (Param::RsTune, "RsTune"),
        (Param::RsDecay, "RsDecay"),
        (Param::RsTone, "RsTone"),
        (Param::RsLevel, "RsLevel"),
        (Param::ClTune, "ClTune"),
        (Param::ClDecay, "ClDecay"),
        (Param::ClTone, "ClTone"),
        (Param::ClLevel, "ClLevel"),
        (Param::MaTune, "MaTune"),
        (Param::MaDecay, "MaDecay"),
        (Param::MaTone, "MaTone"),
        (Param::MaLevel, "MaLevel"),
        (Param::CyTune, "CyTune"),
        (Param::CyDecay, "CyDecay"),
        (Param::CyTone, "CyTone"),
        (Param::CyLevel, "CyLevel"),
        (Param::MtTune, "MtTune"),
        (Param::MtDecay, "MtDecay"),
        (Param::MtTone, "MtTone"),
        (Param::MtLevel, "MtLevel"),
        (Param::LcTune, "LcTune"),
        (Param::LcDecay, "LcDecay"),
        (Param::LcTone, "LcTone"),
        (Param::LcLevel, "LcLevel"),
        (Param::McTune, "McTune"),
        (Param::McDecay, "McDecay"),
        (Param::McTone, "McTone"),
        (Param::McLevel, "McLevel"),
        (Param::HcTune, "HcTune"),
        (Param::HcDecay, "HcDecay"),
        (Param::HcTone, "HcTone"),
        (Param::HcLevel, "HcLevel"),
        (Param::Send1Pre, "Send1Pre"),
        (Param::Send2Pre, "Send2Pre"),
        (Param::Send3Pre, "Send3Pre"),
        (Param::Send4Pre, "Send4Pre"),
        (Param::Send1On, "Send1On"),
        (Param::Send2On, "Send2On"),
        (Param::Send3On, "Send3On"),
        (Param::Send4On, "Send4On"),
        (Param::BdOut, "BdOut"),
        (Param::BdPan, "BdPan"),
        (Param::SnOut, "SnOut"),
        (Param::SnPan, "SnPan"),
        (Param::CpOut, "CpOut"),
        (Param::CpPan, "CpPan"),
        (Param::ChOut, "ChOut"),
        (Param::ChPan, "ChPan"),
        (Param::OhOut, "OhOut"),
        (Param::OhPan, "OhPan"),
        (Param::LtOut, "LtOut"),
        (Param::LtPan, "LtPan"),
        (Param::HtOut, "HtOut"),
        (Param::HtPan, "HtPan"),
        (Param::CbOut, "CbOut"),
        (Param::CbPan, "CbPan"),
        (Param::RsOut, "RsOut"),
        (Param::RsPan, "RsPan"),
        (Param::ClOut, "ClOut"),
        (Param::ClPan, "ClPan"),
        (Param::MaOut, "MaOut"),
        (Param::MaPan, "MaPan"),
        (Param::CyOut, "CyOut"),
        (Param::CyPan, "CyPan"),
        (Param::MtOut, "MtOut"),
        (Param::MtPan, "MtPan"),
        (Param::LcOut, "LcOut"),
        (Param::LcPan, "LcPan"),
        (Param::McOut, "McOut"),
        (Param::McPan, "McPan"),
        (Param::HcOut, "HcOut"),
        (Param::HcPan, "HcPan"),
        (Param::Key, "Key"),
        (Param::CrTune, "CrTune"),
        (Param::CrDecay, "CrDecay"),
        (Param::CrTone, "CrTone"),
        (Param::CrLevel, "CrLevel"),
        (Param::CrOut, "CrOut"),
        (Param::CrPan, "CrPan"),
        (Param::RdTune, "RdTune"),
        (Param::RdDecay, "RdDecay"),
        (Param::RdTone, "RdTone"),
        (Param::RdLevel, "RdLevel"),
        (Param::RdOut, "RdOut"),
        (Param::RdPan, "RdPan"),
        (Param::ArpOn, "ArpOn"),
        (Param::ArpMode, "ArpMode"),
        (Param::ArpOctaves, "ArpOctaves"),
        (Param::ArpRate, "ArpRate"),
        (Param::ArpGate, "ArpGate"),
        (Param::ArpLatch, "ArpLatch"),
        (Param::ArpFree, "ArpFree"),
        (Param::ArpSeed, "ArpSeed"),
        (Param::BdDrive, "BdDrive"),
        (Param::Ctl1, "Ctl1"),
        (Param::Ctl2, "Ctl2"),
        (Param::Ctl3, "Ctl3"),
        (Param::Ctl4, "Ctl4"),
        (Param::Ctl5, "Ctl5"),
        (Param::Ctl6, "Ctl6"),
        (Param::Ctl7, "Ctl7"),
        (Param::Ctl8, "Ctl8"),
        (Param::Ctl9, "Ctl9"),
        (Param::Ctl10, "Ctl10"),
        (Param::Ctl11, "Ctl11"),
        (Param::Ctl12, "Ctl12"),
        (Param::Ctl13, "Ctl13"),
        (Param::Ctl14, "Ctl14"),
        (Param::Ctl15, "Ctl15"),
        (Param::Ctl16, "Ctl16"),
        (Param::Ctl17, "Ctl17"),
        (Param::Ctl18, "Ctl18"),
        (Param::Ctl19, "Ctl19"),
        (Param::Ctl20, "Ctl20"),
        (Param::Ctl21, "Ctl21"),
        (Param::Ctl22, "Ctl22"),
        (Param::Ctl23, "Ctl23"),
        (Param::Ctl24, "Ctl24"),
        (Param::Ctl25, "Ctl25"),
        (Param::Ctl26, "Ctl26"),
        (Param::Ctl27, "Ctl27"),
        (Param::Ctl28, "Ctl28"),
        (Param::Ctl29, "Ctl29"),
        (Param::Ctl30, "Ctl30"),
        (Param::Ctl31, "Ctl31"),
        (Param::Ctl32, "Ctl32"),
        (Param::Vco1On, "Vco1On"),
        (Param::Vco2On, "Vco2On"),
        (Param::Vco3On, "Vco3On"),
        (Param::NoiseOn, "NoiseOn"),
        (Param::GlideOn, "GlideOn"),
        (Param::DecayRelease, "DecayRelease"),
        (Param::OscModOn, "OscModOn"),
        (Param::FilterModOn, "FilterModOn"),
        (Param::A440, "A440"),
        (Param::FilterRev, "FilterRev"),
        (Param::Revision, "Revision"),
        (Param::VcoRev, "VcoRev"),
        (Param::EnvRev, "EnvRev"),
        (Param::PitchBend, "PitchBend"),
        (Param::BendRange, "BendRange"),
    ];

    /// The control a parameter is, 0..32, for a Modular voice's `ctl`.
    pub fn ctl(self) -> Option<usize> {
        let i = (self as u32).checked_sub(Param::Ctl1 as u32)? as usize;
        (i < CTLS).then_some(i)
    }

    /// The parameter of control `i`.
    pub fn ctl_param(i: usize) -> Option<Param> {
        let i = u32::try_from(i).ok().filter(|i| (*i as usize) < CTLS)?;
        Param::from_id(Param::Ctl1 as u32 + i)
    }

    /// The live arpeggiator's parameters: the engine owns them, not the voice,
    /// and a preset leaves them alone.
    pub fn is_arp(self) -> bool {
        matches!(
            self,
            Param::ArpOn
                | Param::ArpMode
                | Param::ArpOctaves
                | Param::ArpRate
                | Param::ArpGate
                | Param::ArpLatch
                | Param::ArpFree
                | Param::ArpSeed
        )
    }

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
                | Param::Send1Pre
                | Param::Send2Pre
                | Param::Send3Pre
                | Param::Send4Pre
                | Param::Send1On
                | Param::Send2On
                | Param::Send3On
                | Param::Send4On
                | Param::Key
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
                | Param::P2In
                | Param::P3In
                | Param::P4In
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

    /// The parameter for a registry name in any case: `cutoff` and `Cutoff`
    /// both name `Param::Cutoff` (ADR-0019).
    pub fn by_name(name: &str) -> Option<Param> {
        Self::ALL
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(p, _)| *p)
    }

    /// Clamp a value into this parameter's range.
    pub fn clamp(self, v: f32) -> f32 {
        let (lo, hi) = self.range();
        if v.is_nan() { lo } else { v.clamp(lo, hi) }
    }

    /// The lowest and highest value this parameter takes.
    pub fn range(self) -> (f32, f32) {
        match self {
            Param::MasterGain => (0.0, 1.0),
            Param::Vco1Wave | Param::Vco2Wave | Param::Vco3Wave => (0.0, 3.0),
            Param::Vco1Coarse | Param::Vco2Coarse | Param::Vco3Coarse => (-24.0, 24.0),
            Param::Vco1Fine | Param::Vco2Fine | Param::Vco3Fine => (-50.0, 50.0),
            Param::Vco1Level | Param::Vco2Level | Param::Vco3Level => (0.0, 1.0),
            Param::PulseWidth => (0.05, 0.95),
            Param::Vco2Sync | Param::Vco3Sync => (0.0, 1.0),
            Param::RingLevel | Param::SubLevel | Param::SubOctave => (0.0, 1.0),
            Param::Vco3KeyFollow | Param::Vco3Low => (0.0, 1.0),
            Param::Vco1On
            | Param::Vco2On
            | Param::Vco3On
            | Param::NoiseOn
            | Param::GlideOn
            | Param::DecayRelease
            | Param::OscModOn
            | Param::FilterModOn
            | Param::A440 => (0.0, 1.0),
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
            // 0 the master, 1–8 a group, 9 nowhere (#161).
            Param::Out => (0.0, 9.0),
            Param::Key => (0.0, 16.0),
            Param::CrTune | Param::RdTune => (-12.0, 12.0),
            Param::CrDecay | Param::RdDecay => (0.25, 4.0),
            Param::CrTone | Param::RdTone => (0.0, 1.0),
            Param::CrLevel | Param::RdLevel => (0.0, 1.0),
            Param::CrOut | Param::RdOut => (0.0, 8.0),
            Param::CrPan | Param::RdPan => (-1.0, 1.0),
            Param::ArpOn | Param::ArpLatch | Param::ArpFree => (0.0, 1.0),
            Param::ArpMode => (0.0, 4.0),
            Param::ArpOctaves => (1.0, 4.0),
            Param::ArpRate => (0.0, 3.0),
            Param::ArpGate => (0.05, 1.0),
            Param::ArpSeed => (0.0, 9999.0),
            Param::Ctl1
            | Param::Ctl2
            | Param::Ctl3
            | Param::Ctl4
            | Param::Ctl5
            | Param::Ctl6
            | Param::Ctl7
            | Param::Ctl8
            | Param::Ctl9
            | Param::Ctl10
            | Param::Ctl11
            | Param::Ctl12
            | Param::Ctl13
            | Param::Ctl14
            | Param::Ctl15
            | Param::Ctl16
            | Param::Ctl17
            | Param::Ctl18
            | Param::Ctl19
            | Param::Ctl20
            | Param::Ctl21
            | Param::Ctl22
            | Param::Ctl23
            | Param::Ctl24
            | Param::Ctl25
            | Param::Ctl26
            | Param::Ctl27
            | Param::Ctl28
            | Param::Ctl29
            | Param::Ctl30
            | Param::Ctl31
            | Param::Ctl32 => (-100_000.0, 100_000.0),
            Param::BdOut
            | Param::SnOut
            | Param::CpOut
            | Param::ChOut
            | Param::OhOut
            | Param::LtOut
            | Param::HtOut
            | Param::CbOut
            | Param::RsOut
            | Param::ClOut
            | Param::MaOut
            | Param::CyOut
            | Param::MtOut
            | Param::LcOut
            | Param::McOut
            | Param::HcOut => (0.0, 8.0),
            Param::BdPan
            | Param::SnPan
            | Param::CpPan
            | Param::ChPan
            | Param::OhPan
            | Param::LtPan
            | Param::HtPan
            | Param::CbPan
            | Param::RsPan
            | Param::ClPan
            | Param::MaPan
            | Param::CyPan
            | Param::MtPan
            | Param::LcPan
            | Param::McPan
            | Param::HcPan => (-1.0, 1.0),
            Param::Send1Pre
            | Param::Send2Pre
            | Param::Send3Pre
            | Param::Send4Pre
            | Param::Send1On
            | Param::Send2On
            | Param::Send3On
            | Param::Send4On => (0.0, 1.0),
            Param::P2In | Param::P3In | Param::P4In => (0.0, 1.0),
            Param::Polyphony => (1.0, 16.0),
            Param::Assign => (0.0, 1.0),
            Param::ChorusMode => (0.0, 3.0),
            Param::XMod | Param::Slope => (0.0, 1.0),
            Param::FilterRev | Param::VcoRev | Param::EnvRev => (1.0, 3.0),
            Param::PitchBend => (-1.0, 1.0),
            Param::BendRange => (0.0, 24.0),
            Param::Revision => (0.0, 4.0),
            // 0..=7 the generated tables, 8..=15 the user's (spec 010 Req 9).
            Param::Wt1Table | Param::Wt2Table => (0.0, 15.0),
            Param::Wt1Pos | Param::Wt2Pos | Param::LfoWt => (0.0, 1.0),
            Param::WtSteps => (0.0, 1.0),
            Param::EnvWt => (-1.0, 1.0),
            // 1..=8 the generated attacks, 9..=16 the user's (spec 010 Req 10).
            Param::Pcm1Sample => (0.0, 16.0),
            Param::Pcm2Sample => (0.0, 16.0),
            Param::Structure => (0.0, 2.0),
            Param::P2Cutoff => (20.0, 20_000.0),
            Param::P2Resonance => (0.0, 1.0),
            Param::P2EnvCutoff => (-1.0, 1.0),
            Param::P2FenvAttack => (0.001, 10.0),
            Param::P2FenvDecay => (0.001, 10.0),
            Param::P2FenvSustain => (0.0, 1.0),
            Param::P2FenvRelease => (0.001, 10.0),
            Param::P2AdsrAttack => (0.001, 10.0),
            Param::P2AdsrDecay => (0.001, 10.0),
            Param::P2AdsrSustain => (0.0, 1.0),
            Param::P2AdsrRelease => (0.001, 10.0),
            Param::Op1R1
            | Param::Op1R2
            | Param::Op1R3
            | Param::Op1R4
            | Param::Op1L1
            | Param::Op1L2
            | Param::Op1L3
            | Param::Op1L4
            | Param::Op1BreakPoint
            | Param::Op1LeftDepth
            | Param::Op1RightDepth
            | Param::Op1Level
            | Param::Op1Fine
            | Param::Op2R1
            | Param::Op2R2
            | Param::Op2R3
            | Param::Op2R4
            | Param::Op2L1
            | Param::Op2L2
            | Param::Op2L3
            | Param::Op2L4
            | Param::Op2BreakPoint
            | Param::Op2LeftDepth
            | Param::Op2RightDepth
            | Param::Op2Level
            | Param::Op2Fine
            | Param::Op3R1
            | Param::Op3R2
            | Param::Op3R3
            | Param::Op3R4
            | Param::Op3L1
            | Param::Op3L2
            | Param::Op3L3
            | Param::Op3L4
            | Param::Op3BreakPoint
            | Param::Op3LeftDepth
            | Param::Op3RightDepth
            | Param::Op3Level
            | Param::Op3Fine
            | Param::Op4R1
            | Param::Op4R2
            | Param::Op4R3
            | Param::Op4R4
            | Param::Op4L1
            | Param::Op4L2
            | Param::Op4L3
            | Param::Op4L4
            | Param::Op4BreakPoint
            | Param::Op4LeftDepth
            | Param::Op4RightDepth
            | Param::Op4Level
            | Param::Op4Fine
            | Param::Op5R1
            | Param::Op5R2
            | Param::Op5R3
            | Param::Op5R4
            | Param::Op5L1
            | Param::Op5L2
            | Param::Op5L3
            | Param::Op5L4
            | Param::Op5BreakPoint
            | Param::Op5LeftDepth
            | Param::Op5RightDepth
            | Param::Op5Level
            | Param::Op5Fine
            | Param::Op6R1
            | Param::Op6R2
            | Param::Op6R3
            | Param::Op6R4
            | Param::Op6L1
            | Param::Op6L2
            | Param::Op6L3
            | Param::Op6L4
            | Param::Op6BreakPoint
            | Param::Op6LeftDepth
            | Param::Op6RightDepth
            | Param::Op6Level
            | Param::Op6Fine
            | Param::PitchR1
            | Param::PitchR2
            | Param::PitchR3
            | Param::PitchR4
            | Param::PitchL1
            | Param::PitchL2
            | Param::PitchL3
            | Param::PitchL4
            | Param::LfoSpeed
            | Param::LfoDelay
            | Param::LfoPitchDepth
            | Param::LfoAmpDepth => (0.0, 99.0),
            Param::Op1LeftCurve
            | Param::Op1RightCurve
            | Param::Op1AmpSens
            | Param::Op2LeftCurve
            | Param::Op2RightCurve
            | Param::Op2AmpSens
            | Param::Op3LeftCurve
            | Param::Op3RightCurve
            | Param::Op3AmpSens
            | Param::Op4LeftCurve
            | Param::Op4RightCurve
            | Param::Op4AmpSens
            | Param::Op5LeftCurve
            | Param::Op5RightCurve
            | Param::Op5AmpSens
            | Param::Op6LeftCurve
            | Param::Op6RightCurve
            | Param::Op6AmpSens => (0.0, 3.0),
            Param::Op1RateScale
            | Param::Op1VelSens
            | Param::Op2RateScale
            | Param::Op2VelSens
            | Param::Op3RateScale
            | Param::Op3VelSens
            | Param::Op4RateScale
            | Param::Op4VelSens
            | Param::Op5RateScale
            | Param::Op5VelSens
            | Param::Op6RateScale
            | Param::Op6VelSens
            | Param::Feedback
            | Param::PitchSens => (0.0, 7.0),
            Param::Op1Mode
            | Param::Op2Mode
            | Param::Op3Mode
            | Param::Op4Mode
            | Param::Op5Mode
            | Param::Op6Mode
            | Param::OscSync
            | Param::LfoSync => (0.0, 1.0),
            Param::Op1Coarse
            | Param::Op2Coarse
            | Param::Op3Coarse
            | Param::Op4Coarse
            | Param::Op5Coarse
            | Param::Op6Coarse
            | Param::Algorithm => (0.0, 31.0),
            Param::Op1Detune
            | Param::Op2Detune
            | Param::Op3Detune
            | Param::Op4Detune
            | Param::Op5Detune
            | Param::Op6Detune => (0.0, 14.0),
            Param::LfoShape => (0.0, 5.0),
            Param::Transpose => (0.0, 48.0),
            Param::BdTune
            | Param::SnTune
            | Param::CpTune
            | Param::ChTune
            | Param::OhTune
            | Param::LtTune
            | Param::HtTune
            | Param::CbTune => (-12.0, 12.0),
            Param::BdDecay
            | Param::SnDecay
            | Param::CpDecay
            | Param::ChDecay
            | Param::OhDecay
            | Param::LtDecay
            | Param::HtDecay
            | Param::CbDecay => (0.25, 4.0),
            Param::BdTone
            | Param::SnTone
            | Param::CpTone
            | Param::ChTone
            | Param::OhTone
            | Param::LtTone
            | Param::HtTone
            | Param::CbTone
            | Param::BdLevel
            | Param::BdDrive
            | Param::SnLevel
            | Param::CpLevel
            | Param::ChLevel
            | Param::OhLevel
            | Param::LtLevel
            | Param::HtLevel
            | Param::CbLevel
            | Param::DrumAccent => (0.0, 1.0),
            Param::RsTune
            | Param::ClTune
            | Param::MaTune
            | Param::CyTune
            | Param::MtTune
            | Param::LcTune
            | Param::McTune
            | Param::HcTune => (-12.0, 12.0),
            Param::RsDecay
            | Param::ClDecay
            | Param::MaDecay
            | Param::CyDecay
            | Param::MtDecay
            | Param::LcDecay
            | Param::McDecay
            | Param::HcDecay => (0.25, 4.0),
            Param::RsTone
            | Param::ClTone
            | Param::MaTone
            | Param::CyTone
            | Param::MtTone
            | Param::LcTone
            | Param::McTone
            | Param::HcTone
            | Param::RsLevel
            | Param::ClLevel
            | Param::MaLevel
            | Param::CyLevel
            | Param::MtLevel
            | Param::LcLevel
            | Param::McLevel
            | Param::HcLevel => (0.0, 1.0),
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
            // Every insert type there is: a new one is in range without a change here.
            Param::I1Type | Param::I2Type | Param::I3Type => {
                (0.0, (crate::fx::insert::InsertType::ALL.len() - 1) as f32)
            }
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
            Param::P1Type | Param::P2Type | Param::P3Type | Param::P4Type => (0.0, 4.0),
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
        }
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

    #[test]
    fn names_are_found_in_any_case_and_never_collide() {
        assert_eq!(Param::by_name("cutoff"), Some(Param::Cutoff));
        assert_eq!(Param::by_name("P2RETURN"), Some(Param::P2Return));
        assert_eq!(Param::by_name("nope"), None);
        for (p, n) in Param::ALL.iter() {
            assert_eq!(Param::by_name(&n.to_lowercase()), Some(*p), "{n}");
        }
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
        use crate::padsampler::PadField;
        use crate::sampler::ZoneField;
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
            ("Pad", rust(&crate::drums::Pad::ALL, |p| p as u32)),
            ("ZoneField", rust(&ZoneField::ALL, |f| f as u32)),
            ("PadField", rust(&PadField::ALL, |f| f as u32)),
            (
                "DeckField",
                rust(&crate::deck::DeckField::ALL, |f| f as u32),
            ),
            (
                "Quantize",
                rust(&crate::launch::Quantize::ALL, |q| q as u32),
            ),
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
