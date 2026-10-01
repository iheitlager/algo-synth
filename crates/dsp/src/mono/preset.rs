//! Mono presets (spec 004 Req 9): Rust data, selected by id.
//!
//! `DEFAULTS` gives every Mono parameter a value in the units `set_param`
//! takes; it is the voice's starting state (`MonoParams::new`,
//! `Engine::new`) and the base every preset starts from, so a preset fully
//! defines the voice. Presets set parameters only until routing adds a patch
//! (spec 004 Req 7, plan.md MVP 5).

use crate::params::Param;

/// A preset id; mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Preset {
    Bass = 0,
    Lead = 1,
    SyncLead = 2,
    BowedString = 3,
}

impl Preset {
    /// Every preset with the name the TypeScript mirror uses.
    pub const ALL: [(Preset, &'static str); 4] = [
        (Preset::Bass, "Bass"),
        (Preset::Lead, "Lead"),
        (Preset::SyncLead, "SyncLead"),
        (Preset::BowedString, "BowedString"),
    ];

    /// The preset for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Preset> {
        Self::ALL
            .iter()
            .find(|(p, _)| *p as u32 == id)
            .map(|(p, _)| *p)
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
            ],
            // VCO 2 hard-synced to a silent VCO 1, an octave and a fifth up.
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
                (LfoRate, 5.5),
            ],
        }
    }
}

/// Every Mono parameter's starting value: VCO 1 alone, a saw, through a
/// 4 kHz ladder, with a short attack.
pub const DEFAULTS: [(Param, f32); 28] = [
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
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{BLOCK, Engine};
    use crate::source::Source;

    /// The parameters that aren't Mono's.
    const SHARED: [Param; 3] = [Param::MasterGain, Param::Attack, Param::Release];

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
                e.set_param(Param::MasterGain, 1.0);
                e.preset(preset);
                e.note_on(Source::Mono, note, 1.0);
                let mut heard = 0.0_f32;
                for i in 0..(48_000 * 5 / 2 / BLOCK) {
                    if i == 48_000 / BLOCK {
                        e.note_off(Source::Mono, note);
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
        e.preset(Preset::Bass);
        e.preset(Preset::Lead);
        for (p, v) in DEFAULTS {
            let want = Preset::Lead
                .changes()
                .iter()
                .find(|(c, _)| *c == p)
                .map_or(v, |(_, c)| *c);
            assert_eq!(e.param_value(p), want, "{p:?}");
        }
    }

    /// No leftovers: a preset after another sounds like it does from fresh.
    #[test]
    fn a_preset_after_another_renders_like_a_fresh_one() {
        let render = |presets: &[Preset]| {
            let mut e = Engine::new(48_000.0);
            for p in presets {
                e.preset(*p);
            }
            e.note_on(Source::Mono, 60, 1.0);
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

    #[test]
    fn typescript_mirror_matches() {
        let ts = include_str!("../../../../web/src/audio/params.ts");
        for (p, name) in Preset::ALL {
            let line = format!("{name}: {},", p as u32);
            assert!(ts.contains(&line), "web/src/audio/params.ts lacks `{line}`");
        }
        assert_eq!(Preset::from_id(4), None);
    }
}
