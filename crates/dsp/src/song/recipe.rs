//! A user wavetable or attack as the recipe that makes it (ADR-0033): its
//! source sample, the analysis and the lab's transforms. The song reads and
//! prints it as `table 1 = upright-piano-kw/C4vH : inharmonic 1.6`, and the
//! lab writes it, so both use this one type.

use crate::analysis::lab::{EDITS, NO_EDITS};
use crate::table::{USER_SAMPLES, USER_TABLES};

/// The transforms by the names a recipe gives them, in `analysis::lab`'s id order.
pub const EDIT_NAMES: [&str; EDITS] = [
    "stretch",
    "inharmonic",
    "freqshift",
    "formant",
    "smear",
    "oddeven",
    "lowpass",
    "lowres",
    "highpass",
    "freeze",
    "decay",
    "noise",
    "keepformants",
];
/// The morph's positions and its table sweep by the names a recipe gives
/// them (spec 010 Req 7, 13), in `analysis::lab`'s morph id order.
pub const MORPH_NAMES: [&str; MORPHS] = ["mpitch", "mamp", "mnoise", "mformant", "sweep"];
pub use crate::analysis::lab::{MORPHS, NO_MORPH};

/// The analysis a recipe runs unless it says otherwise.
pub const WINDOW: usize = 4096;
pub const HOP: usize = 256;
/// The hex digits of a dropped file's SHA-256 its source names.
pub const HASH_DIGITS: usize = 32;

/// What a recipe fills: a PPG user table or a D-50 user attack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Table,
    Attack,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Table => "table",
            Kind::Attack => "attack",
        }
    }

    /// The slots of this kind, numbered from 1.
    pub fn slots(self) -> usize {
        match self {
            Kind::Table => USER_TABLES,
            Kind::Attack => USER_SAMPLES,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub kind: Kind,
    /// 1 to 8: "User 1" to "User 8".
    pub slot: usize,
    /// `<pack>/<sample>` of the samples manifest, `@<hash>` of a dropped file,
    /// a preset at a note or Plaits macros.
    pub source: String,
    /// A second sound to morph with (`A ~ B`), and where between them.
    pub other: Option<String>,
    pub morph: [f32; MORPHS],
    /// Every transform's value, by `analysis::lab` id.
    pub edits: [f32; EDITS],
    pub window: usize,
    pub hop: usize,
}

impl Recipe {
    pub fn new(kind: Kind, slot: usize, source: &str) -> Recipe {
        Recipe {
            kind,
            slot,
            source: source.to_string(),
            other: None,
            morph: NO_MORPH,
            edits: NO_EDITS,
            window: WINDOW,
            hop: HOP,
        }
    }

    /// The line as the song prints it: only what differs from the defaults.
    pub fn print(&self) -> String {
        let mut changes: Vec<String> = self
            .edits
            .iter()
            .zip(NO_EDITS)
            .zip(EDIT_NAMES)
            .filter(|((v, d), _)| (*v - d).abs() > 1e-6)
            .map(|((v, _), name)| format!("{name} {v}"))
            .collect();
        changes.extend(
            self.morph
                .iter()
                .zip(NO_MORPH)
                .zip(MORPH_NAMES)
                .filter(|((v, d), _)| (*v - d).abs() > 1e-6)
                .map(|((v, _), name)| format!("{name} {v}")),
        );
        if self.window != WINDOW {
            changes.push(format!("window {}", self.window));
        }
        if self.hop != HOP {
            changes.push(format!("hop {}", self.hop));
        }
        let mut line = format!("{} {} = {}", self.kind.word(), self.slot, self.source);
        if let Some(other) = &self.other {
            line = format!("{line} ~ {other}");
        }
        if changes.is_empty() {
            line
        } else {
            format!("{line} : {}", changes.join(", "))
        }
    }

    /// Set a change by its name: a transform, `window` or `hop`. False for an
    /// unknown name or a value out of reach.
    pub fn set(&mut self, name: &str, v: f32) -> bool {
        if !v.is_finite() {
            return false;
        }
        match name {
            "window" if [1024.0, 2048.0, 4096.0, 8192.0].contains(&v) => self.window = v as usize,
            "hop" if (16.0..=4096.0).contains(&v) && v.fract() == 0.0 => self.hop = v as usize,
            m if MORPH_NAMES.contains(&m) => {
                let Some(slot) = MORPH_NAMES
                    .iter()
                    .position(|n| *n == m)
                    .and_then(|i| self.morph.get_mut(i))
                else {
                    return false;
                };
                if !(0.0..=1.0).contains(&v) {
                    return false;
                }
                *slot = v;
            }
            _ => match EDIT_NAMES
                .iter()
                .position(|n| *n == name)
                .and_then(|i| self.edits.get_mut(i))
            {
                Some(e) => *e = v,
                None => return false,
            },
        }
        true
    }
}

/// Where a recipe's sound comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source<'a> {
    /// A sample of a manifest pack or kit: its id, and the file name without `.wav`.
    Pack(&'a str, &'a str),
    /// A dropped file the browser keeps, by the first hex digits of its SHA-256.
    Hash(&'a str),
    /// A factory preset rendered by the engine at a MIDI note (spec 010 Req 8).
    Synth(crate::mono::preset::Preset, u8),
    /// Plaits-style harmonics, timbre and morph macros, each 0 to 1 (spec 010 Req 8).
    Plaits(f32, f32, f32),
}

/// A note name as `c4`, `eb3` or `f#5` (C4 is MIDI 60).
pub fn note_number(s: &str) -> Option<u8> {
    let mut chars = s.chars();
    let base = match chars.next()? {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let rest: String = chars.collect();
    let (accidental, octave) = match rest.strip_prefix('#') {
        Some(o) => (1, o.to_string()),
        None => match rest.strip_prefix('b') {
            Some(o) if !o.is_empty() => (-1, o.to_string()),
            _ => (0, rest.clone()),
        },
    };
    let octave: i32 = octave.parse().ok().filter(|o| (-1..=9).contains(o))?;
    u8::try_from((octave + 1) * 12 + base + accidental)
        .ok()
        .filter(|n| *n <= 127)
}

/// A recipe's source: a manifest sample (`upright-piano-kw/C4vH`, a pack id
/// then the sample's file name without `.wav`), `@` and 32 hex digits, a
/// preset at a note (`synth:LaChoir:c4`), or macros (`plaits:0.3:0.5:0.8`).
pub fn parse_source(s: &str) -> Option<Source<'_>> {
    if let Some(hash) = s.strip_prefix('@') {
        let ok = hash.len() == HASH_DIGITS
            && hash
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
        return ok.then_some(Source::Hash(hash));
    }
    if let Some(rest) = s.strip_prefix("synth:") {
        let (name, note) = rest.split_once(':')?;
        let preset = crate::mono::preset::Preset::ALL
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(p, _)| *p)?;
        return Some(Source::Synth(preset, note_number(note)?));
    }
    if let Some(rest) = s.strip_prefix("plaits:") {
        let v: Vec<f32> = rest
            .split(':')
            .map(|x| x.parse::<f32>().ok().filter(|v| (0.0..=1.0).contains(v)))
            .collect::<Option<_>>()?;
        let [h, t, m] = v.as_slice() else {
            return None;
        };
        return Some(Source::Plaits(*h, *t, *m));
    }
    let (pack, sample) = s.split_once('/')?;
    let pack_ok = !pack.is_empty()
        && pack.len() <= 64
        && pack
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    let sample_ok = !sample.is_empty()
        && sample.len() <= 64
        && sample
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    (pack_ok && sample_ok).then_some(Source::Pack(pack, sample))
}

pub fn is_source(s: &str) -> bool {
    parse_source(s).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recipe_prints_only_its_changes() {
        let mut r = Recipe::new(Kind::Table, 1, "upright-piano-kw/C4vH");
        assert_eq!(r.print(), "table 1 = upright-piano-kw/C4vH");
        assert!(r.set("inharmonic", 1.6));
        assert!(r.set("window", 2048.0));
        assert_eq!(
            r.print(),
            "table 1 = upright-piano-kw/C4vH : inharmonic 1.6, window 2048"
        );
        assert!(!r.set("nope", 1.0));
        assert!(!r.set("window", 3000.0));
    }

    #[test]
    fn sources_are_a_pack_sample_or_a_hash() {
        assert!(is_source("upright-piano-kw/C4vH"));
        assert!(is_source(&format!("@{}", "0123456789abcdef".repeat(2))));
        for bad in [
            "piano",
            "a/b/c",
            "Piano/C4",
            "@123",
            "@0123456789ABCDEF0123456789abcdef",
            "/x",
            "x/",
        ] {
            assert!(!is_source(bad), "{bad}");
        }
    }

    #[test]
    fn a_morph_recipe_prints_both_sounds() {
        let mut r = Recipe::new(Kind::Table, 3, "upright-piano-kw/C4vH");
        r.other = Some("synth-pad-choir/C4".into());
        assert!(r.set("sweep", 1.0) && r.set("mformant", 0.3));
        assert!(!r.set("mamp", 2.0));
        assert_eq!(
            r.print(),
            "table 3 = upright-piano-kw/C4vH ~ synth-pad-choir/C4 : mformant 0.3, sweep 1"
        );
    }

    #[test]
    fn synth_and_plaits_sources() {
        assert_eq!(note_number("c4"), Some(60));
        assert_eq!(note_number("eb3"), Some(51));
        assert_eq!(note_number("f#5"), Some(78));
        assert_eq!(note_number("h4"), None);
        assert!(matches!(
            parse_source("synth:LaChoir:c4"),
            Some(Source::Synth(_, 60))
        ));
        assert_eq!(
            parse_source("plaits:0.3:0.5:0.8"),
            Some(Source::Plaits(0.3, 0.5, 0.8))
        );
        for bad in [
            "synth:Nope:c4",
            "synth:LaChoir:x9",
            "plaits:0.3:0.5",
            "plaits:2:0:0",
        ] {
            assert!(!is_source(bad), "{bad}");
        }
    }

    #[test]
    fn the_names_are_the_labs() {
        // The lab's ids and the view's mirror (web/src/audio/spectral.ts) are
        // in this order; a name per transform.
        assert_eq!(EDIT_NAMES.len(), EDITS);
        assert_eq!(EDIT_NAMES[crate::analysis::lab::INHARMONIC], "inharmonic");
        assert_eq!(
            EDIT_NAMES[crate::analysis::lab::KEEP_FORMANTS],
            "keepformants"
        );
    }
}
