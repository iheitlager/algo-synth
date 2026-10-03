//! Mono presets (spec 004 Req 9): Rust data, selected by id.
//!
//! `DEFAULTS` gives every Mono parameter a value in the units `set_param`
//! takes; it is the voice's starting state (`MonoParams::new`,
//! `Engine::new`) and the base every preset starts from, so a preset fully
//! defines the voice, patch and normalled amounts included (spec 004 Req 7).

use crate::mono::model::Model;
use crate::params::Param;

/// A preset id; mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Preset {
    Bass = 0,
    Lead = 1,
    SyncLead = 2,
    BowedString = 3,
    MiniBass = 4,
    MiniLead = 5,
    ProLead = 6,
    ProBass = 7,
}

impl Preset {
    /// Every preset with the name the TypeScript mirror uses.
    pub const ALL: [(Preset, &'static str); 8] = [
        (Preset::Bass, "Bass"),
        (Preset::Lead, "Lead"),
        (Preset::SyncLead, "SyncLead"),
        (Preset::BowedString, "BowedString"),
        (Preset::MiniBass, "MiniBass"),
        (Preset::MiniLead, "MiniLead"),
        (Preset::ProLead, "ProLead"),
        (Preset::ProBass, "ProBass"),
    ];

    /// The preset for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Preset> {
        Self::ALL
            .iter()
            .find(|(p, _)| *p as u32 == id)
            .map(|(p, _)| *p)
    }

    /// The model this preset is for; `changes` sets it.
    pub fn model(self) -> Model {
        match self {
            Preset::Bass | Preset::Lead | Preset::SyncLead | Preset::BowedString => Model::Arp2600,
            Preset::MiniBass | Preset::MiniLead => Model::Minimoog,
            Preset::ProLead | Preset::ProBass => Model::ProOne,
        }
    }

    /// What this preset changes from `DEFAULTS`.
    pub fn changes(self) -> &'static [(Param, f32)] {
        use Param::*;
        match self {
            // Saw and a pulse an octave down, into a low, resonant ladder.
            Preset::Bass => &[
                (Vco2Wave, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Level, 0.7),
                (PulseWidth, 0.35),
                (Cutoff, 600.0),
                (Resonance, 0.35),
                (Drive, 0.4),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.45),
                (AdsrRelease, 0.12),
                // The filter opens with each note and follows the key.
                (EnvCutoff, 0.4),
                (KeyTrack, 0.5),
            ],
            // Two saws a few cents apart and a sub saw, a medium cutoff.
            Preset::Lead => &[
                (Vco2Fine, 7.0),
                (Vco2Level, 0.8),
                (Vco3Coarse, -12.0),
                (Vco3Level, 0.3),
                (Cutoff, 2_500.0),
                (Resonance, 0.3),
                (Drive, 0.2),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.75),
                (AdsrRelease, 0.3),
                (EnvCutoff, 0.25),
                (KeyTrack, 0.5),
                // A little vibrato, the wheel half up.
                (Vibrato, 0.15),
                (ModWheel, 0.5),
            ],
            // VCO 2 hard-synced to a silent VCO 1, an octave and a fifth up.
            // The ADSR sweeps VCO 2's pitch: the classic sync sweep.
            Preset::SyncLead => &[
                (Vco1Level, 0.0),
                (Vco2Coarse, 19.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (Cutoff, 5_000.0),
                (Resonance, 0.2),
                (Drive, 0.3),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.6),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.3),
                (Patch1Source, 5.0),
                (Patch1Dest, 2.0),
                (Patch1Amount, 0.5),
            ],
            // Two saws and a pulse an octave under, overdriven into a low
            // ladder that the contour opens: the Minimoog's bass. Low note
            // priority, as on the instrument.
            Preset::MiniBass => &[
                (Model, 1.0),
                (Vco1Coarse, -12.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 4.0),
                (Vco2Level, 1.0),
                (Vco3Wave, 1.0),
                (Vco3Coarse, -24.0),
                (Vco3Level, 0.8),
                (Cutoff, 450.0),
                (Resonance, 0.3),
                (Drive, 0.5),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.8),
                (FenvAttack, 0.002),
                (FenvDecay, 0.35),
                (FenvSustain, 0.2),
                (EnvCutoff, 0.55),
                (KeyTrack, 0.33),
                (Priority, 1.0),
                (Glide, 0.04),
            ],
            // Two saws a few cents apart and a triangle; VCO 3 in its low
            // range is the vibrato, the wheel half up. Glide, legato.
            Preset::MiniLead => &[
                (Model, 1.0),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.8),
                (Vco3Wave, 2.0),
                (Vco3Coarse, -3.0),
                (Vco3Level, 0.0),
                (Vco3Low, 1.0),
                (Vco3KeyFollow, 0.0),
                (Cutoff, 1_800.0),
                (Resonance, 0.35),
                (Drive, 0.25),
                (AdsrAttack, 0.02),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.8),
                (FenvAttack, 0.02),
                (FenvDecay, 0.5),
                (FenvSustain, 0.5),
                (EnvCutoff, 0.3),
                (KeyTrack, 0.67),
                (Vibrato, 0.2),
                (ModWheel, 0.6),
                (Glide, 0.12),
                (Legato, 1.0),
                (Priority, 1.0),
            ],
            // Oscillator A (VCO 2) synced to a silent B (VCO 1), a fifth
            // over: the filter envelope sweeps A's pitch, the classic
            // poly-mod sync lead. Low note priority, a little glide.
            Preset::ProLead => &[
                (Model, 2.0),
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
            // A saw an octave down and a narrow pulse (A) whose width the
            // LFO moves; full key tracking, as on the Pro-One.
            Preset::ProBass => &[
                (Model, 2.0),
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
            // Three detuned saws and a breath of pink noise for the bow,
            // a slow attack and a long release: the ensemble's start.
            Preset::BowedString => &[
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
                (AdsrSustain, 0.85),
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
        }
    }
}

/// Every Mono parameter's starting value: VCO 1 alone, a saw, through a
/// 4 kHz ladder, with a short attack.
pub const DEFAULTS: [(Param, f32); 79] = [
    (Param::Vco1Wave, 0.0),
    (Param::Vco1Coarse, 0.0),
    (Param::Vco1Fine, 0.0),
    (Param::Vco1Level, 1.0),
    (Param::Vco2Wave, 0.0),
    (Param::Vco2Coarse, 0.0),
    (Param::Vco2Fine, 0.0),
    (Param::Vco2Level, 0.0),
    (Param::Vco3Wave, 0.0),
    (Param::Vco3Coarse, 0.0),
    (Param::Vco3Fine, 0.0),
    (Param::Vco3Level, 0.0),
    (Param::PulseWidth, 0.5),
    (Param::Vco2Sync, 0.0),
    (Param::Vco3Sync, 0.0),
    (Param::NoiseLevel, 0.0),
    (Param::NoiseColour, 0.0),
    (Param::Cutoff, 4_000.0),
    (Param::Resonance, 0.0),
    (Param::Drive, 0.0),
    (Param::AdsrAttack, 0.005),
    (Param::AdsrDecay, 0.3),
    (Param::AdsrSustain, 0.7),
    (Param::AdsrRelease, 0.3),
    (Param::ArAttack, 0.005),
    (Param::ArRelease, 0.3),
    (Param::LfoRate, 4.0),
    (Param::LfoWave, 3.0),
    (Param::Priority, 0.0),
    (Param::Legato, 0.0),
    (Param::Glide, 0.0),
    (Param::Patch1Source, 0.0),
    (Param::Patch1Dest, 0.0),
    (Param::Patch1Amount, 0.0),
    (Param::Patch2Source, 0.0),
    (Param::Patch2Dest, 0.0),
    (Param::Patch2Amount, 0.0),
    (Param::Patch3Source, 0.0),
    (Param::Patch3Dest, 0.0),
    (Param::Patch3Amount, 0.0),
    (Param::Patch4Source, 0.0),
    (Param::Patch4Dest, 0.0),
    (Param::Patch4Amount, 0.0),
    (Param::Patch5Source, 0.0),
    (Param::Patch5Dest, 0.0),
    (Param::Patch5Amount, 0.0),
    (Param::Patch6Source, 0.0),
    (Param::Patch6Dest, 0.0),
    (Param::Patch6Amount, 0.0),
    (Param::Patch7Source, 0.0),
    (Param::Patch7Dest, 0.0),
    (Param::Patch7Amount, 0.0),
    (Param::Patch8Source, 0.0),
    (Param::Patch8Dest, 0.0),
    (Param::Patch8Amount, 0.0),
    (Param::EnvCutoff, 0.0),
    (Param::KeyTrack, 0.0),
    (Param::Vibrato, 0.0),
    (Param::ModWheel, 0.0),
    (Param::Model, 0.0),
    (Param::FenvAttack, 0.005),
    (Param::FenvDecay, 0.3),
    (Param::FenvSustain, 0.7),
    (Param::FenvRelease, 0.3),
    (Param::HpCutoff, 20.0),
    (Param::HpResonance, 0.0),
    (Param::EnvHpCutoff, 0.0),
    (Param::RingLevel, 0.0),
    (Param::SubLevel, 0.0),
    (Param::SubOctave, 0.0),
    (Param::Vco3KeyFollow, 1.0),
    (Param::Vco3Low, 0.0),
    (Param::LfoCutoff, 0.0),
    (Param::LfoPw, 0.0),
    (Param::EnvFreq2, 0.0),
    (Param::OscFreq2, 0.0),
    (Param::EnvPw, 0.0),
    (Param::OscPw, 0.0),
    (Param::OscCutoff, 0.0),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{BLOCK, Engine};

    /// The parameters that aren't Mono's.
    const SHARED: [Param; 1] = [Param::MasterGain];

    #[test]
    fn defaults_cover_every_mono_parameter_once() {
        for (p, _) in Param::ALL {
            let n = DEFAULTS.iter().filter(|(d, _)| *d == p).count();
            let want = usize::from(!SHARED.contains(&p));
            assert_eq!(n, want, "{p:?} in DEFAULTS {n} times");
        }
    }

    #[test]
    fn every_value_is_in_range() {
        let changes = Preset::ALL.iter().flat_map(|(p, _)| p.changes());
        for (p, v) in DEFAULTS.iter().chain(changes) {
            assert_eq!(p.clamp(*v), *v, "{p:?} = {v} is out of range");
            assert!(!SHARED.contains(p), "a preset sets {p:?}");
        }
    }

    #[test]
    fn every_preset_is_bounded() {
        for (preset, name) in Preset::ALL {
            for note in [24, 36, 48, 60, 72, 84, 96] {
                let mut e = Engine::new(48_000.0);
                e.set_param(0, Param::MasterGain, 1.0);
                e.preset(0, preset);
                e.note_on(0, note, 1.0);
                let mut heard = 0.0_f32;
                for i in 0..(48_000 * 5 / 2 / BLOCK) {
                    if i == 48_000 / BLOCK {
                        e.note_off(0, note);
                    }
                    e.render(BLOCK);
                    for s in e.output() {
                        assert!(s.is_finite() && s.abs() <= 1.0, "{name} {note}: {s}");
                        heard = heard.max(s.abs());
                    }
                }
                assert!(heard > 0.02, "{name} {note} is silent");
                assert_eq!(e.active_voices(), 0, "{name} {note} still sounds");
            }
        }
    }

    #[test]
    fn a_preset_sets_every_mono_parameter() {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::Bass);
        e.preset(0, Preset::Lead);
        for (p, v) in DEFAULTS {
            let want = Preset::Lead
                .changes()
                .iter()
                .find(|(c, _)| *c == p)
                .map_or(v, |(_, c)| *c);
            assert_eq!(e.param_value(0, p), want, "{p:?}");
        }
    }

    /// No leftovers: a preset after another sounds like it does from fresh.
    #[test]
    fn a_preset_after_another_renders_like_a_fresh_one() {
        let render = |presets: &[Preset]| {
            let mut e = Engine::new(48_000.0);
            for p in presets {
                e.preset(0, *p);
            }
            e.note_on(0, 60, 1.0);
            let mut out = Vec::new();
            for _ in 0..50 {
                e.render(BLOCK);
                out.extend_from_slice(e.output());
            }
            out
        };
        for (preset, name) in Preset::ALL {
            for (before, _) in Preset::ALL {
                assert!(
                    render(&[before, preset]) == render(&[preset]),
                    "{name} after {before:?}"
                );
            }
        }
    }

    /// The ARP 2600 voice sounds as it did before models (spec 005 Req 2):
    /// rms, peak and two samples of half a second of A3, per preset, from the
    /// last release before the model was added.
    #[test]
    fn arp_presets_keep_their_sound() {
        let gold: [(Preset, [f64; 4]); 4] = [
            (Preset::Bass, [0.117948, 0.479209, 0.040528, 0.162096]),
            (Preset::Lead, [0.169469, 0.478455, -0.246227, -0.055599]),
            (Preset::SyncLead, [0.150151, 0.386691, -0.188180, -0.129467]),
            (
                Preset::BowedString,
                [0.093402, 0.229120, -0.096020, 0.048850],
            ),
        ];
        for (preset, want) in gold {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::MasterGain, 1.0);
            e.preset(0, preset);
            e.note_on(0, 57, 0.8);
            let mut out = Vec::new();
            for _ in 0..(48_000 / 2 / BLOCK) {
                e.render(BLOCK);
                out.extend_from_slice(e.output().get(..BLOCK).unwrap_or(&[]));
            }
            let rms =
                (out.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / out.len() as f64).sqrt();
            let peak = out.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
            let got = [
                rms,
                f64::from(peak),
                f64::from(out[7000]),
                f64::from(out[15000]),
            ];
            for (g, w) in got.iter().zip(want) {
                assert!((g - w).abs() < 2.0e-5, "{preset:?}: {got:?} vs {want:?}");
            }
        }
    }

    /// A preset sets its model, so a switch of model is a whole sound.
    #[test]
    fn every_preset_sets_its_model() {
        for (preset, name) in Preset::ALL {
            let mut e = Engine::new(48_000.0);
            e.preset(0, preset);
            assert_eq!(
                e.param_value(0, Param::Model),
                preset.model() as u32 as f32,
                "{name}"
            );
        }
    }

    /// A new or reset synth is an ARP 2600, and a preset after another
    /// model's leaves nothing of it.
    #[test]
    fn a_new_synth_is_an_arp_2600() {
        let mut e = Engine::new(48_000.0);
        assert_eq!(e.param_value(5, Param::Model), 0.0);
        e.set_param(5, Param::Model, 3.0);
        assert_eq!(e.param_value(5, Param::Model), 3.0);
        e.reset(5);
        assert_eq!(e.param_value(5, Param::Model), 0.0);
        e.set_param(5, Param::Model, 3.0);
        e.preset(5, Preset::Bass);
        assert_eq!(e.param_value(5, Param::Model), 0.0);
    }

    #[test]
    fn unknown_ids_are_none() {
        assert_eq!(Preset::from_id(99), None);
    }
}
