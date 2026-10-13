//! The NES's Ricoh 2A03 (#112): what the model is and its presets. Each
//! note plays a whole chip (`chip::nes_voice`); at most eight at once.

use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{Engine, ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    voices: 8,
    engine: Engine::Nes,
    presets: &[
        // A 25 % pulse over a quieter 50 % one a few cents off, with a late
        // vibrato: a platformer's lead.
        PresetDef::of(
            Preset::NesLead,
            &[
                (Polyphony, 1.0),
                (NesP1Duty, 1.0),
                (NesP1Volume, 10.0),
                (NesP2On, 1.0),
                (NesP2Duty, 2.0),
                (NesP2Volume, 4.0),
                (NesP2Fine, 8.0),
                (NesVibrato, 0.2),
                (NesVibratoRate, 6.0),
                (NesVibratoDelay, 20.0),
            ],
        ),
        // The triangle alone: the NES's bass, no volume, all body.
        PresetDef::of(
            Preset::NesBass,
            &[(Polyphony, 1.0), (NesP1On, 0.0), (NesTriOn, 1.0)],
        ),
        // A decaying 50 % pulse, each note a major arpeggio stepped every
        // frame.
        PresetDef::of(
            Preset::NesArp,
            &[
                (Polyphony, 3.0),
                (NesP1Duty, 2.0),
                (NesP1Volume, 4.0),
                (NesP1Decay, 1.0),
                (NesArp, 1.0),
            ],
        ),
        // The drums NES scores make of the channels, from General MIDI keys:
        // for a drum track.
        PresetDef::of(
            Preset::NesKit,
            &[(Polyphony, 8.0), (NesP1On, 0.0), (NesKit, 1.0)],
        ),
    ],
    ..ModelDef::MONO
};
