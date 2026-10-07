//! The Roland Juno-106: what the model is and its presets (#330).

use crate::mono::model::{EnvVoicing, Filter, Hp, JUNO106, LOCKED, RC_ENV};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    osc: LOCKED,
    // Written by the CPU, not an RC circuit (#340): a near-linear attack,
    // 1.5 ms to 3 s, decay and release from 1.5 ms.
    env: EnvVoicing {
        attack_aim: 3.0,
        attack: [0.0015, 3.0],
        decay: [0.0015, 10.0],
        release: [0.0015, 10.0],
        ..RC_ENV
    },
    voices: 6,
    filter: Filter::Ladder(JUNO106),
    hp: Hp::OnePole,
    filter_env_is_adsr: true,
    pulse_locked: true,
    cutoff_follows_filter_env: false,
    presets: &[
        // A saw and a pulse that the LFO sweeps, a sub, a slow swell and chorus II:
        // the lush Juno pad.
        PresetDef::of(
            Preset::JunoPad,
            &[
                (Polyphony, 6.0),
                (Analog, 0.5),
                (ChorusMode, 2.0),
                (Vco1Level, 0.6),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.6),
                (SubLevel, 0.3),
                (LfoWave, 2.0),
                (LfoRate, 0.6),
                (LfoPw, 0.4),
                (Cutoff, 1_800.0),
                (Resonance, 0.15),
                (AdsrAttack, 0.5),
                (AdsrDecay, 0.7),
                (AdsrSustain, 0.85),
                (AdsrRelease, 0.9),
                (EnvCutoff, 0.15),
                (KeyTrack, 0.5),
            ],
        ),
        // Saw and a swept pulse with both chorus buttons in: the fast, shallow
        // shimmer of the Juno strings.
        PresetDef::of(
            Preset::JunoStrings,
            &[
                (Polyphony, 6.0),
                (Analog, 0.5),
                (ChorusMode, 3.0),
                (Vco1Level, 0.9),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.5),
                (LfoWave, 2.0),
                (LfoRate, 0.6),
                (LfoPw, 0.3),
                (Cutoff, 3_000.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.25),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.5),
                (EnvCutoff, 0.1),
                (KeyTrack, 0.5),
            ],
        ),
        // A saw, a pulse and a sub through a filter the one envelope sweeps on
        // every note, with chorus I: the Juno brass.
        PresetDef::of(
            Preset::JunoBrass,
            &[
                (Polyphony, 6.0),
                (Analog, 0.4),
                (ChorusMode, 1.0),
                (Vco1Level, 1.0),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.6),
                (SubLevel, 0.3),
                (Cutoff, 900.0),
                (Resonance, 0.3),
                (AdsrAttack, 0.05),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.6),
                (KeyTrack, 0.5),
            ],
        ),
        // The sub-oscillator at full level under a saw and a pulse, no chorus:
        // the Juno bass.
        PresetDef::of(
            Preset::JunoBass,
            &[
                (Polyphony, 6.0),
                (Analog, 0.3),
                (Vco1Level, 0.7),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.5),
                (SubLevel, 1.0),
                (Cutoff, 600.0),
                (Resonance, 0.3),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.35),
                (AdsrSustain, 0.5),
                (AdsrRelease, 0.2),
                (EnvCutoff, 0.5),
                (KeyTrack, 0.5),
            ],
        ),
        // A narrow pulse with PWM and a filter that closes quickly, chorus I: a
        // plucked Juno.
        PresetDef::of(
            Preset::JunoPluck,
            &[
                (Polyphony, 6.0),
                (Analog, 0.4),
                (ChorusMode, 1.0),
                (Vco1Level, 0.0),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.9),
                (SubLevel, 0.3),
                (PulseWidth, 0.3),
                (LfoWave, 2.0),
                (LfoRate, 1.2),
                (LfoPw, 0.35),
                (Cutoff, 1_500.0),
                (Resonance, 0.4),
                (AdsrAttack, 0.0015),
                (AdsrDecay, 0.45),
                (AdsrSustain, 0.0),
                (AdsrRelease, 0.3),
                (EnvCutoff, 0.55),
                (KeyTrack, 0.5),
            ],
        ),
        // A bright saw with a little resonance and the high-pass in its first step,
        // chorus II: the classic poly-synth chord sound.
        PresetDef::of(
            Preset::JunoPoly,
            &[
                (Polyphony, 6.0),
                (Analog, 0.5),
                (ChorusMode, 2.0),
                (Vco1Level, 1.0),
                (HpCutoff, 240.0),
                (Cutoff, 2_200.0),
                (Resonance, 0.45),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.6),
                (AdsrRelease, 0.3),
                (EnvCutoff, 0.3),
                (KeyTrack, 0.5),
            ],
        ),
    ],
    ..ModelDef::MONO
};
