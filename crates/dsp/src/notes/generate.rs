//! The generators of spec 002 Req 7 that write notes: `arp`, `walk`, `markov`
//! and `mutate` (ADR-0016). Each is a call on a note line, seeded and total:
//! the same call gives the same events, bounded in length and range.
//!
//! ```text
//! arp([c4,e4,g4],up,16)          a chord, up|down|updown, a rate (a sixteenth)
//! arp([c4,e4,g4],random,16,7)    random takes a seed
//! walk(c4,8,1)                   8 notes, a random walk on the song's scale
//! markov(1,riff,3)               learn order 1 from the frag riff, seed 3
//! mutate(riff,30,5)              change 30 percent of riff's notes, seed 5
//! ```

use super::{Cursor, Event, NoteError, Pitch, TICKS_PER_BAR, err};
use crate::algo::{Rng, Scale, mix};
use crate::arp;

/// Notes of a walk, and of a chord for an arpeggio.
const MAX_WALK: u32 = 32;
const MAX_CHORD: usize = 8;
/// Longest Markov context.
const MAX_ORDER: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArpMode {
    Up,
    Down,
    UpDown,
    Random,
}

impl ArpMode {
    const ALL: [(ArpMode, &'static str); 4] = [
        (ArpMode::Up, "up"),
        (ArpMode::Down, "down"),
        (ArpMode::UpDown, "updown"),
        (ArpMode::Random, "random"),
    ];

    fn from_name(s: &str) -> Option<ArpMode> {
        ArpMode::ALL.iter().find(|m| m.1 == s).map(|m| m.0)
    }

    fn name(self) -> &'static str {
        ArpMode::ALL
            .iter()
            .find(|m| m.0 == self)
            .map_or("", |m| m.1)
    }
}

/// A generator call. `from` and `src` of Markov and Mutate keep the events of
/// the fragment they read, so a call can run again without looking it up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gen {
    Arp {
        chord: Vec<Pitch>,
        mode: ArpMode,
        /// 2, 4, 8 or 16: a half, quarter, eighth or sixteenth note per step.
        rate: u8,
        seed: u32,
    },
    Walk {
        start: u8,
        steps: u32,
        seed: u32,
    },
    Markov {
        order: u8,
        from: String,
        src: Vec<Event>,
        bars: u32,
        seed: u32,
    },
    Mutate {
        from: String,
        /// Percent of the notes changed, 0 to 100.
        amount: u8,
        src: Vec<Event>,
        bars: u32,
        seed: u32,
    },
}

/// A fragment a call may read: its events and bars.
pub type Source = (Vec<Event>, u32);

impl Gen {
    pub fn seed(&self) -> u32 {
        match self {
            Gen::Arp { seed, .. }
            | Gen::Walk { seed, .. }
            | Gen::Markov { seed, .. }
            | Gen::Mutate { seed, .. } => *seed,
        }
    }

    /// Bars before the events repeat.
    pub fn bars(&self) -> u32 {
        match self {
            Gen::Arp { .. } | Gen::Walk { .. } => 1,
            Gen::Markov { bars, .. } | Gen::Mutate { bars, .. } => *bars,
        }
    }

    pub fn print(&self) -> String {
        match self {
            Gen::Arp {
                chord,
                mode,
                rate,
                seed,
            } => {
                let ps: Vec<String> = chord.iter().map(super::pitch_text).collect();
                let seed = if *mode == ArpMode::Random {
                    format!(",{seed}")
                } else {
                    String::new()
                };
                format!("arp([{}],{},{rate}{seed})", ps.join(","), mode.name())
            }
            Gen::Walk { start, steps, seed } => {
                format!("walk({},{steps},{seed})", super::note_name(*start))
            }
            Gen::Markov {
                order, from, seed, ..
            } => format!("markov({order},{from},{seed})"),
            Gen::Mutate {
                from, amount, seed, ..
            } => format!("mutate({from},{amount},{seed})"),
        }
    }

    /// The events of the call for `seed` (its own, or a cycle's for a live
    /// fragment), sorted.
    pub fn events(&self, seed: u32, scale: Option<&Scale>) -> Vec<Event> {
        let mut out = Vec::with_capacity(self.max_events());
        self.events_into(seed, scale, &mut out);
        out
    }

    /// The most events a call can make: room to reserve for `events_into`.
    pub fn max_events(&self) -> usize {
        match self {
            Gen::Arp { rate, .. } => usize::from(*rate),
            Gen::Walk { .. } => MAX_WALK as usize,
            Gen::Markov { src, .. } | Gen::Mutate { src, .. } => src.len(),
        }
    }

    /// As `events`, into `out` (cleared first). With room reserved for
    /// `max_events` it never allocates, so the engine can run it on the audio
    /// thread's schedule (ADR-0002).
    pub fn events_into(&self, seed: u32, scale: Option<&Scale>, out: &mut Vec<Event>) {
        out.clear();
        match self {
            Gen::Arp {
                chord, mode, rate, ..
            } => arp(chord, *mode, *rate, seed, out),
            Gen::Walk { start, steps, .. } => walk(*start, *steps, seed, scale, out),
            Gen::Markov { order, src, .. } => markov(*order, src, seed, out),
            Gen::Mutate { amount, src, .. } => mutate(*amount, src, seed, scale, out),
        }
        out.sort_unstable_by_key(super::sort_key);
        // Past the room reserved, a push would allocate on the audio thread (#233).
        debug_assert!(out.len() <= self.max_events(), "{} events", out.len());
    }
}

fn rng(seed: u32, tag: u32) -> Rng {
    Rng::new(mix(seed, tag))
}

fn arp(chord: &[Pitch], mode: ArpMode, rate: u8, seed: u32, out: &mut Vec<Event>) {
    let mut buf = [Pitch {
        note: 0,
        accent: false,
    }; MAX_CHORD];
    let n = chord.len().min(MAX_CHORD);
    let notes = buf.get_mut(..n).unwrap_or(&mut []);
    notes.copy_from_slice(chord.get(..n).unwrap_or(&[]));
    notes.sort_unstable_by_key(|p| p.note);
    let mut held = [0u8; MAX_CHORD];
    for (h, p) in held.iter_mut().zip(notes.iter()) {
        *h = p.note;
    }
    let held = held.get(..n).unwrap_or(&[]);
    let len = TICKS_PER_BAR / u32::from(rate).max(1);
    let count = TICKS_PER_BAR / len.max(1);
    let mut r = rng(seed, 0xA4);
    for i in 0..count {
        // The deterministic modes are the live arp's pattern (spec 002 Req 7).
        let step = u64::from(i);
        let note = match mode {
            ArpMode::Up => arp::arp_note(held, arp::ArpMode::Up, 1, step, seed),
            ArpMode::Down => arp::arp_note(held, arp::ArpMode::Down, 1, step, seed),
            ArpMode::UpDown => arp::arp_note(held, arp::ArpMode::UpDown, 1, step, seed),
            ArpMode::Random => {
                let k = usize::try_from(r.next_u32()).unwrap_or(0) % n.max(1);
                notes.get(k).map(|p| p.note)
            }
        };
        if let Some(note) = note {
            let accent = notes.iter().any(|p| p.note == note && p.accent);
            out.push(Event {
                start: i * len,
                len,
                note,
                accent,
                vel: 0,
            });
        }
    }
}

fn walk(start: u8, steps: u32, seed: u32, scale: Option<&Scale>, out: &mut Vec<Event>) {
    let Some(scale) = scale else {
        return;
    };
    let steps = steps.clamp(1, MAX_WALK);
    // Two octaves of degrees, starting one octave up from the lowest.
    let span = 2 * scale.degrees();
    let low = start.saturating_sub(12);
    let mut pos = i64::from(scale.degrees());
    let mut r = rng(seed, 0x3A1);
    for i in 0..steps {
        if i > 0 {
            pos += i64::from(r.next_u32() % 3) - 1;
            pos = pos.clamp(0, i64::from(span) - 1);
        }
        let a = i * TICKS_PER_BAR / steps;
        let b = (i + 1) * TICKS_PER_BAR / steps;
        out.push(Event {
            start: a,
            len: (b - a).max(1),
            note: scale.walk(low, u32::try_from(pos).unwrap_or(0)),
            accent: false,
            vel: 0,
        });
    }
}

/// Pitches follow a chain learned from the source's own order of notes; the
/// rhythm is the source's. Every note comes from the source's pitch set.
fn markov(order: u8, src: &[Event], seed: u32, out: &mut Vec<Event>) {
    let m = src.len();
    if m == 0 {
        return;
    }
    let k = usize::from(order.clamp(1, MAX_ORDER));
    let mut r = rng(seed, 0x3A2);
    let note_at = |i: usize| src.get(i).map_or(0, |e| e.note);
    // Start from a window of the source.
    let first = usize::try_from(r.next_u32()).unwrap_or(0) % m;
    let mut ctx = [0u8; MAX_ORDER as usize];
    for (j, c) in ctx.iter_mut().take(k).enumerate() {
        *c = note_at((first + j) % m);
    }
    let follows = |i: usize, ctx: &[u8]| {
        i + k < m
            && ctx
                .iter()
                .take(k)
                .enumerate()
                .all(|(j, c)| note_at(i + j) == *c)
    };
    for e in src {
        let count = (0..m).filter(|i| follows(*i, &ctx)).count();
        let note = if count > 0 {
            let nth = usize::try_from(r.next_u32()).unwrap_or(0) % count;
            (0..m)
                .filter(|i| follows(*i, &ctx))
                .nth(nth)
                .map_or(e.note, |i| note_at(i + k))
        } else {
            note_at(usize::try_from(r.next_u32()).unwrap_or(0) % m)
        };
        out.push(Event { note, ..*e });
        ctx.rotate_left(1);
        if let Some(last) = ctx.get_mut(k - 1) {
            *last = note;
        }
    }
}

/// Each note changes with the given chance: dropped, or moved a few semitones
/// and snapped onto the scale when there is one.
fn mutate(amount: u8, src: &[Event], seed: u32, scale: Option<&Scale>, out: &mut Vec<Event>) {
    let mut r = rng(seed, 0x3A3);
    for e in src {
        if r.next_u32() % 100 >= u32::from(amount) {
            out.push(*e);
            continue;
        }
        let d = r.next_u32() % 5;
        if d == 0 {
            continue;
        }
        let shift = [0i32, -2, -1, 1, 2]
            .get(usize::try_from(d).unwrap_or(0))
            .copied()
            .unwrap_or(0);
        let moved = u8::try_from((i32::from(e.note) + shift).clamp(0, 127)).unwrap_or(e.note);
        let note = scale.map_or(moved, |s| s.walk(moved, 0));
        out.push(Event { note, ..*e });
    }
}

// --- Parsing ---------------------------------------------------------------

/// The arguments of the call at the cursor: each a trimmed range of chars.
fn args(cur: &mut Cursor<'_>) -> Result<Vec<(usize, usize)>, NoteError> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut from = cur.i;
    loop {
        let Some(c) = cur.peek() else {
            return err(cur.col(), "a call ends with )");
        };
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' | ')' if depth == 0 => {
                let (mut a, mut b) = (from, cur.i);
                while cur.c.get(a).is_some_and(|c| c.is_whitespace()) {
                    a += 1;
                }
                while b > a && cur.c.get(b - 1).is_some_and(|c| c.is_whitespace()) {
                    b -= 1;
                }
                out.push((a, b));
                from = cur.i + 1;
                if c == ')' {
                    cur.i += 1;
                    return Ok(out);
                }
            }
            _ => {}
        }
        cur.i += 1;
    }
}

fn text(cur: &Cursor<'_>, r: (usize, usize)) -> String {
    cur.c.get(r.0..r.1).unwrap_or(&[]).iter().collect()
}

fn number(
    cur: &Cursor<'_>,
    r: (usize, usize),
    max: u32,
    msg: &'static str,
) -> Result<u32, NoteError> {
    let t = text(cur, r);
    match t.parse::<u32>() {
        Ok(n) if n <= max => Ok(n),
        _ => err(cur.base + r.0, msg),
    }
}

fn pitch_arg(cur: &Cursor<'_>, r: (usize, usize)) -> Result<Pitch, NoteError> {
    let slice = cur.c.get(r.0..r.1).unwrap_or(&[]);
    let mut sub = Cursor {
        c: slice,
        i: 0,
        base: cur.base + r.0,
        items: 0,
    };
    let p = sub.pitch()?;
    if sub.peek().is_some() {
        return err(sub.col(), "a note goes here, as c4");
    }
    Ok(p)
}

fn chord_arg(cur: &Cursor<'_>, r: (usize, usize)) -> Result<Vec<Pitch>, NoteError> {
    let slice = cur.c.get(r.0..r.1).unwrap_or(&[]);
    let mut sub = Cursor {
        c: slice,
        i: 0,
        base: cur.base + r.0,
        items: 0,
    };
    if sub.peek() != Some('[') {
        return err(sub.col(), "a chord goes here, as [c4,e4,g4]");
    }
    sub.i += 1;
    let chord = sub.chord()?;
    if sub.peek().is_some() {
        return err(sub.col(), "a chord goes here, as [c4,e4,g4]");
    }
    if chord.len() > MAX_CHORD {
        return err(cur.base + r.0, "a chord has at most 8 notes");
    }
    Ok(chord)
}

fn source(
    cur: &Cursor<'_>,
    r: (usize, usize),
    srcs: &dyn Fn(&str) -> Option<Source>,
) -> Result<(String, Source), NoteError> {
    let name = text(cur, r);
    match srcs(&name) {
        Some(s) => Ok((name, s)),
        None => err(
            cur.base + r.0,
            "no note frag with this name comes before this one",
        ),
    }
}

/// A call of `arp`, `walk`, `markov` or `mutate` at the cursor; `None` (the
/// cursor left where it was) when the line is not one.
pub(super) fn parse_call(
    cur: &mut Cursor<'_>,
    scale: Option<&Scale>,
    srcs: &dyn Fn(&str) -> Option<Source>,
) -> Result<Option<Gen>, NoteError> {
    let start = cur.i;
    let mut name = String::new();
    while let Some(c) = cur.peek().filter(char::is_ascii_lowercase) {
        name.push(c);
        cur.i += 1;
    }
    if cur.peek() != Some('(') || !["arp", "walk", "markov", "mutate"].contains(&name.as_str()) {
        cur.i = start;
        return Ok(None);
    }
    let at = start;
    cur.i += 1;
    let list = args(cur)?;
    cur.skip_ws();
    if cur.peek().is_some() {
        return err(cur.col(), "nothing goes after a call");
    }
    let want = |n: &[usize], usage: &'static str| -> Result<(), NoteError> {
        if n.contains(&list.len()) {
            Ok(())
        } else {
            err(cur.base + at, usage)
        }
    };
    let arg = |k: usize| list.get(k).copied().unwrap_or((at, at));
    let seed_at = |k: usize| number(cur, arg(k), u32::MAX, "a seed is a number");
    let call = match name.as_str() {
        "arp" => {
            want(
                &[3, 4],
                "arp takes a chord, a mode, a rate and for random a seed: arp([c4,e4,g4],up,16)",
            )?;
            let chord = chord_arg(cur, arg(0))?;
            let m = text(cur, arg(1));
            let mode = ArpMode::from_name(&m).ok_or(NoteError {
                col: cur.base + arg(1).0,
                msg: "a mode is up, down, updown or random",
            })?;
            let rate = match text(cur, arg(2)).as_str() {
                "2" => 2,
                "4" => 4,
                "8" => 8,
                "16" => 16,
                _ => return err(cur.base + arg(2).0, "a rate is 2, 4, 8 or 16"),
            };
            if (mode == ArpMode::Random) != (list.len() == 4) {
                return err(
                    cur.base + at,
                    "only random takes a seed: arp([c4,e4,g4],random,16,7)",
                );
            }
            let seed = if list.len() == 4 { seed_at(3)? } else { 0 };
            Gen::Arp {
                chord,
                mode,
                rate,
                seed,
            }
        }
        "walk" => {
            want(
                &[3],
                "walk takes a start note, a count and a seed: walk(c4,8,1)",
            )?;
            if scale.is_none() {
                return err(cur.base + at, "a walk needs a scale line before it");
            }
            let start = pitch_arg(cur, arg(0))?.note;
            let steps = number(cur, arg(1), MAX_WALK, "a walk is 1 to 32 notes")?;
            if steps == 0 {
                return err(cur.base + arg(1).0, "a walk is 1 to 32 notes");
            }
            Gen::Walk {
                start,
                steps,
                seed: seed_at(2)?,
            }
        }
        "markov" => {
            want(
                &[3],
                "markov takes an order, a frag and a seed: markov(1,riff,3)",
            )?;
            let order = number(cur, arg(0), u32::from(MAX_ORDER), "an order is 1 to 3")?;
            if order == 0 {
                return err(cur.base + arg(0).0, "an order is 1 to 3");
            }
            let (from, (src, bars)) = source(cur, arg(1), srcs)?;
            Gen::Markov {
                order: u8::try_from(order).unwrap_or(1),
                from,
                src,
                bars,
                seed: seed_at(2)?,
            }
        }
        _ => {
            want(
                &[3],
                "mutate takes a frag, a percent and a seed: mutate(riff,30,5)",
            )?;
            let (from, (src, bars)) = source(cur, arg(0), srcs)?;
            let amount = number(cur, arg(1), 100, "a percent is 0 to 100")?;
            Gen::Mutate {
                from,
                amount: u8::try_from(amount).unwrap_or(100),
                src,
                bars,
                seed: seed_at(2)?,
            }
        }
    };
    Ok(Some(call))
}
