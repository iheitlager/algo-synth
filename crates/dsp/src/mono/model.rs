//! The synth models (spec 005, ADR-0009): which instrument a Mono synth is.
//!
//! One shared voice serves every model; what differs lives here, as small
//! `Copy` answers `MonoVoice::render` matches on (ADR-0002: no boxing, no
//! allocation). Every parameter exists on every model; the panel shows what
//! the instrument has and the presets set the rest to neutral values.

/// A synth model id; mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Model {
    #[default]
    Arp2600 = 0,
    Minimoog = 1,
    ProOne = 2,
    Ms20 = 3,
    Cs15 = 4,
    Sh101 = 5,
    Odyssey = 6,
    Prophet5 = 7,
}

impl Model {
    /// Every model with the name the TypeScript mirror uses.
    pub const ALL: [(Model, &'static str); 8] = [
        (Model::Arp2600, "Arp2600"),
        (Model::Minimoog, "Minimoog"),
        (Model::ProOne, "ProOne"),
        (Model::Ms20, "Ms20"),
        (Model::Cs15, "Cs15"),
        (Model::Sh101, "Sh101"),
        (Model::Odyssey, "Odyssey"),
        (Model::Prophet5, "Prophet5"),
    ];

    /// The model for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Model> {
        Self::ALL
            .iter()
            .find(|(m, _)| *m as u32 == id)
            .map(|(m, _)| *m)
    }
}

/// How a ladder is voiced (spec 004 Req 13): the same filter, set up as the
/// instrument had it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LadderVoicing {
    /// Multiplies the drive into the saturator.
    pub drive: f32,
    /// How much of the bass resonance would take away is given back, per
    /// unit of feedback: 0 loses it as the Moog ladder does.
    pub comp: f32,
    /// Multiplies the feedback, so a voicing can stop short of the full
    /// resonance range.
    pub k_scale: f32,
}

/// How a 12 dB state-variable filter is voiced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvfVoicing {
    /// The resonance (0..=1) at which the damping reaches 0 and the filter
    /// self-oscillates; above 1 it never does.
    pub osc_at: f32,
    /// The least damping, negative where the oscillation may grow into the
    /// saturator.
    pub k_min: f32,
    /// Where the saturating state clips: smaller is harder.
    pub ceiling: f32,
}

pub const MOOG: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.0,
    k_scale: 1.0,
};
pub const PRO_ONE: LadderVoicing = LadderVoicing {
    drive: 1.1,
    comp: 0.3,
    k_scale: 1.0,
};
pub const SH101: LadderVoicing = LadderVoicing {
    drive: 0.7,
    comp: 0.15,
    k_scale: 0.95,
};
/// The ARP 4035/4075 of the later Odysseys: brighter and cleaner than the
/// Moog, a little bass kept under resonance, short of the full range.
pub const ODYSSEY: LadderVoicing = LadderVoicing {
    drive: 0.85,
    comp: 0.2,
    k_scale: 0.97,
};
/// Sharp, and screaming at the top of the knob.
pub const MS20: SvfVoicing = SvfVoicing {
    osc_at: 0.9,
    k_min: -0.06,
    ceiling: 0.5,
};
/// Resonant and smooth, never quite oscillating.
pub const CS15: SvfVoicing = SvfVoicing {
    osc_at: 1.15,
    k_min: 0.1,
    ceiling: 1.2,
};

/// The low-pass a model uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    Ladder(LadderVoicing),
    Svf(SvfVoicing),
}

/// The high-pass stage of a model, if it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hp {
    None,
    /// 12 dB with resonance, before the low-pass.
    Svf,
    /// 6 dB, after the low-pass.
    OnePole,
}

impl Model {
    /// How many voices the instrument has at most (spec 006 Req 1): the
    /// polyphonic models are limited to their own count, the monosynths to the
    /// pool's.
    pub fn voices(self) -> usize {
        match self {
            Model::Prophet5 => 5,
            _ => crate::poly::MAX_VOICES,
        }
    }

    /// The filter and its voicing.
    pub fn filter(self) -> Filter {
        match self {
            Model::Arp2600 | Model::Minimoog => Filter::Ladder(MOOG),
            Model::ProOne | Model::Prophet5 => Filter::Ladder(PRO_ONE),
            Model::Sh101 => Filter::Ladder(SH101),
            Model::Odyssey => Filter::Ladder(ODYSSEY),
            Model::Ms20 => Filter::Svf(MS20),
            Model::Cs15 => Filter::Svf(CS15),
        }
    }

    /// The high-pass stage.
    pub fn hp(self) -> Hp {
        match self {
            Model::Ms20 | Model::Cs15 => Hp::Svf,
            Model::Sh101 | Model::Odyssey => Hp::OnePole,
            Model::Arp2600 | Model::Minimoog | Model::ProOne | Model::Prophet5 => Hp::None,
        }
    }

    /// Whether the filter envelope source (`ModSource::Fenv`, so the poly-mod
    /// and high-pass envelope amounts) is the ADSR: the SH-101 has one
    /// envelope for filter and loudness.
    pub fn filter_env_is_adsr(self) -> bool {
        self == Model::Sh101
    }

    /// Whether VCO 2 is the pulse output of VCO 1: phase-locked to it and at
    /// its pitch, as the SH-101's one oscillator gives saw and pulse together.
    pub fn pulse_locked(self) -> bool {
        self == Model::Sh101
    }

    /// Whether a time set as decay is also the release, on the loudness and
    /// the filter ADSR: the Minimoog's contours have no release knob.
    pub fn decay_is_release(self) -> bool {
        self == Model::Minimoog
    }

    /// Whether VCO 3 is the modulation source of the normals (vibrato,
    /// `LfoCutoff`) instead of the LFO: the Minimoog has no LFO, Osc 3 in
    /// its low range does that job.
    pub fn modulates_with_osc3(self) -> bool {
        self == Model::Minimoog
    }

    /// Whether the high-pass cutoff follows the AR envelope (the CS-15's
    /// own envelope for it) rather than the filter ADSR.
    pub fn hp_follows_ar(self) -> bool {
        self == Model::Cs15
    }

    /// Whether the normalled cutoff follows the filter ADSR. The ARP 2600,
    /// the SH-101 and the Odyssey (whose ADSR is normalled to both filter
    /// and VCA; its AR is a patch source) follow the ADSR (spec 004 Req 12).
    pub fn cutoff_follows_filter_env(self) -> bool {
        !matches!(self, Model::Arp2600 | Model::Sh101 | Model::Odyssey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for (i, (m, _)) in Model::ALL.iter().enumerate() {
            assert_eq!(Model::from_id(*m as u32), Some(*m));
            assert_eq!(*m as usize, i, "ids run from 0 without gaps");
        }
        assert_eq!(Model::from_id(99), None);
        assert_eq!(Model::default(), Model::Arp2600);
    }

    /// Every model with a high-pass stage of 12 dB has the matching filter.
    #[test]
    fn models_pair_their_filters_and_stages() {
        for (m, name) in Model::ALL {
            if m.hp() == Hp::Svf {
                assert!(matches!(m.filter(), Filter::Svf(_)), "{name}");
            }
        }
        const {
            assert!(MS20.osc_at < 1.0 && MS20.k_min < 0.0);
            assert!(CS15.osc_at > 1.0 && CS15.k_min > 0.0);
        }
        assert_eq!(MOOG.comp, 0.0);
    }

    #[test]
    fn polyphonic_models_have_their_own_voice_count() {
        assert_eq!(Model::Prophet5.voices(), 5);
        for (m, name) in Model::ALL {
            assert!(
                m.voices() >= 1 && m.voices() <= crate::poly::MAX_VOICES,
                "{name}"
            );
        }
        assert_eq!(
            Model::Minimoog.voices(),
            crate::poly::MAX_VOICES,
            "a monosynth is not limited"
        );
    }

    #[test]
    fn single_envelope_models_follow_the_adsr() {
        for (m, name) in Model::ALL {
            let single = matches!(m, Model::Arp2600 | Model::Sh101 | Model::Odyssey);
            assert_eq!(m.cutoff_follows_filter_env(), !single, "{name}");
        }
    }
}
