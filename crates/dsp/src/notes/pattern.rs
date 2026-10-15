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
use super::{DEGREE_ZERO, Event, MAX_BARS, MAX_EVENTS, TICKS_PER_BAR, lcm};
use crate::algo::{Euclid, Mode, Scale, mix, pitch_class};

const BAR: u32 = TICKS_PER_BAR;
/// Largest factor of `fast`, `slow`, `ply`, `iter` and `every`.
const MAX_FACTOR: u32 = 16;
/// Most steps in a `struct` rhythm: one a tick.
const MAX_STEPS: usize = BAR as usize;

#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Fast(u32),
    Slow(u32),
    Rev,
    Palindrome,
    /// Semitones, `sub(n)` kept apart so it prints as written.
    Add(i32),
    Sub(i32),
    Ply(u32),
    Iter(u32),
    Degrade(f32),
    Every(u32, Box<Pattern>),
    /// A shift in ticks and the method on the copy.
    Off(u32, Box<Pattern>),
    /// Hits and rests sharing each bar.
    Struct(Vec<bool>),
    /// A struct of `k` hits spread over `n` steps (#582).
    Euclid(Euclid),
    Sometimes(Box<Pattern>),
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
    /// the method with each bar's value, taken in turn.
    PerBar(String, Vec<Pattern>),
    /// `struct(<clip>)`: the rhythm of an earlier clip, its note starts or
    /// its drum hits (ADR-0035, #600).
    StructOf(Context),
    /// Milliseconds between a chord's notes, upward; downward when below 0
    /// (#604).
    Strum(i16),
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
    pub const NAMES: [&'static str; 18] = [
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
        "scale",
        "oct",
        "chord",
        "strum",
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
        let one = || match args.as_slice() {
            [a] => Ok(*a),
            _ => Err("this takes one value"),
        };
        let none = || match args.as_slice() {
            [] => Ok(()),
            _ => Err("this takes no value: ()"),
        };
        // A value a bar, `add("<0 3 5>")`: the method once for each (#629).
        if let ("fast" | "slow" | "add" | "sub" | "ply" | "degrade", [a]) = (name, args.as_slice())
        {
            let a = a.trim();
            if let Some(text) = a.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
                let steps = per_bar(text)?
                    .into_iter()
                    .map(|v| Pattern::parse_in(name, v, srcs))
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(Pattern::PerBar(format!("{name}({a})"), steps));
            }
        }
        match name {
            "fast" => Ok(Pattern::Fast(whole(one()?)?)),
            "slow" => Ok(Pattern::Slow(whole(one()?)?)),
            "ply" => Ok(Pattern::Ply(whole(one()?)?)),
            "iter" => Ok(Pattern::Iter(whole(one()?)?)),
            "rev" => none().map(|()| Pattern::Rev),
            "palindrome" => none().map(|()| Pattern::Palindrome),
            "add" | "sub" => {
                let n = one()?
                    .trim()
                    .parse::<i32>()
                    .ok()
                    .filter(|n| n.abs() <= 48)
                    .ok_or("this takes semitones from -48 to 48")?;
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
            "sometimes" => Ok(Pattern::Sometimes(Box::new(nested(one()?, srcs)?))),
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
            Pattern::Fast(n) => format!("fast({n})"),
            Pattern::Slow(n) => format!("slow({n})"),
            Pattern::Rev => "rev()".to_string(),
            Pattern::Palindrome => "palindrome()".to_string(),
            Pattern::Add(n) => format!("add({n})"),
            Pattern::Sub(n) => format!("sub({n})"),
            Pattern::Ply(n) => format!("ply({n})"),
            Pattern::Iter(n) => format!("iter({n})"),
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
            Pattern::Sometimes(m) => format!("sometimes({})", bare(m)),
            Pattern::Scale(keys) => format!("scale({})", keys.print()),
            Pattern::ScaleOf(c, mode) => format!("scale({} {})", c.name, mode.name()),
            Pattern::Oct(n) => format!("oct({n})"),
            Pattern::ChordOf(c) => format!("chord({})", c.name),
            Pattern::StructOf(c) => format!("struct({})", c.name),
            Pattern::Strum(ms) => format!("strum({ms})"),
            Pattern::PerBar(text, _) => text.clone(),
        }
    }

    /// The line this method makes of `line`.
    pub fn apply(&self, line: &Line) -> Result<Line, &'static str> {
        let out = match self {
            Pattern::Fast(n) => fast(line, *n),
            Pattern::Slow(n) => Line {
                events: line
                    .events
                    .iter()
                    .map(|e| Event {
                        start: e.start * n,
                        len: e.len * n,
                        ..*e
                    })
                    .collect(),
                bars: line.bars * n,
            },
            Pattern::Rev => rev(line),
            Pattern::Palindrome => alternate(line, 2, 1, &Pattern::Rev)?,
            Pattern::Add(n) => transpose(line, *n),
            Pattern::Sub(n) => transpose(line, -n),
            Pattern::Ply(n) => ply(line, *n),
            Pattern::Iter(n) => iter(line, *n),
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
            Pattern::Sometimes(m) => {
                let other = m.apply(line)?;
                let bars = lcm(line.bars, other.bars);
                check(bars, line.events.len() * (bars / line.bars) as usize)?;
                // One coin per moment, so a chord goes one way.
                let heads = |e: &Event| mix(0x50E7_1AE5 ^ e.start, 0x2B) & 1 == 1;
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
            Pattern::PerBar(_, steps) => by_bar(line, steps)?,
            Pattern::Oct(n) => {
                if line.events.iter().any(|e| e.degree) {
                    return Err("oct moves notes: write it after scale");
                }
                transpose(line, 12 * n)
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
            Pattern::PerBar(_, steps) => steps.iter().all(Pattern::pitch_only),
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

    /// A pitch method on event `e`, which starts at tick `t` of the song:
    /// false when the note goes (out of 0 to 127). Nothing allocates (#593).
    pub fn pitch(&self, e: &mut Event, t: u64) -> bool {
        let shift = |e: &mut Event, by: i32| match u8::try_from(i32::from(e.note) + by) {
            Ok(n) if n <= 127 => {
                e.note = n;
                true
            }
            _ => false,
        };
        match self {
            Pattern::Add(n) => shift(e, *n),
            Pattern::Sub(n) => shift(e, -n),
            Pattern::Oct(n) => !e.degree && shift(e, 12 * n),
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
                steps.get(k).is_none_or(|p| p.pitch(e, t))
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

/// The line squeezed n times, repeated until it fills whole bars.
fn fast(line: &Line, n: u32) -> Line {
    let bars = line.bars / gcd(line.bars, n);
    let copies = bars * n / line.bars.max(1);
    let len = line.len();
    let mut events = Vec::new();
    for k in 0..copies {
        events.extend(line.events.iter().map(|e| Event {
            start: div_round(e.start + k * len, n),
            len: div_round(e.len, n).max(1),
            ..*e
        }));
    }
    Line { events, bars }
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

fn transpose(line: &Line, by: i32) -> Line {
    let events = line
        .events
        .iter()
        .filter_map(|e| {
            let n = i32::from(e.note) + by;
            u8::try_from(n)
                .ok()
                .filter(|n| *n <= 127)
                .map(|note| Event { note, ..*e })
        })
        .collect();
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

/// Bar b from the line with method `steps[b mod n]` applied, each of them
/// round with the line (#629).
fn by_bar(line: &Line, steps: &[Pattern]) -> Result<Line, &'static str> {
    let n = u32::try_from(steps.len()).unwrap_or(1).max(1);
    let changed = steps
        .iter()
        .map(|m| m.apply(line))
        .collect::<Result<Vec<_>, _>>()?;
    let bars = changed
        .iter()
        .fold(lcm(line.bars, n), |b, c| lcm(b, c.bars));
    check(bars, 0)?;
    let mut events = Vec::new();
    for (k, c) in (0..).zip(&changed) {
        events.extend(
            c.repeat(bars)
                .events
                .into_iter()
                .filter(|e| (e.start / BAR) % n == k),
        );
    }
    Ok(Line { events, bars })
}

/// The values of `"<a <b c>>"`, one a bar until they come round: a nested
/// `< >` takes its own turns, so `<0 <1 -1>>` is 0 1 0 -1, as in Strudel.
/// A plain value is one for every bar; several in a bar are an error.
fn per_bar(text: &str) -> Result<Vec<&str>, &'static str> {
    let bad = "a value a bar is a number or <a b …>, as \"<0 3 5>\"";
    let t = text.trim();
    let Some(inner) = t.strip_prefix('<') else {
        return if t.is_empty() || t.contains(|c: char| c.is_whitespace() || c == '>') {
            Err(bad)
        } else {
            Ok(vec![t])
        };
    };
    let inner = inner.strip_suffix('>').ok_or(bad)?;
    let (mut items, mut depth, mut start) = (Vec::new(), 0u32, None);
    for (i, c) in inner.char_indices().chain([(inner.len(), ' ')]) {
        match c {
            '<' => {
                start.get_or_insert(i);
                depth += 1;
            }
            '>' => depth = depth.checked_sub(1).ok_or(bad)?,
            c if c.is_whitespace() && depth == 0 => {
                if let Some(s) = start.take() {
                    items.push(inner.get(s..i).ok_or(bad)?);
                }
            }
            _ => {
                start.get_or_insert(i);
            }
        }
    }
    if depth != 0 || items.is_empty() {
        return Err(bad);
    }
    let subs = items
        .into_iter()
        .map(per_bar)
        .collect::<Result<Vec<_>, _>>()?;
    let n = subs.len();
    let round = subs.iter().try_fold(1u32, |p, s| {
        let l = lcm(p, u32::try_from(s.len()).unwrap_or(MAX_BARS + 1));
        (l <= MAX_BARS).then_some(l).ok_or(TOO_LONG)
    })?;
    let total = u32::try_from(n).unwrap_or(MAX_BARS + 1) * round;
    if total > MAX_BARS {
        return Err(TOO_LONG);
    }
    (0..total as usize)
        .map(|b| {
            subs.get(b % n)
                .and_then(|s| s.get(b / n % s.len().max(1)))
                .copied()
                .ok_or(bad)
        })
        .collect()
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
            '(' => depth += 1,
            ')' => depth -= 1,
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
        for text in ["\"<0 3 5>\"", "\"<0 <1 -1>>\"", "\"7\""] {
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
            (
                "add",
                "\"1 2\"",
                "a value a bar is a number or <a b …>, as \"<0 3 5>\"",
            ),
            (
                "add",
                "\"<1 2\"",
                "a value a bar is a number or <a b …>, as \"<0 3 5>\"",
            ),
            (
                "add",
                "\"<>\"",
                "a value a bar is a number or <a b …>, as \"<0 3 5>\"",
            ),
            ("add", "\"<0 99>\"", "this takes semitones from -48 to 48"),
            (
                "fast",
                "\"<1 0>\"",
                "this takes a whole number from 1 to 16",
            ),
        ] {
            assert_eq!(Pattern::parse(name, text), Err(msg), "{name}({text})");
        }
        let long = format!("\"<{}>\"", vec!["1"; 65].join(" "));
        assert_eq!(Pattern::parse("add", &long), Err(TOO_LONG));
        // 33 turns of two: 66 bars before it comes round.
        let nest = format!("\"<{}>\"", vec!["<1 2>"; 33].join(" "));
        assert_eq!(Pattern::parse("add", &nest), Err(TOO_LONG));
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
            ("strum", " 15 ", "strum(15)"),
            ("every", "2, strum(-15)", "every(2, strum(-15))"),
            ("struct", "\"x ~  x\"", "struct(\"x ~ x\")"),
            ("sometimes", "fast(2)", "sometimes(fast(2))"),
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
            ("fast", "0", "this takes a whole number from 1 to 16"),
            ("fast", "1.5", "this takes a whole number from 1 to 16"),
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
            Pattern::Slow(8).apply(&long),
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
