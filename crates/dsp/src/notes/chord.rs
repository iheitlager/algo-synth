//! Chords by name (#103): symbols (`c:m7`, `f3:maj7`, `bb`) and roman
//! numerals in the song's key (`i VI III VII`, `V7`, `viio`, `bVII`), each
//! resolved to its notes when the line is parsed, and voicing, which moves
//! each chord to the inversion nearest the one before it.
//!
//! A symbol is a root, a to g with `#` or `b`, an optional octave (4 when
//! left out) and `:quality`; a bare root is a major triad. A numeral is a
//! degree of the song's scale, maybe `b` or `#`, its case the quality
//! (upper major, lower minor), then `o` (diminished), `o7`, `+` (augmented),
//! `7` or `maj7`. The text is kept as written, so it prints back the same.

use super::{NoteError, Pitch, err};
use crate::algo::Scale;
use crate::notes::Event;

/// Qualities by name, as semitones above the root.
pub const QUALITIES: [(&str, &[u8]); 17] = [
    ("", &[0, 4, 7]),
    ("maj", &[0, 4, 7]),
    ("m", &[0, 3, 7]),
    ("7", &[0, 4, 7, 10]),
    ("maj7", &[0, 4, 7, 11]),
    ("m7", &[0, 3, 7, 10]),
    ("m7b5", &[0, 3, 6, 10]),
    ("dim", &[0, 3, 6]),
    ("dim7", &[0, 3, 6, 9]),
    ("aug", &[0, 4, 8]),
    ("sus2", &[0, 2, 7]),
    ("sus4", &[0, 5, 7]),
    ("6", &[0, 4, 7, 9]),
    ("m6", &[0, 3, 7, 9]),
    ("9", &[0, 4, 7, 10, 14]),
    ("m9", &[0, 3, 7, 10, 14]),
    ("add9", &[0, 4, 7, 14]),
];

const NUMERALS: [&str; 7] = ["i", "ii", "iii", "iv", "v", "vi", "vii"];

/// The octave a chord's root sits in when none is written.
const OCTAVE: i32 = 4;

/// `c4`, `f#3`, `eb2!`: a single note, not a chord.
pub(super) fn is_note(w: &[char]) -> bool {
    let rest = match w {
        [l, r @ ..] if ('a'..='g').contains(l) => r,
        _ => return false,
    };
    let rest = match rest {
        ['#' | 'b', r @ ..] if !r.is_empty() => r,
        r => r,
    };
    matches!(rest, [d] | [d, '!'] if d.is_ascii_digit())
}

/// The notes of the chord named by `w`, whose first char is at column `col`.
pub(super) fn notes_of(
    w: &[char],
    col: usize,
    scale: Option<&Scale>,
) -> Result<Vec<Pitch>, NoteError> {
    if is_numeral(w) {
        numeral(w, col, scale)
    } else {
        symbol(w, col)
    }
}

/// Does `w` start as a roman numeral: maybe `b` or `#`, then `i` or `v` in
/// either case? A root `b` is never followed by those.
fn is_numeral(w: &[char]) -> bool {
    let w = match w {
        ['b' | '#', r @ ..] => r,
        r => r,
    };
    w.first()
        .is_some_and(|c| matches!(c, 'i' | 'v' | 'I' | 'V'))
}

fn chord(root: i32, intervals: &[u8], col: usize) -> Result<Vec<Pitch>, NoteError> {
    intervals
        .iter()
        .map(|i| {
            u8::try_from(root + i32::from(*i))
                .ok()
                .filter(|n| *n <= 127)
                .map(|note| Pitch {
                    note,
                    accent: false,
                })
                .ok_or(NoteError {
                    col,
                    msg: "this chord is out of range",
                })
        })
        .collect()
}

fn symbol(w: &[char], col: usize) -> Result<Vec<Pitch>, NoteError> {
    let bad = "a chord is a root a to g, maybe # or b and an octave, then :quality, as c:m7";
    let mut i = 0;
    let pc: i32 = match w.first() {
        Some('c') => 0,
        Some('d') => 2,
        Some('e') => 4,
        Some('f') => 5,
        Some('g') => 7,
        Some('a') => 9,
        Some('b') => 11,
        _ => {
            return err(
                col,
                "a note is a letter a to g, maybe # or b, and an octave 0 to 9, as c4",
            );
        }
    };
    i += 1;
    let acc = match w.get(i) {
        Some('#') => 1,
        Some('b') => -1,
        _ => 0,
    };
    if acc != 0 {
        i += 1;
    }
    let mut octave = OCTAVE;
    if let Some(d) = w.get(i).and_then(|c| c.to_digit(10)) {
        octave = i32::try_from(d).unwrap_or(OCTAVE);
        i += 1;
    }
    let quality: String = match w.get(i) {
        None => String::new(),
        Some(':') => w.get(i + 1..).unwrap_or(&[]).iter().collect(),
        Some(_) if i == 1 && acc == 0 && octave == OCTAVE => return err(col + i, bad),
        Some(_) => return err(col + i, "a word ends at a space"),
    };
    let Some((_, intervals)) = QUALITIES.iter().find(|(q, _)| *q == quality) else {
        let at = col + i + 1;
        // `"c:m7:4"`: a duration inside quotes.
        if let Some(k) = quality.find(':') {
            return err(at + k, "durations go outside the quotes, as c4:4");
        }
        // `"c4:4"`: a duration inside quotes, not a quality.
        if matches!(quality.as_str(), "1" | "2" | "4" | "8" | "16") {
            return err(at - 1, "durations go outside the quotes, as c4:4");
        }
        return err(
            at,
            "a quality is maj, m, 7, maj7, m7, m7b5, dim, dim7, aug, sus2, sus4, 6, m6, 9, m9 or add9",
        );
    };
    chord((octave + 1) * 12 + pc + acc, intervals, col)
}

fn numeral(w: &[char], col: usize, scale: Option<&Scale>) -> Result<Vec<Pitch>, NoteError> {
    let bad = "a numeral is I to VII, maybe b or #, then o, o7, +, 7 or maj7";
    let mut i = 0;
    let acc = match w.first() {
        Some('b') => -1,
        Some('#') => 1,
        _ => 0,
    };
    if acc != 0 {
        i += 1;
    }
    let start = i;
    while w.get(i).is_some_and(|c| matches!(c, 'i' | 'v' | 'I' | 'V')) {
        i += 1;
    }
    let roman: String = w.get(start..i).unwrap_or(&[]).iter().collect();
    let upper = roman.chars().all(|c| c.is_ascii_uppercase());
    if !upper && !roman.chars().all(|c| c.is_ascii_lowercase()) {
        return err(
            col + start,
            "a numeral is all upper case (major) or all lower case (minor)",
        );
    }
    let Some(degree) = NUMERALS
        .iter()
        .position(|n| *n == roman.to_ascii_lowercase())
    else {
        return err(col + start, bad);
    };
    let suffix: String = w.get(i..).unwrap_or(&[]).iter().collect();
    let intervals: &[u8] = match (suffix.as_str(), upper) {
        ("", true) => &[0, 4, 7],
        ("", false) => &[0, 3, 7],
        ("7", true) => &[0, 4, 7, 10],
        ("7", false) => &[0, 3, 7, 10],
        ("maj7", true) => &[0, 4, 7, 11],
        ("maj7", false) => &[0, 3, 7, 11],
        ("o", _) => &[0, 3, 6],
        ("o7", _) => &[0, 3, 6, 9],
        ("+", _) => &[0, 4, 8],
        _ => return err(col + i, bad),
    };
    let Some(pc) = scale.and_then(|s| s.degree(degree)) else {
        return err(
            col,
            "a roman numeral needs a scale line with seven notes, as scale c minor",
        );
    };
    chord((OCTAVE + 1) * 12 + i32::from(pc) + acc, intervals, col)
}

/// Lowest and highest note a voiced chord may use: C3 to C6.
const RANGE: (i32, i32) = (48, 84);

/// Move each chord (events sharing a start, two or more) to the inversion
/// and octave nearest the chord before it: each note goes to the octave
/// closest to the previous chord's centre. Single notes stay as they are.
pub(super) fn voice(events: &mut [Event]) {
    let mut centre: f32 = 60.0;
    let mut i = 0;
    while i < events.len() {
        let start = events.get(i).map_or(0, |e| e.start);
        let mut j = i;
        while events.get(j).is_some_and(|e| e.start == start) {
            j += 1;
        }
        if let Some(group) = events.get_mut(i..j).filter(|g| g.len() >= 2) {
            let mut used: Vec<i32> = Vec::with_capacity(group.len());
            for e in group.iter_mut() {
                let pc = i32::from(e.note % 12);
                // The note of this pitch class nearest the centre, in range,
                // and not one already taken by the chord.
                let mut best = None;
                for oct in 0..11 {
                    let n = oct * 12 + pc;
                    if n < RANGE.0 || n > RANGE.1 || used.contains(&n) {
                        continue;
                    }
                    let d = (n as f32 - centre).abs();
                    if best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((n, d));
                    }
                }
                if let Some((n, _)) = best {
                    used.push(n);
                    e.note = u8::try_from(n).unwrap_or(e.note);
                }
            }
            centre = group.iter().map(|e| f32::from(e.note)).sum::<f32>() / group.len() as f32;
        }
        i = j.max(i + 1);
    }
}
