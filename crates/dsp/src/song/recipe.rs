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
    /// `<pack>/<sample>` of the samples manifest, or `@<hash>` of a dropped file.
    pub source: String,
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
        if self.window != WINDOW {
            changes.push(format!("window {}", self.window));
        }
        if self.hop != HOP {
            changes.push(format!("hop {}", self.hop));
        }
        let line = format!("{} {} = {}", self.kind.word(), self.slot, self.source);
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

/// A recipe's source: a manifest sample, `upright-piano-kw/C4vH` (a pack id,
/// then the sample's file name without `.wav`), or `@` and 32 hex digits.
pub fn is_source(s: &str) -> bool {
    if let Some(hash) = s.strip_prefix('@') {
        return hash.len() == HASH_DIGITS
            && hash
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
    }
    let Some((pack, sample)) = s.split_once('/') else {
        return false;
    };
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
    pack_ok && sample_ok
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
