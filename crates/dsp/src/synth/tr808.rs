//! The Roland TR-808: what the model is and its presets (#330).

use crate::drums::Machine;
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{Engine, ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    engine: Engine::Drums(Machine::Tr808),
    presets: &[
        // The kit as it comes: every pad at its own decay and a middle tone.
        PresetDef::of(Preset::Kit808, &[]),
        // Short and bright: clipped kick and toms, a snappy snare, closed-in hats.
        PresetDef::of(
            Preset::TightKit,
            &[
                (BdDecay, 0.45),
                (BdTone, 0.7),
                (SnDecay, 0.6),
                (SnTone, 0.85),
                (CpDecay, 0.6),
                (ChDecay, 0.6),
                (ChTone, 0.8),
                (OhDecay, 0.5),
                (OhTone, 0.8),
                (LtDecay, 0.6),
                (HtDecay, 0.6),
                (CbDecay, 0.5),
                (DrumAccent, 0.7),
            ],
        ),
        // Deep and heavy (#264): a kick tuned down to about 44 Hz, long, driven
        // and at full level, with a louder, snappier snare and clap.
        PresetDef::of(
            Preset::Heavy808,
            &[
                (BdTune, -2.0),
                (BdDecay, 1.6),
                (BdTone, 0.7),
                (BdLevel, 1.0),
                (BdDrive, 0.6),
                (SnTone, 0.7),
                (SnLevel, 1.0),
                (CpLevel, 0.9),
                (DrumAccent, 0.7),
            ],
        ),
    ],
    ..ModelDef::MONO
};
