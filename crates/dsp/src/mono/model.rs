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
}

impl Model {
    /// Every model with the name the TypeScript mirror uses.
    pub const ALL: [(Model, &'static str); 6] = [
        (Model::Arp2600, "Arp2600"),
        (Model::Minimoog, "Minimoog"),
        (Model::ProOne, "ProOne"),
        (Model::Ms20, "Ms20"),
        (Model::Cs15, "Cs15"),
        (Model::Sh101, "Sh101"),
    ];

    /// The model for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Model> {
        Self::ALL
            .iter()
            .find(|(m, _)| *m as u32 == id)
            .map(|(m, _)| *m)
    }
}

impl Model {
    /// Whether the normalled cutoff follows the filter ADSR. The ARP 2600
    /// and the SH-101 have one envelope for filter and loudness, so theirs
    /// follows the ADSR (spec 004 Req 11).
    pub fn cutoff_follows_filter_env(self) -> bool {
        !matches!(self, Model::Arp2600 | Model::Sh101)
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
        assert_eq!(Model::from_id(6), None);
        assert_eq!(Model::default(), Model::Arp2600);
    }

    #[test]
    fn single_envelope_models_follow_the_adsr() {
        for (m, name) in Model::ALL {
            let single = matches!(m, Model::Arp2600 | Model::Sh101);
            assert_eq!(m.cutoff_follows_filter_env(), !single, "{name}");
        }
    }
}
