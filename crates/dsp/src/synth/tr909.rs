//! The Roland TR-909: what the model is and its presets (#330).

use crate::drums::Machine;
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{Engine, ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    engine: Engine::Drums(Machine::Tr909),
    presets: &[
        // The 909 as it comes.
        PresetDef::of(Preset::Kit909, &[]),
        // Hard and bright: a driven kick with a loud click, a snappy snare,
        // short toms, crisp hats and a long ride.
        PresetDef::of(
            Preset::Hard909,
            &[
                (BdTone, 0.9),
                (BdDecay, 0.7),
                (SnTone, 0.9),
                (SnDecay, 0.8),
                (LtDecay, 0.7),
                (MtDecay, 0.7),
                (HtDecay, 0.7),
                (ChTone, 0.8),
                (OhTone, 0.8),
                (RdDecay, 1.3),
                (DrumAccent, 0.8),
            ],
        ),
        // The 909 the same way: a kick at about 46 Hz, longer and driven harder.
        PresetDef::of(
            Preset::Heavy909,
            &[
                (BdTune, -2.0),
                (BdDecay, 1.3),
                (BdTone, 0.8),
                (BdLevel, 1.0),
                (BdDrive, 0.5),
                (SnTone, 0.8),
                (SnLevel, 1.0),
                (CpLevel, 0.9),
                (DrumAccent, 0.7),
            ],
        ),
    ],
    ..ModelDef::MONO
};
