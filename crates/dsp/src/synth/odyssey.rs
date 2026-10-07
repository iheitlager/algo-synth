//! The ARP Odyssey: what the model is and its presets (#330).

use crate::mono::model::{Filter, Hp, ODYSSEY, ODYSSEY_REV1, ODYSSEY_REV2};
use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    filter: Filter::Ladder(ODYSSEY),
    // The reissue's Rev switch: the 4023's two poles, the 4035's ladder.
    revs: [
        Some(Filter::Svf(ODYSSEY_REV1)),
        Some(Filter::Ladder(ODYSSEY_REV2)),
    ],
    hp: Hp::OnePole,
    cutoff_follows_filter_env: false,
    presets: &[
        // Two saws a few cents apart into a bright, slightly resonant
        // ladder driven into its saturator; legato glide and a quick
        // vibrato on the wheel: a singing late-70s Odyssey lead in the
        // manner of Billy Currie. An overdrive insert on the strip adds the
        // grit; the bends are the player's (#10).
        PresetDef::of(
            Preset::CurrieLead,
            &[
                (Model, 6.0),
                (Vco1Level, 0.9),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.8),
                (HpCutoff, 60.0),
                (Cutoff, 3_200.0),
                (Resonance, 0.35),
                (Drive, 0.45),
                (AdsrAttack, 0.008),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.3),
                (EnvCutoff, 0.25),
                (KeyTrack, 0.5),
                (LfoRate, 5.5),
                (Vibrato, 0.25),
                (ModWheel, 0.6),
                (Glide, 0.12),
                (Legato, 1.0),
            ],
        ),
        // VCO 2 hard-synced to a silent VCO 1, its pitch swept down by the
        // ADSR on every note, through an open ladder: the Odyssey's sync
        // lead.
        PresetDef::of(
            Preset::OdysseySync,
            &[
                (Model, 6.0),
                (Vco1Level, 0.0),
                (Vco2Coarse, 12.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (HpCutoff, 60.0),
                (Cutoff, 4_500.0),
                (Resonance, 0.25),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.4),
                (Vibrato, 0.2),
                (ModWheel, 0.4),
                (Patch1Source, 5.0),
                (Patch1Dest, 2.0),
                (Patch1Amount, 0.45),
            ],
        ),
    ],
    ..ModelDef::MONO
};
