//! The Sequential Prophet-5: what the model is and its presets (#330).

use crate::mono::model::{
    CA3280, CEM_VCO, CEM3310, Filter, PROPHET5_REV3, PROPHET5_REV12, SSM_VCO, SSM2050,
};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef, RevisionDef};

/// Rev 1 and 2 share the SSM chips and differ in stability: the hand-built
/// Rev 1 drifts most. Rev 3 has the Curtis chips; the Rev 4 is Rev 3's
/// voice, held stable (its Vintage knob at 4).
const REVISIONS: [RevisionDef; 4] = [
    RevisionDef {
        vco: 1,
        filter: 1,
        env: 1,
        analog: 0.8,
    },
    RevisionDef {
        vco: 1,
        filter: 1,
        env: 1,
        analog: 0.6,
    },
    RevisionDef {
        vco: 3,
        filter: 3,
        env: 3,
        analog: 0.4,
    },
    RevisionDef {
        vco: 3,
        filter: 3,
        env: 3,
        analog: 0.1,
    },
];

pub const DEF: ModelDef = ModelDef {
    // Rev 3's parts are its own; Rev 1/2's SSM chips are the switches'.
    osc: CEM_VCO,
    osc_revs: [Some(SSM_VCO); 2],
    env: CEM3310,
    env_revs: [Some(SSM2050); 2],
    // No source tells the SSM2020 of Rev 1/2 apart: one VCA for all.
    vca: CA3280,
    voices: 5,
    filter: Filter::Ladder(PROPHET5_REV3),
    // The Rev 4's switch: the SSM2040 of Rev 1/2.
    revs: [Some(Filter::Ladder(PROPHET5_REV12)); 2],
    revisions: Some(&REVISIONS),
    presets: &[
        // Two saws a few cents apart, the filter opened by its envelope on every
        // note and followed by the key: the Prophet brass. Five voices, a little drift.
        PresetDef::of(
            Preset::P5Brass,
            &[
                (Polyphony, 5.0),
                (Analog, 0.5),
                (Vco1Level, 0.9),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.9),
                (Cutoff, 1_200.0),
                (Resonance, 0.25),
                (AdsrAttack, 0.06),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.25),
                (FenvAttack, 0.08),
                (FenvDecay, 0.5),
                (FenvSustain, 0.6),
                (FenvRelease, 0.3),
                (EnvCutoff, 0.55),
                (KeyTrack, 1.0),
            ],
        ),
        // A saw and a pulse whose width a slow LFO sweeps, a slow attack and a long
        // release: the Prophet string pad.
        PresetDef::of(
            Preset::P5Strings,
            &[
                (Polyphony, 5.0),
                (Analog, 0.6),
                (Vco1Level, 0.8),
                (Vco1Fine, 6.0),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.8),
                (LfoWave, 2.0),
                (LfoRate, 0.5),
                (LfoPw, 0.3),
                (Cutoff, 2_400.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.4),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (FenvAttack, 0.4),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
            ],
        ),
        // Unison on all five voices, two saws an octave down, a snapping filter
        // envelope: a fat unison bass.
        PresetDef::of(
            Preset::P5Bass,
            &[
                (Polyphony, 5.0),
                (Assign, 1.0),
                (UnisonDetune, 0.12),
                (Analog, 0.4),
                (Vco1Coarse, -12.0),
                (Vco1Level, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 4.0),
                (Vco2Level, 0.9),
                (Cutoff, 500.0),
                (Resonance, 0.35),
                (Drive, 0.3),
                (AdsrAttack, 0.003),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.15),
                (FenvAttack, 0.003),
                (FenvDecay, 0.3),
                (FenvSustain, 0.15),
                (EnvCutoff, 0.6),
                (KeyTrack, 1.0),
            ],
        ),
        // A synced to B with the filter envelope sweeping A, in unison with a
        // little spread: a Prophet sync lead.
        PresetDef::of(
            Preset::P5SyncLead,
            &[
                (Polyphony, 5.0),
                (Assign, 1.0),
                (UnisonDetune, 0.1),
                (Analog, 0.4),
                (Vco1Level, 0.0),
                (Vco2Coarse, 12.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (Cutoff, 3_500.0),
                (Resonance, 0.2),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.3),
                (FenvAttack, 0.005),
                (FenvDecay, 0.7),
                (FenvSustain, 0.3),
                (EnvFreq2, 0.35),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
            ],
        ),
        // B frequency-modulating A through poly-mod at an inharmonic ratio, the
        // sound dying with the loudness: a poly-mod bell, five voices of it.
        PresetDef::of(
            Preset::P5Bell,
            &[
                (Polyphony, 5.0),
                (Analog, 0.3),
                (Vco1Wave, 3.0),
                (Vco1Coarse, 22.0),
                (Vco1Level, 0.0),
                (Vco2Wave, 3.0),
                (Vco2Level, 1.0),
                (OscFreq2, 0.9),
                (Cutoff, 6_000.0),
                (AdsrAttack, 0.002),
                (AdsrDecay, 1.6),
                (AdsrSustain, 0.0),
                (AdsrRelease, 1.2),
                (KeyTrack, 1.0),
            ],
        ),
        // Two detuned saws, a slow swell and the LFO breathing the filter: a warm pad.
        PresetDef::of(
            Preset::P5Pad,
            &[
                (Polyphony, 5.0),
                (Analog, 0.7),
                (Vco1Level, 0.8),
                (Vco1Fine, -8.0),
                (Vco2Fine, 8.0),
                (Vco2Level, 0.8),
                (Cutoff, 1_400.0),
                (Resonance, 0.2),
                (AdsrAttack, 0.8),
                (AdsrDecay, 0.8),
                (AdsrSustain, 0.85),
                (AdsrRelease, 1.0),
                (FenvAttack, 0.9),
                (FenvSustain, 0.7),
                (EnvCutoff, 0.25),
                (LfoWave, 3.0),
                (LfoRate, 0.3),
                (LfoCutoff, 0.3),
                (KeyTrack, 0.5),
            ],
        ),
    ],
    ..ModelDef::MONO
};
