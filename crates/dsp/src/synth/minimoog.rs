//! The Minimoog: what the model is and its presets (#330).

use crate::mono::model::{Filter, MOOG};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    filter: Filter::Ladder(MOOG),
    // The contours have no release knob; Osc 3 is the modulation source.
    decay_is_release: true,
    modulates_with_osc3: true,
    presets: &[
        // Two saws and a pulse an octave under, overdriven into a low
        // ladder that the contour opens: the Minimoog's bass. Low note
        // priority, as on the instrument.
        PresetDef::of(
            Preset::MiniBass,
            &[
                (Vco1Coarse, -12.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 4.0),
                (Vco2Level, 1.0),
                (Vco3Wave, 1.0),
                (Vco3Coarse, -24.0),
                (Vco3Level, 0.8),
                (Cutoff, 450.0),
                (Resonance, 0.3),
                (Drive, 0.5),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.8),
                (FenvAttack, 0.002),
                (FenvDecay, 0.35),
                (FenvSustain, 0.2),
                (EnvCutoff, 0.55),
                (KeyTrack, 0.33),
                (Priority, 1.0),
                (Glide, 0.04),
            ],
        ),
        // Two saws a few cents apart and a triangle; VCO 3 in its low
        // range is the vibrato, the wheel half up. Glide, legato.
        PresetDef::of(
            Preset::MiniLead,
            &[
                (Vco2Fine, 7.0),
                (Vco2Level, 0.8),
                (Vco3Wave, 2.0),
                (Vco3Coarse, -3.0),
                (Vco3Level, 0.0),
                (Vco3Low, 1.0),
                (Vco3KeyFollow, 0.0),
                (Cutoff, 1_800.0),
                (Resonance, 0.35),
                (Drive, 0.25),
                (AdsrAttack, 0.02),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.8),
                (FenvAttack, 0.02),
                (FenvDecay, 0.5),
                (FenvSustain, 0.5),
                (EnvCutoff, 0.3),
                (KeyTrack, 0.67),
                (Vibrato, 0.2),
                (ModWheel, 0.6),
                (Glide, 0.12),
                (Legato, 1.0),
                (Priority, 1.0),
            ],
        ),
        // Two saws and a square, a long glide with legato and low note
        // priority, Osc 3 as the vibrato: the singing portamento solo.
        PresetDef::of(
            Preset::LuckyMan,
            &[
                (Vco2Fine, 5.0),
                (Vco2Level, 0.9),
                (Vco3Wave, 2.0),
                (Vco3Coarse, -3.0),
                (Vco3Level, 0.0),
                (Vco3Low, 1.0),
                (Vco3KeyFollow, 0.0),
                (Cutoff, 3_000.0),
                (Resonance, 0.3),
                (Drive, 0.3),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.85),
                (FenvAttack, 0.01),
                (FenvDecay, 0.3),
                (FenvSustain, 0.6),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.67),
                (Vibrato, 0.15),
                (ModWheel, 0.7),
                (Glide, 0.35),
                (Legato, 1.0),
                (Priority, 1.0),
            ],
        ),
        // Three oscillators down at 16' and 32' into a low, resonant
        // filter that a short contour snaps open: a plucked funk bass.
        PresetDef::of(
            Preset::FunkBass,
            &[
                (Vco1Coarse, -12.0),
                (Vco2Wave, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Level, 0.9),
                (Vco3Coarse, -24.0),
                (Vco3Level, 0.6),
                (Cutoff, 300.0),
                (Resonance, 0.45),
                (Drive, 0.4),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.22),
                (AdsrSustain, 0.6),
                (FenvAttack, 0.001),
                (FenvDecay, 0.18),
                (FenvSustain, 0.0),
                (EnvCutoff, 0.8),
                (KeyTrack, 0.67),
                (Priority, 1.0),
            ],
        ),
        // Three saws a few cents apart, a slow attack and a decay that is
        // also a long release: the Minimoog as a string section.
        PresetDef::of(
            Preset::MoogStrings,
            &[
                (Vco2Fine, 8.0),
                (Vco2Level, 0.8),
                (Vco3Fine, -8.0),
                (Vco3Level, 0.8),
                (Cutoff, 1_600.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.4),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.9),
                (FenvAttack, 0.5),
                (FenvDecay, 0.6),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.25),
                (KeyTrack, 0.67),
            ],
        ),
    ],
    ..ModelDef::MONO
};
