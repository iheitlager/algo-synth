//! Completion for the song editor (#595, #642): the words that fit where the
//! cursor is, from the lists the parser reads, so the view only shows them
//! (ADR-0001). Like the lexer it reads the text, not the song: the line the
//! cursor is on, and the names made on the lines above it, as a name is used
//! after the line that makes it. A text with an error still completes. It is
//! total: any text and cursor give a list, maybe empty, never a panic.
//!
//! Offsets count UTF-16 units, as the view indexes its strings.

use super::lex::{Class, Under};
use super::{ALIASES, GROUPS, Kind, STRIPS, current_keyword, fits, signal};
use crate::algo::Mode;
use crate::mono::model::Model;
use crate::mono::preset::Preset;
use crate::notes::pattern::Pattern;
use crate::notes::{self, ArpMode};
use crate::params::Param;

/// Most words a list holds.
pub const MAX_ITEMS: usize = 50;

/// A word that fits.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// How the view colours it, as the lexer would.
    pub class: Class,
    pub word: String,
    /// One line on what it is.
    pub detail: String,
}

/// The words that may replace the text from `from` to `to`: the part of a
/// word typed so far, empty after a `.`, `(`, `:` or a space.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Completion {
    pub from: u32,
    pub to: u32,
    pub items: Vec<Item>,
}

/// As the view reads it: `from\tto`, then `class\tword\tdetail`, a line each.
impl std::fmt::Display for Completion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}\t{}", self.from, self.to)?;
        for i in &self.items {
            writeln!(f, "{}\t{}\t{}", i.class as u8, i.word, i.detail)?;
        }
        Ok(())
    }
}

/// What each line keyword takes, for its detail.
const KEYWORD_USAGE: [(&str, &str); 20] = [
    ("tempo", "tempo <bpm>"),
    ("swing", "swing <percent>"),
    ("scale", "scale <root> <mode>"),
    (
        "setting",
        "setting <name> = <Model> <Preset>: <Param> <value>, …",
    ),
    ("track", "track <name> <kind> [<Model> [<Preset>]]"),
    ("samples", "samples <track> <id>"),
    ("zone", "zone <track> <n> = <source>"),
    ("pad", "pad <track> <n> = <source>"),
    ("table", "table <n> = <source>"),
    ("attack", "attack <n> = <source>"),
    ("strip", "strip <track>: <Param> <value>, …"),
    ("group", "group <n> [<name>]: <Param> <value>, …"),
    ("master", "master: <Param> <value>, …"),
    ("clip", "clip <name> = <track> [.<method>(…)]…"),
    ("auto", "auto <name> = <target>.<Param> <value>… /<bars>"),
    ("snapshot", "snapshot <name>: <target>.<Param> <value>, …"),
    ("mod", "mod <target>.<param> = <signal>"),
    ("scene", "scene <name> <bars>: <clip>…"),
    ("arrange", "arrange <scene>…"),
    ("loop", "loop <first> <last>"),
];

/// What each pattern method does (`Pattern::NAMES`).
const METHODS: [(&str, &str); 17] = [
    ("fast", "n times faster"),
    ("slow", "n times slower"),
    ("rev", "each bar backwards"),
    ("palindrome", "every other bar backwards"),
    ("add", "n semitones up"),
    ("sub", "n semitones down"),
    ("ply", "each note n times"),
    ("iter", "each bar starts 1/n further in"),
    ("degrade", "drop notes by chance"),
    ("every", "a method on every nth bar"),
    ("off", "a later copy with a method"),
    ("struct", "the notes on a rhythm"),
    ("euclid", "the notes on k hits of n steps"),
    ("sometimes", "a method on about half"),
    (
        "scale",
        "the notes onto a scale, or an earlier clip's roots",
    ),
    ("oct", "n octaves up or down"),
    ("chord", "degrees onto an earlier clip's chords"),
];

/// What each generator call plays (`notes::CALLS`, and `euclid`).
const CALLS: [(&str, &str); 7] = [
    ("arp", "a chord or a clip's chords, arpeggiated"),
    ("walk", "a random walk on the scale"),
    ("markov", "a chain learned from a clip"),
    ("mutate", "a clip with some notes moved"),
    ("root", "the roots of a clip's chords"),
    ("prog", "a chord progression on the scale"),
    ("euclid", "hits spread evenly over steps"),
];

/// What each drum pad is (`Pad::ALL`).
const PADS: [(&str, &str); 18] = [
    ("bd", "kick"),
    ("sn", "snare"),
    ("cp", "clap"),
    ("ch", "closed hat"),
    ("oh", "open hat"),
    ("lt", "low tom"),
    ("ht", "high tom"),
    ("cb", "cowbell"),
    ("rs", "rimshot"),
    ("cl", "claves"),
    ("ma", "maracas"),
    ("cy", "cymbal"),
    ("mt", "mid tom"),
    ("lc", "low conga"),
    ("mc", "mid conga"),
    ("hc", "high conga"),
    ("cr", "crash"),
    ("rd", "ride"),
];

/// The words of a clip line after its track.
const CLIP_WORDS: [(&str, &str); 3] = [
    ("live", "new notes every bar"),
    ("bars", "how many bars timed notes run"),
    ("voicing", "each chord near the one before"),
];

/// An LFO's shapes.
const SHAPES: [&str; 5] = ["sine", "cosine", "saw", "tri", "square"];

/// The words that fit at `cursor` in `text`.
pub fn complete(text: &str, cursor: u32) -> Completion {
    let cur = byte_at(text, cursor);
    let head = text.get(..cur).unwrap_or("");
    let start = head.rfind('\n').map_or(0, |i| i + 1);
    let before = head.get(start..).unwrap_or("");
    let names = Names::above(head.get(..start).unwrap_or(""));
    let typed: usize = before
        .chars()
        .rev()
        .take_while(|c| is_word(*c))
        .map(char::len_utf8)
        .sum();
    let (pre, word) = before.split_at(before.len() - typed);
    let mut list = List {
        prefix: word,
        items: Vec::new(),
    };
    // Not in a comment.
    if uncommented(before).len() == before.len() {
        if before.starts_with([' ', '\t']) {
            names.indented(&mut list, pre);
        } else {
            names.top(&mut list, pre);
        }
    }
    let to = head.encode_utf16().count() as u32;
    Completion {
        from: to - word.encode_utf16().count() as u32,
        to,
        items: list.items,
    }
}

/// The byte offset of UTF-16 unit `at` in `text`, at a char boundary.
fn byte_at(text: &str, at: u32) -> usize {
    let mut units = 0;
    for (b, c) in text.char_indices() {
        if units >= at {
            return b;
        }
        units += c.len_utf16() as u32;
    }
    text.len()
}

/// A char of a word that completes: names, modes with their dashes.
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// A line without its comment: `#` at the start or after a space.
fn uncommented(line: &str) -> &str {
    let mut prev_space = true;
    for (i, c) in line.char_indices() {
        if c == '#' && prev_space {
            return line.get(..i).unwrap_or(line);
        }
        prev_space = c.is_whitespace();
    }
    line
}

/// The list being made: words that start with the prefix, in any case, once.
struct List<'a> {
    prefix: &'a str,
    items: Vec<Item>,
}

impl List<'_> {
    fn wants(&self, word: &str) -> bool {
        self.items.len() < MAX_ITEMS
            && word
                .as_bytes()
                .get(..self.prefix.len())
                .is_some_and(|w| w.eq_ignore_ascii_case(self.prefix.as_bytes()))
            && !self.items.iter().any(|i| i.word == word)
    }

    fn add(&mut self, class: Class, word: &str, detail: impl FnOnce() -> String) {
        if self.wants(word) {
            self.items.push(Item {
                class,
                word: word.to_string(),
                detail: detail(),
            });
        }
    }

    fn table(&mut self, class: Class, table: &[(&str, &str)]) {
        for (w, d) in table {
            self.add(class, w, || (*d).to_string());
        }
    }

    fn keywords(&mut self) {
        self.table(Class::Keyword, &KEYWORD_USAGE);
    }

    fn calls(&mut self) {
        self.table(Class::Call, &CALLS);
    }

    fn pads(&mut self) {
        self.table(Class::Pad, &PADS);
    }

    fn methods(&mut self, pattern: bool) {
        if pattern {
            self.table(Class::Call, &METHODS);
        }
        self.params(Set::Track, true);
    }

    fn kinds(&mut self) {
        for (k, d) in [
            ("drums", "lanes of pads"),
            ("synth", "one line of notes"),
            ("sampler", "lanes or notes"),
        ] {
            self.add(Class::Keyword, k, || d.to_string());
        }
    }

    /// The models that play a track of `kind`.
    fn models(&mut self, kind: Option<Kind>) {
        for (m, name) in Model::ALL {
            if kind.is_none_or(|k| fits(k, Some(m))) {
                self.add(Class::Name, name, || {
                    let plays: Vec<&str> = [Kind::Drums, Kind::Synth, Kind::Sampler]
                        .into_iter()
                        .filter(|k| fits(*k, Some(m)))
                        .map(Kind::name)
                        .collect();
                    format!("a model for {}", plays.join(", "))
                });
            }
        }
    }

    /// The presets of the model named `model`.
    fn presets(&mut self, model: &str) {
        let Some(m) = Model::ALL.iter().find(|(_, n)| *n == model).map(|m| m.0) else {
            return;
        };
        for (p, name) in Preset::ALL {
            if p.model() == m {
                self.add(Class::Name, name, || format!("a {model} preset"));
            }
        }
    }

    fn modes(&mut self) {
        for m in Mode::all() {
            for name in m.names() {
                self.add(Class::Name, name, || semitones(m.intervals()));
            }
        }
    }

    fn qualities(&mut self) {
        for (q, steps) in notes::QUALITIES {
            if !q.is_empty() {
                self.add(Class::Note, q, || semitones(steps));
            }
        }
    }

    /// Roman numerals: a degree of the scale, upper case major.
    fn numerals(&mut self) {
        for (k, n) in notes::NUMERALS.iter().enumerate() {
            let degree = k + 1;
            self.add(Class::Note, &n.to_uppercase(), || {
                format!("degree {degree}, major")
            });
            self.add(Class::Note, n, || format!("degree {degree}, minor"));
        }
    }

    fn arp_modes(&mut self) {
        for (_, m) in ArpMode::ALL {
            let d = if m == "random" {
                "random order, with a seed"
            } else {
                "arp mode"
            };
            self.add(Class::Name, m, || d.to_string());
        }
    }

    /// A signal's sources, or after a `.` its methods.
    fn signal(&mut self, dot: bool) {
        let (words, d) = if dot {
            (&signal::METHODS[..], "signal method")
        } else {
            (&signal::SOURCES[..], "signal")
        };
        for w in words {
            self.add(Class::Call, w, || d.to_string());
        }
    }

    /// The parameters of `set`; `lower` as methods and `mod` lines write
    /// them, with Strudel's names first.
    fn params(&mut self, set: Set, lower: bool) {
        let detail = |p: Param| {
            let (lo, hi) = p.range();
            format!("{lo}–{hi}")
        };
        if lower {
            for (a, p) in ALIASES {
                if set.has(p) {
                    self.add(Class::Param, a, || {
                        format!("{} {}", super::param_name(p), detail(p))
                    });
                }
            }
        }
        for (p, name) in Param::ALL {
            if set.has(p) && self.wants(name) {
                let word = if lower {
                    name.to_lowercase()
                } else {
                    name.to_string()
                };
                self.add(Class::Param, &word, || detail(p));
            }
        }
    }

    /// The parameters of `target`: a track, `stripN`, `groupN` or `master`.
    fn target_params(&mut self, target: &str, lower: bool) {
        let numbered = |prefix: &str| {
            target
                .strip_prefix(prefix)
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        };
        let set = if target == "master" {
            Set::Global
        } else if numbered("group") {
            Set::Strip
        } else {
            Set::Track
        };
        self.params(set, lower);
    }
}

/// Which parameters a place takes, as `param_for` and the setting line check.
#[derive(Clone, Copy)]
enum Set {
    /// A track or `stripN`: its synth's and its strip's.
    Track,
    /// A group's strip, or a mixer line's.
    Strip,
    /// `master`'s.
    Global,
    /// A setting's: the synth's own.
    Synth,
}

impl Set {
    fn has(self, p: Param) -> bool {
        if matches!(p, Param::Model | Param::Out) {
            return false;
        }
        match self {
            Set::Track => !p.is_global(),
            Set::Strip => p.is_strip(),
            Set::Global => p.is_global(),
            Set::Synth => !p.is_global() && !p.is_strip(),
        }
    }
}

/// `0 3 7`: steps above the root.
fn semitones(steps: &[u8]) -> String {
    let s: Vec<String> = steps.iter().map(u8::to_string).collect();
    s.join(" ")
}

/// The call whose parentheses the cursor is in: its name, which argument
/// (from 0), and that argument so far.
struct Call<'a> {
    name: &'a str,
    arg: usize,
    text: &'a str,
}

/// The innermost open call in `pre`; `None` inside brackets or quotes.
fn open_call(pre: &str) -> Option<Call<'_>> {
    enum Open<'a> {
        Call(&'a str, usize, usize),
        Bracket,
    }
    let mut stack: Vec<Open> = Vec::new();
    let mut quoted = false;
    for (i, c) in pre.char_indices() {
        match c {
            '"' => quoted = !quoted,
            _ if quoted => {}
            '(' => {
                let head = pre.get(..i).unwrap_or("");
                let n: usize = head
                    .chars()
                    .rev()
                    .take_while(|c| is_word(*c))
                    .map(char::len_utf8)
                    .sum();
                let name = head.get(head.len() - n..).unwrap_or("");
                stack.push(Open::Call(name, 0, i + 1));
            }
            '[' => stack.push(Open::Bracket),
            ')' | ']' => {
                stack.pop();
            }
            ',' => {
                if let Some(Open::Call(_, arg, start)) = stack.last_mut() {
                    *arg += 1;
                    *start = i + 1;
                }
            }
            _ => {}
        }
    }
    match stack.last() {
        Some(Open::Call(name, arg, start)) if !quoted => Some(Call {
            name,
            arg: *arg,
            text: pre.get(*start..).unwrap_or(""),
        }),
        _ => None,
    }
}

/// The word just before `pre`'s last char (a `.` or `:`).
fn word_before(pre: &str) -> &str {
    word_before_by(pre, is_word)
}

/// The word just before `pre`'s last char, of chars that are `word`'s.
fn word_before_by(pre: &str, word: fn(char) -> bool) -> &str {
    let head = pre.get(..pre.len().saturating_sub(1)).unwrap_or("");
    let n: usize = head
        .chars()
        .rev()
        .take_while(|c| word(*c))
        .map(char::len_utf8)
        .sum();
    head.get(head.len() - n..).unwrap_or("")
}

/// `c`, `bb`, `f#3`: a chord's root, maybe with an octave.
fn is_root(w: &str) -> bool {
    let mut cs = w.chars();
    if !cs.next().is_some_and(|c| ('a'..='g').contains(&c)) {
        return false;
    }
    let rest = cs.as_str();
    let rest = rest
        .strip_prefix('#')
        .or_else(|| rest.strip_prefix('b'))
        .unwrap_or(rest);
    rest.len() <= 1 && rest.bytes().all(|b| b.is_ascii_digit())
}

/// The names made above the cursor's line.
struct Names<'a> {
    /// Name and kind.
    tracks: Vec<(&'a str, &'a str)>,
    settings: Vec<&'a str>,
    /// Name and track.
    clips: Vec<(&'a str, &'a str)>,
    autos: Vec<&'a str>,
    snapshots: Vec<&'a str>,
    /// Name and bars.
    scenes: Vec<(&'a str, &'a str)>,
    /// What the indented lines under the last top-level line hold.
    under: Under,
    /// The clip they belong to, which can't read itself.
    open: Option<&'a str>,
}

impl<'a> Names<'a> {
    fn above(text: &'a str) -> Names<'a> {
        let mut n = Names {
            tracks: Vec::new(),
            settings: Vec::new(),
            clips: Vec::new(),
            autos: Vec::new(),
            snapshots: Vec::new(),
            scenes: Vec::new(),
            under: Under::Nothing,
            open: None,
        };
        for raw in text.split('\n') {
            if raw.starts_with([' ', '\t']) {
                continue;
            }
            let w: Vec<&str> = uncommented(raw).split_whitespace().collect();
            let Some(first) = w.first() else {
                continue;
            };
            let at = |k: usize| w.get(k).copied();
            let name = || at(1).map(|s| s.trim_end_matches(':'));
            n.under = Under::Nothing;
            n.open = None;
            match current_keyword(first, at(1), at(2)) {
                "track" => {
                    if let (Some(t), Some(k)) = (at(1), at(2)) {
                        n.tracks.push((t, k));
                    }
                }
                "setting" => {
                    n.settings.extend(name());
                    if at(3) == Some("Modular") {
                        n.under = Under::Code;
                    }
                }
                "clip" => {
                    if let (Some(c), Some(t)) = (at(1), at(3)) {
                        n.clips.push((c, t));
                        n.open = Some(c);
                        n.under = match n.kind(t) {
                            Some("drums") => Under::Lanes,
                            Some("synth") => Under::Notes,
                            Some("sampler") => Under::Either,
                            _ => Under::Nothing,
                        };
                    }
                }
                "auto" => n.autos.extend(name()),
                "snapshot" => n.snapshots.extend(name()),
                "scene" => {
                    if let Some(s) = name() {
                        n.scenes
                            .push((s, at(2).unwrap_or("").trim_end_matches(':')));
                    }
                }
                _ => {}
            }
        }
        n
    }

    /// The kind of the track named `track`.
    fn kind(&self, track: &str) -> Option<&'a str> {
        self.tracks
            .iter()
            .rev()
            .find(|(n, _)| *n == track)
            .map(|(_, k)| *k)
    }

    fn tracks(&self, list: &mut List) {
        for (t, k) in &self.tracks {
            list.add(Class::Name, t, || format!("a {k} track"));
        }
    }

    /// Clips of notes, which a call or `.scale(<clip>)` may read.
    fn note_clips(&self, list: &mut List) {
        for (c, t) in &self.clips {
            if matches!(self.kind(t), Some("synth" | "sampler")) && self.open != Some(c) {
                list.add(Class::Name, c, || format!("a clip on {t}"));
            }
        }
    }

    fn targets(&self, list: &mut List) {
        self.tracks(list);
        list.add(Class::Name, "master", || "the master".to_string());
        for k in 1..=STRIPS {
            list.add(Class::Name, &format!("strip{k}"), || format!("strip {k}"));
        }
        for k in 1..=GROUPS {
            list.add(Class::Name, &format!("group{k}"), || {
                format!("group bus {k}")
            });
        }
    }

    /// An indented line: a lane, a line of notes, or Modular code.
    fn indented(&self, list: &mut List, pre: &str) {
        let first = pre.trim().is_empty();
        match self.under {
            Under::Lanes if first => list.pads(),
            Under::Lanes => self.lane(list, pre),
            Under::Notes => self.notes(list, pre),
            Under::Either if first => {
                list.pads();
                list.calls();
            }
            Under::Either => {
                self.lane(list, pre);
                self.notes(list, pre);
            }
            Under::Nothing | Under::Code => {}
        }
    }

    /// After a lane's pad, `euclid`.
    fn lane(&self, list: &mut List, pre: &str) {
        if pre.ends_with(char::is_whitespace) && pre.split_whitespace().count() == 1 {
            list.add(Class::Call, "euclid", || CALLS[6].1.to_string());
        }
    }

    fn notes(&self, list: &mut List, pre: &str) {
        if pre.trim().is_empty() {
            return list.calls();
        }
        if let Some(call) = open_call(pre) {
            match (call.name, call.arg) {
                ("arp", 0) | ("root", 0) | ("mutate", 0) | ("markov", 1) => self.note_clips(list),
                ("arp", 1) => list.arp_modes(),
                _ => {}
            }
            return;
        }
        let root = word_before_by(pre, |c| c.is_ascii_alphanumeric() || c == '#');
        if pre.ends_with(':') && is_root(root) {
            return list.qualities();
        }
        if pre.trim_start().starts_with("euclid(") && pre.ends_with(") ") {
            list.add(Class::Keyword, "scale", || {
                "up the song's scale, hit by hit".to_string()
            });
        }
        list.numerals();
    }

    /// A top-level line: its keyword, then what that keyword takes.
    fn top(&self, list: &mut List, pre: &str) {
        let w: Vec<&str> = pre.split_whitespace().collect();
        let Some(first) = w.first() else {
            return list.keywords();
        };
        let kw = current_keyword(first, w.get(1).copied(), w.get(2).copied());
        let kw = kw.trim_end_matches(':');
        let prev = pre.chars().last();
        // The word's place on the line, when a space is before it.
        let at = pre.ends_with(char::is_whitespace).then_some(w.len());
        // After `:` or `,` of a list of values.
        let listed = matches!(pre.trim_end().chars().last(), Some(':' | ','));

        if let Some(call) = open_call(pre) {
            return self.in_call(list, kw, &call, prev == Some('.'));
        }
        if prev == Some('.') {
            let target = word_before(pre);
            return match kw {
                "clip" if target.is_empty() && w.len() >= 4 => {
                    let lanes = w.get(3).and_then(|t| self.kind(t)) == Some("drums");
                    list.methods(!lanes);
                }
                "auto" | "snapshot" if !target.is_empty() => list.target_params(target, false),
                "mod" if !target.is_empty() && !pre.contains('=') => {
                    list.target_params(target, true);
                }
                "mod" => list.signal(true),
                _ => {}
            };
        }
        match (kw, at) {
            ("scale", _) if prev == Some(':') => list.modes(),
            ("scale", Some(2)) => list.modes(),
            ("track", Some(2)) => list.kinds(),
            ("track", Some(3)) => {
                let kind = match w.get(2).copied() {
                    Some("drums") => Some(Kind::Drums),
                    Some("synth") => Some(Kind::Synth),
                    Some("sampler") => Some(Kind::Sampler),
                    _ => None,
                };
                list.models(kind);
                for s in &self.settings {
                    list.add(Class::Name, s, || "a setting".to_string());
                }
            }
            ("track", Some(k)) if k >= 4 => {
                if k == 4 {
                    list.presets(w.get(3).copied().unwrap_or(""));
                }
                list.add(Class::Keyword, "mute", || "the track is silent".to_string());
                list.add(Class::Keyword, "solo", || {
                    "only soloed tracks play".to_string()
                });
            }
            ("setting", _) if listed && pre.contains(':') => list.params(Set::Synth, false),
            ("setting", Some(3)) => list.models(None),
            ("setting", Some(4)) => list.presets(w.get(3).copied().unwrap_or("")),
            ("clip", Some(3)) => self.tracks(list),
            ("clip", Some(k)) if k >= 4 => list.table(Class::Keyword, &CLIP_WORDS),
            ("auto", Some(3)) | ("mod", Some(1)) => self.targets(list),
            ("auto", Some(4)) => list.add(Class::Keyword, "ramp", || {
                "ramp <from> <to>: a linear ramp".to_string()
            }),
            ("snapshot", _) if listed => self.targets(list),
            ("mod", _) if pre.contains('=') => list.signal(false),
            ("scene", _) if prev == Some('[') => {
                for s in &self.snapshots {
                    list.add(Class::Name, s, || "a snapshot".to_string());
                }
            }
            ("scene", Some(k)) if k >= 3 && pre.contains(':') => {
                for (c, t) in &self.clips {
                    list.add(Class::Name, c, || format!("a clip on {t}"));
                }
                for a in &self.autos {
                    list.add(Class::Name, a, || "an auto".to_string());
                }
            }
            ("arrange", Some(k)) if k >= 1 => {
                for (s, bars) in &self.scenes {
                    list.add(Class::Name, s, || format!("a scene of {bars} bars"));
                }
            }
            ("strip", Some(1)) | ("samples" | "zone" | "pad", Some(1)) => self.tracks(list),
            ("strip" | "group", _) if listed => list.params(Set::Strip, false),
            ("master", _) if listed => list.params(Set::Global, false),
            _ => {}
        }
    }

    /// Inside a call's parentheses on a top-level line: a method's
    /// arguments, or a signal.
    fn in_call(&self, list: &mut List, kw: &str, call: &Call, dot: bool) {
        let signal = |list: &mut List| match (call.name, call.arg) {
            ("lfo", 1) => {
                for s in SHAPES {
                    list.add(Class::Name, s, || "an LFO shape".to_string());
                }
            }
            (name, _) if signal::METHODS.contains(&name) || signal::SOURCES.contains(&name) => {}
            _ => list.signal(dot),
        };
        match kw {
            "clip" => match (call.name, call.arg) {
                ("scale", 0) => {
                    let mode = call.text.trim_start().contains(char::is_whitespace)
                        || call.text.ends_with(':');
                    if mode {
                        list.modes();
                    } else {
                        self.note_clips(list);
                    }
                }
                ("chord", 0) => self.note_clips(list),
                ("every" | "off", 1) | ("sometimes", 0) => list.table(Class::Call, &METHODS),
                (name, _) if Pattern::NAMES.contains(&name) => {}
                _ => signal(list),
            },
            "mod" => signal(list),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::KEYWORDS;
    use super::*;
    use crate::drums::Pad;

    /// The words offered with the cursor at `|`.
    fn at(text: &str) -> Vec<String> {
        let (a, b) = text.split_once('|').expect("a cursor");
        let text = format!("{a}{b}");
        let c = complete(&text, a.encode_utf16().count() as u32);
        c.items.into_iter().map(|i| i.word).collect()
    }

    const SONG: &str = "scale c minor
setting oud = Sh101 Sh101Lead
track kit drums
track lead synth oud
clip beat = kit
  bd x...x...
clip riff = lead
  \"c4 e4\"
auto sweep = lead.Cutoff ramp 300 4000 /8
snapshot drop: kit.Level 0.5
scene main 8: beat riff
";

    fn song(line: &str) -> Vec<String> {
        at(&format!("{SONG}{line}"))
    }

    #[test]
    fn a_line_starts_with_a_keyword() {
        assert_eq!(at("|").len(), 20);
        assert_eq!(at("sc|"), ["scale", "scene"]);
        assert_eq!(at("tempo 120\nAR|"), ["arrange"]);
        // Not the old words, nor in a comment.
        assert!(at("fr|").is_empty());
        assert!(at("# sc|").is_empty());
    }

    #[test]
    fn the_range_is_the_word_typed_so_far() {
        let text = "tempo 1\nsca";
        let c = complete(text, 11);
        assert_eq!((c.from, c.to), (8, 11));
        assert_eq!(c.items[0].word, "scale");
        assert_eq!(c.items[0].class, Class::Keyword);
        assert_eq!(c.items[0].detail, "scale <root> <mode>");
        // UTF-16 units, as the view counts them.
        let c = complete("# é😀\ncl", 8);
        assert_eq!((c.from, c.to, c.items.len()), (6, 8, 1));
    }

    #[test]
    fn a_track_takes_a_kind_a_model_and_its_preset() {
        assert_eq!(at("track a |"), ["drums", "synth", "sampler"]);
        let drums = at("track a drums |");
        assert!(drums.contains(&"Tr909".to_string()), "{drums:?}");
        assert!(!drums.contains(&"Minimoog".to_string()), "{drums:?}");
        assert_eq!(at("track a synth Mini|"), ["Minimoog"]);
        let presets = at("track a synth Minimoog |");
        assert!(presets.contains(&"MiniBass".to_string()), "{presets:?}");
        assert!(!presets.contains(&"Kit909".to_string()), "{presets:?}");
        assert!(presets.ends_with(&["mute".to_string(), "solo".to_string()]));
        assert_eq!(song("track b synth ou|"), ["oud"]);
    }

    #[test]
    fn a_clip_takes_a_track_its_words_and_methods() {
        assert_eq!(song("clip c = |"), ["kit", "lead"]);
        assert_eq!(song("clip c = lead v|"), ["voicing"]);
        assert_eq!(song("clip c = lead .fa|"), ["fast"]);
        assert_eq!(song("clip c = lead .lp|"), ["lpf", "lpq"]);
        assert_eq!(song("clip c = lead .reso|"), ["resonance"]);
        // Lanes take no pattern methods.
        assert!(!song("clip c = kit .|").contains(&"fast".to_string()));
        assert!(song("clip c = kit .|").contains(&"room".to_string()));
    }

    #[test]
    fn method_arguments_are_clips_modes_and_methods() {
        assert_eq!(song("clip c = lead .scale(|"), ["riff"]);
        assert_eq!(
            song("clip c = lead .scale(riff har|"),
            ["harmonic-minor", "harmonic-major"]
        );
        assert_eq!(
            song("clip c = lead .scale(a1:dor|"),
            ["dorian", "dorian-b2", "dorian-sharp-4"]
        );
        assert_eq!(song("clip c = lead .chord(|"), ["riff"]);
        assert_eq!(song("clip c = lead .every(4, r|"), ["rev"]);
        assert!(song("clip c = lead .fast(|").is_empty());
        assert_eq!(song("clip c = lead .cutoff(sin|"), ["sine", "sine2"]);
        assert_eq!(song("clip c = lead .cutoff(sine.ex|"), ["exprange"]);
        assert_eq!(
            song("clip c = lead .cutoff(lfo(2, s|"),
            ["sine", "saw", "square"]
        );
    }

    #[test]
    fn notes_take_calls_qualities_numerals_and_clips() {
        let notes = format!("{SONG}clip c = lead\n  ");
        let at = |s: &str| at(&format!("{notes}{s}"));
        assert_eq!(at("|").len(), 7);
        assert_eq!(at("ar|"), ["arp"]);
        assert_eq!(at("arp(|"), ["riff"]);
        assert_eq!(at("arp([c4,e4],u|"), ["up", "updown"]);
        assert_eq!(at("markov(1,|"), ["riff"]);
        assert!(at("walk(|").is_empty());
        assert_eq!(at("\"c:m|"), ["maj", "m", "maj7", "m7", "m7b5", "m6", "m9"]);
        assert_eq!(at("c3:4 f#:d|"), ["dim", "dim7"]);
        assert_eq!(at("\"<i V|"), ["V", "v", "VI", "vi", "VII", "vii"]);
        assert_eq!(at("euclid(5,8) s|"), ["scale"]);
    }

    #[test]
    fn lanes_take_pads_and_euclid() {
        let lanes = format!("{SONG}clip c = kit\n  bd x...\n  ");
        assert_eq!(
            at(&format!("{lanes}c|")),
            ["cp", "ch", "cb", "cl", "cy", "cr"]
        );
        assert_eq!(at(&format!("{lanes}cb e|")), ["euclid"]);
        assert!(at(&format!("{lanes}cb x.x|")).is_empty());
    }

    #[test]
    fn targets_take_their_parameters() {
        assert_eq!(song("auto a = l|"), ["lead"]);
        assert_eq!(song("auto a = st|").len(), 16);
        assert_eq!(song("auto a = lead.Cutof|"), ["Cutoff"]);
        assert_eq!(song("auto a = lead.Cutoff r|"), ["ramp"]);
        // A group takes its strip's, master the global ones.
        assert!(song("auto a = group1.Cutof|").is_empty());
        assert_eq!(song("auto a = group1.Lev|"), ["Level"]);
        assert!(song("auto a = master.Lev|").is_empty());
        assert_eq!(song("auto a = master.MasterG|"), ["MasterGain"]);
        assert_eq!(song("snapshot s: kit.Level 1, le|"), ["lead"]);
        assert_eq!(song("mod l|"), ["lead"]);
        assert_eq!(song("mod lead.cutof|"), ["cutoff"]);
        assert_eq!(song("mod lead.cutoff = per|"), ["perlin"]);
        assert_eq!(song("mod lead.cutoff = perlin.sl|"), ["slow"]);
        // Never the model or the routing.
        assert!(song("auto a = lead.Mode|").is_empty());
        assert!(song("auto a = lead.Ou|").is_empty());
    }

    #[test]
    fn scenes_and_arrange_take_the_songs_names() {
        assert_eq!(song("scene b 4: |"), ["beat", "riff", "sweep"]);
        assert_eq!(song("scene b 4: beat [|"), ["drop"]);
        assert_eq!(song("arrange main |"), ["main"]);
        // Only names made above: a clip below the cursor is not one yet.
        assert_eq!(
            at("track k drums\nscene a 1: |\nclip c = k\n  bd x"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn settings_scales_and_mixer_lines() {
        assert_eq!(
            at("scale c dor|"),
            ["dorian", "dorian-b2", "dorian-sharp-4"]
        );
        assert_eq!(at("scale c:phrygian-d|"), ["phrygian-dominant"]);
        assert_eq!(at("setting s = Sh101 Sh101B|"), ["Sh101Bass"]);
        assert_eq!(at("setting s = Sh101 Sh101Bass: Cutof|"), ["Cutoff"]);
        // A setting is the synth's own: no mixer parameters.
        assert!(at("setting s = Sh101 Sh101Bass: Cutoff 1, Lev|").is_empty());
        assert_eq!(song("strip l|"), ["lead"]);
        assert_eq!(song("strip lead: Level 1, Pa|"), ["Pan"]);
        assert_eq!(at("master: MasterG|"), ["MasterGain"]);
    }

    #[test]
    fn modular_code_takes_nothing() {
        let code = "setting m = Modular\n  SynthDef(\\m, { |freq| Sin|";
        assert!(at(code).is_empty());
    }

    #[test]
    fn every_list_has_its_details() {
        fn names(t: &[(&'static str, &str)]) -> Vec<&'static str> {
            t.iter().map(|e| e.0).collect()
        }
        assert_eq!(names(&KEYWORD_USAGE), KEYWORDS);
        assert_eq!(names(&METHODS), Pattern::NAMES);
        assert_eq!(names(&CALLS)[..6], notes::CALLS);
        assert_eq!(names(&PADS), Pad::ALL.map(|p| p.1));
        assert_eq!(names(&CLIP_WORDS), ["live", "bars", "voicing"]);
        // Each list is as long as it may be, and no more.
        assert!(at("auto a = lead.|").len() <= MAX_ITEMS);
    }

    /// Like the lexer, any text and cursor complete, in bounds.
    #[test]
    fn any_text_completes_in_bounds() {
        let alphabet: Vec<char> = "abcx .:,()[]<>\"#=/-_\n\t 1é😀".chars().collect();
        let words = [
            "clip ", "track ", "auto ", "mod ", "scene ", ".scale(", "arp(", "  ",
        ];
        let mut seed: u32 = 7;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 16) as usize
        };
        for _ in 0..500 {
            let mut text = String::new();
            for _ in 0..next() % 60 {
                if next() % 4 == 0 {
                    text.push_str(words[next() % words.len()]);
                } else {
                    text.push(alphabet[next() % alphabet.len()]);
                }
            }
            let units = text.encode_utf16().count() as u32;
            for cursor in 0..=units + 1 {
                let c = complete(&text, cursor);
                assert!(c.from <= c.to && c.to <= units, "{text:?} at {cursor}");
                assert!(c.items.len() <= MAX_ITEMS);
            }
        }
    }
}
