//! General MIDI (#652): the multisampler with a program of the FluidR3 GM
//! bank per preset. A preset names its pack, `gm-<name>`, which the song
//! loads on the track when no `samples` line names another.

use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{Engine, ModelDef, PresetDef};

/// The sample as recorded through an open filter; `attack` and `release`
/// in seconds are what differ between the programs.
macro_rules! program {
    ($preset:expr, $attack:expr, $release:expr) => {
        PresetDef::of(
            $preset,
            &[
                (Polyphony, 16.0),
                (Analog, 0.0),
                (Cutoff, 20_000.0),
                (Resonance, 0.0),
                (AdsrAttack, $attack),
                (AdsrDecay, 0.3),
                (AdsrSustain, 1.0),
                (AdsrRelease, $release),
                (FenvAttack, 0.001),
                (FenvDecay, 0.3),
                (FenvSustain, 1.0),
                (EnvCutoff, 0.0),
                (KeyTrack, 0.0),
            ],
        )
    };
}

pub const DEF: ModelDef = ModelDef {
    voices: 16,
    engine: Engine::Sampler,
    presets: &[
        program!(Preset::GmPiano, 0.001, 0.5),
        program!(Preset::GmElectricPiano, 0.001, 0.4),
        program!(Preset::GmVibraphone, 0.001, 0.8),
        program!(Preset::GmMarimba, 0.001, 0.3),
        program!(Preset::GmXylophone, 0.001, 0.2),
        program!(Preset::GmOrgan, 0.005, 0.08),
        program!(Preset::GmNylonGuitar, 0.001, 0.4),
        program!(Preset::GmAcousticBass, 0.001, 0.2),
        program!(Preset::GmFingerBass, 0.001, 0.15),
        program!(Preset::GmStrings, 0.08, 0.6),
        program!(Preset::GmTrumpet, 0.01, 0.15),
        program!(Preset::GmFlute, 0.03, 0.2),
    ],
    ..ModelDef::MONO
};

/// Each preset's pack, as `tools/samples/packs.json` names the programs.
const PACKS: [(Preset, &str); 12] = [
    (Preset::GmPiano, "gm-piano"),
    (Preset::GmElectricPiano, "gm-electric-piano"),
    (Preset::GmVibraphone, "gm-vibraphone"),
    (Preset::GmMarimba, "gm-marimba"),
    (Preset::GmXylophone, "gm-xylophone"),
    (Preset::GmOrgan, "gm-organ"),
    (Preset::GmNylonGuitar, "gm-nylon-guitar"),
    (Preset::GmAcousticBass, "gm-acoustic-bass"),
    (Preset::GmFingerBass, "gm-finger-bass"),
    (Preset::GmStrings, "gm-strings"),
    (Preset::GmTrumpet, "gm-trumpet"),
    (Preset::GmFlute, "gm-flute"),
];

impl Preset {
    /// The sample pack a General MIDI preset plays (#652); `None` for any
    /// other preset.
    pub fn pack(self) -> Option<&'static str> {
        PACKS.iter().find(|(p, _)| *p == self).map(|(_, id)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each preset has its pack, named as the ledger names its program.
    #[test]
    fn every_program_names_its_pack() {
        let ledger = include_str!("../../../../tools/samples/packs.json");
        for d in DEF.presets {
            let pack = d.preset.pack().expect("a pack");
            assert!(ledger.contains(&format!("\"{pack}\":")), "{pack}");
        }
        assert_eq!(PACKS.len(), DEF.presets.len());
        assert_eq!(Preset::SamplerKeys.pack(), None);
    }
}
