//! Chord recognition (#634): the chords a clip plays, named as the song
//! language writes them (`a:m7/g`) and, in the song's key, as the roman
//! numerals the parser reads (`i7`, `bVII`).
//!
//! A set of notes is named by its pitch classes against `QUALITIES`: every
//! note of the chord sounding, at most two more as tensions or melody. The
//! choice between names is fixed: an exact match before one with extra
//! notes, then the chord with more notes, then the one whose root is the
//! bass, then `QUALITIES` order, then the root nearest above the bass. So
//! c-e-g-a over a c is `c:6`, and over an a it is `a:m7`.
//!
//! Recognition only reads; it never changes a song's text (#588). Naming a
//! set of notes doesn't allocate, so a live clip may ask during `render`.

use super::{Event, NAMES, QUALITIES, TICKS_PER_BAR};
use crate::algo::{Mode, Scale};

/// Notes a chord may carry beyond its quality and still be named.
const MAX_EXTRA: u32 = 2;
/// A short note counts toward the harmony only when it starts on a strong
/// beat, the first or third of the bar.
const STRONG: u32 = TICKS_PER_BAR / 2;
const BEAT: u32 = TICKS_PER_BAR / 4;

/// A named chord: its root's pitch class, its quality (a name in
/// `QUALITIES`) and its bass's pitch class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub root: u8,
    pub quality: &'static str,
    pub bass: u8,
}

impl Chord {
    /// The semitones of its quality above the root.
    pub fn intervals(&self) -> &'static [u8] {
        QUALITIES
            .iter()
            .find(|(q, _)| *q == self.quality)
            .map_or(&[0, 4, 7], |(_, i)| i)
    }

    /// As a chord symbol is written: `a:m7`, `g`, `c:maj7/e`.
    pub fn name(&self) -> String {
        let mut s = pc_name(self.root).to_string();
        if !self.quality.is_empty() {
            s.push(':');
            s.push_str(self.quality);
        }
        self.push_bass(&mut s);
        s
    }

    /// As the parser reads a roman numeral in `scale`'s key: `i7`, `bVII`,
    /// `viio7`, `V/b`. `None` for a scale without seven notes or a quality
    /// no numeral spells (`m7b5`, `sus2`, `6`, `9` and their kin).
    pub fn numeral(&self, scale: &Scale) -> Option<String> {
        let (upper, suffix) = match self.quality {
            "" => (true, ""),
            "m" => (false, ""),
            "7" => (true, "7"),
            "m7" => (false, "7"),
            "maj7" => (true, "maj7"),
            "dim" => (false, "o"),
            "dim7" => (false, "o7"),
            "aug" => (true, "+"),
            _ => return None,
        };
        let at = |pc: u8| (0..7).find(|d| scale.degree(*d) == Some(pc));
        let (acc, degree) = match at(self.root) {
            Some(d) => ("", d),
            None => match at((self.root + 1) % 12) {
                Some(d) => ("b", d),
                None => ("#", at((self.root + 11) % 12)?),
            },
        };
        let roman = super::NUMERALS.get(degree)?;
        let roman = if upper {
            roman.to_ascii_uppercase()
        } else {
            (*roman).to_string()
        };
        let mut s = format!("{acc}{roman}{suffix}");
        self.push_bass(&mut s);
        Some(s)
    }

    fn push_bass(&self, s: &mut String) {
        if self.bass != self.root {
            s.push('/');
            s.push_str(pc_name(self.bass));
        }
    }
}

/// The scale a chord's quality takes, as jazz chord-scale theory picks it
/// (#622); `None` for a quality without one.
pub fn chord_scale(quality: &str) -> Option<Mode> {
    let mode = match quality {
        "" | "maj" | "maj7" | "6" | "add9" => "major",
        "m" | "m7" | "m6" | "m9" => "dorian",
        "7" | "9" | "sus2" | "sus4" => "mixolydian",
        "m7b5" => "locrian",
        "dim" | "dim7" => "diminished",
        "aug" => "whole-tone",
        _ => return None,
    };
    Mode::from_name(mode)
}

fn pc_name(pc: u8) -> &'static str {
    NAMES.get(usize::from(pc % 12)).copied().unwrap_or("c")
}

/// How well a reading fits, bigger better, in the order of preference the
/// module docs give.
type Fit = (bool, u32, bool, usize, u8);

/// The chord `notes` make (MIDI notes, in any order), or `None` when they
/// are fewer than three pitch classes or match no quality.
pub fn recognize(notes: &[u8]) -> Option<Chord> {
    let bass = notes.iter().min()? % 12;
    let set = notes.iter().fold(0u16, |m, n| m | 1 << (n % 12));
    if set.count_ones() < 3 {
        return None;
    }
    let mut best: Option<(Fit, Chord)> = None;
    for up in 0..12u8 {
        let root = (bass + up) % 12;
        if set & 1 << root == 0 {
            continue;
        }
        for (k, (quality, intervals)) in QUALITIES.iter().enumerate() {
            // `maj` spells the same chord as the bare root.
            if *quality == "maj" {
                continue;
            }
            let chord = intervals
                .iter()
                .fold(0u16, |m, i| m | 1 << ((root + i) % 12));
            let extra = (set & !chord).count_ones();
            if chord & !set != 0 || extra > MAX_EXTRA {
                continue;
            }
            let score: Fit = (
                extra == 0,
                chord.count_ones(),
                root == bass,
                usize::MAX - k,
                11 - up,
            );
            if best.is_none_or(|(b, _)| score > b) {
                best = Some((
                    score,
                    Chord {
                        root,
                        quality,
                        bass,
                    },
                ));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// Does `e` count toward the harmony: a note (not a degree) that starts on
/// a strong beat or lasts a beat or more, so a passing melody note doesn't
/// rename the chord under it.
fn harmonic(e: &Event) -> bool {
    !e.degree && (e.start % STRONG == 0 || e.len >= BEAT)
}

/// The chords of a line of `bars` bars, by time: at each change of the
/// notes sounding, its start in ticks and the chord there, `None` for no
/// chord; a chord held across a change is listed once. Total: any line
/// gives a list, empty for no notes.
pub fn chords(events: &[Event], bars: u32) -> Vec<(u32, Option<Chord>)> {
    let end = bars.max(1) * TICKS_PER_BAR;
    let notes: Vec<&Event> = events.iter().filter(|e| harmonic(e)).collect();
    let mut times: Vec<u32> = notes
        .iter()
        .flat_map(|e| [e.start, e.start.saturating_add(e.len)])
        .filter(|t| *t < end)
        .collect();
    times.sort_unstable();
    times.dedup();
    let mut out: Vec<(u32, Option<Chord>)> = Vec::new();
    let mut sounding = Vec::new();
    for t in times {
        sounding.clear();
        sounding.extend(
            notes
                .iter()
                .filter(|e| e.start <= t && t < e.start.saturating_add(e.len))
                .map(|e| e.note),
        );
        let chord = recognize(&sounding);
        if sounding.is_empty() && out.is_empty() {
            continue;
        }
        if out.last().is_none_or(|(_, c)| *c != chord) {
            out.push((t, chord));
        }
    }
    out
}

/// The chords of a line on one line of text: each chord's place as
/// bar.beat (`+n` ticks past the beat when off it), its name and, with a seven-note `scale`, its numeral, `-` for
/// no chord: `1.1 a:m7 i7 | 2.1 d:9 | 3.1 g bVII`. Empty for no notes.
pub fn describe(events: &[Event], bars: u32, scale: Option<&Scale>) -> String {
    chords(events, bars)
        .iter()
        .map(|(t, c)| {
            let mut at = format!("{}.{}", t / TICKS_PER_BAR + 1, t % TICKS_PER_BAR / BEAT + 1);
            if t % BEAT != 0 {
                at.push_str(&format!("+{}", t % BEAT));
            }
            match c {
                Some(c) => match scale.and_then(|s| c.numeral(s)) {
                    Some(n) => format!("{at} {} {n}", c.name()),
                    None => format!("{at} {}", c.name()),
                },
                None => format!("{at} -"),
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}
