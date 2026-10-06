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
    Juno106 = 8,
    Jupiter8 = 9,
    Matrix12 = 10,
    PpgWave = 11,
    D50 = 12,
    Dx7 = 13,
    PolyMoog = 14,
    /// The drum kit: eighteen synthesized pads, one voice each (#114, #148).
    Tr808 = 15,
    /// The multisampler (`sampler`): zones of the sample store.
    Sampler = 16,
    /// The drum/pad sampler (`padsampler`): sixteen pads playing samples.
    PadSampler = 17,
    /// The TR-909 (#148): the drum kit's pads with the 909's sounds.
    Tr909 = 18,
    /// A voice written in the song as a graph of unit generators (ADR-0020).
    Modular = 19,
}

impl Model {
    /// Every model with the name the TypeScript mirror uses.
    pub const ALL: [(Model, &'static str); 20] = [
        (Model::Arp2600, "Arp2600"),
        (Model::Minimoog, "Minimoog"),
        (Model::ProOne, "ProOne"),
        (Model::Ms20, "Ms20"),
        (Model::Cs15, "Cs15"),
        (Model::Sh101, "Sh101"),
        (Model::Odyssey, "Odyssey"),
        (Model::Prophet5, "Prophet5"),
        (Model::Juno106, "Juno106"),
        (Model::Jupiter8, "Jupiter8"),
        (Model::Matrix12, "Matrix12"),
        (Model::PpgWave, "PpgWave"),
        (Model::D50, "D50"),
        (Model::Dx7, "Dx7"),
        (Model::PolyMoog, "PolyMoog"),
        (Model::Tr808, "Tr808"),
        (Model::Sampler, "Sampler"),
        (Model::PadSampler, "PadSampler"),
        (Model::Tr909, "Tr909"),
        (Model::Modular, "Modular"),
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
/// The Jupiter-8's four-pole: clean and a little bass kept under resonance.
pub const JUPITER: LadderVoicing = LadderVoicing {
    drive: 0.9,
    comp: 0.25,
    k_scale: 0.98,
};
/// Its two-pole setting: resonant but short of oscillating.
pub const JUPITER12: SvfVoicing = SvfVoicing {
    osc_at: 1.1,
    k_min: 0.08,
    ceiling: 1.0,
};
/// The Matrix-12's four-pole: clean, with the bass kept under resonance.
pub const MATRIX: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.35,
    k_scale: 1.0,
};
/// Its two-pole setting: smooth, short of oscillating.
pub const MATRIX12: SvfVoicing = SvfVoicing {
    osc_at: 1.05,
    k_min: 0.05,
    ceiling: 1.1,
};
/// The PPG Wave's four-pole (an SSM-style ladder): clean, bass kept.
pub const PPG: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.2,
    k_scale: 1.0,
};
/// The D-50's partial filters: clean, a little bass kept.
pub const D50: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.2,
    k_scale: 0.98,
};
/// The Polymoog's resonator filter: strongly resonant, vocal rather than screaming.
pub const POLYMOOG: SvfVoicing = SvfVoicing {
    osc_at: 1.05,
    k_min: 0.04,
    ceiling: 1.0,
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
            Model::Juno106 => 6,
            Model::Jupiter8 => 8,
            Model::Matrix12 => 12,
            Model::PpgWave => 8,
            Model::D50 => 16,
            Model::Dx7 | Model::PolyMoog | Model::Sampler | Model::PadSampler => 16,
            _ => crate::poly::MAX_VOICES,
        }
    }

    /// The filter and its voicing.
    pub fn filter(self) -> Filter {
        match self {
            Model::Arp2600 | Model::Minimoog => Filter::Ladder(MOOG),
            Model::ProOne | Model::Prophet5 => Filter::Ladder(PRO_ONE),
            Model::Sh101 | Model::Juno106 => Filter::Ladder(SH101),
            Model::Jupiter8 => Filter::Ladder(JUPITER),
            Model::Matrix12 => Filter::Ladder(MATRIX),
            Model::PpgWave => Filter::Ladder(PPG),
            // The kit has no filter of its own; the ladder's setting goes unused.
            Model::D50
            | Model::Dx7
            | Model::Tr808
            | Model::Tr909
            | Model::Sampler
            | Model::PadSampler
            | Model::Modular => Filter::Ladder(D50),
            Model::Odyssey => Filter::Ladder(ODYSSEY),
            Model::Ms20 => Filter::Svf(MS20),
            Model::Cs15 => Filter::Svf(CS15),
            Model::PolyMoog => Filter::Svf(POLYMOOG),
        }
    }

    /// The 12 dB low-pass of a model with the slope switch (the Jupiter-8's
    /// two-pole setting), if it has one.
    pub fn filter_12db(self) -> Option<Filter> {
        match self {
            Model::Jupiter8 => Some(Filter::Svf(JUPITER12)),
            Model::Matrix12 => Some(Filter::Svf(MATRIX12)),
            _ => None,
        }
    }

    /// Whether the voice is the DX7's six-operator FM voice (spec 006 Req 13).
    pub fn uses_fm(self) -> bool {
        self == Model::Dx7
    }

    /// Whether the synth is the drum kit, whose keys hit pads (#114).
    pub fn uses_drums(self) -> bool {
        matches!(self, Model::Tr808 | Model::Tr909)
    }

    /// The drum machine a kit model is; the 808 for any other.
    pub fn drum_machine(self) -> crate::drums::Machine {
        match self {
            Model::Tr909 => crate::drums::Machine::Tr909,
            _ => crate::drums::Machine::Tr808,
        }
    }

    /// Whether the voice is the D-50's two-partial LA voice (spec 006 Req 12).
    pub fn uses_la(self) -> bool {
        self == Model::D50
    }

    /// Whether the voice is a graph of unit generators (ADR-0020).
    pub fn uses_graph(self) -> bool {
        self == Model::Modular
    }

    /// Whether its voices are the Mono voice, monophonic or in a poly pool:
    /// the voices that read per-voice values (ADR-0023).
    pub fn uses_mono_voice(self) -> bool {
        !(self.uses_la()
            || self.uses_fm()
            || self.uses_drums()
            || self.uses_sampler()
            || self.uses_pads()
            || self.uses_graph())
    }

    /// Whether the synth is the drum/pad sampler (#124).
    pub fn uses_pads(self) -> bool {
        self == Model::PadSampler
    }

    /// Whether the voice is the multisampler's (#123).
    pub fn uses_sampler(self) -> bool {
        self == Model::Sampler
    }

    /// Whether VCO 1 and VCO 2 are wavetable oscillators (spec 006 Req 11).
    pub fn uses_tables(self) -> bool {
        self == Model::PpgWave
    }

    /// Whether the voice runs the second LFO and the ramp, the sources the
    /// Matrix-12's modulation matrix adds.
    pub fn has_matrix(self) -> bool {
        self == Model::Matrix12
    }

    /// The high-pass stage.
    pub fn hp(self) -> Hp {
        match self {
            Model::Ms20 | Model::Cs15 => Hp::Svf,
            Model::Sh101 | Model::Odyssey | Model::Juno106 | Model::Jupiter8 | Model::Matrix12 => {
                Hp::OnePole
            }
            Model::Arp2600
            | Model::Minimoog
            | Model::ProOne
            | Model::Prophet5
            | Model::PpgWave
            | Model::D50
            | Model::Dx7
            | Model::PolyMoog
            | Model::Tr808
            | Model::Sampler
            | Model::PadSampler
            | Model::Tr909
            | Model::Modular => Hp::None,
        }
    }

    /// Whether the filter envelope source (`ModSource::Fenv`, so the poly-mod
    /// and high-pass envelope amounts) is the ADSR: the SH-101 has one
    /// envelope for filter and loudness.
    pub fn filter_env_is_adsr(self) -> bool {
        matches!(self, Model::Sh101 | Model::Juno106)
    }

    /// Whether VCO 2 is the pulse output of VCO 1: phase-locked to it and at
    /// its pitch, as the SH-101's one oscillator gives saw and pulse together.
    pub fn pulse_locked(self) -> bool {
        matches!(self, Model::Sh101 | Model::Juno106)
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
        !matches!(
            self,
            Model::Arp2600 | Model::Sh101 | Model::Odyssey | Model::Juno106
        )
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

    /// Only a model with the slope switch has a 12 dB setting, and it is a
    /// state-variable filter beside the ladder.
    #[test]
    fn only_the_jupiter_has_a_slope_switch() {
        for (m, name) in Model::ALL {
            assert_eq!(
                m.filter_12db().is_some(),
                matches!(m, Model::Jupiter8 | Model::Matrix12),
                "{name}"
            );
        }
        assert!(matches!(Model::Jupiter8.filter(), Filter::Ladder(_)));
        assert!(matches!(
            Model::Jupiter8.filter_12db(),
            Some(Filter::Svf(_))
        ));
    }

    #[test]
    fn single_envelope_models_follow_the_adsr() {
        for (m, name) in Model::ALL {
            let single = matches!(
                m,
                Model::Arp2600 | Model::Sh101 | Model::Odyssey | Model::Juno106
            );
            assert_eq!(m.cutoff_follows_filter_env(), !single, "{name}");
        }
    }
}
