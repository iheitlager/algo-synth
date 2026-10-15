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

pub const NUMERALS: [&str; 7] = ["i", "ii", "iii", "iv", "v", "vi", "vii"];

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
    Ok(parse(w, col, scale)?.0)
}

/// What voicing needs of a chord written by name: its root's pitch class
/// and, when an ending (`/1`, `/g`, #607) fixes one, the bass's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Shape {
    pub root: u8,
    pub bass: Option<u8>,
}

/// The shape of a chord name that has parsed before.
pub(super) fn shape_of(text: &str, scale: Option<&Scale>) -> Option<Shape> {
    let w: Vec<char> = text.chars().collect();
    parse(&w, 0, scale).ok().map(|(_, s)| s)
}

fn parse(w: &[char], col: usize, scale: Option<&Scale>) -> Result<(Vec<Pitch>, Shape), NoteError> {
    let slash = w.iter().position(|c| *c == '/');
    let head = w.get(..slash.unwrap_or(w.len())).unwrap_or(&[]);
    let (root, intervals) = if is_numeral(head) {
        numeral(head, col, scale)?
    } else {
        symbol(head, col)?
    };
    let mut notes: Vec<i32> = intervals.iter().map(|i| root + i32::from(*i)).collect();
    let bass = match slash {
        Some(k) => Some(ending(
            &mut notes,
            root,
            w.get(k + 1..).unwrap_or(&[]),
            col + k + 1,
        )?),
        None => None,
    };
    let shape = Shape {
        root: pc(root),
        bass,
    };
    Ok((chord(&notes, col)?, shape))
}

fn pc(note: i32) -> u8 {
    u8::try_from(note.rem_euclid(12)).unwrap_or(0)
}

/// The ending after `/` (#607), whose first char is at column `at`: `/1` to
/// `/3` move the lowest notes up an octave; `/g` puts a g at the bottom, the
/// chord's own (the inversion it makes) or one in the octave below the root.
/// Gives the bass's pitch class.
fn ending(notes: &mut Vec<i32>, root: i32, e: &[char], at: usize) -> Result<u8, NoteError> {
    let bad = "after / comes an inversion 1 to 3 or a bass note a to g, as c:m7/1 or c/g";
    if let [d] = e
        && let Some(n) = d.to_digit(10)
    {
        let n = usize::try_from(n).unwrap_or(0);
        if n == 0 || n > 3 {
            return err(at, bad);
        }
        if n >= notes.len() {
            return err(at, "an inversion moves fewer notes than the chord has");
        }
        for _ in 0..n {
            notes.sort_unstable();
            if let Some(low) = notes.first_mut() {
                *low += 12;
            }
        }
        notes.sort_unstable();
        return Ok(notes.first().map_or(0, |n| pc(*n)));
    }
    let name: String = e.iter().collect();
    let Some(b) = crate::algo::pitch_class(&name) else {
        return err(at, bad);
    };
    if let Some(own) = notes.iter().copied().find(|n| pc(*n) == b) {
        for n in notes.iter_mut() {
            while *n < own {
                *n += 12;
            }
        }
        notes.sort_unstable();
    } else {
        notes.insert(0, root - (root - i32::from(b)).rem_euclid(12));
    }
    Ok(b)
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

fn chord(notes: &[i32], col: usize) -> Result<Vec<Pitch>, NoteError> {
    notes
        .iter()
        .map(|n| {
            u8::try_from(*n)
                .ok()
                .filter(|n| *n <= 127)
                .map(|note| Pitch {
                    note,
                    accent: false,
                    degree: false,
                })
                .ok_or(NoteError {
                    col,
                    msg: "this chord is out of range",
                })
        })
        .collect()
}

/// A chord symbol's root note and intervals.
fn symbol(w: &[char], col: usize) -> Result<(i32, &'static [u8]), NoteError> {
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
    Ok(((octave + 1) * 12 + pc + acc, *intervals))
}

/// A roman numeral's root note and intervals in the song's key.
fn numeral(
    w: &[char],
    col: usize,
    scale: Option<&Scale>,
) -> Result<(i32, &'static [u8]), NoteError> {
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
    let intervals: &'static [u8] = match (suffix.as_str(), upper) {
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
    Ok(((OCTAVE + 1) * 12 + i32::from(pc) + acc, intervals))
}

/// How `voicing` spreads a chord (#606), as Strudel's `voicing()`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// The inversion nearest the chord before.
    #[default]
    Close,
    /// Close, with the second note from the bottom an octave up.
    Open,
    /// Close, with the second note from the top an octave down.
    Drop2,
    /// Close, with the third note from the top an octave down.
    Drop3,
    /// Root at the bottom, then only the third and seventh.
    Shell,
    /// Fourths of the song's scale stacked from the root.
    Quartal,
}

pub const STYLES: [(&str, Style); 6] = [
    ("close", Style::Close),
    ("open", Style::Open),
    ("drop2", Style::Drop2),
    ("drop3", Style::Drop3),
    ("shell", Style::Shell),
    ("quartal", Style::Quartal),
];

/// A clip's `voicing` (#103) or `voicing(style[,anchor])` (#606).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Voicing {
    /// `None` for a bare `voicing`, which is close.
    pub style: Option<Style>,
    /// The note the top of every chord stays at or below.
    pub anchor: Option<u8>,
}

impl Voicing {
    /// `voicing`, `voicing(drop2)` or `voicing(drop2,c5)`: the word's text.
    pub fn parse(text: &str) -> Result<Voicing, &'static str> {
        let styles = "a voicing is close, open, drop2, drop3, shell or quartal, then maybe an anchor note, as voicing(drop2,c5)";
        if text == "voicing" {
            return Ok(Voicing::default());
        }
        let Some(args) = text
            .strip_prefix("voicing(")
            .and_then(|r| r.strip_suffix(')'))
        else {
            return Err(styles);
        };
        let (style, anchor) = match args.split_once(',') {
            Some((s, a)) => (s.trim(), Some(a.trim())),
            None => (args.trim(), None),
        };
        let Some((_, style)) = STYLES.iter().find(|(n, _)| *n == style) else {
            return Err(styles);
        };
        let anchor = match anchor {
            Some(a) => Some(anchor_note(a).ok_or("an anchor is a note with its octave, as c5")?),
            None => None,
        };
        Ok(Voicing {
            style: Some(*style),
            anchor,
        })
    }

    pub fn print(&self) -> String {
        let name = |s: Style| STYLES.iter().find(|(_, t)| *t == s).map_or("", |(n, _)| *n);
        match (self.style, self.anchor) {
            (None, _) => "voicing".to_string(),
            (Some(s), None) => format!("voicing({})", name(s)),
            (Some(s), Some(a)) => format!("voicing({},{})", name(s), super::note_name(a)),
        }
    }
}

/// `c5`, `f#4`: a note with its octave, as an anchor.
fn anchor_note(a: &str) -> Option<u8> {
    let w: Vec<char> = a.chars().collect();
    if !is_note(&w) || a.ends_with('!') {
        return None;
    }
    let (name, oct) = a.split_at(a.len() - 1);
    let oct = i32::try_from(oct.parse::<u32>().ok()?).ok()?;
    let n = (oct + 1) * 12 + i32::from(crate::algo::pitch_class(name)?);
    // `cb4` and `b#4` name the pitch class, not the octave's edge.
    let n = match name {
        "cb" => n + 12,
        "b#" => n - 12,
        _ => n,
    };
    u8::try_from(n).ok().filter(|n| *n <= 127)
}

/// Lowest and highest note a voiced chord may use: C3 to C6.
const RANGE: (i32, i32) = (48, 84);

/// Voice each chord (events sharing a start, two or more) in `v`'s style,
/// near the chord before: the close notes each go to the octave nearest the
/// previous close chord's centre, within C3–C6 or the three octaves under
/// the anchor; the style then moves notes by octaves and the anchor caps
/// the top. A fixed bass (`shapes`, by word: a chord's `/` ending, or the root
/// of a shell) stays out of that and goes under the rest. Single notes stay
/// as they are.
pub(super) fn voice(
    events: &mut Vec<Event>,
    shapes: &[Option<Shape>],
    v: Voicing,
    scale: Option<&Scale>,
) {
    let style = v.style.unwrap_or_default();
    let range = v.anchor.map_or(RANGE, |a| {
        (i32::from(a) - (RANGE.1 - RANGE.0), i32::from(a))
    });
    let mut centre: f32 = v.anchor.map_or(60.0, |a| f32::from(a) - 6.0);
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    let mut i = 0;
    while i < events.len() {
        let start = events.get(i).map_or(0, |e| e.start);
        let mut j = i;
        while events.get(j).is_some_and(|e| e.start == start) {
            j += 1;
        }
        let group = events.get(i..j).unwrap_or(&[]);
        i = j.max(i + 1);
        if group.len() < 2 {
            out.extend_from_slice(group);
            continue;
        }
        let word = group.first().map_or(0, |e| e.word);
        let shape = shapes
            .get(usize::from(word))
            .copied()
            .flatten()
            .filter(|_| group.iter().all(|e| e.word == word));
        let lowest = group.iter().map(|e| e.note).min().unwrap_or(0);
        let root = shape.map_or(lowest % 12, |s| s.root);
        let bass_pc = shape
            .and_then(|s| s.bass)
            .or((style == Style::Shell).then_some(root));
        // The bass is the lowest note of its pitch class, as written.
        let mut upper: Vec<Event> = group.to_vec();
        upper.sort_by_key(|e| e.note);
        let bass = bass_pc
            .and_then(|b| upper.iter().position(|e| e.note % 12 == b))
            .map(|k| upper.remove(k));
        if style == Style::Shell {
            let iv = |e: &Event| (i32::from(e.note) - i32::from(root)).rem_euclid(12);
            let third = if upper.iter().any(|e| matches!(iv(e), 3 | 4)) {
                [3, 4]
            } else {
                [2, 5]
            };
            upper.retain(|e| matches!(iv(e), 0 | 9..=11) || third.contains(&iv(e)));
        }
        if style == Style::Quartal {
            quartal(&mut upper, root, centre, range, scale);
        } else {
            close(&mut upper, centre, range);
        }
        // The voices lead from the close chord, so a drop does not drift
        // the next chord down.
        if !upper.is_empty() {
            centre = upper.iter().map(|e| f32::from(e.note)).sum::<f32>() / upper.len() as f32;
        }
        let n = upper.len();
        let moved = match style {
            Style::Open if n >= 3 => Some((1, 12)),
            Style::Drop2 if n >= 3 => Some((n - 2, -12)),
            Style::Drop3 if n >= 4 => Some((n - 3, -12)),
            _ => None,
        };
        if let Some((k, by)) = moved
            && let Some(e) = upper.get_mut(k)
        {
            e.note = shift(e.note, by);
        }
        if let Some(a) = v.anchor {
            while upper.iter().any(|e| e.note > a) && upper.iter().all(|e| e.note >= 12) {
                for e in &mut upper {
                    e.note -= 12;
                }
            }
        }
        if let Some(mut b) = bass {
            let floor = upper
                .iter()
                .map(|e| i32::from(e.note))
                .min()
                .unwrap_or(range.0);
            let pc = i32::from(b.note % 12);
            let n = floor - 1 - (floor - 1 - pc).rem_euclid(12);
            b.note = u8::try_from(n).unwrap_or(b.note);
            out.push(b);
        }
        out.extend(upper);
    }
    out.sort_unstable_by_key(super::sort_key);
    *events = out;
}

fn shift(note: u8, by: i32) -> u8 {
    u8::try_from(i32::from(note) + by)
        .ok()
        .filter(|n| *n <= 127)
        .unwrap_or(note)
}

/// Each note to the octave of its pitch class nearest `centre`, in `range`
/// and not one already taken by the chord.
fn close(notes: &mut [Event], centre: f32, range: (i32, i32)) {
    let mut used: Vec<i32> = Vec::with_capacity(notes.len());
    for e in notes.iter_mut() {
        let pc = i32::from(e.note % 12);
        let mut best = None;
        for oct in 0..11 {
            let n = oct * 12 + pc;
            if n < range.0 || n > range.1 || used.contains(&n) {
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
    notes.sort_unstable_by_key(|e| e.note);
}

/// The notes become a stack of fourths from `root`, each the scale note
/// three steps up (a perfect fourth when the root is not in the scale), the
/// stack in the octave whose middle is nearest `centre` inside `range`.
fn quartal(notes: &mut [Event], root: u8, centre: f32, range: (i32, i32), scale: Option<&Scale>) {
    let iv = scale.map(|s| (s.root, s.mode.intervals()));
    let step = |at: i32| -> i32 {
        let Some((sr, iv)) = iv else { return 5 };
        let rel = u8::try_from((at - i32::from(sr)).rem_euclid(12)).unwrap_or(0);
        let Some(k) = iv.iter().position(|i| *i == rel) else {
            return 5;
        };
        let next = iv.get((k + 3) % iv.len()).copied().unwrap_or(0);
        (i32::from(next) - i32::from(rel)).rem_euclid(12)
    };
    let mut stack = Vec::with_capacity(notes.len());
    let mut at = i32::from(root);
    for _ in 0..notes.len() {
        stack.push(at);
        at += step(at);
    }
    let (lo, hi) = (
        stack.first().copied().unwrap_or(0),
        stack.last().copied().unwrap_or(0),
    );
    let mid = (lo + hi) as f32 / 2.0;
    let mut best: Option<(i32, f32)> = None;
    for oct in -1..11 {
        let by = oct * 12;
        if lo + by < range.0 || hi + by > range.1 {
            continue;
        }
        let d = (mid + by as f32 - centre).abs();
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((by, d));
        }
    }
    let by = best.map_or(range.0 - range.0.rem_euclid(12), |(b, _)| b);
    for (e, n) in notes.iter_mut().zip(stack) {
        e.note = u8::try_from((n + by).clamp(0, 127)).unwrap_or(e.note);
    }
}
