//! The Casio VL-1, the VL-Tone (#723, ADR-0040): what the model is and its
//! presets. A monophonic calculator-keyboard; its tones are the 8-digit
//! code's waveform with an envelope (`chip::vl_voice`), and `VlKit` is its
//! rhythm unit's Po, Pi and Sha.

use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{Engine, ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    voices: 8,
    engine: Engine::Vl,
    presets: &[
        // The tones as their buttons play them: one note at a time, and the
        // code each would be in ADSR mode.
        PresetDef::of(Preset::VlPiano, &[(Polyphony, 1.0)]),
        // Fantasy: a nasal tone that holds and warbles.
        PresetDef::of(
            Preset::VlFantasy,
            &[
                (Polyphony, 1.0),
                (VlWave, 1.0),
                (VlDecay, 4.0),
                (VlSustain, 6.0),
                (VlSustainTime, 9.0),
                (VlRelease, 5.0),
                (VlTremolo, 3.0),
            ],
        ),
        // Violin: a slow attack on the staircase saw, a late vibrato.
        PresetDef::of(
            Preset::VlViolin,
            &[
                (Polyphony, 1.0),
                (VlWave, 2.0),
                (VlAttack, 4.0),
                (VlDecay, 3.0),
                (VlSustain, 8.0),
                (VlSustainTime, 9.0),
                (VlRelease, 3.0),
                (VlVibrato, 4.0),
            ],
        ),
        // Flute: the square, a soft attack and a little vibrato.
        PresetDef::of(
            Preset::VlFlute,
            &[
                (Polyphony, 1.0),
                (VlWave, 3.0),
                (VlAttack, 2.0),
                (VlDecay, 2.0),
                (VlSustain, 8.0),
                (VlSustainTime, 9.0),
                (VlRelease, 2.0),
                (VlVibrato, 2.0),
            ],
        ),
        // Guitar I: plucked, a longer decay than the piano's.
        PresetDef::of(
            Preset::VlGuitar,
            &[
                (Polyphony, 1.0),
                (VlWave, 4.0),
                (VlDecay, 8.0),
                (VlRelease, 4.0),
            ],
        ),
        // The rhythm unit from the keys: General MIDI drums play Po, Pi
        // and Sha, eight at once so hits overlap.
        PresetDef::of(Preset::VlKit, &[(Polyphony, 8.0), (VlKit, 1.0)]),
    ],
    ..ModelDef::MONO
};
