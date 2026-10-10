//! Launching scenes live (#487, epic #491): Ableton's Session view in the
//! song's words (ADR-0031). A launched scene plays in place of the
//! arrangement, looping, until the next launch; going back to the
//! arrangement picks it up where the clock is, as Ableton's Back to
//! Arrangement does. A launch lands on a bar line, or at once in phase.
//!
//! The song text does not change: a launch is a performance (ADR-0018).

use crate::song::STEPS_PER_BAR;

/// When a launch lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Quantize {
    /// On the next bar line.
    Bar = 0,
    /// When the playing scene ends (its next first bar); the next bar
    /// without an arrangement.
    End = 1,
    /// On the next phrase line, every `PHRASE_BARS` bars of the clock, as a
    /// deck's start (ADR-0029).
    Phrase = 2,
    /// At once: the scene starts as far into its first bar as the clock is
    /// into its own, so it is in phase.
    Now = 3,
}

impl Quantize {
    /// Every value with the name the TypeScript mirror uses.
    pub const ALL: [(Quantize, &'static str); 4] = [
        (Quantize::Bar, "Bar"),
        (Quantize::End, "End"),
        (Quantize::Phrase, "Phrase"),
        (Quantize::Now, "Now"),
    ];

    pub fn from_id(id: u32) -> Option<Quantize> {
        Self::ALL
            .iter()
            .find(|(q, _)| *q as u32 == id)
            .map(|(q, _)| *q)
    }

    /// Whether a launch waiting for this lands on the bar line at clock step
    /// `k`; `boundary` says a scene starts there.
    pub fn due(self, k: u64, boundary: bool) -> bool {
        match self {
            Quantize::Bar | Quantize::Now => true,
            Quantize::End => boundary,
            Quantize::Phrase => k % (PHRASE_BARS * STEPS_PER_BAR) == 0,
        }
    }
}

/// A phrase, as the decks count it: eight bars.
pub const PHRASE_BARS: u64 = 8;

/// What a launch asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Scene(usize),
    /// Back to the written arrangement.
    Arrangement,
}

/// The launch state of the song: what plays in place of the arrangement,
/// and what waits for its moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    /// The launched scene and the clock step its first bar fell on.
    pub playing: Option<(usize, u64)>,
    /// What lands next, and when.
    pub queued: Option<(Target, Quantize)>,
}

impl Launch {
    /// The step of a scene of `bars` bars, launched on clock step `from`,
    /// that clock step `k` plays: it loops, also before `from` after a seek.
    pub fn local(from: u64, k: u64, bars: u32) -> u64 {
        let len = i128::from(bars.max(1)) * i128::from(STEPS_PER_BAR);
        let d = (i128::from(k) - i128::from(from)).rem_euclid(len);
        u64::try_from(d).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launched_scene_loops_from_its_bar() {
        assert_eq!(Launch::local(32, 32, 2), 0);
        assert_eq!(Launch::local(32, 47, 2), 15);
        assert_eq!(
            Launch::local(32, 64, 2),
            0,
            "two bars later it starts again"
        );
        assert_eq!(
            Launch::local(32, 31, 2),
            31,
            "before its bar, after a seek back"
        );
    }

    #[test]
    fn quantize_lands_on_its_line() {
        assert!(Quantize::Bar.due(16, false));
        assert!(!Quantize::End.due(16, false) && Quantize::End.due(16, true));
        assert!(!Quantize::Phrase.due(16, true) && Quantize::Phrase.due(128, false));
        assert_eq!(Quantize::from_id(2), Some(Quantize::Phrase));
        assert_eq!(Quantize::from_id(4), None);
    }
}
