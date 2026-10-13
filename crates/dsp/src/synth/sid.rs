//! The MOS 6581/8580 SID (#112): what the model is and its presets. Each
//! note plays a whole chip (`chip::sid_voice`); at most eight at once.

use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{Engine, ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    voices: 8,
    engine: Engine::Sid,
    presets: &[
        // A pulse lead whose width sweeps every frame, through the 6581's
        // resonant low-pass, with a delayed vibrato: the lead of a mid-80s
        // game score.
        PresetDef::of(
            Preset::SidLead,
            &[
                (Polyphony, 1.0),
                (Sid1Wave, 4.0),
                (Sid1Pw, 1024.0),
                (Sid1PwSweep, 24.0),
                (Sid1Attack, 0.0),
                (Sid1Decay, 9.0),
                (Sid1Sustain, 10.0),
                (Sid1Release, 8.0),
                (Sid1Filter, 1.0),
                (SidCutoff, 900.0),
                (SidResonance, 8.0),
                (SidVibrato, 0.25),
                (SidVibratoRate, 6.0),
                (SidVibratoDelay, 15.0),
            ],
        ),
        // A pulse bass under a closing resonant low-pass, a triangle an
        // octave down unfiltered under it.
        PresetDef::of(
            Preset::SidBass,
            &[
                (Polyphony, 1.0),
                (Sid1Wave, 4.0),
                (Sid1Pw, 1536.0),
                (Sid1Decay, 6.0),
                (Sid1Sustain, 4.0),
                (Sid1Release, 3.0),
                (Sid1Filter, 1.0),
                (Sid2Wave, 1.0),
                (Sid2Coarse, -12.0),
                (Sid2Decay, 7.0),
                (Sid2Sustain, 8.0),
                (Sid2Release, 3.0),
                (SidCutoff, 1400.0),
                (SidResonance, 10.0),
                (SidCutoffSweep, -24.0),
            ],
        ),
        // The chip's three voices, each note a major-chord arpeggio stepped
        // every frame: the C64's chords.
        PresetDef::of(
            Preset::SidArp,
            &[
                (Polyphony, 3.0),
                (Sid1Wave, 4.0),
                (Sid1Pw, 2048.0),
                (Sid1PwSweep, 16.0),
                (Sid1Decay, 8.0),
                (Sid1Sustain, 8.0),
                (Sid1Release, 6.0),
                (SidArp, 1.0),
                (SidArpSpeed, 1.0),
            ],
        ),
        // A saw synced to a silent oscillator 3 an octave and a fifth down,
        // and a ring-modulated triangle: the 8580's bright sync lead.
        PresetDef::of(
            Preset::SidSync,
            &[
                (Polyphony, 1.0),
                (SidChip, 1.0),
                (Sid1Wave, 2.0),
                (Sid1Coarse, 7.0),
                (Sid1Sync, 1.0),
                (Sid1Sustain, 12.0),
                (Sid2Wave, 1.0),
                (Sid2Coarse, 5.0),
                (Sid2Ring, 1.0),
                (Sid2Sustain, 12.0),
                (Sid3Coarse, -12.0),
                (SidFilterMode, 0.0),
                (SidVibrato, 0.15),
            ],
        ),
    ],
    ..ModelDef::MONO
};
