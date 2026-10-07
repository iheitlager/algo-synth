//! The Roland Jupiter-8: what the model is and its presets (#330).

use crate::mono::model::{BA662, DISCRETE_VCO, EnvVoicing, Filter, Hp, JUPITER, JUPITER12, RC_ENV};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    osc: DISCRETE_VCO,
    vca: BA662,
    // The IR3R01 (#340): attack 1.5 ms to 6 s, decay and release from
    // 1.5 ms.
    env: EnvVoicing {
        attack: [0.0015, 6.0],
        decay: [0.0015, 10.0],
        release: [0.0015, 10.0],
        ..RC_ENV
    },
    voices: 8,
    filter: Filter::Ladder(JUPITER),
    slope12: Some(Filter::Svf(JUPITER12)),
    hp: Hp::OnePole,
    presets: &[
        // Two saws a few cents apart, the filter opened by its envelope on every
        // note: the Jupiter brass.
        PresetDef::of(
            Preset::JupiterBrass,
            &[
                (Polyphony, 8.0),
                (Analog, 0.4),
                (Vco1Level, 0.9),
                (Vco2Fine, 8.0),
                (Vco2Level, 0.9),
                (Cutoff, 1_000.0),
                (Resonance, 0.2),
                (Slope, 1.0),
                (AdsrAttack, 0.05),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.25),
                (FenvAttack, 0.07),
                (FenvDecay, 0.5),
                (FenvSustain, 0.55),
                (EnvCutoff, 0.6),
                (KeyTrack, 0.6),
            ],
        ),
        // A saw and a pulse swept by the LFO, a slow attack and the filter at
        // 12 dB: a soft string pad.
        PresetDef::of(
            Preset::JupiterStrings,
            &[
                (Polyphony, 8.0),
                (Analog, 0.5),
                (Vco1Level, 0.8),
                (Vco2Wave, 1.0),
                (Vco2Fine, 6.0),
                (Vco2Level, 0.7),
                (LfoWave, 2.0),
                (LfoRate, 0.5),
                (LfoPw, 0.3),
                (Cutoff, 2_600.0),
                (Resonance, 0.1),
                (Slope, 0.0),
                (AdsrAttack, 0.35),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (FenvAttack, 0.4),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
            ],
        ),
        // Unison on all eight voices, two saws an octave down, a 24 dB filter
        // snapping shut: the big Jupiter bass.
        PresetDef::of(
            Preset::JupiterBass,
            &[
                (Polyphony, 8.0),
                (Assign, 1.0),
                (UnisonDetune, 0.15),
                (Analog, 0.4),
                (Vco1Coarse, -12.0),
                (Vco1Level, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 5.0),
                (Vco2Level, 0.9),
                (Cutoff, 450.0),
                (Resonance, 0.3),
                (Slope, 1.0),
                (Drive, 0.3),
                (AdsrAttack, 0.003),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.15),
                (FenvAttack, 0.003),
                (FenvDecay, 0.3),
                (FenvSustain, 0.15),
                (EnvCutoff, 0.65),
                (KeyTrack, 1.0),
            ],
        ),
        // VCO 2 synced to VCO 1 and swept by the filter envelope through
        // cross-modulation: the Jupiter sync lead, in unison.
        PresetDef::of(
            Preset::JupiterSync,
            &[
                (Polyphony, 8.0),
                (Assign, 1.0),
                (UnisonDetune, 0.1),
                (Analog, 0.4),
                (Vco1Level, 0.0),
                (Vco2Coarse, 7.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (EnvFreq2, 0.45),
                (Cutoff, 3_800.0),
                (Resonance, 0.2),
                (Slope, 1.0),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.3),
                (FenvAttack, 0.005),
                (FenvDecay, 0.6),
                (FenvSustain, 0.3),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
            ],
        ),
        // VCO 2 frequency-modulating VCO 1: a metallic, clangorous sound that
        // the envelope tames.
        PresetDef::of(
            Preset::JupiterXMod,
            &[
                (Polyphony, 8.0),
                (Analog, 0.4),
                (Vco1Wave, 3.0),
                (Vco1Level, 0.9),
                (Vco2Wave, 3.0),
                (Vco2Coarse, 7.0),
                (Vco2Level, 0.0),
                (XMod, 0.6),
                (Cutoff, 5_000.0),
                (Resonance, 0.15),
                (Slope, 1.0),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.9),
                (AdsrSustain, 0.1),
                (AdsrRelease, 0.6),
                (FenvAttack, 0.002),
                (FenvDecay, 0.5),
                (FenvSustain, 0.2),
                (EnvCutoff, 0.3),
                (KeyTrack, 1.0),
            ],
        ),
        // Two saws, a slow swell, and the LFO breathing the filter: a warm pad.
        PresetDef::of(
            Preset::JupiterPad,
            &[
                (Polyphony, 8.0),
                (Analog, 0.6),
                (Vco1Level, 0.8),
                (Vco1Fine, -9.0),
                (Vco2Fine, 9.0),
                (Vco2Level, 0.8),
                (Cutoff, 1_500.0),
                (Resonance, 0.2),
                (Slope, 1.0),
                (AdsrAttack, 0.9),
                (AdsrDecay, 0.8),
                (AdsrSustain, 0.85),
                (AdsrRelease, 1.1),
                (FenvAttack, 1.0),
                (FenvSustain, 0.7),
                (EnvCutoff, 0.25),
                (LfoWave, 3.0),
                (LfoRate, 0.35),
                (LfoCutoff, 0.3),
                (KeyTrack, 0.5),
            ],
        ),
    ],
    ..ModelDef::MONO
};
