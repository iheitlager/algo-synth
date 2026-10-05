//! Pattern methods (ADR-0019, #215): Strudel's musical transformations,
//! written on a frag's line and applied to its events when the song loads.
//!
//! ```text
//! frag riff = lead .fast(2) .every(4, rev) .off(1/8, add(12))
//! ```
//!
//! A cycle is a bar. `fast(n)` and `slow(n)` squeeze or stretch the line;
//! `rev` plays each bar backwards and `palindrome` every other one; `add(n)`
//! and `sub(n)` transpose by semitones; `ply(n)` repeats each note n times in
//! its own length; `iter(n)` starts each bar a further 1/n in; `degrade(p)`
//! drops each note with chance p, from a fixed seed (ADR-0005); `every(n, m)`
//! applies method m on every nth bar, from the first; `off(t, m)` layers a
//! copy t of a bar later with m applied.
//!
//! Every method maps a line (events and a length in bars) to a line, so
//! nothing happens in `render`. The result is held to `MAX_EVENTS` events and
//! `MAX_BARS` bars; times are on the 48-tick grid, so a squeeze rounds.

use super::{Event, MAX_BARS, MAX_EVENTS, TICKS_PER_BAR, lcm};
use crate::algo::mix;

const BAR: u32 = TICKS_PER_BAR;
/// Largest factor of `fast`, `slow`, `ply`, `iter` and `every`.
const MAX_FACTOR: u32 = 16;

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
    pub const NAMES: [&'static str; 11] = [
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
    ];

    /// The method `name(args)`; `args` is the text between its brackets.
    pub fn parse(name: &str, args: &str) -> Result<Pattern, &'static str> {
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
                    return Err("this takes a number and a method, e.g. every(4, rev)");
                };
                let inner = Box::new(nested(m)?);
                if name == "every" {
                    Ok(Pattern::Every(whole(a)?, inner))
                } else {
                    Ok(Pattern::Off(ticks(a)?, inner))
                }
            }
            _ => Err("no such pattern method"),
        }
    }

    /// The method as written on a frag line, without its dot.
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
        };
        check(out.bars, out.events.len())?;
        Ok(out.sorted())
    }
}

/// Every pattern of a frag applied in order.
pub fn apply_all(patterns: &[Pattern], line: Line) -> Result<Line, &'static str> {
    patterns.iter().try_fold(line, |l, p| p.apply(&l))
}

fn check(bars: u32, events: usize) -> Result<(), &'static str> {
    if bars > MAX_BARS {
        Err("the pattern runs past 32 bars")
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
fn nested(s: &str) -> Result<Pattern, &'static str> {
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
    Pattern::parse(name, args)
}

/// The arguments between a method's brackets, split at commas outside
/// brackets; none for empty text.
fn split_args(s: &str) -> Vec<&str> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
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
            ("add", "99", "this takes semitones from -48 to 48"),
            ("degrade", "2", "this takes a chance from 0 to 1"),
            (
                "every",
                "4",
                "this takes a number and a method, e.g. every(4, rev)",
            ),
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
        ] {
            assert_eq!(Pattern::parse(name, args), Err(msg), "{name}({args})");
        }
        let long = Line {
            events: vec![ev(0, 1, 60)],
            bars: 16,
        };
        assert_eq!(
            Pattern::Slow(4).apply(&long),
            Err("the pattern runs past 32 bars")
        );
        let many = run("ply(8)", &run("ply(16)", &four()));
        assert_eq!(many.events.len(), 512);
        assert_eq!(
            Pattern::Ply(2).apply(&many),
            Err("the pattern makes more than 512 notes")
        );
    }
}
