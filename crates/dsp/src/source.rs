//! The sound sources a track can own (ADR-0005).
//!
//! One enum, no trait objects: each source is a fixed, preallocated voice
//! pool. In the base all three play the engine's test voice; they get their
//! own DSP in plan.md MVP 2 (Mono), MVP 6 (Drums) and MVP 7 (Wave).

/// A source id as it crosses the C ABI; mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Source {
    /// ARP 2600-style semi-modular mono voice: three VCOs, noise, a 4-pole
    /// ladder filter (the Moog sound lives here too), envelopes, and normalled
    /// routing that a patch can override. Six of them play the ensemble score.
    #[default]
    Mono = 0,
    /// PPG-style wavetable: 64-wave tables swept by envelope, polyphonic.
    Wave = 1,
    /// Analog-style drum kit: every pad a small synth model (808/909 lineage).
    Drums = 2,
}

impl Source {
    /// Every source with the name the TypeScript mirror uses.
    pub const ALL: [(Source, &'static str); 3] = [
        (Source::Mono, "Mono"),
        (Source::Wave, "Wave"),
        (Source::Drums, "Drums"),
    ];

    /// The source for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Source> {
        Self::ALL
            .iter()
            .find(|(s, _)| *s as u32 == id)
            .map(|(s, _)| *s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_mirror_matches() {
        let ts = include_str!("../../../web/src/audio/params.ts");
        for (s, name) in Source::ALL {
            let line = format!("{name}: {},", s as u32);
            assert!(ts.contains(&line), "web/src/audio/params.ts lacks `{line}`");
        }
    }
}
