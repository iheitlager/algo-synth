//! The multisampler: what the model is and its presets (#330).

use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{Engine, ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    voices: 16,
    engine: Engine::Sampler,
    presets: &[
        // The filter open and the envelope out of the way: the sample as recorded.
        PresetDef::of(
            Preset::SamplerKeys,
            &[
                (Model, 16.0),
                (Polyphony, 16.0),
                (Analog, 0.0),
                (Cutoff, 20_000.0),
                (Resonance, 0.0),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.3),
                (AdsrSustain, 1.0),
                (AdsrRelease, 0.25),
                (FenvAttack, 0.001),
                (FenvDecay, 0.3),
                (FenvSustain, 1.0),
                (EnvCutoff, 0.0),
                (KeyTrack, 0.0),
            ],
        ),
        // A slow swell and a long fall, the filter closing with the note.
        PresetDef::of(
            Preset::SamplerPad,
            &[
                (Model, 16.0),
                (Polyphony, 16.0),
                (Analog, 0.0),
                (Cutoff, 6_000.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.4),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.9),
                (AdsrRelease, 1.2),
                (FenvAttack, 0.5),
                (FenvDecay, 1.0),
                (FenvSustain, 0.6),
                (EnvCutoff, 0.4),
                (KeyTrack, 0.5),
            ],
        ),
    ],
    ..ModelDef::MONO
};
