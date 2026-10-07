//! The Roland SH-101: what the model is and its presets (#330).

use crate::mono::model::CEM_VCO;
use crate::mono::model::{Filter, SH101};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    osc: CEM_VCO,
    filter: Filter::Ladder(SH101),
    // One oscillator gives saw and pulse; one envelope for filter and VCA.
    filter_env_is_adsr: true,
    pulse_locked: true,
    cutoff_follows_filter_env: false,
    presets: &[
        // Saw and pulse together, and a sub two octaves down, through a
        // resonant low-pass that the one envelope opens on every note.
        PresetDef::of(
            Preset::Sh101Bass,
            &[
                (Analog, 0.4),
                (Vco1Level, 0.6),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.6),
                (SubLevel, 0.9),
                (SubOctave, 1.0),
                (Cutoff, 500.0),
                (Resonance, 0.45),
                (Drive, 0.3),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.3),
                (AdsrRelease, 0.15),
                (EnvCutoff, 0.6),
                (KeyTrack, 0.5),
            ],
        ),
        // Saw with a pulse whose width the LFO moves, a sub an octave
        // down, vibrato on the wheel, glide: the 101 lead.
        PresetDef::of(
            Preset::Sh101Lead,
            &[
                (Analog, 0.4),
                (Vco1Level, 1.0),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.8),
                (SubLevel, 0.4),
                (Cutoff, 2_500.0),
                (Resonance, 0.35),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.3),
                (KeyTrack, 0.5),
                (LfoWave, 2.0),
                (LfoRate, 5.5),
                (LfoPw, 0.3),
                (Vibrato, 0.12),
                (ModWheel, 1.0),
                (Glide, 0.1),
                (Legato, 1.0),
            ],
        ),
        // A saw into a snapping, near-self-oscillating filter, with a
        // short slide between legato notes: an acid bassline.
        PresetDef::of(
            Preset::AcidBass,
            &[
                (Analog, 0.4),
                (Cutoff, 350.0),
                (Resonance, 0.85),
                (Drive, 0.4),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.5),
                (AdsrRelease, 0.1),
                (EnvCutoff, 0.8),
                (KeyTrack, 0.3),
                (Glide, 0.07),
                (Legato, 1.0),
            ],
        ),
        // The sub-oscillator carrying a short pluck of a low-pass sweep.
        PresetDef::of(
            Preset::SubPluck,
            &[
                (Analog, 0.4),
                (Vco1Level, 0.6),
                (SubLevel, 1.0),
                (Cutoff, 900.0),
                (Resonance, 0.2),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.8),
                (AdsrSustain, 0.1),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.7),
                (KeyTrack, 0.5),
            ],
        ),
        // Saw and a pulse whose width the LFO moves, a sub, a high-pass and
        // a slow attack that opens the filter with it.
        PresetDef::of(
            Preset::Sh101Strings,
            &[
                (Analog, 0.4),
                (Vco1Level, 0.7),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.7),
                (SubLevel, 0.4),
                (Cutoff, 2_600.0),
                (Resonance, 0.15),
                (AdsrAttack, 0.4),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (EnvCutoff, 0.15),
                (LfoWave, 2.0),
                (LfoRate, 0.5),
                (LfoPw, 0.4),
            ],
        ),
    ],
    ..ModelDef::MONO
};
