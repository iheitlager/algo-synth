//! The Sequential Pro-One: what the model is and its presets (#330).

use crate::mono::model::{CEM_VCO, CEM3310, Filter, PRO_ONE};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    // The mod wheel scales the LFO into the filter as into the pitch (#440).
    wheel_scales_filter_mod: true,
    osc: CEM_VCO,
    env: CEM3310,
    filter: Filter::Ladder(PRO_ONE),
    presets: &[
        // Oscillator A (VCO 2) synced to a silent B (VCO 1), a fifth
        // over: the filter envelope sweeps A's pitch, the classic
        // poly-mod sync lead. Low note priority, a little glide.
        PresetDef::of(
            Preset::ProLead,
            &[
                (Analog, 0.4),
                (Vco1Level, 0.0),
                (Vco2Coarse, 12.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (Cutoff, 3_500.0),
                (Resonance, 0.2),
                (Drive, 0.2),
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
                (LfoWave, 2.0),
                (LfoRate, 5.5),
                (Vibrato, 0.15),
                (ModWheel, 0.5),
                (Priority, 1.0),
                (Glide, 0.03),
            ],
        ),
        // A saw an octave down and a narrow pulse (A) whose width the
        // LFO moves; full key tracking, as on the Pro-One.
        PresetDef::of(
            Preset::ProBass,
            &[
                (Analog, 0.4),
                (Vco1Coarse, -12.0),
                (Vco2Wave, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 5.0),
                (Vco2Level, 0.8),
                (PulseWidth, 0.35),
                (LfoWave, 2.0),
                (LfoRate, 0.8),
                (LfoPw, 0.25),
                (Cutoff, 700.0),
                (Resonance, 0.4),
                (Drive, 0.3),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.15),
                (FenvAttack, 0.002),
                (FenvDecay, 0.3),
                (FenvSustain, 0.15),
                (EnvCutoff, 0.5),
                (KeyTrack, 1.0),
                (Priority, 1.0),
            ],
        ),
        // A slow poly-mod sweep of the synced oscillator on every note,
        // from a high start down to the pitch: the sync sweep.
        PresetDef::of(
            Preset::SyncSweep,
            &[
                (Analog, 0.4),
                (Vco1Level, 0.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (Cutoff, 6_000.0),
                (Resonance, 0.15),
                (AdsrAttack, 0.003),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.25),
                (FenvAttack, 0.003),
                (FenvDecay, 1.2),
                (FenvSustain, 0.0),
                (EnvFreq2, 0.8),
                (KeyTrack, 0.5),
                (Priority, 1.0),
            ],
        ),
        // A sine B at an inharmonic ratio frequency-modulating a sine A
        // (poly-mod), the sound dying with the loudness: a bell.
        PresetDef::of(
            Preset::PolyModBell,
            &[
                (Analog, 0.4),
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
        // A saw and a pulse (A) that the LFO sweeps, slightly apart, with
        // a slow attack and release.
        PresetDef::of(
            Preset::ProStrings,
            &[
                (Analog, 0.4),
                (Vco1Fine, 9.0),
                (Vco1Level, 0.8),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.8),
                (LfoWave, 2.0),
                (LfoRate, 0.5),
                (LfoPw, 0.3),
                (Cutoff, 2_200.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.3),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.5),
                (FenvAttack, 0.4),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
            ],
        ),
    ],
    ..ModelDef::MONO
};
