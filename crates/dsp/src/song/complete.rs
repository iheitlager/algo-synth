//! Completion for the song editor (#595, #642): the words that fit where the
//! cursor is, from the lists the parser reads, so the view only shows them
//! (ADR-0001). Like the lexer it reads the text, not the song: the line the
//! cursor is on, and the names made on the lines above it, as a name is used
//! after the line that makes it. A text with an error still completes. It is
//! total: any text and cursor give a list, maybe empty, never a panic.
//!
//! Offsets count UTF-16 units, as the view indexes its strings.

use super::lex::{Class, Under};
use super::{ALIASES, GROUPS, Kind, STRIPS, UNITS, current_keyword, fits, model_named, signal};
use crate::algo::{Euclid, Mode, Scale};
use crate::mono::model::Model;
use crate::mono::preset::Preset;
use crate::notes::pattern::Pattern;
use crate::notes::{self, ArpMode, Event};
use crate::params::Param;
use crate::usage;

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
    /// How the call the cursor is in is written (#644), or empty.
    pub help: String,
    /// What the call the cursor is in or just after gives (#645): a
    /// rhythm, chords, notes or roots; or empty.
    pub preview: String,
}

/// As the view reads it: `from\tto\thelp\tpreview`, then
/// `class\tword\tdetail`, a line each.
impl std::fmt::Display for Completion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}\t{}\t{}\t{}",
            self.from, self.to, self.help, self.preview
        )?;
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
const METHODS: [(&str, &str); 27] = [
    ("fast", "n times faster"),
    ("slow", "n times slower"),
    ("rev", "each bar backwards"),
    ("palindrome", "every other bar backwards"),
    ("add", "n semitones up, or a signal"),
    ("sub", "n semitones down"),
    ("ply", "each note n times"),
    ("iter", "each bar starts 1/n further in"),
    ("degrade", "drop notes by chance"),
    ("every", "a method on every nth bar"),
    ("off", "a later copy with a method"),
    ("struct", "the notes on a rhythm"),
    ("euclid", "the notes on k hits of n steps"),
    ("sometimes", "a method on about half"),
    ("often", "a method on about 3 in 4"),
    ("rarely", "a method on about 1 in 4"),
    ("almostNever", "a method on about 1 in 10"),
    ("almostAlways", "a method on about 9 in 10"),
    ("sometimesBy", "a method on a chance of p"),
    (
        "scale",
        "the notes onto a scale, or an earlier clip's roots",
    ),
    ("oct", "n octaves up or down"),
    ("chord", "degrees onto an earlier clip's chords"),
    ("strum", "ms between a chord's notes"),
    ("arp", "the line's chords by note index"),
    ("jux", "left as is, right through a method"),
    ("shuffle", "each bar's n parts in a new order"),
    ("scramble", "n parts drawn from the bar's"),
];

/// What each generator call plays (`notes::CALLS`, and `euclid`).
const CALLS: [(&str, &str); 8] = [
    ("arp", "a chord or a clip's chords, arpeggiated"),
    ("walk", "a random walk on the scale"),
    ("markov", "a chain learned from a clip"),
    ("mutate", "a clip with some notes moved"),
    ("root", "the roots of a clip's chords"),
    ("prog", "a chord progression on the scale"),
    ("bass", "a bass line on a clip's chords"),
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

/// Is `word`, after a clip's track, the clip it is made from (#649)?
pub(super) fn from_clip(word: Option<&str>) -> bool {
    // A name, not `voicing(drop2,` (#606): no bracket or comma in a clip's name.
    word.is_some_and(|w| {
        w.starts_with(|c: char| c.is_ascii_alphabetic())
            && w.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !CLIP_WORDS.iter().any(|(k, _)| *k == w)
    })
}

/// The words of a clip line after its track.
const CLIP_WORDS: [(&str, &str); 3] = [
    ("live", "new notes every bar"),
    ("bars", "how many bars timed notes run"),
    ("voicing", "each chord near the one before"),
];

/// How `voicing(…)` spreads a chord (`notes::STYLES`, #606).
const VOICINGS: [(&str, &str); 7] = [
    ("close", "the inversion nearest the chord before"),
    ("open", "the second note from the bottom an octave up"),
    ("drop2", "the second note from the top an octave down"),
    ("drop3", "the third note from the top an octave down"),
    ("shell", "root, third and seventh only"),
    ("quartal", "fourths of the scale from the root"),
    ("lefthand", "a rootless left hand, as Strudel's"),
];

/// An LFO's shapes.
const SHAPES: [&str; 5] = ["sine", "cosine", "saw", "tri", "square"];

/// The words that fit at `cursor` in `text`.
pub fn complete(text: &str, cursor: u32) -> Completion {
    let cur = byte_at(text, cursor);
    let head = text.get(..cur).unwrap_or("");
    let start = head.rfind('\n').map_or(0, |i| i + 1);
    let before = head.get(start..).unwrap_or("");
    let mut names = Names::above(head.get(..start).unwrap_or(""));
    // On a top-level line the clip is the one being written, not the one
    // above, so every clip above may be read.
    if !before.starts_with([' ', '\t']) {
        names.open = None;
    }
    let mut typed: usize = before
        .chars()
        .rev()
        .take_while(|c| is_word(*c))
        .map(char::len_utf8)
        .sum();
    // An arp mode takes an `&` inside it: `up&down` (#612).
    if let Some(amp) = before
        .get(..before.len() - typed)
        .and_then(|p| p.strip_suffix('&'))
    {
        if open_call(amp).is_some_and(|c| c.name == "arp" && c.arg == 1) {
            typed += 1 + amp.chars().rev().take_while(|c| is_word(*c)).count();
        }
    }
    // A Modular setting's code is SuperCollider's (#647), from its first line.
    if before.starts_with([' ', '\t']) && names.under == Under::Code {
        let code = text.get(names.code..).unwrap_or("");
        let skip = text
            .get(..names.code)
            .map_or(0, |t| t.encode_utf16().count()) as u32;
        let mut c = crate::modular::complete::complete(code, cursor.saturating_sub(skip));
        c.from += skip;
        c.to += skip;
        return c;
    }
    let (pre, word) = before.split_at(before.len() - typed);
    let mut list = List {
        prefix: word,
        items: Vec::new(),
    };
    let (mut help, mut preview) = (None, None);
    // Not in a comment.
    if uncommented(before).len() == before.len() {
        let indented = before.starts_with([' ', '\t']);
        if indented {
            names.indented(&mut list, pre);
        } else {
            names.top(&mut list, pre);
        }
        help = names.help(before, indented);
        let rest = text.get(cur..).unwrap_or("");
        let line = text
            .get(start..cur + rest.find('\n').unwrap_or(rest.len()))
            .unwrap_or(before);
        let above = head.get(..start).unwrap_or("");
        preview = names.preview(above, line, before.len(), indented);
    }
    let to = head.encode_utf16().count() as u32;
    Completion {
        from: to - word.encode_utf16().count() as u32,
        to,
        items: list.items,
        help: help.unwrap_or_default(),
        preview: preview.unwrap_or_default(),
    }
}

/// The byte offset of UTF-16 unit `at` in `text`, at a char boundary.
pub(crate) fn byte_at(text: &str, at: u32) -> usize {
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
pub(crate) fn is_word(c: char) -> bool {
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
pub(crate) struct List<'a> {
    pub(crate) prefix: &'a str,
    pub(crate) items: Vec<Item>,
}

impl List<'_> {
    pub(crate) fn wants(&self, word: &str) -> bool {
        self.items.len() < MAX_ITEMS
            && word
                .as_bytes()
                .get(..self.prefix.len())
                .is_some_and(|w| w.eq_ignore_ascii_case(self.prefix.as_bytes()))
            && !self.items.iter().any(|i| i.word == word)
    }

    pub(crate) fn add(&mut self, class: Class, word: &str, detail: impl FnOnce() -> String) {
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

    /// `model` the track's, where the song names it (#679).
    fn methods(&mut self, pattern: bool, model: Option<Model>) {
        if pattern {
            self.table(Class::Call, &METHODS);
        }
        self.adsr(model);
        self.params(Set::Track(model), true);
    }

    /// `adsr`, Strudel's envelope in one method (#677), where the model
    /// reads the ADSR.
    fn adsr(&mut self, model: Option<Model>) {
        if Set::Track(model).has(Param::AdsrAttack) {
            self.add(Class::Param, "adsr", || {
                "\"attack:decay:sustain:release\"".to_string()
            });
        }
    }

    /// A drum clip made from a clip (#653): the methods of time.
    fn time_methods(&mut self, model: Option<Model>) {
        for (w, d) in METHODS {
            if !matches!(w, "add" | "sub" | "scale" | "oct" | "chord" | "strum") {
                self.add(Class::Call, w, || d.to_string());
            }
        }
        self.adsr(model);
        self.params(Set::Track(model), true);
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

    /// A prog's sizes and styles (#608, #610); its beats are numbers.
    fn prog_words(&mut self) {
        for (_, w) in notes::Size::ALL {
            self.add(Class::Name, w, || "chord size".to_string());
        }
        for (_, w) in notes::ProgStyle::ALL {
            self.add(Class::Name, w, || "progression style".to_string());
        }
    }

    fn arp_modes(&mut self) {
        for (_, m) in ArpMode::WORDS {
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
        // The effects in real units (#660): a method and `mod master.` take
        // Strudel's names, the master line ours.
        for u in UNITS {
            let strudel = u.name.starts_with(|c: char| c.is_ascii_lowercase());
            let fits = match set {
                Set::Track(_) => lower && strudel,
                Set::Global => lower == strudel,
                _ => false,
            };
            if fits {
                self.add(Class::Param, u.name, || unit_detail(u.param).to_string());
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

    /// The parameters of `target`: a track, `stripN`, `groupN` or `master`;
    /// `model` a track's, where the song names it (#679).
    fn target_params(&mut self, target: &str, model: Option<Model>, lower: bool) {
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
            Set::Track(model)
        };
        self.params(set, lower);
    }
}

/// Which parameters a place takes, as `param_for` and the setting line check;
/// with a model, only what it reads (#673, #679).
#[derive(Clone, Copy)]
enum Set {
    /// A track or `stripN`: its synth's and its strip's.
    Track(Option<Model>),
    /// A group's strip, or a mixer line's.
    Strip,
    /// `master`'s.
    Global,
    /// A setting's: the synth's own.
    Synth(Option<Model>),
}

impl Set {
    fn has(self, p: Param) -> bool {
        if matches!(p, Param::Model | Param::Out) {
            return false;
        }
        let read = |m: Option<Model>| m.is_none_or(|m| m.reads(p));
        match self {
            Set::Track(m) => !p.is_global() && read(m),
            Set::Strip => p.is_strip(),
            Set::Global => p.is_global(),
            Set::Synth(m) => !p.is_global() && !p.is_strip() && read(m),
        }
    }
}

/// The call the cursor at byte `at` of `line` is in, or just after: its
/// name and its arguments, to its `)` or the end of the line.
fn call_at(line: &str, at: usize) -> Option<(&str, &str)> {
    let before = line.get(..at)?;
    fn inner(pre: &str) -> Option<(&str, usize)> {
        match opened(pre).0.last() {
            Some(Open::Call(name, _, _, paren)) => Some((*name, *paren)),
            _ => None,
        }
    }
    let (name, paren) =
        inner(before).or_else(|| inner(before.strip_suffix(')')?.trim_end_matches(' ')))?;
    let mut depth = 0usize;
    let mut end = line.len();
    for (i, c) in line.char_indices().skip_while(|(i, _)| *i <= paren) {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => {
                end = i;
                break;
            }
            ')' => depth -= 1,
            _ => {}
        }
    }
    Some((name, line.get(paren + 1..end)?))
}

/// The song's scale, from the last `scale` line in `above`.
fn scale_above(above: &str) -> Option<Scale> {
    let line = above
        .lines()
        .rev()
        .find(|l| l.split_whitespace().next() == Some("scale"))?;
    super::Song::parse(line).ok()?.scale
}

/// The chords of a progression as roman numerals on `scale`, a bar each:
/// upper case major, lower minor, `o` diminished, `+` augmented.
fn numerals(events: &[Event], scale: &Scale) -> Option<String> {
    let mut out = Vec::new();
    for chord in events.chunk_by(|a, b| a.start == b.start) {
        let root = chord.iter().map(|e| e.note).min()?;
        let degree = (0..7).find(|d| scale.degree(*d) == Some(root % 12))?;
        let above = |k: usize| chord.get(k).map(|e| (e.note + 12 - root % 12) % 12);
        let n = notes::NUMERALS.get(degree)?;
        let (third, fifth) = (above(1), above(2));
        let mut word = if third == Some(4) {
            n.to_uppercase()
        } else {
            (*n).to_string()
        };
        match fifth {
            Some(6) => word.push('o'),
            Some(8) => word.push('+'),
            _ => {}
        }
        out.push(word);
    }
    Some(out.join(" ")).filter(|s| !s.is_empty())
}

/// Most bars of roots a preview shows.
const MAX_ROOTS: u32 = 16;

/// The root `.scale(<clip> …)` takes at each bar's start (ADR-0035): the
/// lowest note the clip plays there, else the last it played, else its last.
fn roots(events: &[Event], bars: u32) -> Option<String> {
    let last = events.iter().max_by_key(|e| e.start)?;
    let mut out: Vec<String> = (0..bars.min(MAX_ROOTS))
        .map(|b| {
            let t = b * notes::TICKS_PER_BAR;
            let sounding = events
                .iter()
                .filter(|e| e.start <= t && t < e.start + e.len);
            let start = events
                .iter()
                .filter(|e| e.start <= t)
                .map(|e| e.start)
                .max();
            let at = |s: u32| events.iter().filter(move |e| e.start == s);
            let low = sounding
                .map(|e| e.note)
                .min()
                .or_else(|| start.and_then(|s| at(s).map(|e| e.note).min()))
                .or_else(|| at(last.start).map(|e| e.note).min())
                .unwrap_or(last.note);
            notes::note_name(low)
        })
        .collect();
    if bars > MAX_ROOTS {
        out.push("…".to_string());
    }
    Some(out.join(" "))
}

/// What a unit's value is in.
fn unit_detail(p: Param) -> &'static str {
    match p {
        Param::P1A => "the echo's time in seconds",
        Param::P1B => "the echo's feedback, 0–0.95",
        Param::P2A => "the reverb's tail in seconds",
        _ => "the low-pass in the reverb's tail in Hz",
    }
}

/// A parameter method's help: its range, and a signal over it.
fn param_help(name: &str) -> Option<String> {
    if name.eq_ignore_ascii_case("adsr") {
        return Some(
            "adsr takes \"attack:decay:sustain:release\", each a number, [a b] or <a b>: adsr(\"[.1 0]:.2:[1 0]\")"
                .to_string(),
        );
    }
    if let Some(u) = super::unit(name) {
        return Some(format!("{name} is {}: {name}(0.25)", unit_detail(u.param)));
    }
    let p = ALIASES
        .iter()
        .find(|(a, _)| *a == name)
        .map(|(_, p)| *p)
        .or_else(|| Param::by_name(name))
        .filter(|p| Set::Track(None).has(*p))?;
    let (lo, hi) = p.range();
    Some(format!(
        "{name} takes a number or a signal from {lo} to {hi}: {name}(sine.range({lo}, {hi}))"
    ))
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

/// What is open at the end of a line so far: a call (its name, argument,
/// where that starts and where its `(` is) or a bracket.
enum Open<'a> {
    Call(&'a str, usize, usize, usize),
    Bracket,
}

/// The innermost open call in `pre`; `None` inside brackets or quotes.
fn open_call(pre: &str) -> Option<Call<'_>> {
    let (stack, quoted) = opened(pre);
    match stack.last() {
        Some(Open::Call(name, arg, start, _)) if !quoted => Some(Call {
            name,
            arg: *arg,
            text: pre.get(*start..).unwrap_or(""),
        }),
        _ => None,
    }
}

/// The calls and brackets open at the end of `pre`, outermost first, and
/// whether a quote is.
fn opened(pre: &str) -> (Vec<Open<'_>>, bool) {
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
                stack.push(Open::Call(name, 0, i + 1, i));
            }
            '[' => stack.push(Open::Bracket),
            ')' | ']' => {
                stack.pop();
            }
            ',' => {
                if let Some(Open::Call(_, arg, start, _)) = stack.last_mut() {
                    *arg += 1;
                    *start = i + 1;
                }
            }
            _ => {}
        }
    }
    (stack, quoted)
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
    /// Name, kind and the model it names, itself or by its setting.
    tracks: Vec<(&'a str, &'a str, Option<Model>)>,
    /// Name and model.
    settings: Vec<(&'a str, Option<Model>)>,
    /// Name and track.
    clips: Vec<(&'a str, &'a str)>,
    autos: Vec<&'a str>,
    snapshots: Vec<&'a str>,
    /// Name and bars.
    scenes: Vec<(&'a str, &'a str)>,
    /// What the indented lines under the last top-level line hold.
    under: Under,
    /// Where a Modular setting's code starts (bytes): the line after it.
    code: usize,
    /// The clip they belong to, which can't read itself.
    open: Option<&'a str>,
    /// Each track's model or setting, as its line names it.
    sounds: Vec<(&'a str, &'a str)>,
    /// The text before the first track: the settings (#674).
    head: &'a str,
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
            code: 0,
            open: None,
            sounds: Vec::new(),
            head: text,
        };
        // The end of each line read, its `\n` with it.
        let mut end = 0;
        for raw in text.split('\n') {
            end += raw.len() + 1;
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
                    if n.head.len() == text.len() {
                        n.head = text.get(..end - raw.len() - 1).unwrap_or("");
                    }
                    if let (Some(t), Some(k)) = (at(1), at(2)) {
                        let model = at(3).and_then(|m| {
                            model_named(m).or_else(|| {
                                n.settings
                                    .iter()
                                    .find(|(s, _)| *s == m)
                                    .and_then(|(_, m)| *m)
                            })
                        });
                        n.tracks.push((t, k, model));
                    }
                    if let (Some(t), Some(s)) = (at(1), at(3)) {
                        n.sounds.push((t, s));
                    }
                }
                "setting" => {
                    n.settings
                        .extend(name().map(|s| (s, at(3).and_then(model_named))));
                    if at(3) == Some("Modular") {
                        n.under = Under::Code;
                        n.code = end;
                    }
                }
                "clip" => {
                    if let (Some(c), Some(t)) = (at(1), at(3)) {
                        n.clips.push((c, t));
                        n.open = Some(c);
                        n.under = match n.kind(t) {
                            // Made from a clip: no line of its own (#649).
                            _ if from_clip(at(4)) => Under::Nothing,
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
            .find(|(n, _, _)| *n == track)
            .map(|(_, k, _)| *k)
    }

    /// The model of the track named `track`, where the song names it.
    fn model(&self, track: &str) -> Option<Model> {
        self.tracks
            .iter()
            .rev()
            .find(|(n, _, _)| *n == track)
            .and_then(|(_, _, m)| *m)
    }

    /// The Modular setting a track plays, as the setting lines above read.
    fn setting(&self, track: &str) -> Option<super::Setting> {
        let sound = self
            .sounds
            .iter()
            .rev()
            .find(|(t, _)| *t == track)
            .map(|(_, s)| *s)?;
        if !self.settings.iter().any(|(s, _)| *s == sound) {
            return None;
        }
        let song = super::Song::parse(self.head).ok()?;
        song.settings.into_iter().find(|st| st.name == sound)
    }

    /// The knobs a track's Modular setting names (#674). On a clip, a
    /// pattern method wins over a knob of its name: it says how to reach
    /// the knob there (#688).
    fn knobs(&self, list: &mut List, track: &str) {
        let Some(st) = self.setting(track) else {
            return;
        };
        for (i, k) in st.knobs.iter().enumerate().filter(|(_, k)| !k.is_empty()) {
            list.add(Class::Param, k, || {
                if Pattern::NAMES.contains(&k.as_str()) {
                    format!(
                        "a knob of {}: on a clip .ctl{} ({k} is a pattern method)",
                        st.name,
                        i + 1
                    )
                } else {
                    format!("a knob of {}", st.name)
                }
            });
        }
    }

    /// How a knob's method is written (#688): its setting and its range.
    fn knob_help(&self, track: &str, name: &str) -> Option<String> {
        let st = self.setting(track)?;
        let i = st
            .knobs
            .iter()
            .position(|k| !k.is_empty() && k.eq_ignore_ascii_case(name))?;
        let patch = crate::modular::sc::compile(st.code.as_deref()?).ok()?;
        let k = patch.knobs.iter().find(|k| k.ctl == i)?;
        let (lo, hi) = (k.lo, k.hi);
        Some(format!(
            "{name} is a knob of {}, from {lo} to {hi}: {name}(sine.range({lo}, {hi}))",
            st.name
        ))
    }

    fn tracks(&self, list: &mut List) {
        for (t, k, _) in &self.tracks {
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

    /// The clips on a track whose kind `fits`.
    fn clips_on(&self, list: &mut List, fits: impl Fn(&str) -> bool) {
        for (c, t) in &self.clips {
            if self.kind(t).is_some_and(&fits) && self.open != Some(c) {
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

    /// How the call the cursor is in is written (#644): a generator, a
    /// pattern method (or one nested in `every`, `off`, `sometimes`), a
    /// signal's function, or a parameter method's range.
    fn help(&self, before: &str, indented: bool) -> Option<String> {
        let (stack, _) = opened(before);
        let calls: Vec<&str> = stack
            .iter()
            .filter_map(|o| match o {
                Open::Call(name, ..) => Some(*name),
                Open::Bracket => None,
            })
            .collect();
        let (outer, inner) = (*calls.first()?, *calls.last()?);
        if indented {
            return match self.under {
                Under::Lanes | Under::Notes | Under::Either => usage::of(&usage::CALLS, inner),
                Under::Nothing | Under::Code => None,
            }
            .map(str::to_string);
        }
        let w: Vec<&str> = before.split_whitespace().collect();
        let first = w.first().copied().unwrap_or("");
        match current_keyword(first, w.get(1).copied(), w.get(2).copied()) {
            "clip" if outer == "voicing" => Some(usage::VOICING.to_string()),
            "clip" if Pattern::NAMES.contains(&outer) => {
                usage::of(&usage::METHODS, inner).map(str::to_string)
            }
            "clip" if inner == outer => w
                .get(3)
                .and_then(|t| self.knob_help(t, outer))
                .or_else(|| param_help(outer)),
            "clip" | "mod" => usage::of(&usage::SIGNAL, inner).map(str::to_string),
            _ => None,
        }
    }

    /// What the call at byte `at` of `line` gives (#645), without playing
    /// it: `euclid`'s rhythm, `prog`'s chords as numerals, `walk`'s notes,
    /// and the roots `.scale(<clip> …)` takes from the clip, a bar each.
    /// `above` is the text before the line, for the scale and the clips.
    fn preview(&self, above: &str, line: &str, at: usize, indented: bool) -> Option<String> {
        let (name, args) = call_at(line, at)?;
        let notes = indented && matches!(self.under, Under::Notes | Under::Either);
        match name {
            "euclid" => {
                let e = Euclid::args(&format!("{args})")).ok()?;
                Some(
                    e.pattern()
                        .iter()
                        .map(|h| if *h { 'x' } else { '.' })
                        .collect(),
                )
            }
            "prog" | "walk" if notes => {
                let scale = scale_above(above);
                let n = notes::parse_in(&format!("{name}({args})"), 0, scale.as_ref()).ok()?;
                if name == "prog" {
                    numerals(&n.events, &scale?)
                } else {
                    let names: Vec<String> =
                        n.events.iter().map(|e| notes::note_name(e.note)).collect();
                    Some(names.join(" ")).filter(|s| !s.is_empty())
                }
            }
            "scale" if !indented && line.starts_with("clip") => {
                let clip = args.split_whitespace().next()?;
                let song = super::Song::parse(above).ok()?;
                let n = song.clips.iter().find(|c| c.name == clip)?.notes.as_ref()?;
                roots(&n.events, n.bars)
            }
            _ => None,
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
            list.add(Class::Call, "euclid", || CALLS[7].1.to_string());
        }
    }

    fn notes(&self, list: &mut List, pre: &str) {
        if pre.trim().is_empty() {
            return list.calls();
        }
        if let Some(call) = open_call(pre) {
            match (call.name, call.arg) {
                ("arp", 0) | ("root", 0) | ("bass", 0) | ("mutate", 0) | ("markov", 1) => {
                    self.note_clips(list)
                }
                ("bass", 1) => {
                    for (_, s) in notes::BassStyle::ALL {
                        list.add(Class::Name, s, || "bass style".to_string());
                    }
                }
                ("arp", 1) => list.arp_modes(),
                ("prog", k) if k >= 2 => list.prog_words(),
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
        // Inside quotes: a rhythm or a sequence, no words to offer.
        if opened(pre).1 {
            return;
        }
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
                    self.knobs(list, w.get(3).copied().unwrap_or(""));
                    let lanes = w.get(3).and_then(|t| self.kind(t)) == Some("drums");
                    let model = w.get(3).and_then(|t| self.model(t));
                    if lanes && from_clip(w.get(4).filter(|_| w.len() >= 5).copied()) {
                        list.time_methods(model);
                    } else {
                        list.methods(!lanes, model);
                    }
                }
                "auto" | "snapshot" if !target.is_empty() => {
                    self.knobs(list, target);
                    list.target_params(target, self.model(target), false);
                }
                "mod" if !target.is_empty() && !pre.contains('=') => {
                    self.knobs(list, target);
                    list.target_params(target, self.model(target), true);
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
                for (s, _) in &self.settings {
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
            ("setting", _) if listed && pre.contains(':') => {
                list.params(Set::Synth(w.get(3).and_then(|m| model_named(m))), false);
            }
            ("setting", Some(3)) => list.models(None),
            ("setting", Some(4)) => list.presets(w.get(3).copied().unwrap_or("")),
            ("clip", Some(3)) => self.tracks(list),
            ("clip", Some(k)) if k >= 4 => {
                // An earlier clip to make this one from (#649, #653): of notes
                // for notes, of lanes for lanes, either on a sampler.
                if k == 4 {
                    match w.get(3).and_then(|t| self.kind(t)) {
                        Some("drums") => self.clips_on(list, |k| k == "drums"),
                        Some("sampler") => self.clips_on(list, |_| true),
                        _ => self.note_clips(list),
                    }
                }
                list.table(Class::Keyword, &CLIP_WORDS);
            }
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
                ("voicing", 0) => list.table(Class::Name, &VOICINGS),
                // Any earlier clip's rhythm: its notes' starts or its hits (#600).
                ("struct", 0) => {
                    for (c, t) in &self.clips {
                        list.add(Class::Name, c, || format!("a clip on {t}"));
                    }
                }
                ("every" | "off" | "sometimesBy", 1)
                | ("sometimes" | "often" | "rarely" | "almostNever" | "almostAlways" | "jux", 0) => {
                    list.table(Class::Call, &METHODS)
                }
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
    use super::super::{KEYWORDS, Song};
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
        // #606: a voicing's styles.
        assert_eq!(song("clip c = lead voicing(dr|"), ["drop2", "drop3"]);
        // An earlier clip of notes to make it from (#649), not on drums.
        assert_eq!(song("clip c = lead r|"), ["riff"]);
        assert!(song("clip c = lead |").contains(&"live".to_string()));
        assert!(!song("clip c = kit r|").contains(&"riff".to_string()));
        // The clip just above may be read from the next clip's line.
        assert_eq!(
            song("clip d = lead\n  \"c4\"\nclip c = lead .chord(|"),
            ["riff", "d"]
        );
        assert_eq!(song("clip c = lead .fa|"), ["fast"]);
        assert_eq!(song("clip c = lead .lp|"), ["lpf", "lpq"]);
        assert_eq!(song("clip c = lead .reso|"), ["resonance"]);
        // The effects in real units (#660): Strudel's names on a clip, ours
        // on the master line.
        assert_eq!(song("clip c = lead .delayt|"), ["delaytime"]);
        assert_eq!(song("clip c = lead .roomd|"), ["roomdim"]);
        assert_eq!(song("master: Echo|"), ["EchoTime", "EchoFeedback"]);
        // Lanes take no pattern methods.
        assert!(!song("clip c = kit .|").contains(&"fast".to_string()));
        // A drum clip made from one takes earlier drum clips and the
        // methods of time (#653).
        assert!(song("clip c = kit b|").contains(&"beat".to_string()));
        assert!(!song("clip c = kit r|").contains(&"riff".to_string()));
        let time = song("clip c = kit beat .|");
        assert!(time.contains(&"fast".to_string()) && time.contains(&"room".to_string()));
        assert!(!time.contains(&"add".to_string()) && !time.contains(&"strum".to_string()));
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
        assert_eq!(song("clip c = lead .struct(|"), ["beat", "riff"]);
        assert!(song("clip c = lead .struct(\"x |").is_empty());
        assert_eq!(song("clip c = lead .every(4, r|"), ["rev", "rarely"]);
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
        assert_eq!(at("|").len(), 8);
        assert_eq!(at("ar|"), ["arp"]);
        // #611: bass reads a clip, then takes a style.
        assert_eq!(at("bass(|"), ["riff"]);
        assert_eq!(at("bass(riff,w|"), ["walk"]);
        assert_eq!(at("arp(|"), ["riff"]);
        assert_eq!(at("arp([c4,e4],u|"), ["up", "updown", "up&down"]);
        // #612: Tidal's modes too.
        assert_eq!(at("arp([c4,e4],pin|"), ["pinkyup", "pinkyupdown"]);
        assert_eq!(at("arp([c4,e4],up&|"), ["up&down"]);
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

    /// #679: where the song names a track's model, only what it reads (#673).
    #[test]
    fn a_tracks_parameters_are_what_its_model_reads() {
        let moog = "track solo synth Minimoog\nclip a = solo .";
        assert!(at(&format!("{moog}lfos|")).is_empty());
        assert!(at(&format!("{moog}lfor|")).is_empty());
        assert_eq!(at(&format!("{moog}vco3coa|")), ["vco3coarse"]);
        assert!(at(&format!("{moog}sen|")).contains(&"send1".to_string()));
        assert_eq!(at("track b synth Juno106\nclip a = b .lfor|"), ["lforate"]);
        assert!(at("track solo synth Minimoog\nmod solo.lfor|").is_empty());
        assert!(at("track solo synth Minimoog\nauto a = solo.LfoR|").is_empty());
        assert!(at("track d drums Tr808\nsnapshot s: d.Cutof|").is_empty());
        assert_eq!(at("track d drums Tr808\nsnapshot s: d.BdTun|"), ["BdTune"]);
        // By its setting, and on the setting's own line.
        assert!(song("mod lead.op1lev|").is_empty());
        assert!(at("setting s = Minimoog MiniLead: LfoR|").is_empty());
        assert_eq!(at("setting s = Juno106 JunoBrass: LfoR|"), ["LfoRate"]);
        // A track that names no model gets every one.
        assert_eq!(at("track t synth\nmod t.op1lev|"), ["op1level"]);
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
    fn modular_code_takes_supercolliders_words() {
        let code = "setting m = Modular\n  SynthDef(\\m, { |freq|\n    Sin";
        let c = complete(code, code.encode_utf16().count() as u32);
        assert_eq!(c.items[0].word, "SinOsc");
        assert_eq!(
            (c.from, c.to),
            (c.to - 3, code.encode_utf16().count() as u32)
        );
        let c = complete(code, 25);
        assert!(c.from <= c.to && c.to == 25);
        // The song's words again after it.
        assert_eq!(
            at("setting m = Modular\n  SynthDef(\\m, { 0 }).add;\ntr|"),
            ["track"]
        );
    }

    /// The help in the call at `|`.
    fn help(text: &str) -> String {
        let (a, b) = text.split_once('|').expect("a cursor");
        complete(&format!("{a}{b}"), a.encode_utf16().count() as u32).help
    }

    #[test]
    fn a_call_shows_how_it_is_written() {
        let notes = format!("{SONG}clip c = lead\n  ");
        assert_eq!(help(&format!("{notes}arp(|")), usage::ARP);
        assert_eq!(help(&format!("{notes}arp([c4,e4|")), usage::ARP);
        assert_eq!(help(&format!("{notes}euclid(5,8) a1|")), "");
        assert_eq!(
            help(&format!("{SONG}clip d = kit\n  cb euclid(3|")),
            usage::EUCLID
        );
        assert_eq!(
            help(&format!("{SONG}clip d = lead .every(4, fast(|")),
            usage::METHODS[0].1
        );
        assert_eq!(
            help(&format!("{SONG}clip d = lead .every(4, |")),
            usage::EVERY
        );
        assert_eq!(
            help(&format!("{SONG}clip d = lead .scale(riff |")),
            usage::SCALE
        );
        assert_eq!(
            help(&format!("{SONG}clip d = lead .struct(\"x |")),
            usage::STRUCT
        );
        assert_eq!(
            help(&format!("{SONG}clip d = lead .cutoff(|")),
            "cutoff takes a number or a signal from 20 to 20000: cutoff(sine.range(20, 20000))"
        );
        assert_eq!(
            help(&format!("{SONG}clip d = lead .cutoff(sine.slow(|")),
            usage::SIGNAL[6].1
        );
        assert_eq!(help("mod lead.cutoff = lfo(|"), usage::SIGNAL[0].1);
        assert_eq!(help("clip c = lead voicing(drop2,|"), usage::VOICING);
        assert_eq!(help("clip d = lead .fast(2) |"), "");
        assert_eq!(help("tempo(|"), "");
    }

    /// The preview of the call at `|`.
    fn preview(text: &str) -> String {
        let (a, b) = text.split_once('|').expect("a cursor");
        complete(&format!("{a}{b}"), a.encode_utf16().count() as u32).preview
    }

    #[test]
    fn a_call_shows_what_it_gives() {
        let notes = format!("{SONG}clip c = lead\n  ");
        // In the call, just after it, and with its `)` still to come.
        assert_eq!(preview(&format!("{notes}euclid(5,8|) c4")), "x.xx.xx.");
        assert_eq!(preview(&format!("{notes}euclid(3,8)|")), "x..x..x.");
        assert_eq!(preview(&format!("{notes}euclid(3,8,2|")), ".x..x.x.");
        assert_eq!(
            preview(&format!("{SONG}clip d = kit\n  cb euclid(2,4)|")),
            "x.x."
        );
        assert_eq!(
            preview(&format!("{SONG}clip d = lead .euclid(1,3|)")),
            "x.."
        );
        assert_eq!(preview(&format!("{notes}euclid(9,8|")), "");
        // On the song's scale: c minor.
        // c natural minor: the dominant is minor, ii diminished.
        assert_eq!(preview(&format!("{notes}prog(4,7|")), "i iio iio v");
        let major = "scale c major\ntrack l synth\nclip c = l\n  prog(4,7|";
        assert!(preview(major).starts_with("I "), "{}", preview(major));
        assert!(preview(major).ends_with(" V"), "{}", preview(major));
        // The notes the clip plays.
        let walk = format!("{notes}walk(c4,4,1)");
        let song = Song::parse(&walk).expect("a song");
        let played: Vec<String> = song.clips[2]
            .notes
            .as_ref()
            .expect("notes")
            .events
            .iter()
            .map(|e| notes::note_name(e.note))
            .collect();
        assert_eq!(preview(&format!("{walk}|")), played.join(" "));
        assert_eq!(preview(&format!("{notes}prog(4,|")), "");
        // Two chords a bar, and the words after the seed (#608-#610).
        assert_eq!(
            preview(&format!("{notes}prog(2,7,2|")).split(' ').count(),
            4
        );
        assert_eq!(
            preview(&format!("{notes}prog(2,7,\"2 2\"|)"))
                .split(' ')
                .count(),
            4
        );
        assert_eq!(at(&format!("{notes}prog(4,7,j|")), ["jazz"]);
        assert_eq!(at(&format!("{notes}prog(4,7,7th,s|")), ["sus"]);
        // Without a scale, no chords.
        assert_eq!(preview("track l synth\nclip c = l\n  prog(4,7|"), "");
    }

    #[test]
    fn scale_of_a_clip_shows_its_roots_a_bar() {
        let song =
            "track bass synth\ntrack lead synth\nclip pedal = bass\n  \"<a1 d1> <~ [f1 g1]>\"\n";
        let at = |line: &str| preview(&format!("{song}{line}"));
        assert_eq!(at("clip arp = lead .scale(pedal |minor)"), "a1 d1");
        assert_eq!(at("clip arp = lead .scale(ped|"), "");
        // Not a clip: no roots.
        assert_eq!(at("clip arp = lead .scale(a1 minor)|"), "");
        let rest = "track b synth\nclip r = b\n  \"~ c2\"\nclip x = b .scale(r minor)|";
        assert_eq!(
            preview(rest),
            "c2",
            "before its first note, its last chord sounds"
        );
    }

    #[test]
    fn the_errors_say_what_the_help_says() {
        let err = |text: &str| Song::parse(text).expect_err("an error").msg;
        let notes = "scale c minor\ntrack lead synth\nclip c = lead\n  ";
        assert_eq!(
            err(&format!("{notes}arp(c4)")),
            help(&format!("{notes}arp(|"))
        );
        assert_eq!(
            err(&format!("{notes}prog(4)")),
            help(&format!("{notes}prog(|"))
        );
        assert_eq!(
            err(&format!("{notes}euclid(9)")),
            help(&format!("{notes}euclid(|"))
        );
        let clip = "track lead synth\nclip c = lead .every(4)\n  \"c4\"";
        assert_eq!(err(clip), help("track lead synth\nclip c = lead .every(|"));
    }

    #[test]
    fn every_list_has_its_details() {
        fn names(t: &[(&'static str, &str)]) -> Vec<&'static str> {
            t.iter().map(|e| e.0).collect()
        }
        assert_eq!(names(&KEYWORD_USAGE), KEYWORDS);
        assert_eq!(names(&METHODS), Pattern::NAMES);
        assert_eq!(names(&CALLS)[..7], notes::CALLS);
        assert_eq!(names(&PADS), Pad::ALL.map(|p| p.1));
        assert_eq!(names(&CLIP_WORDS), ["live", "bars", "voicing"]);
        let styles: Vec<&str> = crate::notes::STYLES.iter().map(|(n, _)| *n).collect();
        assert_eq!(names(&VOICINGS), styles);
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

    /// #674: a Modular track's methods and targets offer its setting's
    /// knobs first, by the names its SynthDef gives them.
    #[test]
    fn a_modular_track_offers_its_knobs() {
        let head = "setting vibra = Modular ModularBasic\n  SynthDef(\\vibra, { |freq = 220, gate = 1, vib = 4, vibmod = 6|\n    LFTri.ar(freq * (SinOsc.kr(vib) * vibmod).midiratio)\n  }).add;\ntrack lead synth vibra\ntrack bass synth Minimoog\n";
        // The cursor at the end: the code's `|` is not one.
        let end = |line: &str| {
            let text = format!("{head}{line}");
            complete(&text, text.encode_utf16().count() as u32).items
        };
        let words = |line: &str| end(line).into_iter().map(|i| i.word).collect::<Vec<_>>();
        assert_eq!(words("clip a = lead .vi")[..2], ["vib", "vibmod"]);
        assert_eq!(words("mod lead.vi")[..2], ["vib", "vibmod"]);
        assert_eq!(words("auto w = lead.vibm"), ["vibmod"]);
        assert_eq!(words("snapshot s: lead.vibm"), ["vibmod"]);
        assert!(!words("clip b = bass .vi").contains(&"vib".to_string()));
        assert_eq!(end("mod lead.vib")[0].detail, "a knob of vibra");
    }

    /// #677: a clip offers `adsr` where its model reads the ADSR.
    #[test]
    fn a_clip_offers_adsr() {
        assert!(at("track p synth Juno106\nclip a = p .ad|").contains(&"adsr".to_string()));
        assert!(!at("track e synth Dx7\nclip a = e .ad|").contains(&"adsr".to_string()));
    }

    /// #688: inside a knob's method, its help and range; a knob named like a
    /// pattern method says how a clip reaches it; `.adsr(` has its help.
    #[test]
    fn a_knob_has_help_and_a_pattern_name_says_how() {
        let head = "setting vibra = Modular ModularBasic\n  SynthDef(\\vibra, { |freq = 220, gate = 1, vib = 4, fast = 2|\n    LFTri.ar(freq * (SinOsc.kr(vib) * fast).midiratio)\n  }).add;\ntrack lead synth vibra\n";
        let end = |line: &str| {
            let text = format!("{head}{line}");
            complete(&text, text.encode_utf16().count() as u32)
        };
        let help = end("clip a = lead .vib(").help;
        assert!(
            help.starts_with("vib is a knob of vibra, from 0 to 8"),
            "{help}"
        );
        let fast = end("clip a = lead .fa").items;
        let knob = fast
            .iter()
            .find(|i| i.word == "fast" && i.class == Class::Param);
        assert_eq!(
            knob.map(|i| i.detail.as_str()),
            Some("a knob of vibra: on a clip .ctl2 (fast is a pattern method)")
        );
        assert!(end("clip a = lead .adsr(").help.starts_with("adsr takes"));
    }
}
