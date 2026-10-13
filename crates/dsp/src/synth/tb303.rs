//! The Roland TB-303 (#536): one oscillator into a diode ladder, a decay
//! for the filter, and the accent and slide its basslines are made of.

use crate::mono::model::{CLEAN_VCA, DISCRETE_VCO, Filter, RC_ENV, TB303};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    osc: DISCRETE_VCO,
    vca: CLEAN_VCA,
    env: RC_ENV,
    filter: Filter::Ladder(TB303),
    // Its filter envelope is a decay from each note, and an accent charges
    // the sweep circuit with it.
    accent_sweep: true,
    presets: &[
        // A saw, the ladder half open and resonant, a short decay: the
        // squelch comes from the accents, the glide from the slides.
        PresetDef::of(
            Preset::Tb303Acid,
            &[
                (Analog, 0.3),
                (Cutoff, 1_200.0),
                (Resonance, 0.8),
                (EnvCutoff, 0.55),
                (FenvAttack, 0.003),
                (FenvDecay, 0.35),
                (FenvSustain, 0.0),
                (FenvRelease, 0.35),
                (AdsrAttack, 0.003),
                (AdsrDecay, 3.0),
                (AdsrSustain, 0.0),
                (AdsrRelease, 0.008),
                (Accent, 0.6),
                (Glide, 0.06),
                (Legato, 1.0),
            ],
        ),
        // The resonance up to the edge of the whistle and a long decay:
        // every note sweeps, the accents shriek.
        PresetDef::of(
            Preset::Tb303Squelch,
            &[
                (Analog, 0.3),
                (Cutoff, 900.0),
                (Resonance, 0.92),
                (EnvCutoff, 0.75),
                (FenvAttack, 0.003),
                (FenvDecay, 0.9),
                (FenvSustain, 0.0),
                (FenvRelease, 0.9),
                (AdsrAttack, 0.003),
                (AdsrDecay, 3.0),
                (AdsrSustain, 0.0),
                (AdsrRelease, 0.008),
                (Accent, 0.8),
                (Glide, 0.06),
                (Legato, 1.0),
            ],
        ),
        // The square, the filter low and barely resonant: a round, hollow
        // bass under a track rather than on top of it.
        PresetDef::of(
            Preset::Tb303Square,
            &[
                (Analog, 0.3),
                (Vco1Wave, 1.0),
                (Cutoff, 700.0),
                (Resonance, 0.35),
                (KeyTrack, 0.5),
                (EnvCutoff, 0.4),
                (FenvAttack, 0.003),
                (FenvDecay, 0.4),
                (FenvSustain, 0.0),
                (FenvRelease, 0.4),
                (AdsrAttack, 0.003),
                (AdsrDecay, 3.0),
                (AdsrSustain, 0.0),
                (AdsrRelease, 0.008),
                (Accent, 0.4),
                (Glide, 0.06),
                (Legato, 1.0),
            ],
        ),
    ],
    ..ModelDef::MONO
};
