//! The Korg MS-20: what the model is and its presets (#330).

use crate::mono::model::{Filter, Hp, MS20};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    filter: Filter::Svf(MS20),
    hp: Hp::Svf,
    presets: &[
        // Two saws through a thin high-pass and a peaking low-pass that
        // the envelope pushes toward self-oscillation: the MS-20 scream.
        PresetDef::of(
            Preset::Ms20Lead,
            &[
                (Vco2Fine, 8.0),
                (Vco2Level, 0.6),
                (HpCutoff, 120.0),
                (HpResonance, 0.3),
                (Cutoff, 1_800.0),
                (Resonance, 0.78),
                (Drive, 0.3),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.25),
                (FenvAttack, 0.01),
                (FenvDecay, 0.5),
                (FenvSustain, 0.4),
                (EnvCutoff, 0.35),
                (EnvHpCutoff, 0.1),
                (KeyTrack, 0.5),
                (Glide, 0.08),
                (Legato, 1.0),
            ],
        ),
        // A pulse and a saw an octave down, ring-modulated, with the
        // sample-and-hold stepping the cutoff on the patch panel.
        PresetDef::of(
            Preset::Ms20Wobble,
            &[
                (Vco1Wave, 1.0),
                (Vco1Coarse, -12.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 6.0),
                (Vco2Level, 0.7),
                (RingLevel, 0.35),
                (HpCutoff, 60.0),
                (Cutoff, 500.0),
                (Resonance, 0.85),
                (Drive, 0.2),
                (AdsrAttack, 0.003),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.2),
                (LfoWave, 0.0),
                (LfoRate, 6.0),
                (Patch1Source, 8.0),
                (Patch1Dest, 5.0),
                (Patch1Amount, 0.5),
            ],
        ),
        // A low saw into a nearly self-oscillating low-pass that a short
        // envelope throws open: the squelch.
        PresetDef::of(
            Preset::Ms20Squelch,
            &[
                (Vco1Coarse, -12.0),
                (HpCutoff, 60.0),
                (Cutoff, 400.0),
                (Resonance, 0.9),
                (Drive, 0.5),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.6),
                (AdsrRelease, 0.1),
                (FenvAttack, 0.001),
                (FenvDecay, 0.22),
                (FenvSustain, 0.05),
                (EnvCutoff, 0.85),
                (KeyTrack, 0.3),
                (Glide, 0.06),
                (Legato, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 4.0),
                (Vco2Level, 0.8),
            ],
        ),
        // Noise through the high-pass and a resonant low-pass that a very
        // slow LFO sweeps: a jet going over.
        PresetDef::of(
            Preset::JetSweep,
            &[
                (Vco1Level, 0.0),
                (NoiseLevel, 1.0),
                (HpCutoff, 300.0),
                (HpResonance, 0.5),
                (Cutoff, 1_200.0),
                (Resonance, 0.7),
                (AdsrAttack, 1.2),
                (AdsrDecay, 0.5),
                (AdsrSustain, 1.0),
                (AdsrRelease, 1.0),
                (LfoWave, 3.0),
                (LfoRate, 0.15),
                (LfoCutoff, 0.9),
            ],
        ),
        // Two detuned saws through a gentle high-pass and low-pass, a slow
        // attack, vibrato on the wheel.
        PresetDef::of(
            Preset::Ms20Strings,
            &[
                (Vco2Fine, 10.0),
                (Vco2Level, 0.8),
                (HpCutoff, 200.0),
                (HpResonance, 0.1),
                (Cutoff, 2_400.0),
                (Resonance, 0.25),
                (AdsrAttack, 0.35),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (FenvAttack, 0.5),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.2),
                (LfoWave, 3.0),
                (LfoRate, 5.0),
                (Vibrato, 0.12),
                (ModWheel, 1.0),
            ],
        ),
    ],
    ..ModelDef::MONO
};
