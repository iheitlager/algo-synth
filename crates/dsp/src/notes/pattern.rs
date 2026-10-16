//! Pattern methods (ADR-0019, #215): Strudel's musical transformations,
//! written on a clip's line and applied to its events when the song loads.
//!
//! ```text
//! clip riff = lead .fast(2) .every(4, rev) .off(1/8, add(12))
//! ```
//!
//! A cycle is a bar. `fast(n)` and `slow(n)` squeeze or stretch the line;
//! `rev` plays each bar backwards and `palindrome` every other one; `add(n)`
//! and `sub(n)` transpose by semitones; `ply(n)` repeats each note n times in
//! its own length; `iter(n)` starts each bar a further 1/n in; `degrade(p)`
//! drops each note with chance p, from a fixed seed (ADR-0005); `every(n, m)`
//! applies method m on every nth bar, from the first; `off(t, m)` layers a
//! copy t of a bar later with m applied; `struct("x ~ x x")` plays the notes
//! sounding at each hit of a rhythm, each bar, and `struct(<clip>)` at each
//! hit of an earlier clip (#600); `sometimes(m)` takes about half
//! the moments from the line with m applied, from a fixed seed; `scale(c minor)`
//! moves each note to the nearest note of the scale and makes each degree a
//! note, counted from the root in octave 3 or the one written (`a1`, #570);
//! `scale(<a1 d2>/4 minor)` takes the roots in turn, a bar each or `/n` bars
//! each, by the bar a note starts in (#571); `strum(ms)` starts a chord's
//! notes ms apart, the engine playing them between ticks (#604). `fast`,
//! `slow`, `add`, `sub`, `ply` and `degrade` take a value a bar as well:
//! `add("<0 <1 -1>>")` adds 0, 1, 0 and -1 in turn, a nested `< >` taking
//! its own turns, as in Strudel (#629).
//!
//! Every method maps a line (events and a length in bars) to a line, so
//! nothing happens in `render`. The result is held to `MAX_EVENTS` events and
//! `MAX_BARS` bars; times are on the 48-tick grid, so a squeeze rounds.

use super::generate::{MAX_CHORD, chord_at};
use super::{CHANCE_BARS, DEGREE_ZERO, Event, MAX_BARS, MAX_EVENTS, Play, TICKS_PER_BAR, lcm};
use crate::algo::{Euclid, Mode, Rng, Scale, mix, pitch_class};

const BAR: u32 = TICKS_PER_BAR;
/// How far `jux` puts each side: all the way, as Strudel's.
const JUX_PAN: i8 = 100;
/// Largest factor of `fast`, `slow`, `ply`, `iter` and `every`.
const MAX_FACTOR: u32 = 16;
/// Most steps in a `struct` rhythm: one a tick.
const MAX_STEPS: usize = BAR as usize;

/// A part of a bar one value of a pattern method holds (#676): its ticks
/// in the bar, the method with that value, and where the value is written in
/// the method's quoted argument (byte offset, length).
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub from: u32,
    pub to: u32,
    pub method: Pattern,
    pub at: (usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Fast(Ratio),
    Slow(Ratio),
    Rev,
    Palindrome,
    /// Cents, written in semitones (`add(0.5)`, #699); `sub(n)` kept apart
    /// so it prints as written.
    Add(i32),
    Sub(i32),
    /// `add([0,2,4])`: a copy of each note per offset, sounding together
    /// (#650).
    Stack(Vec<i32>),
    Ply(u32),
    Iter(u32),
    /// Each bar cut into n parts, played in a new order each bar (#615).
    Shuffle(u32),
    /// n parts drawn from each bar's, some twice, some left out (#615).
    Scramble(u32),
    Degrade(f32),
    Every(u32, Box<Pattern>),
    /// A shift in ticks and the method on the copy.
    Off(u32, Box<Pattern>),
    /// Hits and rests sharing each bar.
    Struct(Vec<bool>),
    /// A struct of `k` hits spread over `n` steps (#582).
    Euclid(Euclid),
    /// A method on some moments, by chance: `sometimes(m)` and its kin
    /// (#700), as written, and `sometimesBy(p, m)`.
    Sometimes(Chance, Box<Pattern>),
    Scale(Keys),
    /// `scale(<clip> <mode>)`: the roots are the notes an earlier clip plays
    /// (ADR-0035, #591).
    ScaleOf(Context, Mode),
    /// Whole octaves, after the degrees are notes (#591).
    Oct(i32),
    /// `chord(<clip>)`: degrees count the notes of the chord an earlier clip
    /// plays (ADR-0035, #592).
    ChordOf(Context),
    /// A value a bar (#629): the method as written, `add("<0 3 5>")`, and
    /// for each bar in turn the parts its values split it into (#676).
    PerBar(String, Vec<Vec<Part>>),
    /// `struct(<clip>)`: the rhythm of an earlier clip, its note starts or
    /// its drum hits (ADR-0035, #600).
    StructOf(Context),
    /// Milliseconds between a chord's notes, upward; downward when below 0
    /// (#604).
    Strum(i16),
    /// `jux(m)`: the line on the left, and a copy with method m on the
    /// right, as Strudel's (#701).
    Jux(Box<Pattern>),
    /// `arp("0 3 <2 0>")`: the line's own chords played by index, as
    /// Strudel's arp (#696): the indexes as written, and as a line of degrees.
    Arp(String, Line),
    /// `chop(n)`: each note cut into n parts, one after another, each
    /// playing the next n-th of its sample (#716).
    Chop(u32),
    /// `slice(n, "0 2 <1 3>")`: the sample cut into n parts and part k
    /// played for each index k, on the indexes' rhythm (#716): the count,
    /// the indexes as written, and as a line of degrees.
    Slice(u32, String, Line),
    /// `fit()`: each note's part of its sample plays over the note's length,
    /// at any tempo, as Strudel's fit (#717).
    Fit,
    /// `splice(n, "0 2 <1 3>")`: `slice`, each part fitted to its step
    /// (#717): the count, the indexes as written, and as a line of degrees.
    Splice(u32, String, Line),
    /// `speed(x)`: each note's sampler speed times x, below 0 backwards, in
    /// hundredths (#718).
    Speed(i16),
    /// `accelerate(a)`: a speed added to each note's by its end, in
    /// hundredths, as Tidal's (#718).
    Accelerate(i16),
    /// `coarse(n)`: each output value held for n samples, as Tidal's (#718).
    Coarse(u8),
    /// `mul(speed(…))`: Strudel's spelling of a speed that multiplies, as
    /// `speed` does (#718).
    Mul(Box<Pattern>),
    /// `stut(n, gain, time)`, Tidal's, and `echo(n, time, gain)`, Strudel's
    /// order (#719): n copies of each note `time` of a bar apart, the k-th
    /// at gain^k of its velocity; the time in ticks and as written.
    Stut(Stut),
    /// `spread(m1, m2, …)`, Tidal's `spread ($)`: bar k through method k
    /// mod n, `None` for `id`, the bar as it is (#719).
    Spread(Vec<Option<Pattern>>),
}

/// The copies of `stut` and `echo` (#719).
#[derive(Clone, Debug, PartialEq)]
pub struct Stut {
    pub count: u32,
    pub gain: f32,
    pub ticks: u32,
    pub time: String,
    /// Written as Strudel's `echo(n, time, gain)`.
    pub echo: bool,
}

/// A factor of `fast` or `slow`, `num/den` in lowest terms, each 1 to 16
/// (#702): `2`, `3/4`, `1.5`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ratio {
    pub num: u32,
    pub den: u32,
}

impl Ratio {
    pub const fn whole(n: u32) -> Ratio {
        Ratio { num: n, den: 1 }
    }

    /// `3/4`, `1.5` or `2`; a decimal is the fraction with the smallest
    /// denominator that it is to within a millionth.
    fn parse(s: &str) -> Result<Ratio, &'static str> {
        let bad = "this takes a ratio, each side 1 to 16: 2, 3/4 or 1.5";
        let s = s.trim();
        let (num, den) = match s.split_once('/') {
            Some((a, b)) => (
                a.trim().parse::<u32>().map_err(|_| bad)?,
                b.trim().parse::<u32>().map_err(|_| bad)?,
            ),
            None => {
                let x = s.parse::<f64>().map_err(|_| bad)?;
                (1..=MAX_FACTOR)
                    .find_map(|d| {
                        let n = x * f64::from(d);
                        ((n - n.round()).abs() < 1e-6 && n.round() >= 1.0 && n.round() <= 256.0)
                            .then(|| (n.round() as u32, d))
                    })
                    .ok_or(bad)?
            }
        };
        let g = gcd(num, den);
        let r = Ratio {
            num: num / g.max(1),
            den: den / g.max(1),
        };
        let fits = |n: u32| (1..=MAX_FACTOR).contains(&n);
        if fits(r.num) && fits(r.den) {
            Ok(r)
        } else {
            Err(bad)
        }
    }

    fn print(self) -> String {
        if self.den == 1 {
            self.num.to_string()
        } else {
            format!("{}/{}", self.num, self.den)
        }
    }
}

/// How often `sometimes` and its kin apply their method (#700), with
/// Strudel's chances.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Chance {
    Sometimes,
    Often,
    Rarely,
    AlmostNever,
    AlmostAlways,
    /// `sometimesBy(p, m)`.
    By(f32),
}

/// The named chances and their methods' names.
pub const CHANCES: [(&str, Chance); 5] = [
    ("sometimes", Chance::Sometimes),
    ("often", Chance::Often),
    ("rarely", Chance::Rarely),
    ("almostNever", Chance::AlmostNever),
    ("almostAlways", Chance::AlmostAlways),
];

impl Chance {
    pub fn p(self) -> f32 {
        match self {
            Chance::Sometimes => 0.5,
            Chance::Often => 0.75,
            Chance::Rarely => 0.25,
            Chance::AlmostNever => 0.1,
            Chance::AlmostAlways => 0.9,
            Chance::By(p) => p,
        }
    }

    /// Does the method take the moment starting at `start`? One coin per
    /// moment, so a chord goes one way. The draw's top bit is the hash's low
    /// bit inverted, so at a half it is the coin `sometimes` always used.
    fn heads(self, start: u32) -> bool {
        let h = (!mix(0x50E7_1AE5 ^ start, 0x2B)).rotate_right(1);
        f64::from(h) / (f64::from(u32::MAX) + 1.0) < f64::from(self.p())
    }
}

/// An earlier clip a method reads for its harmony (ADR-0035): its name as
/// written, its events and its bars, kept so the method applies again
/// without looking it up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub name: String,
    pub events: Vec<Event>,
    pub bars: u32,
}

impl Context {
    /// The clip's notes sounding at tick `t` of the song, lowest first, into
    /// `buf`, and how many: those starting latest at or before `t` modulo the
    /// clip's length, or, before its first, its last chord, as it loops.
    /// Nothing allocates, so a live clip may ask in `render` (#593).
    pub fn chord(&self, t: u64, buf: &mut [u8; MAX_CHORD]) -> usize {
        let len = u64::from(self.bars.max(1) * BAR);
        let t = u32::try_from(t % len).unwrap_or(0);
        chord_at(&self.events, t, buf)
            .or_else(|| chord_at(&self.events, u32::MAX, buf))
            .map_or(0, |(_, n)| n)
    }

    /// Event `e` at tick `t` made a note of the chord sounding there: a
    /// degree counts its notes from the lowest (0), on up an octave past the
    /// top and down below the root; a note moves to the nearest chord note.
    /// False when the clip has no notes or the note leaves 0 to 127.
    pub fn place(&self, e: &mut Event, t: u64) -> bool {
        let mut buf = [0u8; MAX_CHORD];
        let n = self.chord(t, &mut buf);
        let Some(chord) = buf.get(..n).filter(|c| !c.is_empty()) else {
            return false;
        };
        let note = if e.degree {
            let d = i32::from(e.note) - i32::from(DEGREE_ZERO);
            let len = i32::try_from(chord.len()).unwrap_or(1);
            let k = usize::try_from(d.rem_euclid(len)).unwrap_or(0);
            chord.get(k).map(|&b| i32::from(b) + 12 * d.div_euclid(len))
        } else {
            // The chord note nearest in pitch class, down on a tie.
            let pc = |x: i32| x.rem_euclid(12);
            chord
                .iter()
                .map(|&b| {
                    let up = pc(i32::from(b) - i32::from(e.note));
                    if up < 6 { up } else { up - 12 }
                })
                .min_by_key(|d| (d.abs(), *d))
                .map(|d| i32::from(e.note) + d)
        };
        match note
            .and_then(|n| u8::try_from(n).ok())
            .filter(|n| *n <= 127)
        {
            Some(n) => {
                e.note = n;
                e.degree = false;
                true
            }
            None => false,
        }
    }

    /// Event `e` at tick `t` made the note of the chord sounding there at
    /// its index, counted from the lowest (0) and wrapping round the chord
    /// with no octave added, as Strudel's arp (#696). False when the clip
    /// has no notes there.
    /// The note takes the word of its chord, so the chord lights as it plays.
    pub fn pick(&self, e: &mut Event, t: u64) -> bool {
        let len = u64::from(self.bars.max(1) * BAR);
        let t = u32::try_from(t % len).unwrap_or(0);
        let mut buf = [0u8; MAX_CHORD];
        let Some((start, n)) = chord_at(&self.events, t, &mut buf)
            .or_else(|| chord_at(&self.events, u32::MAX, &mut buf))
        else {
            return false;
        };
        let k = (i32::from(e.note) - i32::from(DEGREE_ZERO)).rem_euclid(n.max(1) as i32);
        let Some(&note) = buf.get(..n).and_then(|c| c.get(k as usize)) else {
            return false;
        };
        e.note = note;
        e.degree = false;
        if let Some(src) = self
            .events
            .iter()
            .find(|s| s.start == start && s.note == note)
        {
            e.word = src.word;
        }
        true
    }

    /// The root sounding at tick `t`: the lowest note there.
    pub fn root(&self, t: u64) -> Option<u8> {
        let mut buf = [0u8; MAX_CHORD];
        if self.chord(t, &mut buf) > 0 {
            buf.first().copied()
        } else {
            None
        }
    }
}

/// The earlier clips a method may read, by name: their events, their bars
/// and whether they are a drum clip's hits (one event a hit, a step long).
pub type Sources<'a> = dyn Fn(&str) -> Option<(Vec<Event>, u32, bool)> + 'a;

/// Most roots in a `scale` progression.
const MAX_ROOTS: usize = 16;

/// The scales of `.scale(…)` (#571): one, or a progression `<a1 d2>/4`
/// whose roots take turns, each for `hold` bars.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keys {
    pub steps: Vec<Step>,
    pub hold: u32,
    /// The mode after the roots, for those that name none.
    pub mode: Option<Mode>,
}

/// A root of a progression and whether it names its own mode (`a1:dorian`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub key: Key,
    pub own: bool,
}

impl Keys {
    /// `a1 minor`, `<a1 d2>/4 minor` or `<a1:minor d1:dorian>`.
    fn parse(arg: &str) -> Result<Keys, &'static str> {
        let bad = crate::usage::SCALE;
        let arg = arg.trim();
        let (roots, hold, rest): (Vec<&str>, u32, &str) = if let Some(inner) = arg.strip_prefix('<')
        {
            let (inner, after) = inner.split_once('>').ok_or("a < of roots is not closed")?;
            let (hold, rest) = match after.strip_prefix('/') {
                Some(h) => {
                    let (n, rest) = h.split_once(char::is_whitespace).unwrap_or((h, ""));
                    let n = n
                        .parse::<u32>()
                        .ok()
                        .filter(|n| (1..=MAX_FACTOR).contains(n))
                        .ok_or("a root is held 1 to 16 bars, as <a1 d2>/4")?;
                    (n, rest)
                }
                None => (1, after),
            };
            (inner.split_whitespace().collect(), hold, rest)
        } else {
            let (root, rest) = arg.split_once(char::is_whitespace).unwrap_or((arg, ""));
            (vec![root], 1, rest)
        };
        if roots.is_empty() || roots.iter().any(|r| r.is_empty()) {
            return Err(bad);
        }
        if roots.len() > MAX_ROOTS {
            return Err("a progression has at most 16 roots");
        }
        let mut words = rest.split_whitespace();
        let mode = match (words.next(), words.next()) {
            (None, _) => None,
            (Some(m), None) => Some(Mode::from_name(m).ok_or_else(|| Mode::unknown(m))?),
            _ => return Err(bad),
        };
        let steps = roots
            .iter()
            .map(|r| {
                let (root, own) = match r.split_once(':') {
                    Some((root, m)) => (
                        root,
                        Some(Mode::from_name(m).ok_or_else(|| Mode::unknown(m))?),
                    ),
                    None => (*r, None),
                };
                Ok(Step {
                    key: Key::parse(root, own.or(mode).ok_or(bad)?)?,
                    own: own.is_some(),
                })
            })
            .collect::<Result<Vec<Step>, &'static str>>()?;
        // One root is held for good: the same scale however long.
        let hold = if steps.len() == 1 { 1 } else { hold };
        Ok(Keys { steps, hold, mode })
    }

    fn print(&self) -> String {
        let roots: Vec<String> = self
            .steps
            .iter()
            .map(|s| {
                let root = s.key.root_text();
                if s.own {
                    format!("{root}:{}", s.key.scale.mode.name())
                } else {
                    root
                }
            })
            .collect();
        let mut out = match roots.as_slice() {
            [one] => one.clone(),
            many => format!("<{}>", many.join(" ")),
        };
        if self.hold != 1 {
            out.push_str(&format!("/{}", self.hold));
        }
        if let Some(m) = self.mode {
            out.push_str(&format!(" {}", m.name()));
        }
        out
    }

    /// Bars before the progression comes round again.
    fn bars(&self) -> u32 {
        match self.steps.len() {
            0 | 1 => 1,
            n => u32::try_from(n).unwrap_or(1) * self.hold,
        }
    }

    /// The key in force in bar `bar`.
    fn at(&self, bar: u32) -> Option<&Key> {
        let n = u32::try_from(self.steps.len()).unwrap_or(1).max(1);
        let i = usize::try_from(bar / self.hold.max(1) % n).unwrap_or(0);
        self.steps.get(i).map(|s| &s.key)
    }
}

const BAD_ROOT: &str = "a root is a letter a to g, maybe # or b, maybe an octave";

/// A scale and the octave of its root, if written: a step of `.scale(…)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub scale: Scale,
    pub octave: Option<u8>,
}

impl Key {
    /// `a1` or `a`: a root and maybe its octave, 0 to 9.
    fn parse(root: &str, mode: Mode) -> Result<Key, &'static str> {
        let bad = BAD_ROOT;
        let (name, octave) = match root.char_indices().last() {
            Some((i, d)) if d.is_ascii_digit() => (&root[..i], d.to_digit(10)),
            _ => (root, None),
        };
        let root = pitch_class(name).ok_or(bad)?;
        Ok(Key {
            scale: Scale { root, mode },
            octave: octave.and_then(|o| u8::try_from(o).ok()),
        })
    }

    /// The root as a MIDI note: degree 0.
    pub fn root_note(&self) -> u8 {
        (self.octave.unwrap_or(super::ROOT_OCTAVE) + 1) * 12 + self.scale.root
    }

    /// `a1` or `a`, as written.
    fn root_text(&self) -> String {
        let name = super::NAMES
            .get(usize::from(self.scale.root))
            .copied()
            .unwrap_or("c");
        match self.octave {
            Some(o) => format!("{name}{o}"),
            None => name.to_string(),
        }
    }

    /// A degree made a note of this key, a note moved to the nearest of it.
    fn place(&self, e: &mut Event) {
        e.note = if e.degree {
            let d = i32::from(e.note) - i32::from(DEGREE_ZERO);
            self.scale.degree_note(self.root_note(), d)
        } else {
            self.scale.snap(e.note)
        };
        e.degree = false;
    }
}

/// A degree made a note of the scale of `mode` from `root` (degree 0), a
/// note moved to the nearest of it: a key read from a clip (ADR-0035).
pub fn place_on(e: &mut Event, root: u8, mode: Mode) {
    let scale = Scale {
        root: root % 12,
        mode,
    };
    e.note = if e.degree {
        scale.degree_note(root, i32::from(e.note) - i32::from(DEGREE_ZERO))
    } else {
        scale.snap(e.note)
    };
    e.degree = false;
}

/// A line of events looping over `bars` bars.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub events: Vec<Event>,
    pub bars: u32,
}

impl Line {
    fn len(&self) -> u32 {
        self.bars * BAR
    }

    /// The line played over and over to fill `bars`, a multiple of its own.
    fn repeat(&self, bars: u32) -> Line {
        let times = (bars / self.bars.max(1)).max(1);
        let mut events = Vec::with_capacity(self.events.len() * times as usize);
        for k in 0..times {
            events.extend(self.events.iter().map(|e| Event {
                start: e.start + k * self.len(),
                ..*e
            }));
        }
        Line { events, bars }
    }

    fn sorted(mut self) -> Line {
        self.events.sort_by_key(|e| (e.start, e.note));
        self
    }
}

impl Pattern {
    /// The names a method can have, for telling them from parameters.
    pub const NAMES: [&'static str; 38] = [
        "fast",
        "slow",
        "rev",
        "palindrome",
        "add",
        "sub",
        "ply",
        "iter",
        "degrade",
        "every",
        "off",
        "struct",
        "euclid",
        "sometimes",
        "often",
        "rarely",
        "almostNever",
        "almostAlways",
        "sometimesBy",
        "scale",
        "oct",
        "chord",
        "strum",
        "arp",
        "jux",
        "shuffle",
        "scramble",
        "chop",
        "slice",
        "fit",
        "splice",
        "speed",
        "accelerate",
        "coarse",
        "mul",
        "stut",
        "echo",
        "spread",
    ];

    /// The method `name(args)`; `args` is the text between its brackets.
    pub fn parse(name: &str, args: &str) -> Result<Pattern, &'static str> {
        Pattern::parse_in(name, args, &|_| None)
    }

    /// As `parse`, with the earlier clips a method may read (ADR-0035).
    pub fn parse_in(name: &str, args: &str, srcs: &Sources) -> Result<Pattern, &'static str> {
        let raw = args;
        let args = split_args(args);
        let whole = |s: &str| {
            s.trim()
                .parse::<u32>()
                .ok()
                .filter(|n| (1..=MAX_FACTOR).contains(n))
                .ok_or("this takes a whole number from 1 to 16")
        };
        // A sample cuts finer than a factor: Strudel's chop(32).
        let parts = |s: &str| {
            s.trim()
                .parse::<u32>()
                .ok()
                .filter(|n| (1..=64).contains(n))
                .ok_or("this takes a whole number of parts from 1 to 64")
        };
        let one = || match args.as_slice() {
            [a] => Ok(*a),
            _ => Err("this takes one value"),
        };
        let none = || match args.as_slice() {
            [] => Ok(()),
            _ => Err("this takes no value: ()"),
        };
        // A value a bar, `add("<0 3 5>")`: the method once for each (#629).
        if let (
            "fast" | "slow" | "add" | "sub" | "ply" | "degrade" | "shuffle" | "scramble" | "chop"
            | "speed" | "accelerate" | "coarse",
            [a],
        ) = (name, args.as_slice())
        {
            let a = a.trim();
            if let Some(text) = a.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
                let steps = per_bar(text)?
                    .into_iter()
                    .map(|parts| {
                        parts
                            .into_iter()
                            .map(|(from, to, v)| {
                                let at = (v.as_ptr() as usize).saturating_sub(a.as_ptr() as usize);
                                debug_assert!(
                                    at + v.len() <= a.len(),
                                    "a value is a slice of its text"
                                );
                                Ok(Part {
                                    from,
                                    to,
                                    method: Pattern::parse_in(name, v, srcs)?,
                                    at: (at, v.len()),
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(Pattern::PerBar(format!("{name}({a})"), steps));
            }
        }
        match name {
            "fast" => Ok(Pattern::Fast(Ratio::parse(one()?)?)),
            "slow" => Ok(Pattern::Slow(Ratio::parse(one()?)?)),
            "ply" => Ok(Pattern::Ply(whole(one()?)?)),
            "chop" => Ok(Pattern::Chop(parts(one()?)?)),
            "fit" => none().map(|()| Pattern::Fit),
            "stut" | "echo" => {
                let echo = name == "echo";
                let [n, a, b] = args.as_slice() else {
                    return Err(if echo {
                        crate::usage::ECHO
                    } else {
                        crate::usage::STUT
                    });
                };
                let (gain, time) = if echo { (b, a) } else { (a, b) };
                let count = whole(n)?;
                let gain = gain
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|g| (0.0..=1.0).contains(g))
                    .ok_or("a gain is from 0 to 1, each copy that much of the one before")?;
                Ok(Pattern::Stut(Stut {
                    count,
                    gain,
                    ticks: ticks(time)?,
                    time: time.trim().to_string(),
                    echo,
                }))
            }
            "spread" => {
                if args.len() < 2 || args.len() > 8 {
                    return Err(crate::usage::SPREAD);
                }
                args.iter()
                    .map(|a| match a.trim() {
                        "id" => Ok(None),
                        m => nested(m, srcs).map(Some),
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(Pattern::Spread)
            }
            "speed" | "accelerate" => {
                let x = one()?
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|x| x.is_finite() && x.abs() <= 32.0)
                    .map(|x| (x * 100.0).round() as i16)
                    .ok_or("this takes a speed from -32 to 32, 1 as is, -1 backwards")?;
                Ok(if name == "speed" {
                    Pattern::Speed(x)
                } else {
                    Pattern::Accelerate(x)
                })
            }
            "coarse" => one()?
                .trim()
                .parse::<u8>()
                .ok()
                .filter(|n| (1..=64).contains(n))
                .map(Pattern::Coarse)
                .ok_or("this takes a whole number from 1 to 64, 1 as is"),
            "mul" => {
                let m = nested(one()?, srcs)?;
                let speed = match &m {
                    Pattern::Speed(_) => true,
                    Pattern::PerBar(text, _) => text.starts_with("speed("),
                    _ => false,
                };
                if speed {
                    Ok(Pattern::Mul(Box::new(m)))
                } else {
                    Err("mul takes a speed: mul(speed(-1))")
                }
            }
            "slice" | "splice" => {
                let bad = if name == "slice" {
                    crate::usage::SLICE
                } else {
                    crate::usage::SPLICE
                };
                let [count, arg] = args.as_slice() else {
                    return Err(bad);
                };
                let n = parts(count)?;
                let arg = arg.trim();
                if !arg.starts_with('"') {
                    return Err(bad);
                }
                let idx = super::parse(arg, 0).map_err(|e| e.msg)?;
                if idx.events.is_empty() || idx.events.iter().any(|e| !e.degree) {
                    return Err(bad);
                }
                let text = idx.print();
                let line = Line {
                    events: idx.events,
                    bars: idx.bars,
                };
                Ok(if name == "slice" {
                    Pattern::Slice(n, text, line)
                } else {
                    Pattern::Splice(n, text, line)
                })
            }
            "iter" => Ok(Pattern::Iter(whole(one()?)?)),
            "shuffle" => Ok(Pattern::Shuffle(whole(one()?)?)),
            "scramble" => Ok(Pattern::Scramble(whole(one()?)?)),
            "rev" => none().map(|()| Pattern::Rev),
            "palindrome" => none().map(|()| Pattern::Palindrome),
            "add" | "sub" => {
                let semis = |s: &str| cents_of(s).ok_or("this takes semitones from -48 to 48");
                let a = one()?.trim();
                if name == "add"
                    && let Some(list) = a.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
                {
                    let offsets = list.split(',').map(semis).collect::<Result<Vec<_>, _>>()?;
                    return Ok(Pattern::Stack(offsets));
                }
                let n = semis(a)?;
                Ok(if name == "add" {
                    Pattern::Add(n)
                } else {
                    Pattern::Sub(n)
                })
            }
            "degrade" => {
                let p = match args.as_slice() {
                    [] => 0.5,
                    [a] => a
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|p| (0.0..=1.0).contains(p))
                        .ok_or("this takes a chance from 0 to 1")?,
                    _ => return Err("this takes one value"),
                };
                Ok(Pattern::Degrade(p))
            }
            "every" | "off" => {
                let [a, m] = args.as_slice() else {
                    return Err(if name == "every" {
                        crate::usage::EVERY
                    } else {
                        crate::usage::OFF
                    });
                };
                let inner = Box::new(nested(m, srcs)?);
                if name == "every" {
                    Ok(Pattern::Every(whole(a)?, inner))
                } else {
                    Ok(Pattern::Off(ticks(a)?, inner))
                }
            }
            "struct" => {
                let bad = crate::usage::STRUCT;
                let arg = one()?.trim();
                if let Some((events, bars, _)) = srcs(arg) {
                    if bars > MAX_BARS {
                        return Err("the pattern runs past 64 bars");
                    }
                    if events.is_empty() {
                        return Err("this clip has no hits to take a rhythm from");
                    }
                    return Ok(Pattern::StructOf(Context {
                        name: arg.to_string(),
                        events,
                        bars,
                    }));
                }
                let text = arg
                    .strip_prefix('"')
                    .and_then(|t| t.strip_suffix('"'))
                    .ok_or(bad)?;
                let steps = text
                    .split_whitespace()
                    .map(|w| match w {
                        "x" => Ok(true),
                        "~" => Ok(false),
                        _ => Err(bad),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if steps.is_empty() || steps.len() > MAX_STEPS {
                    return Err("a rhythm has 1 to 48 steps");
                }
                Ok(Pattern::Struct(steps))
            }
            "euclid" => {
                let e = Euclid::args(&format!("{raw})"))?;
                if e.n as usize > MAX_STEPS {
                    return Err("a rhythm has 1 to 48 steps");
                }
                Ok(Pattern::Euclid(e))
            }
            "sometimesBy" => {
                let [p, m] = args.as_slice() else {
                    return Err(crate::usage::SOMETIMES_BY);
                };
                let p = p
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|p| (0.0..=1.0).contains(p))
                    .ok_or("this takes a chance from 0 to 1")?;
                Ok(Pattern::Sometimes(
                    Chance::By(p),
                    Box::new(nested(m, srcs)?),
                ))
            }
            "sometimes" | "often" | "rarely" | "almostNever" | "almostAlways" => {
                let c = CHANCES.iter().find(|(w, _)| *w == name).map(|(_, c)| *c);
                let c = c.unwrap_or(Chance::Sometimes);
                Ok(Pattern::Sometimes(c, Box::new(nested(one()?, srcs)?)))
            }
            "scale" => {
                let arg = one()?.trim();
                let (first, rest) = arg.split_once(char::is_whitespace).unwrap_or((arg, ""));
                // A clip's name wins over a root (ADR-0035).
                match srcs(first) {
                    Some((_, _, true)) => Err("a drum clip has no notes to take roots from"),
                    Some((events, bars, false)) => {
                        let mode = match rest.split_whitespace().collect::<Vec<_>>()[..] {
                            [m] => Mode::from_name(m).ok_or_else(|| Mode::unknown(m))?,
                            _ => return Err(crate::usage::SCALE),
                        };
                        if events.is_empty() {
                            return Err("this clip has no notes to take roots from");
                        }
                        Ok(Pattern::ScaleOf(
                            Context {
                                name: first.to_string(),
                                events,
                                bars,
                            },
                            mode,
                        ))
                    }
                    None => Keys::parse(arg).map(Pattern::Scale).map_err(|m| {
                        if m == BAD_ROOT && !first.starts_with('<') {
                            "a root is a letter a to g, maybe # or b, maybe an octave, or an earlier clip"
                        } else {
                            m
                        }
                    }),
                }
            }
            "chord" => {
                let name = one()?.trim();
                let (events, bars, drums) = srcs(name).ok_or(crate::usage::CHORD)?;
                if drums {
                    return Err("a drum clip has no notes to take chords from");
                }
                if events.is_empty() {
                    return Err("this clip has no notes to take chords from");
                }
                Ok(Pattern::ChordOf(Context {
                    name: name.to_string(),
                    events,
                    bars,
                }))
            }
            "jux" => Ok(Pattern::Jux(Box::new(nested(one()?, srcs)?))),
            "arp" => {
                let bad = crate::usage::ARP_METHOD;
                let arg = one()?.trim();
                if !arg.starts_with('"') {
                    return Err(bad);
                }
                let idx = super::parse(arg, 0).map_err(|e| e.msg)?;
                if idx.events.iter().any(|e| !e.degree) {
                    return Err(
                        "arp takes indexes into the chord, 0 its lowest note: arp(\"0 2 1\")",
                    );
                }
                if idx.events.is_empty() {
                    return Err(bad);
                }
                Ok(Pattern::Arp(
                    idx.print(),
                    Line {
                        events: idx.events,
                        bars: idx.bars,
                    },
                ))
            }
            "strum" => one()?
                .trim()
                .parse::<i16>()
                .ok()
                .filter(|n| *n != 0 && n.abs() <= 100)
                .map(Pattern::Strum)
                .ok_or("this takes milliseconds from 1 to 100, below 0 to strum down"),
            "oct" => one()?
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|n| n.abs() <= 4)
                .map(Pattern::Oct)
                .ok_or("this takes octaves from -4 to 4"),
            _ => Err("no such pattern method"),
        }
    }

    /// The method as written on a clip line, without its dot.
    pub fn print(&self) -> String {
        let bare = |p: &Pattern| match p {
            Pattern::Rev => "rev".to_string(),
            Pattern::Palindrome => "palindrome".to_string(),
            other => other.print(),
        };
        match self {
            Pattern::Fast(r) => format!("fast({})", r.print()),
            Pattern::Slow(r) => format!("slow({})", r.print()),
            Pattern::Rev => "rev()".to_string(),
            Pattern::Palindrome => "palindrome()".to_string(),
            Pattern::Add(n) => format!("add({})", semitones(*n)),
            Pattern::Sub(n) => format!("sub({})", semitones(*n)),
            Pattern::Stack(offsets) => {
                let words: Vec<String> = offsets.iter().map(|c| semitones(*c)).collect();
                format!("add([{}])", words.join(","))
            }
            Pattern::Ply(n) => format!("ply({n})"),
            Pattern::Iter(n) => format!("iter({n})"),
            Pattern::Shuffle(n) => format!("shuffle({n})"),
            Pattern::Scramble(n) => format!("scramble({n})"),
            Pattern::Degrade(p) => format!("degrade({p})"),
            Pattern::Every(n, m) => format!("every({n}, {})", bare(m)),
            Pattern::Off(t, m) => {
                let g = gcd(*t, BAR);
                format!("off({}/{}, {})", t / g, BAR / g, bare(m))
            }
            Pattern::Struct(steps) => {
                let words: Vec<&str> = steps.iter().map(|h| if *h { "x" } else { "~" }).collect();
                format!("struct(\"{}\")", words.join(" "))
            }
            Pattern::Euclid(e) => e.print(),
            Pattern::Sometimes(Chance::By(p), m) => format!("sometimesBy({p}, {})", bare(m)),
            Pattern::Sometimes(c, m) => {
                let name = CHANCES.iter().find(|(_, k)| k == c).map_or("", |(w, _)| *w);
                format!("{name}({})", bare(m))
            }
            Pattern::Scale(keys) => format!("scale({})", keys.print()),
            Pattern::ScaleOf(c, mode) => format!("scale({} {})", c.name, mode.name()),
            Pattern::Oct(n) => format!("oct({n})"),
            Pattern::ChordOf(c) => format!("chord({})", c.name),
            Pattern::StructOf(c) => format!("struct({})", c.name),
            Pattern::Strum(ms) => format!("strum({ms})"),
            Pattern::Arp(text, _) => format!("arp({text})"),
            Pattern::Chop(n) => format!("chop({n})"),
            Pattern::Slice(n, text, _) => format!("slice({n}, {text})"),
            Pattern::Fit => "fit()".to_string(),
            Pattern::Speed(x) => format!("speed({})", hundredths(*x)),
            Pattern::Accelerate(x) => format!("accelerate({})", hundredths(*x)),
            Pattern::Coarse(n) => format!("coarse({n})"),
            Pattern::Mul(m) => format!("mul({})", bare(m)),
            Pattern::Stut(s) if s.echo => format!("echo({}, {}, {})", s.count, s.time, s.gain),
            Pattern::Stut(s) => format!("stut({}, {}, {})", s.count, s.gain, s.time),
            Pattern::Spread(ms) => {
                let words: Vec<String> = ms
                    .iter()
                    .map(|m| m.as_ref().map_or_else(|| "id".to_string(), bare))
                    .collect();
                format!("spread({})", words.join(", "))
            }
            Pattern::Splice(n, text, _) => format!("splice({n}, {text})"),
            Pattern::Jux(m) => format!("jux({})", bare(m)),
            Pattern::PerBar(text, _) => text.clone(),
        }
    }

    /// The line this method makes of `line`.
    pub fn apply(&self, line: &Line) -> Result<Line, &'static str> {
        let out = match self {
            Pattern::Fast(r) => speed(line, r.num, r.den)?,
            Pattern::Slow(r) => speed(line, r.den, r.num)?,
            Pattern::Rev => rev(line),
            Pattern::Palindrome => alternate(line, 2, 1, &Pattern::Rev)?,
            Pattern::Add(n) => transpose(line, *n),
            Pattern::Sub(n) => transpose(line, -n),
            Pattern::Stack(offsets) => {
                check(line.bars, line.events.len() * offsets.len())?;
                let mut out = Line {
                    events: Vec::new(),
                    bars: line.bars,
                };
                for n in offsets {
                    out.events.extend(transpose(line, *n).events);
                }
                out
            }
            Pattern::Ply(n) => ply(line, *n),
            Pattern::Chop(n) => chop(line, *n)?,
            Pattern::Slice(n, _, idx) => slice(line, *n, idx)?,
            Pattern::Fit => fit(line.clone()),
            Pattern::Speed(x) => played(line, |p| {
                p.speed = (i32::from(p.speed) * i32::from(*x) / 100)
                    .clamp(-i32::from(Play::MAX), i32::from(Play::MAX))
                    as i16;
            }),
            Pattern::Accelerate(x) => played(line, |p| {
                p.accel = p.accel.saturating_add(*x).clamp(-Play::MAX, Play::MAX);
            }),
            Pattern::Coarse(n) => played(line, |p| p.coarse = *n),
            Pattern::Mul(m) => m.apply(line)?,
            Pattern::Stut(s) => stut(line, s)?,
            Pattern::Spread(ms) => {
                // A part a bar, each through its method; `id` the bar as is.
                let as_is = Pattern::Fast(Ratio::whole(1));
                let steps: Vec<Vec<Part>> = ms
                    .iter()
                    .map(|m| {
                        vec![Part {
                            from: 0,
                            to: BAR,
                            method: m.clone().unwrap_or_else(|| as_is.clone()),
                            at: (0, 0),
                        }]
                    })
                    .collect();
                by_bar(line, &steps)?
            }
            Pattern::Splice(n, _, idx) => fit(slice(line, *n, idx)?),
            Pattern::Iter(n) => iter(line, *n),
            Pattern::Shuffle(n) => reorder(line, *n, true)?,
            Pattern::Scramble(n) => reorder(line, *n, false)?,
            Pattern::Degrade(p) => Line {
                events: line
                    .events
                    .iter()
                    .copied()
                    .filter(|e| {
                        let h = mix(0xDE67_ADE0 ^ e.start, u32::from(e.note));
                        f64::from(h) / (f64::from(u32::MAX) + 1.0) >= f64::from(*p)
                    })
                    .collect(),
                bars: line.bars,
            },
            Pattern::Every(n, m) => alternate(line, *n, 0, m)?,
            Pattern::Off(t, m) => {
                let mut copy = m.apply(line)?;
                let len = copy.len();
                for e in &mut copy.events {
                    e.start = (e.start + t) % len.max(1);
                }
                let bars = lcm(line.bars, copy.bars);
                check(bars, line.events.len() * (bars / line.bars) as usize)?;
                let mut out = line.repeat(bars);
                out.events.extend(copy.repeat(bars).events);
                out
            }
            Pattern::Struct(steps) => structure(line, steps),
            Pattern::Euclid(e) => structure(line, &e.pattern()),
            Pattern::StructOf(c) => {
                // The line and its source round together, each hit of the
                // source played at the song tick it falls on (ADR-0035).
                let bars = lcm(line.bars, c.bars);
                check(bars, 0)?;
                restructure(line, c, bars)
            }
            Pattern::Sometimes(c, m) => {
                let other = m.apply(line)?;
                let bars = lcm(line.bars, other.bars);
                check(bars, line.events.len() * (bars / line.bars) as usize)?;
                let heads = |e: &Event| c.heads(e.start);
                let mut events: Vec<Event> = line
                    .repeat(bars)
                    .events
                    .into_iter()
                    .filter(|e| !heads(e))
                    .collect();
                events.extend(other.repeat(bars).events.into_iter().filter(heads));
                Line { events, bars }
            }
            Pattern::Scale(keys) => {
                // Long enough for the line and the progression to come round
                // together; each note takes the key of the bar it starts in.
                let bars = lcm(line.bars, keys.bars());
                check(bars, line.events.len() * (bars / line.bars.max(1)) as usize)?;
                let mut out = line.repeat(bars);
                for e in &mut out.events {
                    if let Some(k) = keys.at(e.start / BAR) {
                        k.place(e);
                    }
                }
                out
            }
            Pattern::ScaleOf(c, mode) => {
                // Each note takes the root sounding in the clip where it
                // starts, the two round together (ADR-0035).
                let bars = lcm(line.bars, c.bars);
                check(bars, line.events.len() * (bars / line.bars.max(1)) as usize)?;
                let mut out = line.repeat(bars);
                for e in &mut out.events {
                    if let Some(root) = c.root(u64::from(e.start)) {
                        place_on(e, root, *mode);
                    }
                }
                out
            }
            Pattern::ChordOf(c) => {
                // Each degree a note of the chord sounding in the clip where
                // it starts, the two round together (ADR-0035).
                let bars = lcm(line.bars, c.bars);
                check(bars, line.events.len() * (bars / line.bars.max(1)) as usize)?;
                let mut out = line.repeat(bars);
                out.events.retain_mut(|e| c.place(e, u64::from(e.start)));
                out
            }
            Pattern::Strum(ms) => strum(line, *ms),
            Pattern::Jux(m) => {
                let copy = m.apply(line)?;
                let bars = lcm(line.bars, copy.bars);
                check(bars, line.events.len() * (bars / line.bars.max(1)) as usize)?;
                let side = |l: &Line, pan: i8| {
                    let mut l = l.repeat(bars);
                    for e in &mut l.events {
                        e.pan = pan;
                    }
                    l.events
                };
                let mut events = side(line, -JUX_PAN);
                events.extend(side(&copy, JUX_PAN));
                Line { events, bars }
            }
            Pattern::Arp(_, idx) => {
                if line.events.iter().any(|e| e.degree) {
                    return Err("arp plays notes: write it after scale");
                }
                // The line's chords as a clip read for harmony, the indexes
                // looked up in it as the two round together (ADR-0035).
                let chords = Context {
                    name: String::new(),
                    events: line.events.clone(),
                    bars: line.bars,
                };
                let bars = lcm(idx.bars, line.bars);
                check(bars, idx.events.len() * (bars / idx.bars.max(1)) as usize)?;
                let mut out = idx.repeat(bars);
                out.events
                    .retain_mut(|e| chords.pick(e, u64::from(e.start)));
                out
            }
            Pattern::PerBar(_, steps) => by_bar(line, steps)?,
            Pattern::Oct(n) => {
                if line.events.iter().any(|e| e.degree) {
                    return Err("oct moves notes: write it after scale");
                }
                transpose(line, 1200 * n)
            }
        };
        check(out.bars, out.events.len())?;
        Ok(out.sorted())
    }
}

impl Pattern {
    /// Does this method change only pitches, one note at a time, so a live
    /// clip can take it in `render` (#593)?
    pub fn pitch_only(&self) -> bool {
        match self {
            Pattern::PerBar(_, steps) => steps.iter().flatten().all(|p| p.method.pitch_only()),
            p => matches!(
                p,
                Pattern::Add(_)
                    | Pattern::Sub(_)
                    | Pattern::Oct(_)
                    | Pattern::Scale(_)
                    | Pattern::ScaleOf(..)
                    | Pattern::ChordOf(_)
            ),
        }
    }

    /// Does this method change pitches or play times in milliseconds, so a
    /// drum clip, whose lanes hold neither, can't take it (#653)?
    pub fn moves_pitch(&self) -> bool {
        match self {
            Pattern::Every(_, m) | Pattern::Off(_, m) | Pattern::Sometimes(_, m) => m.moves_pitch(),
            // A lane holds a hit, an accent or a ghost, not gain^k (#719).
            Pattern::Stut(_) => true,
            Pattern::Spread(ms) => ms.iter().flatten().any(Pattern::moves_pitch),
            // A drum clip's lanes hold no pan, so jux is for notes too, nor
            // the part of a sample a hit plays (#716).
            Pattern::Stack(_)
            | Pattern::Strum(_)
            | Pattern::Arp(..)
            | Pattern::Jux(_)
            | Pattern::Chop(_)
            | Pattern::Slice(..)
            | Pattern::Fit
            | Pattern::Splice(..)
            | Pattern::Speed(_)
            | Pattern::Accelerate(_)
            | Pattern::Coarse(_)
            | Pattern::Mul(_) => true,
            p => p.pitch_only(),
        }
    }

    /// A pitch method on event `e`, which starts at tick `t` of the song:
    /// false when the note goes (out of 0 to 127). Nothing allocates (#593).
    pub fn pitch(&self, e: &mut Event, t: u64) -> bool {
        let shift = |e: &mut Event, by: i32| match moved(e, by) {
            Some(m) => {
                *e = m;
                true
            }
            None => false,
        };
        match self {
            Pattern::Add(n) => shift(e, *n),
            Pattern::Sub(n) => shift(e, -n),
            Pattern::Oct(n) => !e.degree && shift(e, 1200 * n),
            Pattern::Scale(keys) => {
                let bar =
                    u32::try_from(t / u64::from(BAR) % u64::from(keys.bars().max(1))).unwrap_or(0);
                if let Some(k) = keys.at(bar) {
                    k.place(e);
                }
                true
            }
            Pattern::ScaleOf(c, mode) => {
                if let Some(root) = c.root(t) {
                    place_on(e, root, *mode);
                }
                true
            }
            Pattern::ChordOf(c) => c.place(e, t),
            Pattern::PerBar(_, steps) => {
                let n = u64::try_from(steps.len()).unwrap_or(1).max(1);
                let k = usize::try_from(t / u64::from(BAR) % n).unwrap_or(0);
                let x = u32::try_from(t % u64::from(BAR)).unwrap_or(0);
                steps
                    .get(k)
                    .and_then(|parts| parts.iter().find(|p| p.from <= x && x < p.to))
                    .is_none_or(|p| p.method.pitch(e, t))
            }
            _ => true,
        }
    }
}

/// The pitch methods of a live clip on the bar it has just made, which
/// starts at tick `base` of the song: in place, nothing allocated (#593).
pub fn pitch_all(patterns: &[Pattern], events: &mut Vec<Event>, base: u64) {
    events.retain_mut(|e| {
        let t = base + u64::from(e.start);
        patterns.iter().all(|p| p.pitch(e, t))
    });
}

/// Every pattern of a clip applied in order.
pub fn apply_all(patterns: &[Pattern], line: Line) -> Result<Line, &'static str> {
    patterns.iter().try_fold(line, |l, p| p.apply(&l))
}

fn check(bars: u32, events: usize) -> Result<(), &'static str> {
    if bars > MAX_BARS {
        Err("the pattern runs past 64 bars")
    } else if events > MAX_EVENTS {
        Err("the pattern makes more than 512 notes")
    } else {
        Ok(())
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

/// `x / n`, rounded to the nearest tick.
fn div_round(x: u32, n: u32) -> u32 {
    (x + n / 2) / n.max(1)
}

/// The line `num/den` times as fast, repeated until it fills whole bars
/// (#702): `fast(3/4)` of a bar is 4/3 of a bar, three times round in four.
fn speed(line: &Line, num: u32, den: u32) -> Result<Line, &'static str> {
    let span = line.bars.max(1) * den;
    let copies = num / gcd(span, num);
    let bars = u64::from(copies) * u64::from(span) / u64::from(num.max(1));
    let bars = u32::try_from(bars).unwrap_or(u32::MAX);
    check(bars, line.events.len() * copies as usize)?;
    let len = line.len();
    let at = |x: u32| div_round(x * den, num);
    let mut events = Vec::new();
    for k in 0..copies {
        events.extend(line.events.iter().map(|e| Event {
            start: at(e.start + k * len),
            len: at(e.len).max(1),
            ..*e
        }));
    }
    Ok(Line { events, bars })
}

/// Each bar backwards: a note that ended `x` ticks before the bar's end
/// starts `x` ticks after its beginning.
fn rev(line: &Line) -> Line {
    let events = line
        .events
        .iter()
        .map(|e| {
            let bar = e.start / BAR * BAR;
            let end = (e.start - bar + e.len).min(BAR);
            Event {
                start: bar + BAR - end,
                ..*e
            }
        })
        .collect();
    Line {
        events,
        bars: line.bars,
    }
}

/// `x` semitones (`12`, `-0.5`, at most two decimals) in cents, within
/// four octaves (#699).
fn cents_of(x: &str) -> Option<i32> {
    let x = x.trim();
    let decimals = x.split_once('.').map_or(0, |(_, d)| d.len());
    let c = (x.parse::<f64>().ok()? * 100.0).round();
    (decimals <= 2 && c.abs() <= 4800.0).then_some(c as i32)
}

/// Cents as semitones, as written: `12`, `0.1`.
fn semitones(cents: i32) -> String {
    (f64::from(cents) / 100.0).to_string()
}

/// `e` moved by `by` cents, its note the nearest key and its cents what is
/// left, −50 to 49; `None` past 0 to 127.
fn moved(e: &Event, by: i32) -> Option<Event> {
    let all = i32::from(e.note) * 100 + i32::from(e.cents) + by;
    let note = (all + 50).div_euclid(100);
    let note = u8::try_from(note).ok().filter(|n| *n <= 127)?;
    let cents = i16::try_from(all - i32::from(note) * 100).unwrap_or(0);
    Some(Event { note, cents, ..*e })
}

/// The line moved by `by` cents, a note leaving 0 to 127 dropped.
fn transpose(line: &Line, by: i32) -> Line {
    let events = line.events.iter().filter_map(|e| moved(e, by)).collect();
    Line {
        events,
        bars: line.bars,
    }
}

fn ply(line: &Line, n: u32) -> Line {
    let mut events = Vec::new();
    for e in &line.events {
        for k in 0..n {
            events.push(Event {
                start: e.start + div_round(k * e.len, n),
                len: div_round(e.len, n).max(1),
                ..*e
            });
        }
    }
    Line {
        events,
        bars: line.bars,
    }
}

/// Each note in `n` parts across its length, part k playing the k-th n-th
/// of the note's part of its sample (#716). A part is at least a tick (48
/// to a bar): a shorter note is cut into as many as it has ticks.
fn chop(line: &Line, n: u32) -> Result<Line, &'static str> {
    let parts: u32 = line.events.iter().map(|e| n.min(e.len).max(1)).sum();
    check(line.bars, parts as usize)?;
    let mut events = Vec::new();
    for e in &line.events {
        let n = n.min(e.len).max(1);
        for k in 0..n {
            let start = e.start + div_round(k * e.len, n);
            events.push(Event {
                start,
                len: (e.start + div_round((k + 1) * e.len, n) - start).max(1),
                region: e.region.part(k, n),
                fit: fit_part(e.fit, n),
                ..*e
            });
        }
    }
    Ok(Line {
        events,
        bars: line.bars,
    })
}

/// On the rhythm of the indexes `idx`, the notes of `line` sounding at each,
/// playing part k of `n` of their sample for index k (#716), as the two
/// round together.
fn slice(line: &Line, n: u32, idx: &Line) -> Result<Line, &'static str> {
    let bars = lcm(idx.bars, line.bars);
    check(
        bars,
        idx.events.len() * (bars / idx.bars.max(1)) as usize * line.events.len().max(1),
    )?;
    let at = idx.repeat(bars);
    let total = line.len().max(1);
    let mut events = Vec::new();
    for i in &at.events {
        let k = (i32::from(i.note) - i32::from(DEGREE_ZERO)).rem_euclid(n as i32) as u32;
        events.extend(
            line.events
                .iter()
                .filter(|e| (i.start % total + total - e.start) % total < e.len)
                .map(|e| Event {
                    start: i.start,
                    len: i.len,
                    region: e.region.part(k, n),
                    fit: fit_part(e.fit, n),
                    ..*e
                }),
        );
    }
    Ok(Line { events, bars })
}

/// `count` copies of each note, `ticks` apart round the line, the k-th at
/// `gain`^k of its velocity (#719).
fn stut(line: &Line, s: &Stut) -> Result<Line, &'static str> {
    check(line.bars, line.events.len() * s.count as usize)?;
    let total = line.len().max(1);
    let mut events = Vec::new();
    for e in &line.events {
        let v = e.velocity();
        for k in 0..s.count {
            let vel = (v * s.gain.powi(k as i32) * 127.0).round();
            if vel < 1.0 {
                break;
            }
            events.push(Event {
                start: (e.start + k * s.ticks) % total,
                vel: vel.min(127.0) as u8,
                ..*e
            });
        }
    }
    Ok(Line {
        events,
        bars: line.bars,
    })
}

/// Every note's sampler playing changed by `f` (#718).
fn played(line: &Line, f: impl Fn(&mut Play)) -> Line {
    let mut out = line.clone();
    for e in &mut out.events {
        f(&mut e.play);
    }
    out
}

/// Hundredths as a number: `100` is `1`, `-50` is `-0.5`.
fn hundredths(x: i16) -> String {
    format!("{}", f32::from(x) / 100.0)
}

/// Every note fitted: its part of the sample plays over its length now
/// (#717), whatever later methods do to the length.
fn fit(mut line: Line) -> Line {
    for e in &mut line.events {
        e.fit = e.len;
    }
    line
}

/// A fitted length cut as its part is, into n (#717).
fn fit_part(fit: u32, n: u32) -> u32 {
    if fit == 0 {
        0
    } else {
        div_round(fit, n).max(1)
    }
}

/// Bar m starts (m mod n) / n of a bar further into the line.
fn iter(line: &Line, n: u32) -> Line {
    let bars = lcm(line.bars, n);
    let all = line.repeat(bars);
    let len = all.len().max(1);
    let mut events = Vec::new();
    for m in 0..bars {
        let from = m * BAR + div_round((m % n) * BAR, n);
        for e in &all.events {
            let rel = (e.start + len - from % len) % len;
            if rel < BAR {
                events.push(Event {
                    start: m * BAR + rel,
                    ..*e
                });
            }
        }
    }
    Line { events, bars }
}

/// Each bar cut into `n` parts played in another order (#615): a
/// permutation of them (`shuffle`), or `n` draws with repeats (`scramble`),
/// seeded by the bar, as `degrade` is. A note keeps its place in its part
/// and is cut at the part's end; the line repeats after `CHANCE_BARS`.
fn reorder(line: &Line, n: u32, shuffle: bool) -> Result<Line, &'static str> {
    if n <= 1 {
        return Ok(line.clone());
    }
    let bars = lcm(line.bars, CHANCE_BARS);
    check(bars, line.events.len() * (bars / line.bars.max(1)) as usize)?;
    let all = line.repeat(bars);
    let edge = |k: u32| div_round(k * BAR, n);
    let mut parts: Vec<u32> = (0..n).collect();
    let mut events = Vec::new();
    for b in 0..bars {
        let mut rng = Rng::new(mix(0x5C4A_3B1E ^ u32::from(shuffle), b));
        if shuffle {
            for k in (1..parts.len()).rev() {
                parts.swap(k, rng.next_u32() as usize % (k + 1));
            }
        } else {
            for p in &mut parts {
                *p = rng.next_u32() % n;
            }
        }
        for (to, &from) in (0..n).zip(&parts) {
            let (lo, hi) = (b * BAR + edge(from), b * BAR + edge(from + 1));
            for e in all.events.iter().filter(|e| (lo..hi).contains(&e.start)) {
                events.push(Event {
                    start: b * BAR + edge(to) + (e.start - lo),
                    len: e.len.min(hi - e.start),
                    ..*e
                });
            }
        }
        parts = (0..n).collect();
    }
    Ok(Line { events, bars })
}

/// Method `m` on the bars whose number is `phase` mod `n`, the line as it
/// is on the others.
fn alternate(line: &Line, n: u32, phase: u32, m: &Pattern) -> Result<Line, &'static str> {
    let base = line.repeat(lcm(line.bars, n));
    check(base.bars, base.events.len())?;
    let changed = m.apply(&base)?;
    let bars = lcm(base.bars, changed.bars);
    check(bars, base.events.len() * (bars / base.bars) as usize)?;
    let on = |e: &Event| (e.start / BAR) % n == phase;
    let mut events: Vec<Event> = base
        .repeat(bars)
        .events
        .into_iter()
        .filter(|e| !on(e))
        .collect();
    events.extend(changed.repeat(bars).events.into_iter().filter(on));
    Ok(Line { events, bars })
}

/// Bar b from the line with the methods of `steps[b mod n]` applied, each
/// to the notes that start in its part of the bar, round with the line
/// (#629, #676). A method that moves notes in time moves the whole line;
/// its part keeps those that land in it.
fn by_bar(line: &Line, steps: &[Vec<Part>]) -> Result<Line, &'static str> {
    let n = u32::try_from(steps.len()).unwrap_or(1).max(1);
    // Each distinct method once, however many bars and parts it holds.
    let mut methods: Vec<&Pattern> = Vec::new();
    for p in steps.iter().flatten() {
        if !methods.contains(&&p.method) {
            methods.push(&p.method);
        }
    }
    let changed = methods
        .iter()
        .map(|m| m.apply(line))
        .collect::<Result<Vec<_>, _>>()?;
    let bars = changed
        .iter()
        .fold(lcm(line.bars, n), |b, c| lcm(b, c.bars));
    check(bars, 0)?;
    let mut events = Vec::new();
    for (m, c) in methods.iter().zip(&changed) {
        events.extend(c.repeat(bars).events.into_iter().filter(|e| {
            let x = e.start % BAR;
            steps
                .get(((e.start / BAR) % n) as usize)
                .and_then(|parts| parts.iter().find(|p| p.from <= x && x < p.to))
                .is_some_and(|p| p.method == **m)
        }));
    }
    Ok(Line { events, bars })
}

const BAD_PER_BAR: &str = "a value a bar is a number, [a b] or <a b …>, as \"<0 [3 5]>\"";

/// A value in a part of a bar: its first and past-last tick, and its text.
type Value<'a> = (u32, u32, &'a str);

/// The values of a pattern method's quoted argument, in mini-notation.
enum Tree<'a> {
    Val(&'a str),
    /// `[a b]`, or `a b` at the top: sharing the bar.
    Share(Vec<Tree<'a>>),
    /// `<a b>`: one a bar, an inner one taking its own turns; `<a b>/n`
    /// holds each for n bars (#700).
    Turns(Vec<Tree<'a>>, u32),
}

/// Deepest nesting of `[ ]` and `< >` in a value a bar, as in a signal.
const MAX_TREE_DEPTH: u32 = 32;
/// Most values in one value a bar, as in a signal's sequence.
const MAX_TREE_VALUES: u32 = 64;

/// The items of `t` from byte `i` up to `close` (or the end), `depth` deep;
/// `values` counts the values read so far.
fn tree_items<'a>(
    t: &'a str,
    i: &mut usize,
    close: Option<u8>,
    depth: u32,
    values: &mut u32,
) -> Result<Vec<Tree<'a>>, &'static str> {
    if depth > MAX_TREE_DEPTH {
        return Err("a value a bar nests at most 32 deep");
    }
    let b = t.as_bytes();
    let mut out = Vec::new();
    loop {
        while b.get(*i).is_some_and(u8::is_ascii_whitespace) {
            *i += 1;
        }
        match b.get(*i).copied() {
            None if close.is_none() => return Ok(out),
            None => return Err(BAD_PER_BAR),
            Some(c) if Some(c) == close => {
                *i += 1;
                return Ok(out);
            }
            // `[0,2,4]`: a stack, one value, as a chord is in a note line.
            // Looked for only up to the next bracket, so the scan stays linear.
            Some(b'[') if stack_end(t, *i).is_some() => {
                let k = stack_end(t, *i).unwrap_or(*i);
                leaf(&mut out, t.get(*i..=k).ok_or(BAD_PER_BAR)?, values)?;
                *i = k + 1;
            }
            Some(c @ (b'[' | b'<')) => {
                *i += 1;
                let shut = Some(if c == b'[' { b']' } else { b'>' });
                let inner = tree_items(t, i, shut, depth + 1, values)?;
                if inner.is_empty() {
                    return Err(BAD_PER_BAR);
                }
                out.push(if c == b'[' {
                    Tree::Share(inner)
                } else {
                    Tree::Turns(inner, hold_of(t, i)?)
                });
            }
            Some(b']' | b'>') => return Err(BAD_PER_BAR),
            Some(_) => {
                let start = *i;
                while b
                    .get(*i)
                    .is_some_and(|c| !c.is_ascii_whitespace() && !b"[]<>".contains(c))
                {
                    *i += 1;
                }
                leaf(&mut out, t.get(start..*i).ok_or(BAD_PER_BAR)?, values)?;
            }
        }
    }
}

/// The `/n` after a `< >` at byte `i`, and past it: 1 without.
fn hold_of(t: &str, i: &mut usize) -> Result<u32, &'static str> {
    let b = t.as_bytes();
    if b.get(*i) != Some(&b'/') {
        return Ok(1);
    }
    let from = *i + 1;
    let mut end = from;
    while b.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    *i = end;
    t.get(from..end)
        .and_then(|n| n.parse::<u32>().ok())
        .filter(|n| (1..=MAX_FACTOR).contains(n))
        .ok_or("a < > holds each value 1 to 16 bars: \"<0 12>/4\"")
}

/// One more value, `v`, counted in `values`.
fn leaf<'a>(out: &mut Vec<Tree<'a>>, v: &'a str, values: &mut u32) -> Result<(), &'static str> {
    *values += 1;
    if *values > MAX_TREE_VALUES {
        return Err("a value a bar has at most 64 values");
    }
    out.push(Tree::Val(v));
    Ok(())
}

/// Where the stack (`[0,2,4]`) opening at byte `i` closes: its `]`, when a
/// comma comes before it and no other bracket does.
fn stack_end(t: &str, i: usize) -> Option<usize> {
    let rest = t.get(i + 1..)?;
    let k = rest.find(['[', ']', '<', '>'])?;
    (rest.as_bytes().get(k) == Some(&b']') && rest.get(..k)?.contains(',')).then_some(i + 1 + k)
}

impl<'a> Tree<'a> {
    /// Bars until it comes round.
    fn period(&self) -> Result<u32, &'static str> {
        let round = |items: &[Tree<'a>]| {
            items.iter().try_fold(1u32, |p, it| {
                let l = lcm(p, it.period()?);
                (l <= MAX_BARS).then_some(l).ok_or(TOO_LONG)
            })
        };
        let p = match self {
            Tree::Val(_) => 1,
            Tree::Share(items) => round(items)?,
            Tree::Turns(items, hold) => u32::try_from(items.len())
                .unwrap_or(MAX_BARS + 1)
                .saturating_mul(*hold)
                .saturating_mul(round(items)?),
        };
        (p <= MAX_BARS).then_some(p).ok_or(TOO_LONG)
    }

    /// The parts of ticks `lo..hi` in bar `b` and their values.
    fn parts(&self, b: u32, lo: u32, hi: u32, out: &mut Vec<Value<'a>>) {
        match self {
            Tree::Val(v) => out.push((lo, hi, v)),
            Tree::Share(items) => {
                let n = u32::try_from(items.len()).unwrap_or(1).max(1);
                // Each part from the first tick at or past its share, so a
                // note on a boundary that falls between ticks takes the part
                // it is in.
                let edge = |k: u32| lo + ((hi - lo) * k).div_ceil(n);
                for (k, it) in (0..).zip(items) {
                    it.parts(b, edge(k), edge(k + 1), out);
                }
            }
            Tree::Turns(items, hold) => {
                let n = u32::try_from(items.len()).unwrap_or(1).max(1);
                let b = b / (*hold).max(1);
                if let Some(it) = items.get((b % n) as usize) {
                    it.parts(b / n, lo, hi, out);
                }
            }
        }
    }
}

/// The values of `"<a [b c]>"` for each bar until they come round, as the
/// parts of the bar each holds (#676): `[ ]`, or values side by side,
/// share the bar; `< >` takes one a bar, a nested one taking its own turns,
/// so `<0 <1 -1>>` is 0 1 0 -1, as in Strudel.
fn per_bar(text: &str) -> Result<Vec<Vec<Value<'_>>>, &'static str> {
    let mut i = 0;
    let mut items = tree_items(text, &mut i, None, 0, &mut 0)?;
    let tree = match items.len() {
        0 => return Err(BAD_PER_BAR),
        1 => items.remove(0),
        _ => Tree::Share(items),
    };
    let bars = tree.period()?;
    Ok((0..bars)
        .map(|b| {
            let mut out = Vec::new();
            tree.parts(b, 0, BAR, &mut out);
            out
        })
        .collect())
}

const TOO_LONG: &str = "a value a bar comes round within 64 bars";

/// Each bar on a rhythm: at every hit, the notes sounding there, held to the
/// next step.
fn structure(line: &Line, steps: &[bool]) -> Line {
    let n = u32::try_from(steps.len()).unwrap_or(1).max(1);
    let total = line.len().max(1);
    let mut events = Vec::new();
    for m in 0..line.bars {
        for (k, _) in (0..n).zip(steps).filter(|(_, hit)| **hit) {
            let at = m * BAR + div_round(k * BAR, n);
            let len = (m * BAR + div_round((k + 1) * BAR, n) - at).max(1);
            events.extend(
                line.events
                    .iter()
                    .filter(|e| (at + total - e.start) % total < e.len)
                    .map(|e| Event {
                        start: at,
                        len,
                        ..*e
                    }),
            );
        }
    }
    Line {
        events,
        bars: line.bars,
    }
}

/// `bars` bars on the hits of `src`: at each start of its events (once
/// however many start there), the notes of `line` sounding there, held as
/// long as the first event there.
fn restructure(line: &Line, src: &Context, bars: u32) -> Line {
    let total = line.len().max(1);
    let span = src.bars.max(1) * BAR;
    let mut events = Vec::new();
    for k in 0..bars / src.bars.max(1) {
        let mut last = None;
        for h in &src.events {
            if last == Some(h.start) || h.start >= span {
                continue;
            }
            last = Some(h.start);
            let at = k * span + h.start;
            events.extend(
                line.events
                    .iter()
                    .filter(|e| (at % total + total - e.start % total) % total < e.len)
                    .map(|e| Event {
                        start: at,
                        len: h.len.max(1),
                        ..*e
                    }),
            );
        }
    }
    Line { events, bars }
}

/// Each chord of `line` (its notes sharing a start, graces aside) spread
/// `ms` apart from its lowest note up, or from its highest down when `ms`
/// is below 0; a single note stays on its tick.
fn strum(line: &Line, ms: i16) -> Line {
    let mut events = line.events.clone();
    events.sort_by_key(|e| (e.start, e.note));
    let mut i = 0;
    while i < events.len() {
        let start = events.get(i).map_or(0, |e| e.start);
        let end = i + events
            .iter()
            .skip(i)
            .take_while(|e| e.start == start)
            .count();
        let mut chord: Vec<&mut Event> = events
            .get_mut(i..end)
            .unwrap_or(&mut [])
            .iter_mut()
            .filter(|e| e.nudge >= 0)
            .collect();
        chord.sort_by_key(|e| e.note);
        if ms < 0 {
            chord.reverse();
        }
        for (k, e) in chord.into_iter().enumerate() {
            e.nudge = i16::try_from(k).unwrap_or(0).saturating_mul(ms.abs());
        }
        i = end.max(i + 1);
    }
    Line {
        events,
        bars: line.bars,
    }
}

/// `1/8`, `3/16` or `0.25` of a bar, in ticks.
fn ticks(s: &str) -> Result<u32, &'static str> {
    let s = s.trim();
    let bad = "a time is a fraction of a bar, e.g. 1/8";
    let frac = match s.split_once('/') {
        Some((a, b)) => {
            let (a, b) = (a.trim().parse::<f64>(), b.trim().parse::<f64>());
            match (a, b) {
                (Ok(a), Ok(b)) if b > 0.0 => a / b,
                _ => return Err(bad),
            }
        }
        None => s.parse::<f64>().map_err(|_| bad)?,
    };
    let t = (frac * f64::from(BAR)).round();
    if frac > 0.0 && frac < 1.0 && t >= 1.0 {
        Ok(t as u32)
    } else {
        Err("a time is between 0 and 1 bar, at least a tick")
    }
}

/// The method inside `every` or `off`: `rev`, `fast(2)`, `add(12)`.
fn nested(s: &str, srcs: &Sources) -> Result<Pattern, &'static str> {
    let s = s.trim();
    let (name, args) = match s.split_once('(') {
        Some((n, rest)) => (
            n.trim(),
            rest.strip_suffix(')').ok_or("a method's ( is not closed")?,
        ),
        None => (s, ""),
    };
    if !Pattern::NAMES.contains(&name) {
        return Err("a pattern method goes here, e.g. rev or fast(2)");
    }
    Pattern::parse_in(name, args, srcs)
}

/// The arguments between a method's brackets, split at commas outside
/// brackets; none for empty text.
fn split_args(s: &str) -> Vec<&str> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut depth, mut start, mut quoted) = (0i32, 0, false);
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 && !quoted => {
                out.push(s.get(start..i).unwrap_or(""));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s.get(start..).unwrap_or(""));
    out
}

#[cfg(test)]
mod tests {

    fn one_hit(len: u32) -> Line {
        Line {
            events: vec![ev(0, len, 36)],
            bars: 1,
        }
    }

    /// #716: chop cuts each hit into parts across its length, each playing
    /// the next part of its sample.
    #[test]
    fn chop_plays_each_hits_parts_in_turn() {
        let out = Pattern::parse("chop", "4")
            .expect("parses")
            .apply(&one_hit(48))
            .expect("chops");
        let got: Vec<_> = out
            .events
            .iter()
            .map(|e| (e.start, e.len, e.region.bounds()))
            .collect();
        let quarter = |k: u32| crate::notes::Region::WHOLE.part(k, 4).bounds();
        assert_eq!(
            got,
            [
                (0, 12, quarter(0)),
                (12, 12, quarter(1)),
                (24, 12, quarter(2)),
                (36, 12, quarter(3))
            ]
        );
        assert_eq!(
            Pattern::parse("chop", "4").map(|p| p.print()),
            Ok("chop(4)".to_string())
        );
        // Chop inside a slice takes parts of the part.
        let sliced = Pattern::parse("slice", "2, \"1\"")
            .expect("parses")
            .apply(&one_hit(48))
            .expect("slices");
        let twice = Pattern::Chop(2).apply(&sliced).expect("chops");
        let (lo, hi) = twice.events[1].region.bounds();
        assert!(
            (lo - 0.75).abs() < 1e-3 && (hi - 1.0).abs() < 1e-3,
            "{lo} {hi}"
        );
        // Parts are whole ticks: three ticks chopped in four is three parts,
        // which still play the whole of its part.
        let short = Pattern::Chop(4).apply(&one_hit(3)).expect("chops");
        assert_eq!(short.events.len(), 3);
        assert_eq!(short.events[2].region.bounds().1, 1.0);
        for bad in ["0", "65", "x"] {
            assert!(Pattern::parse("chop", bad).is_err(), "{bad}");
        }
    }

    /// #717: fit marks notes to fill their length; splice is a slice whose
    /// parts are fitted; a chop after fit keeps each part fitted, so the
    /// parts play on at one rate, as Strudel's `s("amen/4").fit().chop(16)`.
    #[test]
    fn fit_and_splice_fill_their_notes() {
        let fitted = Pattern::Fit.apply(&one_hit(48)).expect("fits");
        assert!(fitted.events.iter().all(|e| e.fit == 48));
        assert_eq!(
            Pattern::parse("fit", "").map(|p| p.print()),
            Ok("fit()".to_string())
        );
        assert!(Pattern::parse("fit", "2").is_err());
        let chopped = Pattern::Chop(16).apply(&fitted).expect("chops");
        assert!(chopped.events.iter().all(|e| e.fit == 3 && e.len == 3));
        let p = Pattern::parse("splice", "8, \"<0 1 2 3 4*2 5 6 [6 7]>*2\"").expect("parses");
        assert_eq!(p.print(), "splice(8, \"<0 1 2 3 4*2 5 6 [6 7]>*2\")");
        let spliced = p.apply(&one_hit(48)).expect("splices");
        let sliced = Pattern::parse("slice", "8, \"<0 1 2 3 4*2 5 6 [6 7]>*2\"")
            .expect("parses")
            .apply(&one_hit(48))
            .expect("slices");
        assert_eq!(spliced.events.len(), sliced.events.len());
        assert!(
            spliced
                .events
                .iter()
                .zip(&sliced.events)
                .all(|(a, b)| a.fit == a.len
                    && b.fit == 0
                    && (a.start, a.len, a.region) == (b.start, b.len, b.region))
        );
        // A part fitted then plied keeps its rate: each copy starts it again.
        let plied = Pattern::Ply(2).apply(&chopped).expect("plies");
        assert!(plied.events.iter().all(|e| e.fit == 3 && e.len < 3));
    }

    /// #718: speed multiplies each note's speed, so `mul(speed(-1))`, its
    /// Strudel spelling, reverses whatever plays; accelerate adds; coarse
    /// sets; a quoted value is one a step.
    #[test]
    fn speed_accelerate_and_coarse_play_on_the_notes() {
        let line = Pattern::Speed(200).apply(&one_hit(48)).expect("speeds");
        let line = Pattern::parse("mul", "speed(-1)")
            .expect("parses")
            .apply(&line)
            .expect("muls");
        assert_eq!(line.events[0].play.speed, -200);
        assert_eq!(
            Pattern::parse("mul", "speed(-1)").map(|p| p.print()),
            Ok("mul(speed(-1))".to_string())
        );
        assert_eq!(
            Pattern::parse("speed", "0.5").map(|p| p.print()),
            Ok("speed(0.5)".to_string())
        );
        assert!(Pattern::parse("mul", "add(1)").is_err());
        let swept = Pattern::Accelerate(100).apply(&line).expect("sweeps");
        assert_eq!(swept.events[0].play.accel, 100);
        let lofi = Pattern::Coarse(16).apply(&one_hit(48)).expect("coarse");
        assert_eq!(lofi.events[0].play.coarse, 16);
        for (name, bad) in [
            ("speed", "33"),
            ("coarse", "0"),
            ("coarse", "65"),
            ("accelerate", "x"),
        ] {
            assert!(Pattern::parse(name, bad).is_err(), "{name}({bad})");
        }
        // One a step: "1 2 -1" over three notes of a bar.
        let three = Line {
            events: vec![ev(0, 16, 36), ev(16, 16, 36), ev(32, 16, 36)],
            bars: 1,
        };
        let stepped = Pattern::parse("speed", "\"1 2 -1\"")
            .expect("parses")
            .apply(&three)
            .expect("steps");
        let speeds: Vec<i16> = stepped.events.iter().map(|e| e.play.speed).collect();
        assert_eq!(speeds, [100, 200, -100]);
        // Strudel's sometimesBy(.25, mul(speed("-1"))): about a quarter of
        // 400 hits backwards, the same every time.
        let many = Line {
            events: (0..400).map(|k| ev(k * 3, 3, 36)).collect(),
            bars: 25,
        };
        let p = Pattern::parse("sometimesBy", "0.25, mul(speed(-1))").expect("parses");
        let once = p.apply(&many).expect("draws");
        let back = once.events.iter().filter(|e| e.play.speed < 0).count();
        assert!((70..=130).contains(&back), "{back} of 400 backwards");
        assert_eq!(p.apply(&many), Ok(once));
    }

    /// #719: stut makes n copies of each note a time apart, each `gain` of
    /// the one before; echo is the same in Strudel's order.
    #[test]
    fn stut_and_echo_copy_each_note_softer() {
        let p = Pattern::parse("stut", "4, 0.25, 0.05").expect("parses");
        assert_eq!(p.print(), "stut(4, 0.25, 0.05)");
        let out = p.apply(&one_hit(48)).expect("copies");
        let got: Vec<_> = out.events.iter().map(|e| (e.start, e.vel)).collect();
        // 0.05 of a bar is two ticks of 48; a hit is 0.75 of full velocity.
        assert_eq!(got, [(0, 95), (2, 24), (4, 6), (6, 1)]);
        let echo = Pattern::parse("echo", "4, 0.05, 0.25").expect("parses");
        assert_eq!(echo.print(), "echo(4, 0.05, 0.25)");
        assert_eq!(echo.apply(&one_hit(48)), Ok(out));
        for bad in ["4, 2, 0.05", "4, 0.25", "0, 0.25, 0.05", "4, 0.25, 0.001"] {
            assert!(Pattern::parse("stut", bad).is_err(), "{bad}");
        }
    }

    /// #719: spread plays bar k through its k-th method, round and round;
    /// `id` leaves a bar as it is.
    #[test]
    fn spread_takes_a_method_a_bar() {
        let two = Line {
            events: vec![ev(0, 24, 36), ev(24, 24, 48)],
            bars: 1,
        };
        let p = Pattern::parse("spread", "rev, fast(2), chop(16)").expect("parses");
        assert_eq!(p.print(), "spread(rev, fast(2), chop(16))");
        let out = p.apply(&two).expect("spreads");
        assert_eq!(out.bars, 3);
        let in_bar = |b: u32| -> Vec<(u32, u8)> {
            out.events
                .iter()
                .filter(|e| e.start / 48 == b)
                .map(|e| (e.start % 48, e.note))
                .collect()
        };
        assert_eq!(in_bar(0), [(0, 48), (24, 36)], "bar 1 reversed");
        assert_eq!(
            in_bar(1),
            [(0, 36), (12, 48), (24, 36), (36, 48)],
            "bar 2 twice as fast"
        );
        assert_eq!(in_bar(2).len(), 32, "bar 3 chopped in 16 each");
        let id = Pattern::parse("spread", "id, rev").expect("parses");
        assert_eq!(id.print(), "spread(id, rev)");
        let out = id.apply(&two).expect("spreads");
        assert_eq!(
            out.events
                .iter()
                .take(2)
                .map(|e| e.note)
                .collect::<Vec<_>>(),
            [36, 48]
        );
        for bad in ["rev", "id, nope(1)"] {
            assert!(Pattern::parse("spread", bad).is_err(), "{bad}");
        }
    }

    /// #716: slice plays part k of n for each index, on the indexes' rhythm,
    /// as Strudel's: `slice(8, "<0 1 2 3 4*2 5 6 [6 7]>*2")` prints back.
    #[test]
    fn slice_plays_the_indexed_parts_on_their_rhythm() {
        let p = Pattern::parse("slice", "4, \"0 2 ~ 3\"").expect("parses");
        let out = p.apply(&one_hit(48)).expect("slices");
        let part = |k: u32| crate::notes::Region::WHOLE.part(k, 4).bounds();
        let got: Vec<_> = out
            .events
            .iter()
            .map(|e| (e.start, e.len, e.region.bounds()))
            .collect();
        assert_eq!(
            got,
            [(0, 12, part(0)), (12, 12, part(2)), (36, 12, part(3))]
        );
        // An index past n wraps round.
        let wrap = Pattern::parse("slice", "4, \"5\"")
            .expect("parses")
            .apply(&one_hit(48))
            .expect("slices");
        assert_eq!(wrap.events[0].region.bounds(), part(1));
        let recipe = "8, \"<0 1 2 3 4*2 5 6 [6 7]>*2\"";
        let p = Pattern::parse("slice", recipe).expect("parses");
        assert_eq!(p.print(), "slice(8, \"<0 1 2 3 4*2 5 6 [6 7]>*2\")");
        let out = p.apply(&one_hit(48)).expect("slices");
        // Bar 1 plays slices 0 and 1, bar 2 slices 2 and 3 (#721).
        let first: Vec<_> = out.events.iter().take(4).map(|e| e.region).collect();
        let want: Vec<_> = (0..4)
            .map(|k| crate::notes::Region::WHOLE.part(k, 8))
            .collect();
        assert_eq!(first, want);
        for bad in ["4", "4, 2", "0, \"1\"", "4, \"c4\""] {
            assert!(Pattern::parse("slice", bad).is_err(), "{bad}");
        }
    }

    use super::*;

    fn ev(start: u32, len: u32, note: u8) -> Event {
        Event {
            start,
            len,
            note,
            accent: false,
            vel: 0,
            word: 0,
            degree: false,
            when: Default::default(),
            nudge: 0,
            cents: 0,
            pan: 0,
            region: crate::notes::Region::WHOLE,
            fit: 0,
            play: crate::notes::Play::SAME,
        }
    }

    /// c d e f as quarters in one bar.
    fn four() -> Line {
        Line {
            events: (0..4).map(|k| ev(k * 12, 12, 60 + 2 * k as u8)).collect(),
            bars: 1,
        }
    }

    fn starts(l: &Line) -> Vec<(u32, u8)> {
        l.events.iter().map(|e| (e.start, e.note)).collect()
    }

    fn run(p: &str, l: &Line) -> Line {
        let (name, args) = p.split_once('(').expect("call");
        let p = Pattern::parse(name, args.strip_suffix(')').expect("closed")).expect(name);
        p.apply(l).expect("applies")
    }

    #[test]
    fn fast_and_slow_squeeze_and_stretch() {
        let f = run("fast(2)", &four());
        assert_eq!(f.bars, 1);
        assert_eq!(
            starts(&f),
            vec![
                (0, 60),
                (6, 62),
                (12, 64),
                (18, 66),
                (24, 60),
                (30, 62),
                (36, 64),
                (42, 66)
            ]
        );
        assert!(f.events.iter().all(|e| e.len == 6));
        let s = run("slow(2)", &four());
        assert_eq!((s.bars, s.events[1].start, s.events[1].len), (2, 24, 24));
        // A two-bar line made three times faster fills two bars again.
        let two = four().repeat(2);
        assert_eq!(run("fast(3)", &two).bars, 2);
    }

    /// #676: values side by side, or in `[ ]`, split the bar; each part's
    /// method moves the notes that start in it, nesting with `< >`.
    #[test]
    fn values_in_a_bar_split_it() {
        let plain: Vec<u8> = four().events.iter().map(|e| e.note).collect();
        let add = |text: &str| -> Vec<(u32, u8)> { starts(&run(text, &four())) };
        let shift = |by: [u8; 4], bar: u32| -> Vec<(u32, u8)> {
            (0..4)
                .map(|k| (bar * BAR + k as u32 * 12, plain[k] + by[k]))
                .collect()
        };
        assert_eq!(add("add(\"0 3\")"), shift([0, 0, 3, 3], 0));
        assert_eq!(add("add(\"[0 3]\")"), add("add(\"0 3\")"));
        let nested = run("add(\"<0 [3 5]>\")", &four());
        assert_eq!(nested.bars, 2);
        assert_eq!(starts(&nested)[4..], shift([3, 3, 5, 5], 1)[..]);
        // A method that moves time moves the whole line; each half keeps
        // the notes that land in it: four quarters, then eighths.
        let f = run("fast(\"1 2\")", &four());
        let at: Vec<u32> = f.events.iter().map(|e| e.start).collect();
        assert_eq!(at, [0, 12, 24, 30, 36, 42]);
        // `ply` doubles only the second half's notes.
        assert_eq!(run("ply(\"1 2\")", &four()).events.len(), 6);
        // Five parts: each from the first tick at or past its share, as
        // Strudel's boundaries at 9.6, 19.2, … fall between ticks.
        let sixteenths = Line {
            events: (0..16).map(|k| ev(k * 3, 3, 60)).collect(),
            bars: 1,
        };
        let by: Vec<u8> = run("add(\"0 1 2 3 4\")", &sixteenths)
            .events
            .iter()
            .map(|e| e.note - 60)
            .collect();
        assert_eq!(by, [0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4]);
        // A live clip takes its part by the song's tick.
        let p = Pattern::parse("add", "\"0 3\"").expect("parses");
        let mut e = ev(0, 1, 60);
        assert!(p.pitch(&mut e, 23));
        assert_eq!(e.note, 60);
        assert!(p.pitch(&mut e, 24));
        assert_eq!(e.note, 63);
    }

    /// #629: a value a bar is the method with each value, bar by bar.
    #[test]
    fn a_value_a_bar_takes_turns() {
        let a = run("add(\"<0 3 5>\")", &four());
        let mut hand = four().repeat(3);
        for e in &mut hand.events {
            e.note += [0, 3, 5][(e.start / BAR) as usize];
        }
        assert_eq!((a.bars, starts(&a)), (3, starts(&hand)));
        // Nested turns, as in Strudel: 0 1 0 -1.
        let n = run("sub(\"<0 <-1 1>>\")", &four());
        let firsts: Vec<u8> = (0..4).map(|b| n.events[b * 4].note).collect();
        assert_eq!((n.bars, firsts), (4, vec![60, 61, 60, 59]));
        // fast alternates: four notes, then eight.
        let f = run("fast(\"<1 2>\")", &four());
        let count = |b: u32| f.events.iter().filter(|e| e.start / BAR == b).count();
        assert_eq!((f.bars, count(0), count(1)), (2, 4, 8));
        // slow 2 on bar one plays the second half of the stretched line.
        let s = run("slow(\"<1 2>\")", &four());
        assert_eq!(starts(&s)[4..], [(48, 64), (72, 66)]);
        // A plain value in quotes is every bar's.
        assert_eq!(
            starts(&run("ply(\"2\")", &four())),
            starts(&run("ply(2)", &four()))
        );
        assert_eq!(run("degrade(\"<0 1>\")", &four()).events.len(), 4);
    }

    #[test]
    fn a_value_a_bar_prints_as_written_and_checks_its_values() {
        for text in [
            "\"<0 3 5>\"",
            "\"<0 <1 -1>>\"",
            "\"7\"",
            "\"1 2\"",
            "\"<0 [3 5]>\"",
        ] {
            let p = Pattern::parse("add", text).expect(text);
            assert_eq!(p.print(), format!("add({text})"));
            assert!(p.pitch_only());
        }
        assert!(
            !Pattern::parse("fast", "\"<1 2>\"")
                .expect("fast")
                .pitch_only()
        );
        for (name, text, msg) in [
            ("add", "\"<1 2\"", BAD_PER_BAR),
            ("add", "\"<>\"", BAD_PER_BAR),
            ("add", "\"[1 2\"", BAD_PER_BAR),
            ("add", "\"1 ]\"", BAD_PER_BAR),
            ("add", "\"<0 99>\"", "this takes semitones from -48 to 48"),
            (
                "fast",
                "\"<1 0>\"",
                "this takes a ratio, each side 1 to 16: 2, 3/4 or 1.5",
            ),
        ] {
            assert_eq!(Pattern::parse(name, text), Err(msg), "{name}({text})");
        }
        // At most 64 values, nested or not, and 32 deep (#684 review).
        let many = "at most 64 values";
        let long = format!("\"<{}>\"", vec!["1"; 65].join(" "));
        assert_eq!(
            Pattern::parse("add", &long).map_err(|e| e.contains(many)),
            Err(true)
        );
        let spread = format!("\"{}\"", vec!["[1 1 1 1 1]"; 13].join(" "));
        assert_eq!(
            Pattern::parse("add", &spread).map_err(|e| e.contains(many)),
            Err(true)
        );
        let deep = format!("\"{}1{}\"", "<".repeat(5000), ">".repeat(5000));
        assert_eq!(
            Pattern::parse("add", &deep),
            Err("a value a bar nests at most 32 deep")
        );
        let open = format!("\"{}\"", "[".repeat(100_000));
        assert!(
            Pattern::parse("add", &open).is_err(),
            "unclosed, and quickly"
        );
        // Turns of 7 and 13: 182 bars before it comes round.
        let nest = "\"<<1 2 3 4 5 6 7> <1 2 3 4 5 6 7 8 9 10 11 12 13>>\"";
        assert_eq!(Pattern::parse("add", nest), Err(TOO_LONG));
        // Within 64 bars, but sixteen times 48 notes on bar two.
        let wide = Line {
            events: (0..48).map(|k| ev(k, 1, 60)).collect(),
            bars: 1,
        };
        let p = Pattern::parse("ply", "\"<1 16>\"").expect("ply");
        assert_eq!(p.apply(&wide), Err("the pattern makes more than 512 notes"));
    }

    /// On a live clip, the bar of the song picks the value (#593).
    #[test]
    fn a_value_a_bar_on_a_live_bar_follows_the_song() {
        let p = [Pattern::parse("add", "\"<0 12>\"").expect("add")];
        for (base, want) in [(0, 60), (u64::from(BAR), 72), (2 * u64::from(BAR), 60)] {
            let mut events = vec![ev(0, 12, 60)];
            pitch_all(&p, &mut events, base);
            assert_eq!(events[0].note, want);
        }
    }

    #[test]
    fn rev_and_palindrome_turn_bars_round() {
        assert_eq!(
            starts(&run("rev()", &four())),
            vec![(0, 66), (12, 64), (24, 62), (36, 60)]
        );
        let p = run("palindrome()", &four());
        assert_eq!(p.bars, 2);
        assert_eq!(
            starts(&p),
            vec![
                (0, 60),
                (12, 62),
                (24, 64),
                (36, 66),
                (48, 66),
                (60, 64),
                (72, 62),
                (84, 60)
            ]
        );
    }

    #[test]
    fn add_sub_and_ply_change_notes() {
        assert_eq!(run("add(12)", &four()).events[0].note, 72);
        assert_eq!(run("sub(1)", &four()).events[0].note, 59);
        // Out of MIDI's range, a note is dropped.
        assert_eq!(run("add(48)", &run("add(48)", &four())).events.len(), 0);
        let p = run("ply(3)", &four());
        assert_eq!(p.events.len(), 12);
        assert_eq!(
            p.events[..3]
                .iter()
                .map(|e| (e.start, e.len))
                .collect::<Vec<_>>(),
            vec![(0, 4), (4, 4), (8, 4)]
        );
    }

    #[test]
    fn iter_starts_each_bar_further_in() {
        let it = run("iter(4)", &four());
        assert_eq!(it.bars, 4);
        let first = |b: u32| {
            it.events
                .iter()
                .find(|e| e.start == b * BAR)
                .map(|e| e.note)
        };
        assert_eq!(
            (first(0), first(1), first(2), first(3)),
            (Some(60), Some(62), Some(64), Some(66))
        );
        assert_eq!(it.events.len(), 16);
    }

    #[test]
    fn every_and_off_layer_a_method() {
        let e = run("every(2, rev)", &four());
        assert_eq!(e.bars, 2);
        assert_eq!(e.events[0].note, 66, "bar 0 reversed");
        assert_eq!(e.events[4].note, 60, "bar 1 as written");
        let e = run("every(3, fast(2))", &four());
        assert_eq!((e.bars, e.events.len()), (3, 16));
        let o = run("off(1/8, add(12))", &four());
        assert_eq!(o.events.len(), 8);
        assert_eq!(starts(&o)[..2], [(0, 60), (6, 72)]);
        // The copy of the last note wraps round to the bar's start.
        let late = run("off(3/4, add(7))", &four());
        assert!(late.events.iter().any(|e| (e.start, e.note) == (0, 69)));
    }

    /// A chord a bar long, and a single note in the second bar.
    fn chord_then_note() -> Line {
        Line {
            events: vec![
                ev(0, 48, 64),
                ev(0, 48, 71),
                ev(0, 48, 60),
                ev(0, 48, 67),
                ev(48, 48, 62),
            ],
            bars: 2,
        }
    }

    fn nudges(l: &Line) -> Vec<(u32, u8, i16)> {
        l.events
            .iter()
            .map(|e| (e.start, e.note, e.nudge))
            .collect()
    }

    #[test]
    fn strum_spreads_a_chord_up_or_down() {
        assert_eq!(
            nudges(&run("strum(15)", &chord_then_note())),
            [
                (0, 60, 0),
                (0, 64, 15),
                (0, 67, 30),
                (0, 71, 45),
                (48, 62, 0)
            ]
        );
        assert_eq!(
            nudges(&run("strum(-15)", &chord_then_note())),
            [
                (0, 60, 45),
                (0, 64, 30),
                (0, 67, 15),
                (0, 71, 0),
                (48, 62, 0)
            ]
        );
        let lens: Vec<u32> = run("strum(15)", &chord_then_note())
            .events
            .iter()
            .map(|e| e.len)
            .collect();
        assert_eq!(lens, [48; 5], "the ends stay");
        // Inside every, off and sometimes it strums the bars they change.
        let mut chord = chord_then_note();
        chord.events.pop();
        chord.bars = 1;
        let e = run("every(2, strum(10))", &chord);
        assert_eq!(e.bars, 2);
        let top: Vec<i16> = e
            .events
            .iter()
            .filter(|e| e.note == 71)
            .map(|e| e.nudge)
            .collect();
        assert_eq!(top, [30, 0]);
        let o = run("off(1/2, strum(10))", &chord_then_note());
        assert!(
            o.events
                .iter()
                .any(|e| (e.start, e.note, e.nudge) == (24, 71, 30))
        );
        let s = run("sometimes(strum(10))", &chord_then_note());
        assert!(s.events.iter().all(|e| e.nudge % 10 == 0));
    }

    #[test]
    fn struct_plays_the_notes_sounding_at_each_hit() {
        // c d e f in quarters, on a rhythm of eighths: hits on 1, 1+, 3.
        let s = run("struct(\"x x ~ ~ x ~ ~ ~\")", &four());
        assert_eq!(starts(&s), vec![(0, 60), (6, 60), (24, 64)]);
        assert!(s.events.iter().all(|e| e.len == 6));
        // A rest under a hit plays nothing.
        let one = Line {
            events: vec![ev(0, 12, 60)],
            bars: 1,
        };
        assert_eq!(starts(&run("struct(\"x x x x\")", &one)), vec![(0, 60)]);
    }

    #[test]
    fn euclid_is_a_struct_of_spread_hits() {
        // Strudel's note("c2 a2 eb2").euclid(5,8): c2 . c2 a2 . a2 eb2 .
        let line = Line {
            events: vec![ev(0, 16, 36), ev(16, 16, 45), ev(32, 16, 39)],
            bars: 1,
        };
        let e = run("euclid(5,8)", &line);
        assert_eq!(e, run("struct(\"x ~ x x ~ x x ~\")", &line));
        assert_eq!(
            starts(&e),
            vec![(0, 36), (12, 36), (18, 45), (30, 45), (36, 39)]
        );
        // Rotated left by two: x ~ ~ x ~ ~ x ~ becomes ~ x ~ ~ x ~ x ~.
        assert_eq!(
            run("euclid(3,8,2)", &four()),
            run("struct(\"~ x ~ ~ x ~ x ~\")", &four())
        );
        // As the method of every, off and sometimes.
        let ev2 = run("every(2, euclid(3,8))", &four());
        assert_eq!((ev2.bars, ev2.events.len()), (2, 3 + 4));
    }

    #[test]
    fn sometimes_takes_about_half_from_the_method() {
        let line = run("ply(8)", &four());
        let s = run("sometimes(add(12))", &line);
        assert_eq!(s.events.len(), line.events.len());
        let up = s.events.iter().filter(|e| e.note >= 72).count();
        assert!(up > 8 && up < 24, "{up} of 32");
        assert_eq!(s, run("sometimes(add(12))", &line), "seeded");
    }

    /// #701: jux puts the line on the left and the changed copy on the right.
    #[test]
    fn jux_pans_the_line_left_and_its_copy_right() {
        let j = run("jux(rev)", &four());
        assert_eq!(j.bars, 1);
        let side = |pan: i8| -> Vec<(u32, u8)> {
            let mut v: Vec<_> = j
                .events
                .iter()
                .filter(|e| e.pan == pan)
                .map(|e| (e.start, e.note))
                .collect();
            v.sort_unstable();
            v
        };
        assert_eq!(side(-100), starts(&four()));
        assert_eq!(side(100), starts(&run("rev()", &four())));
        assert_eq!(j.events.len(), 8);
        // The two come round together.
        assert_eq!(run("jux(slow(2))", &four()).bars, 2);
        assert_eq!(Pattern::parse("jux", " rev ").unwrap().print(), "jux(rev)");
        assert_eq!(
            Pattern::parse("jux", "add(7)").unwrap().print(),
            "jux(add(7))"
        );
        assert!(Pattern::parse("jux", "").is_err());
        assert!(Pattern::parse("jux", "room(1)").is_err());
    }

    /// #699: a fraction of a semitone moves a note's cents, its note the
    /// nearest key; whole semitones leave the cents alone.
    #[test]
    fn add_takes_a_fraction_of_a_semitone() {
        let pitch = |e: &Event| (e.note, e.cents);
        let a = run("add(0.5)", &four());
        assert_eq!(pitch(&a.events[0]), (61, -50));
        let b = run("add(0.25)", &a);
        assert_eq!(pitch(&b.events[0]), (61, -25));
        assert_eq!(pitch(&run("sub(0.1)", &four()).events[0]), (60, -10));
        assert_eq!(pitch(&run("add(12)", &b).events[0]), (73, -25));
        assert_eq!(pitch(&run("oct(-1)", &b).events[0]), (49, -25));
        let s = run("add([0,0.1])", &four());
        let firsts: Vec<_> = s
            .events
            .iter()
            .filter(|e| e.start == 0)
            .map(pitch)
            .collect();
        assert_eq!(firsts, [(60, 0), (60, 10)]);
        // A live clip's bar moves the same way.
        let mut e = four().events[0];
        assert!(Pattern::parse("add", "1.5").unwrap().pitch(&mut e, 0));
        assert_eq!(pitch(&e), (62, -50));
        for (name, args, printed) in [
            ("add", "0.5", "add(0.5)"),
            ("sub", " 0.25 ", "sub(0.25)"),
            ("add", "-1.5", "add(-1.5)"),
            ("add", "[0, 0.1]", "add([0,0.1])"),
            ("add", "\"<0 0.5>\"", "add(\"<0 0.5>\")"),
        ] {
            assert_eq!(Pattern::parse(name, args).unwrap().print(), printed);
        }
        for bad in ["0.123", "48.5", "x1"] {
            assert!(Pattern::parse("add", bad).is_err(), "add({bad})");
        }
    }

    /// #702: a ratio squeezes or stretches, the line round in whole bars.
    #[test]
    fn fast_and_slow_take_a_ratio() {
        let f = run("fast(3/4)", &four());
        assert_eq!(f.bars, 4);
        let notes: Vec<(u32, u8)> = starts(&f);
        assert_eq!(notes.len(), 12);
        assert_eq!(
            &notes[..5],
            &[(0, 60), (16, 62), (32, 64), (48, 66), (64, 60)]
        );
        assert!(f.events.iter().all(|e| e.len == 16));
        assert_eq!(run("fast(1.5)", &four()), run("fast(3/2)", &four()));
        assert_eq!(run("fast(0.5)", &four()), run("slow(2)", &four()));
        assert_eq!(run("slow(1/2)", &four()), run("fast(2)", &four()));
        assert_eq!(run("slow(4/3)", &four()), run("fast(3/4)", &four()));
        // Whole factors as before.
        assert_eq!(run("fast(6/3)", &four()), run("fast(2)", &four()));
        for (args, printed) in [
            ("3/4", "fast(3/4)"),
            ("0.75", "fast(3/4)"),
            ("4/2", "fast(2)"),
        ] {
            assert_eq!(Pattern::parse("fast", args).unwrap().print(), printed);
        }
        for bad in ["0", "17", "1/17", "17/16", "0.123", "x", "3/", "-1"] {
            assert!(Pattern::parse("fast", bad).is_err(), "fast({bad})");
        }
        // Too long to come round.
        let long = Line {
            events: vec![ev(0, 1, 60)],
            bars: 16,
        };
        assert!(
            Pattern::parse("fast", "15/16")
                .unwrap()
                .apply(&long)
                .is_err()
        );
        // A ratio a bar.
        let p = run("fast(\"<1 3/2>\")", &four());
        assert!(p.bars >= 2);
    }

    /// #700: Strudel's chances, seeded, a chord going one way.
    #[test]
    fn the_chances_apply_their_method_about_that_often() {
        let line = run("ply(16)", &four()).repeat(4);
        let up = |p: &str| {
            let s = run(p, &line);
            assert_eq!(s.events.len(), line.events.len(), "{p}");
            assert_eq!(s, run(p, &line), "{p} is seeded");
            s.events.iter().filter(|e| e.note >= 72).count()
        };
        let n = line.events.len() as f32;
        for (p, want) in [
            ("often(add(12))", 0.75),
            ("rarely(add(12))", 0.25),
            ("almostNever(add(12))", 0.1),
            ("almostAlways(add(12))", 0.9),
            ("sometimesBy(0.3, add(12))", 0.3),
        ] {
            let got = up(p) as f32 / n;
            assert!((got - want).abs() < 0.06, "{p}: {got}");
        }
        assert_eq!(up("sometimesBy(0, add(12))"), 0);
        assert_eq!(up("sometimesBy(1, add(12))"), line.events.len());
        // A half is the coin sometimes has always thrown.
        assert_eq!(
            run("sometimesBy(0.5, add(12))", &line),
            run("sometimes(add(12))", &line)
        );
        // The moments a chance takes are among those a likelier one takes.
        let taken = |p: &str| -> Vec<u32> {
            let s = run(p, &line);
            s.events
                .iter()
                .filter(|e| e.note >= 72)
                .map(|e| e.start)
                .collect()
        };
        let often = taken("often(add(12))");
        assert!(taken("rarely(add(12))").iter().all(|t| often.contains(t)));
        for bad in [
            "sometimesBy(2, rev)",
            "sometimesBy(0.5)",
            "often(2)",
            "rarely()",
        ] {
            let (name, args) = bad.split_once('(').unwrap();
            assert!(
                Pattern::parse(name, args.strip_suffix(')').unwrap()).is_err(),
                "{bad}"
            );
        }
    }

    /// #700: `<a b>/n` in a value a bar holds each n bars.
    #[test]
    fn a_value_a_bar_over_n_holds_each_n_bars() {
        let a = run("add(\"<0 12>/2\")", &four());
        let firsts: Vec<u8> = (0..4).map(|b| a.events[b * 4].note).collect();
        assert_eq!((a.bars, firsts), (4, vec![60, 60, 72, 72]));
        let p = Pattern::parse("add", "\"<0 12>/2\"").unwrap();
        assert_eq!(p.print(), "add(\"<0 12>/2\")");
        for bad in ["\"<0 12>/0\"", "\"<0 12>/17\"", "\"<0 12>/x\""] {
            assert!(Pattern::parse("add", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn scale_moves_notes_to_the_nearest_scale_note() {
        let chromatic = Line {
            events: (0..12).map(|k| ev(k * 4, 4, 60 + k as u8)).collect(),
            bars: 1,
        };
        let notes: Vec<u8> = run("scale(c minor)", &chromatic)
            .events
            .iter()
            .map(|e| e.note)
            .collect();
        // c minor: c d eb f g ab bb; a tie (e, a, b between two) goes down.
        assert_eq!(notes, vec![60, 60, 62, 63, 63, 65, 65, 67, 68, 68, 70, 70]);
    }

    /// #571: each note takes the key of the bar it starts in, and the line
    /// runs until it and the progression come round together.
    #[test]
    fn a_scale_progression_changes_key_by_the_bar() {
        // degree 0 and 1 in each of three bars
        let line = Line {
            events: (0..6u8).map(|k| deg(u32::from(k) * 24, k % 2)).collect(),
            bars: 3,
        };
        let out = run("scale(<a1 d2>/2 minor-pentatonic)", &line);
        assert_eq!(out.bars, 12, "3 bars against 4");
        assert!(out.events.iter().all(|e| !e.degree));
        let notes: Vec<u8> = out.events.iter().map(|e| e.note).collect();
        // a1 c2 | a1 c2 | d2 f2 | d2 f2 | a1 c2 | ...
        assert_eq!(notes.get(..8), Some(&[33, 36, 33, 36, 38, 41, 38, 41][..]));
        // a note is snapped to the key of its bar
        let line = Line {
            events: vec![ev(0, 4, 59), ev(BAR, 4, 59)],
            bars: 2,
        };
        let out = run("scale(<c4:major d4:minor>)", &line);
        let notes: Vec<u8> = out.events.iter().map(|e| e.note).collect();
        // b3 is in c major; in d minor it ties between bb3 and c4 and goes down
        assert_eq!(notes, [59, 58]);
        // too long to come round
        let long = Line {
            events: vec![deg(0, 0)],
            bars: 7,
        };
        let p = Pattern::parse("scale", "<a1 d1 e1>/4 minor").expect("parses");
        assert_eq!(p.apply(&long), Err("the pattern runs past 64 bars"));
    }

    fn deg(start: u32, d: u8) -> Event {
        Event {
            note: DEGREE_ZERO + d,
            degree: true,
            ..ev(start, 4, 0)
        }
    }

    #[test]
    fn add_of_a_list_stacks_each_note() {
        // Degrees before scale: "0 3" is [0,2,4] [3,5,7] (#650).
        let line = Line {
            events: vec![deg(0, 0), deg(12, 3)],
            bars: 1,
        };
        let degrees = |l: &Line| -> Vec<(u32, u8)> {
            l.events
                .iter()
                .map(|e| (e.start, e.note - DEGREE_ZERO))
                .collect()
        };
        assert_eq!(
            degrees(&run("add([0,2,4])", &line)),
            [(0, 0), (0, 2), (0, 4), (12, 3), (12, 5), (12, 7)]
        );
        // Semitones after scale; a note past 127 is dropped, not wrapped.
        let high = Line {
            events: vec![ev(0, 12, 60), ev(12, 12, 120)],
            bars: 1,
        };
        assert_eq!(
            starts(&run("add([0, 4, 7])", &high)),
            [(0, 60), (0, 64), (0, 67), (12, 120), (12, 124), (12, 127)]
        );
        assert_eq!(starts(&run("add([-12])", &high)), [(0, 48), (12, 108)]);
        // With every and off.
        let e = run("every(2, add([0,7]))", &four());
        assert_eq!((e.bars, e.events.len()), (2, 12));
        let o = run("off(1/8, add([12,19]))", &four());
        assert_eq!(starts(&o)[..3], [(0, 60), (6, 72), (6, 79)]);
        // More than MAX_EVENTS notes is an error.
        let many = run("ply(16)", &run("ply(4)", &four()));
        let p = Pattern::parse("add", "[0,4,7]").expect("parses");
        assert_eq!(p.apply(&many), Err("the pattern makes more than 512 notes"));
    }

    /// c d e f g a b c' as eighths in one bar.
    fn eight() -> Line {
        Line {
            events: (0..8).map(|k| ev(k * 6, 6, 60 + k as u8)).collect(),
            bars: 1,
        }
    }

    fn bar_notes(l: &Line, b: u32) -> Vec<u8> {
        l.events
            .iter()
            .filter(|e| e.start / BAR == b)
            .map(|e| e.note)
            .collect()
    }

    /// #615: each bar's parts once each, in an order that changes.
    #[test]
    fn shuffle_plays_every_part_once_a_bar() {
        let s = run("shuffle(4)", &eight());
        assert_eq!(s.bars, 8, "repeats after 8 bars, as ? does");
        assert_eq!(s, run("shuffle(4)", &eight()), "the same every run");
        let mut orders = Vec::new();
        for b in 0..8 {
            let notes = bar_notes(&s, b);
            let mut sorted = notes.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (60..68).collect::<Vec<u8>>(), "bar {b}");
            // A part's two eighths stay together, in order.
            for pair in notes.chunks(2) {
                assert_eq!((pair[0] - 60) % 2, 0, "bar {b}: {notes:?}");
                assert_eq!(pair[1], pair[0] + 1, "bar {b}: {notes:?}");
            }
            orders.push(notes);
        }
        orders.dedup();
        assert!(orders.len() > 3, "{orders:?}");
        assert_eq!(run("shuffle(1)", &eight()), eight());
    }

    #[test]
    fn scramble_draws_parts_with_repeats() {
        let s = run("scramble(4)", &eight());
        assert_eq!((s.bars, s.events.len()), (8, 64));
        assert_eq!(s, run("scramble(4)", &eight()));
        let repeats = (0..8).filter(|&b| {
            let mut notes = bar_notes(&s, b);
            notes.sort_unstable();
            notes.dedup();
            notes.len() < 8
        });
        assert!(repeats.count() > 0, "some part twice in some bar");
        assert_ne!(run("scramble(4)", &eight()), run("shuffle(4)", &eight()));
        assert_eq!(run("scramble(1)", &eight()), eight());
    }

    #[test]
    fn a_reordered_note_is_cut_at_its_part_and_works_inside_others() {
        let whole = Line {
            events: vec![ev(0, 48, 60)],
            bars: 1,
        };
        let s = run("shuffle(2)", &whole);
        assert!(
            s.events.iter().all(|e| e.len == 24 && e.start % 24 == 0),
            "{s:?}"
        );
        let e = run("every(2, shuffle(4))", &eight());
        assert_eq!(e.bars, 8);
        assert_eq!(
            bar_notes(&e, 1),
            (60..68).collect::<Vec<u8>>(),
            "odd bars as written"
        );
        assert_eq!(run("off(1/8, scramble(2))", &eight()).events.len(), 128);
        assert_eq!(run("sometimes(shuffle(2))", &eight()).bars, 8);
        // A value a bar (#629).
        assert_eq!(run("shuffle(\"<1 4>\")", &eight()).bars, 8);
    }

    #[test]
    fn degrade_is_seeded() {
        let line = run("ply(16)", &four());
        let a = run("degrade(0.5)", &line);
        assert_eq!(a, run("degrade(0.5)", &line));
        assert!(
            a.events.len() > 16 && a.events.len() < 48,
            "{}",
            a.events.len()
        );
        assert_eq!(run("degrade(0)", &line).events.len(), 64);
        assert_eq!(run("degrade(1)", &line).events.len(), 0);
    }

    #[test]
    fn methods_print_as_they_parse() {
        for (name, args, printed) in [
            ("fast", "2", "fast(2)"),
            ("rev", "", "rev()"),
            ("degrade", "", "degrade(0.5)"),
            ("every", "4, rev()", "every(4, rev)"),
            ("every", " 4 ,fast( 2 ) ", "every(4, fast(2))"),
            ("off", "0.125, add(12)", "off(1/8, add(12))"),
            ("off", "3/16,palindrome", "off(3/16, palindrome)"),
            ("sub", "-3", "sub(-3)"),
            ("shuffle", " 4", "shuffle(4)"),
            ("every", "2, scramble(3)", "every(2, scramble(3))"),
            ("add", " [0, 2 ,4] ", "add([0,2,4])"),
            ("every", "2, add([0,-12])", "every(2, add([0,-12]))"),
            ("strum", " 15 ", "strum(15)"),
            ("every", "2, strum(-15)", "every(2, strum(-15))"),
            ("struct", "\"x ~  x\"", "struct(\"x ~ x\")"),
            ("sometimes", "fast(2)", "sometimes(fast(2))"),
            ("often", "rev", "often(rev)"),
            ("almostNever", " ply(2) ", "almostNever(ply(2))"),
            ("sometimesBy", "0.25, add(12)", "sometimesBy(0.25, add(12))"),
            ("euclid", "5,8", "euclid(5,8)"),
            ("euclid", " 3, 8 ,2", "euclid(3,8,2)"),
            ("euclid", "3,8,0", "euclid(3,8)"),
            ("every", "2, euclid(3,8)", "every(2, euclid(3,8))"),
            ("oct", " -1", "oct(-1)"),
            ("scale", " eb  minor ", "scale(d# minor)"),
            ("scale", "a1 minor-pentatonic", "scale(a1 minor-pentatonic)"),
            ("scale", "<a1  d2>/4 minor", "scale(<a1 d2>/4 minor)"),
            (
                "scale",
                "<a1:minor d1:dorian>",
                "scale(<a1:minor d1:dorian>)",
            ),
            (
                "scale",
                "<a1 d1:dorian> minor",
                "scale(<a1 d1:dorian> minor)",
            ),
            ("scale", "<a1>/4 minor", "scale(a1 minor)"),
        ] {
            let p = Pattern::parse(name, args).expect(name);
            assert_eq!(p.print(), printed);
            let (n, a) = printed.split_once('(').expect("call");
            assert_eq!(
                Pattern::parse(n, a.strip_suffix(')').expect("closed")),
                Ok(p)
            );
        }
    }

    #[test]
    fn bad_methods_say_why() {
        for (name, args, msg) in [
            (
                "fast",
                "0",
                "this takes a ratio, each side 1 to 16: 2, 3/4 or 1.5",
            ),
            ("shuffle", "0", "this takes a whole number from 1 to 16"),
            ("scramble", "17", "this takes a whole number from 1 to 16"),
            ("ply", "1.5", "this takes a whole number from 1 to 16"),
            ("fast", "", "this takes one value"),
            ("rev", "2", "this takes no value: ()"),
            (
                "strum",
                "0",
                "this takes milliseconds from 1 to 100, below 0 to strum down",
            ),
            (
                "strum",
                "101",
                "this takes milliseconds from 1 to 100, below 0 to strum down",
            ),
            (
                "strum",
                "-101",
                "this takes milliseconds from 1 to 100, below 0 to strum down",
            ),
            (
                "strum",
                "1.5",
                "this takes milliseconds from 1 to 100, below 0 to strum down",
            ),
            ("add", "99", "this takes semitones from -48 to 48"),
            ("add", "[0,99]", "this takes semitones from -48 to 48"),
            ("add", "[]", "this takes semitones from -48 to 48"),
            ("sub", "[0,2]", "this takes semitones from -48 to 48"),
            ("degrade", "2", "this takes a chance from 0 to 1"),
            ("every", "4", crate::usage::EVERY),
            (
                "every",
                "4, cutoff(3)",
                "a pattern method goes here, e.g. rev or fast(2)",
            ),
            (
                "off",
                "1, rev",
                "a time is between 0 and 1 bar, at least a tick",
            ),
            ("off", "x, rev", "a time is a fraction of a bar, e.g. 1/8"),
            ("struct", "x ~ x", crate::usage::STRUCT),
            ("struct", "\"x o\"", crate::usage::STRUCT),
            ("struct", "\"\"", "a rhythm has 1 to 48 steps"),
            (
                "euclid",
                "3",
                "euclid takes hits, steps and maybe a rotation: euclid(3,8) or euclid(3,8,2)",
            ),
            (
                "euclid",
                "3,8,1,1",
                "euclid takes hits, steps and maybe a rotation: euclid(3,8) or euclid(3,8,2)",
            ),
            ("euclid", "3,0", "euclid has 1 to 64 steps"),
            ("euclid", "9,8", "euclid cannot have more hits than steps"),
            ("euclid", "3,64", "a rhythm has 1 to 48 steps"),
            (
                "sometimes",
                "cutoff",
                "a pattern method goes here, e.g. rev or fast(2)",
            ),
            ("scale", "c", crate::usage::SCALE),
            (
                "scale",
                "h minor",
                "a root is a letter a to g, maybe # or b, maybe an octave, or an earlier clip",
            ),
            ("oct", "5", "this takes octaves from -4 to 4"),
            ("oct", "", "this takes one value"),
            ("scale", "c weird", "no scale has this mode"),
            ("scale", "<a1 d1>", crate::usage::SCALE),
            ("scale", "<a1:minor d1>", crate::usage::SCALE),
            ("scale", "<> minor", crate::usage::SCALE),
            ("scale", "<a1 d1 minor", "a < of roots is not closed"),
            (
                "scale",
                "<a1 d1>/0 minor",
                "a root is held 1 to 16 bars, as <a1 d2>/4",
            ),
            ("scale", "<a1:weird> minor", "no scale has this mode"),
            ("scale", "a1 minor major", crate::usage::SCALE),
        ] {
            assert_eq!(Pattern::parse(name, args), Err(msg), "{name}({args})");
        }
        let long = Line {
            events: vec![ev(0, 1, 60)],
            bars: 16,
        };
        assert_eq!(
            Pattern::Slow(Ratio::whole(8)).apply(&long),
            Err("the pattern runs past 64 bars")
        );
        let many = run("ply(8)", &run("ply(16)", &four()));
        assert_eq!(many.events.len(), 512);
        assert_eq!(
            Pattern::Ply(2).apply(&many),
            Err("the pattern makes more than 512 notes")
        );
    }
}
