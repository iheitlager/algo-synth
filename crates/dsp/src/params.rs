//! The one parameter registry (ADR-0004).
//!
//! Every knob in the UI maps to `set_param(id, value)`. The ids here are the
//! source of truth; `web/src/audio/params.ts` mirrors them and a test below
//! fails if the two drift apart.

/// A parameter id as it crosses the C ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Param {
    /// Master output gain, 0..=1.
    MasterGain = 0,
    /// Test-voice attack time in seconds, 0.001..=5.
    Attack = 1,
    /// Test-voice release time in seconds, 0.005..=10.
    Release = 2,
}

impl Param {
    /// Every parameter with the name the TypeScript mirror uses.
    pub const ALL: [(Param, &'static str); 3] = [
        (Param::MasterGain, "MasterGain"),
        (Param::Attack, "Attack"),
        (Param::Release, "Release"),
    ];

    /// The parameter for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Param> {
        Self::ALL
            .iter()
            .find(|(p, _)| *p as u32 == id)
            .map(|(p, _)| *p)
    }

    /// Clamp a value into this parameter's range.
    pub fn clamp(self, v: f32) -> f32 {
        let (lo, hi) = match self {
            Param::MasterGain => (0.0, 1.0),
            Param::Attack => (0.001, 5.0),
            Param::Release => (0.005, 10.0),
        };
        if v.is_nan() { lo } else { v.clamp(lo, hi) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for (p, _) in Param::ALL {
            assert_eq!(Param::from_id(p as u32), Some(p));
        }
        assert_eq!(Param::from_id(999), None);
    }

    #[test]
    fn clamp_rejects_nan_and_out_of_range() {
        assert_eq!(Param::MasterGain.clamp(f32::NAN), 0.0);
        assert_eq!(Param::MasterGain.clamp(7.0), 1.0);
        assert_eq!(Param::Attack.clamp(0.0), 0.001);
    }

    /// ADR-0004: the TypeScript mirror names every id exactly as Rust does.
    #[test]
    fn typescript_mirror_matches() {
        let ts = include_str!("../../../web/src/audio/params.ts");
        for (p, name) in Param::ALL {
            let line = format!("{name}: {},", p as u32);
            assert!(ts.contains(&line), "web/src/audio/params.ts lacks `{line}`");
        }
    }
}
