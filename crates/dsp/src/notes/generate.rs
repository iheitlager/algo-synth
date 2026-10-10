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
//! prog(4,7)                      four bars of chords in the song's key, seed 7
//! root(prog)                     the root of each chord of prog, as a bass line
//! arp(prog,up,16)                prog's chords arpeggiated, each from its start
//! ```

use super::{Cursor, Event, NoteError, Pitch, TICKS_PER_BAR, err};
use crate::algo::{Rng, Scale, mix};
use crate::arp;

/// The calls a line of notes may be, `euclid` aside; no other is read.
pub const CALLS: [&str; 6] = ["arp", "walk", "markov", "mutate", "root", "prog"];

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
    pub const ALL: [(ArpMode, &'static str); 4] = [
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
    /// The chords of an earlier frag, each arpeggiated from where it starts (#103).
    ArpProg {
        from: String,
        src: Vec<Event>,
        bars: u32,
        mode: ArpMode,
        rate: u8,
        seed: u32,
    },
    /// The lowest note of each chord of an earlier frag, moved to `octave` (#103).
    Root {
        from: String,
        src: Vec<Event>,
        bars: u32,
        octave: u8,
    },
    /// A progression in the song's key, a triad a bar (#103).
    Prog {
        bars: u32,
        seed: u32,
    },
}

/// Bars a progression may have, and the octave `root` plays in by default.
const MAX_PROG: u32 = 16;
const ROOT_OCTAVE: u8 = 2;

/// A fragment a call may read: its events and bars.
pub type Source = (Vec<Event>, u32);

impl Gen {
    pub fn seed(&self) -> u32 {
        match self {
            Gen::Arp { seed, .. }
            | Gen::Walk { seed, .. }
            | Gen::Markov { seed, .. }
            | Gen::Mutate { seed, .. }
            | Gen::ArpProg { seed, .. }
            | Gen::Prog { seed, .. } => *seed,
            Gen::Root { .. } => 0,
        }
    }

    /// Bars before the events repeat.
    pub fn bars(&self) -> u32 {
        match self {
            Gen::Arp { .. } | Gen::Walk { .. } => 1,
            Gen::Markov { bars, .. }
            | Gen::Mutate { bars, .. }
            | Gen::ArpProg { bars, .. }
            | Gen::Root { bars, .. }
            | Gen::Prog { bars, .. } => *bars,
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
            Gen::ArpProg {
                from,
                mode,
                rate,
                seed,
                ..
            } => {
                let seed = if *mode == ArpMode::Random {
                    format!(",{seed}")
                } else {
                    String::new()
                };
                format!("arp({from},{},{rate}{seed})", mode.name())
            }
            Gen::Root { from, octave, .. } if *octave == ROOT_OCTAVE => format!("root({from})"),
            Gen::Root { from, octave, .. } => format!("root({from},{octave})"),
            Gen::Prog { bars, seed } => format!("prog({bars},{seed})"),
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
            Gen::Markov { src, .. } | Gen::Mutate { src, .. } | Gen::Root { src, .. } => src.len(),
            Gen::ArpProg { rate, bars, .. } => usize::from(*rate) * *bars as usize,
            Gen::Prog { bars, .. } => 3 * *bars as usize,
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
            Gen::ArpProg {
                src,
                bars,
                mode,
                rate,
                ..
            } => arp_prog(src, *bars, *mode, *rate, seed, out),
            Gen::Root { src, octave, .. } => root(src, *octave, out),
            Gen::Prog { bars, .. } => prog(*bars, seed, scale, out),
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
                word: 0,
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
            word: 0,
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

/// The notes of the chord sounding at tick `t` in `src`: those starting at
/// the latest start at or before `t`, lowest first, into `buf`. Its start and
/// the number of notes, or `None` before the first chord.
fn chord_at(src: &[Event], t: u32, buf: &mut [u8; MAX_CHORD]) -> Option<(u32, usize)> {
    let start = src.iter().map(|e| e.start).filter(|s| *s <= t).max()?;
    let mut n = 0;
    for e in src.iter().filter(|e| e.start == start) {
        if let Some(slot) = buf.get_mut(n) {
            *slot = e.note;
            n += 1;
        }
    }
    if let Some(notes) = buf.get_mut(..n) {
        notes.sort_unstable();
    }
    Some((start, n))
}

/// `arp(prog,…)`: at each step, the chord of `src` sounding then, played from
/// its first step as the arp plays a held chord.
fn arp_prog(src: &[Event], bars: u32, mode: ArpMode, rate: u8, seed: u32, out: &mut Vec<Event>) {
    let len = TICKS_PER_BAR / u32::from(rate).max(1);
    let count = bars * TICKS_PER_BAR / len.max(1);
    let mut r = rng(seed, 0xA5);
    let mut buf = [0u8; MAX_CHORD];
    for i in 0..count {
        let t = i * len;
        let Some((start, n)) = chord_at(src, t, &mut buf) else {
            continue;
        };
        let held = buf.get(..n).unwrap_or(&[]);
        let step = u64::from((t - start) / len.max(1));
        let note = match mode {
            ArpMode::Up => arp::arp_note(held, arp::ArpMode::Up, 1, step, seed),
            ArpMode::Down => arp::arp_note(held, arp::ArpMode::Down, 1, step, seed),
            ArpMode::UpDown => arp::arp_note(held, arp::ArpMode::UpDown, 1, step, seed),
            ArpMode::Random => {
                let k = usize::try_from(r.next_u32()).unwrap_or(0) % n.max(1);
                held.get(k).copied()
            }
        };
        if let Some(note) = note {
            out.push(Event {
                start: t,
                len,
                note,
                accent: false,
                vel: 0,
                word: 0,
            });
        }
    }
}

/// `root(prog)`: for each start in `src`, its lowest note's pitch class in
/// `octave`, as long as the longest note there.
fn root(src: &[Event], octave: u8, out: &mut Vec<Event>) {
    let mut i = 0;
    while let Some(first) = src.get(i) {
        let mut low = first.note;
        let mut len = first.len;
        let mut j = i;
        while let Some(e) = src.get(j).filter(|e| e.start == first.start) {
            low = low.min(e.note);
            len = len.max(e.len);
            j += 1;
        }
        let note = (u16::from(octave) + 1) * 12 + u16::from(low % 12);
        out.push(Event {
            start: first.start,
            len,
            note: u8::try_from(note.min(127)).unwrap_or(127),
            accent: false,
            vel: 0,
            word: 0,
        });
        i = j.max(i + 1);
    }
}

/// Degrees by function: tonic (I vi iii), subdominant (IV ii), dominant (V vii).
const FUNCTIONS: [&[usize]; 3] = [&[0, 5, 2], &[3, 1], &[4, 6]];
/// The chance (of 8) of each next function after tonic, subdominant, dominant.
const MOVES: [[u32; 3]; 3] = [[1, 4, 3], [1, 2, 5], [6, 0, 2]];

/// `prog(bars,seed)`: a triad a bar on the song's scale, starting on the
/// tonic (I) and ending on the dominant (V), moving between functions as
/// common practice does: tonic to anything, subdominant towards the
/// dominant, the dominant home.
fn prog(bars: u32, seed: u32, scale: Option<&Scale>, out: &mut Vec<Event>) {
    let Some(scale) = scale.filter(|s| s.degree(0).is_some()) else {
        return;
    };
    let mut r = rng(seed, 0x9C0);
    let mut function = 0usize;
    for bar in 0..bars.min(MAX_PROG) {
        let degree = if bar == 0 {
            0
        } else if bar + 1 == bars {
            function = 2;
            4
        } else {
            let pick = r.next_u32() % 8;
            let moves = MOVES.get(function).copied().unwrap_or([8, 0, 0]);
            let mut acc = 0;
            for (f, w) in moves.iter().enumerate() {
                acc += w;
                if pick < acc {
                    function = f;
                    break;
                }
            }
            let choices = FUNCTIONS.get(function).copied().unwrap_or(&[0]);
            choices
                .get(usize::try_from(r.next_u32()).unwrap_or(0) % choices.len().max(1))
                .copied()
                .unwrap_or(0)
        };
        // Root, third and fifth stacked from the degree, in octave 4.
        let mut below = 59u8;
        for k in [0, 2, 4] {
            let pc = scale.degree((degree + k) % 7).unwrap_or(0);
            let mut note = 60 + pc;
            while note <= below {
                note += 12;
            }
            below = note;
            out.push(Event {
                start: bar * TICKS_PER_BAR,
                len: TICKS_PER_BAR,
                note,
                accent: false,
                vel: 0,
                word: 0,
            });
        }
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
        scale: cur.scale,
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
        scale: cur.scale,
    };
    if sub.peek() != Some('[') {
        // A chord by name (#103): `arp(c:m7,up,16)`, `arp(V7,up,16)`.
        if sub.peek().is_some_and(char::is_alphabetic) && !super::chord::is_note(slice) {
            return super::chord::notes_of(slice, sub.col(), cur.scale);
        }
        return err(sub.col(), "a chord goes here, as [c4,e4,g4] or c:m7");
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
    if cur.peek() != Some('(') || !CALLS.contains(&name.as_str()) {
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
            // A frag's name: its progression, chord by chord (#103).
            let prog = Some(text(cur, arg(0)))
                .filter(|t| !t.starts_with('['))
                .and_then(|t| srcs(&t).map(|s| (t, s)));
            let chord = if prog.is_some() {
                Vec::new()
            } else {
                chord_arg(cur, arg(0))?
            };
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
            match prog {
                Some((from, (src, bars))) => Gen::ArpProg {
                    from,
                    src,
                    bars,
                    mode,
                    rate,
                    seed,
                },
                None => Gen::Arp {
                    chord,
                    mode,
                    rate,
                    seed,
                },
            }
        }
        "root" => {
            want(
                &[1, 2],
                "root takes a frag and maybe an octave: root(prog) or root(prog,3)",
            )?;
            let (from, (src, bars)) = source(cur, arg(0), srcs)?;
            let octave = if list.len() == 2 {
                number(cur, arg(1), 7, "an octave is 0 to 7")?
            } else {
                u32::from(ROOT_OCTAVE)
            };
            Gen::Root {
                from,
                src,
                bars,
                octave: u8::try_from(octave).unwrap_or(ROOT_OCTAVE),
            }
        }
        "prog" => {
            want(&[2], "prog takes a number of bars and a seed: prog(4,7)")?;
            if scale.and_then(|s| s.degree(0)).is_none() {
                return err(
                    cur.base + at,
                    "a prog needs a scale line with seven notes, as scale c minor",
                );
            }
            let bars = number(cur, arg(0), MAX_PROG, "a prog is 1 to 16 bars")?;
            if bars == 0 {
                return err(cur.base + arg(0).0, "a prog is 1 to 16 bars");
            }
            Gen::Prog {
                bars,
                seed: seed_at(1)?,
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
