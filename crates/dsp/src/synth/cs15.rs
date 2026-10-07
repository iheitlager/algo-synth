//! The Yamaha CS-15: what the model is and its presets (#330).

use crate::mono::model::DISCRETE_VCO;
use crate::mono::model::{CS15, Filter, Hp};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    osc: DISCRETE_VCO,
    filter: Filter::Svf(CS15),
    hp: Hp::Svf,
    hp_follows_ar: true,
    presets: &[
        // Two saws a few cents apart; the low-pass swells with its
        // envelope as a brass note does, the high-pass opens a little
        // with the AR: the CS-15's brass.
        PresetDef::of(
            Preset::Cs15Brass,
            &[
                (Analog, 0.4),
                (Vco2Fine, 9.0),
                (Vco2Level, 0.9),
                (HpCutoff, 150.0),
                (Cutoff, 900.0),
                (Resonance, 0.35),
                (AdsrAttack, 0.08),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.85),
                (AdsrRelease, 0.2),
                (FenvAttack, 0.12),
                (FenvDecay, 0.4),
                (FenvSustain, 0.6),
                (EnvCutoff, 0.6),
                (ArAttack, 0.15),
                (EnvHpCutoff, 0.2),
                (KeyTrack, 0.4),
                (LfoWave, 2.0),
                (LfoRate, 5.5),
                (Vibrato, 0.1),
                (ModWheel, 0.3),
            ],
        ),
        // A pulse and a saw an octave up with a little ring mod, a
        // snappy low-pass and glide: a thin, vocal lead.
        PresetDef::of(
            Preset::Cs15Lead,
            &[
                (Analog, 0.4),
                (Vco1Wave, 1.0),
                (PulseWidth, 0.4),
                (Vco2Coarse, 12.0),
                (Vco2Level, 0.5),
                (RingLevel, 0.2),
                (HpCutoff, 300.0),
                (HpResonance, 0.2),
                (Cutoff, 3_000.0),
                (Resonance, 0.4),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.75),
                (AdsrRelease, 0.2),
                (FenvAttack, 0.005),
                (FenvDecay, 0.25),
                (FenvSustain, 0.35),
                (EnvCutoff, 0.4),
                (ArAttack, 0.2),
                (EnvHpCutoff, 0.15),
                (Glide, 0.06),
                (Legato, 1.0),
            ],
        ),
        // Two saws, a slow swell of the low-pass and a little glide, with
        // vibrato: a big, slow brass.
        PresetDef::of(
            Preset::BladeBrass,
            &[
                (Analog, 0.4),
                (Vco2Fine, 8.0),
                (Vco2Level, 0.9),
                (HpCutoff, 100.0),
                (Cutoff, 600.0),
                (Resonance, 0.4),
                (AdsrAttack, 0.2),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (FenvAttack, 0.35),
                (FenvDecay, 0.8),
                (FenvSustain, 0.6),
                (EnvCutoff, 0.75),
                (ArAttack, 0.3),
                (EnvHpCutoff, 0.15),
                (LfoWave, 3.0),
                (LfoRate, 5.0),
                (Vibrato, 0.1),
                (ModWheel, 1.0),
                (Glide, 0.1),
            ],
        ),
        // A saw and a pulse whose width the LFO moves, through the two
        // filters: synth strings.
        PresetDef::of(
            Preset::Cs15Strings,
            &[
                (Analog, 0.4),
                (Vco2Wave, 1.0),
                (Vco2Fine, -6.0),
                (Vco2Level, 0.7),
                (LfoWave, 2.0),
                (LfoRate, 0.7),
                (LfoPw, 0.3),
                (HpCutoff, 180.0),
                (Cutoff, 2_800.0),
                (Resonance, 0.15),
                (AdsrAttack, 0.3),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.5),
                (FenvAttack, 0.3),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.15),
            ],
        ),
    ],
    ..ModelDef::MONO
};
