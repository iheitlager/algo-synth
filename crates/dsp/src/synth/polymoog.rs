//! The Moog Polymoog: what the model is and its presets (#330).

use crate::mono::model::{Filter, LOCKED, POLYMOOG};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    osc: LOCKED,
    voices: 16,
    filter: Filter::Svf(POLYMOOG),
    presets: &[
        // The Polymoog's Strings: a saw and a pulse, a slow attack and release, the
        // resonant filter left open, and the ensemble chorus.
        PresetDef::of(
            Preset::PolyStrings,
            &[
                (Polyphony, 16.0),
                (Analog, 0.4),
                (Vco1Level, 0.8),
                (Vco2Wave, 1.0),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.6),
                (PulseWidth, 0.4),
                (LfoWave, 2.0),
                (LfoRate, 0.45),
                (LfoPw, 0.25),
                (Cutoff, 3_200.0),
                (Resonance, 0.12),
                (ChorusMode, 3.0),
                (AdsrAttack, 0.3),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.55),
                (FenvAttack, 0.3),
                (FenvSustain, 0.9),
                (EnvCutoff, 0.1),
                (KeyTrack, 0.5),
            ],
        ),
        // The Vox Humana: a pulse through a high resonance whose peak falls with the
        // filter envelope, so the vowel opens and closes with each note.
        PresetDef::of(
            Preset::VoxHumana,
            &[
                (Polyphony, 16.0),
                (Analog, 0.3),
                (Vco1Wave, 1.0),
                (Vco1Level, 0.9),
                (PulseWidth, 0.3),
                (Vco2Coarse, -12.0),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.5),
                (Cutoff, 500.0),
                (Resonance, 0.78),
                (ChorusMode, 1.0),
                (AdsrAttack, 0.08),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.85),
                (AdsrRelease, 0.3),
                (FenvAttack, 0.02),
                (FenvDecay, 0.7),
                (FenvSustain, 0.2),
                (EnvCutoff, 0.55),
                (KeyTrack, 0.7),
            ],
        ),
        // Funk: a short pulse with a quick filter snap and a tight release.
        PresetDef::of(
            Preset::PolyFunk,
            &[
                (Polyphony, 16.0),
                (Analog, 0.3),
                (Vco1Wave, 1.0),
                (Vco1Level, 0.9),
                (PulseWidth, 0.5),
                (Vco2Level, 0.6),
                (Vco2Fine, 5.0),
                (Cutoff, 700.0),
                (Resonance, 0.45),
                (AdsrAttack, 0.003),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.4),
                (AdsrRelease, 0.12),
                (FenvAttack, 0.003),
                (FenvDecay, 0.2),
                (FenvSustain, 0.1),
                (EnvCutoff, 0.7),
                (KeyTrack, 0.6),
            ],
        ),
        // Brass: saws with a swell in the filter.
        PresetDef::of(
            Preset::PolyBrass,
            &[
                (Polyphony, 16.0),
                (Analog, 0.4),
                (Vco1Level, 0.9),
                (Vco2Fine, 8.0),
                (Vco2Level, 0.9),
                (Cutoff, 900.0),
                (Resonance, 0.25),
                (AdsrAttack, 0.06),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.2),
                (FenvAttack, 0.08),
                (FenvDecay, 0.45),
                (FenvSustain, 0.5),
                (EnvCutoff, 0.6),
                (KeyTrack, 0.6),
            ],
        ),
    ],
    ..ModelDef::MONO
};
