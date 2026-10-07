//! The drum/pad sampler: what the model is and its presets (#330).

use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{Engine, ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    voices: 16,
    engine: Engine::Pads,
    presets: &[
        // The kit at full level; the pads' own settings are not parameters (`padsampler`).
        PresetDef::of(
            Preset::PadsLoud,
            &[
                (Model, 17.0),
                (Polyphony, 16.0),
                (Analog, 0.0),
                (Vco1Level, 1.0),
            ],
        ),
        PresetDef::of(
            Preset::PadsSoft,
            &[
                (Model, 17.0),
                (Polyphony, 16.0),
                (Analog, 0.0),
                (Vco1Level, 0.6),
            ],
        ),
    ],
    ..ModelDef::MONO
};
