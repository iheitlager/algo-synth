//! The ARP 2600: what the model is and its presets (#330).

use crate::mono::model::{CLEAN_VCA, DISCRETE_VCO, Filter, MOOG, VcaVoicing};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    osc: DISCRETE_VCO,
    // The 4019's exponential control input, normalled from the ADSR, so a
    // decay falls evenly in decibels (#341).
    vca: VcaVoicing {
        expo: true,
        ..CLEAN_VCA
    },
    filter: Filter::Ladder(MOOG),
    // Its ADSR is normalled to filter and VCA (spec 004 Req 12).
    cutoff_follows_filter_env: false,
    presets: &[
        // Saw and a pulse an octave down, into a low, resonant ladder.
        PresetDef::of(
            Preset::Bass,
            &[
                (Analog, 0.4),
                (Vco2Wave, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Level, 0.7),
                (PulseWidth, 0.35),
                (Cutoff, 600.0),
                (Resonance, 0.35),
                (Drive, 0.4),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.885),
                (AdsrRelease, 0.12),
                // The filter opens with each note and follows the key.
                (EnvCutoff, 0.4),
                (KeyTrack, 0.5),
            ],
        ),
        // Two saws a few cents apart and a sub saw, a medium cutoff.
        PresetDef::of(
            Preset::Lead,
            &[
                (Analog, 0.4),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.8),
                (Vco3Coarse, -12.0),
                (Vco3Level, 0.3),
                (Cutoff, 2_500.0),
                (Resonance, 0.3),
                (Drive, 0.2),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.958),
                (AdsrRelease, 0.3),
                (EnvCutoff, 0.25),
                (KeyTrack, 0.5),
                // A little vibrato, the wheel half up.
                (Vibrato, 0.15),
                (ModWheel, 0.5),
            ],
        ),
        // VCO 2 hard-synced to a silent VCO 1, an octave and a fifth up.
        // The ADSR sweeps VCO 2's pitch: the classic sync sweep.
        PresetDef::of(
            Preset::SyncLead,
            &[
                (Analog, 0.4),
                (Vco1Level, 0.0),
                (Vco2Coarse, 19.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (Cutoff, 5_000.0),
                (Resonance, 0.2),
                (Drive, 0.3),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.926),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.3),
                (Patch1Source, 5.0),
                (Patch1Dest, 2.0),
                (Patch1Amount, 0.5),
            ],
        ),
        // Three detuned saws and a breath of pink noise for the bow,
        // a slow attack and a long release: the ensemble's start.
        PresetDef::of(
            Preset::BowedString,
            &[
                (Analog, 0.4),
                (Vco1Level, 0.8),
                (Vco2Fine, 6.0),
                (Vco2Level, 0.7),
                (Vco3Fine, -5.0),
                (Vco3Level, 0.6),
                (NoiseColour, 1.0),
                (NoiseLevel, 0.05),
                (Cutoff, 1_800.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.25),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.976),
                (AdsrRelease, 0.6),
                // Vibrato at 5.5 Hz, the wheel all the way up; bow pressure
                // (velocity) brightens the tone.
                (LfoRate, 5.5),
                (Vibrato, 0.1),
                (ModWheel, 1.0),
                (KeyTrack, 0.6),
                (EnvCutoff, 0.15),
                (Patch1Source, 10.0),
                (Patch1Dest, 5.0),
                (Patch1Amount, 0.2),
            ],
        ),
        // Fast sample-and-hold steps on both pitches and the cutoff, a
        // thin pulse and a sine an octave up: the chirping robot.
        PresetDef::of(
            Preset::R2D2,
            &[
                (Analog, 0.4),
                (Vco1Wave, 1.0),
                (Vco1Level, 1.0),
                (PulseWidth, 0.3),
                (Vco2Wave, 3.0),
                (Vco2Coarse, 12.0),
                (Vco2Level, 0.8),
                (Cutoff, 3_500.0),
                (Resonance, 0.5),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.12),
                (AdsrSustain, 0.968),
                (AdsrRelease, 0.1),
                (LfoWave, 0.0),
                (LfoRate, 9.0),
                (Patch1Source, 8.0),
                (Patch1Dest, 1.0),
                (Patch1Amount, 0.5),
                (Patch2Source, 8.0),
                (Patch2Dest, 2.0),
                (Patch2Amount, 0.7),
                (Patch3Source, 8.0),
                (Patch3Dest, 5.0),
                (Patch3Amount, 0.3),
            ],
        ),
        // Two saws stepping to random pitches at the LFO's rate, through a
        // resonant filter the envelope opens: the sample-and-hold arpeggio.
        PresetDef::of(
            Preset::ShArp,
            &[
                (Analog, 0.4),
                (Vco2Fine, 6.0),
                (Vco2Level, 0.8),
                (Cutoff, 1_800.0),
                (Resonance, 0.55),
                (Drive, 0.2),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.968),
                (AdsrRelease, 0.15),
                (EnvCutoff, 0.3),
                (KeyTrack, 0.5),
                (LfoWave, 0.0),
                (LfoRate, 5.5),
                (Patch1Source, 8.0),
                (Patch1Dest, 1.0),
                (Patch1Amount, 0.5),
                (Patch2Source, 8.0),
                (Patch2Dest, 2.0),
                (Patch2Amount, 0.5),
            ],
        ),
        // Two detuned saws and a pulse whose width a slow LFO moves, a
        // slow attack and a long release: an ensemble's shimmer.
        PresetDef::of(
            Preset::SolinaStrings,
            &[
                (Analog, 0.4),
                (Vco1Level, 0.8),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.7),
                (Vco3Wave, 1.0),
                (Vco3Level, 0.5),
                (Cutoff, 2_800.0),
                (Resonance, 0.05),
                (AdsrAttack, 0.5),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.985),
                (AdsrRelease, 0.7),
                (EnvCutoff, 0.1),
                (KeyTrack, 0.5),
                (LfoWave, 2.0),
                (LfoRate, 0.7),
                (LfoPw, 0.35),
            ],
        ),
    ],
    ..ModelDef::MONO
};
