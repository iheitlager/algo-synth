//! Note clips (spec 002 Req 3, ADR-0016): the text of a pitched line,
//! parsed, printed and compiled to events on a grid of 48 ticks to the bar.
//!
//! Two notations, never mixed in one line:
//!
//! ```text
//! "c4 [e4 g4] ~ <c5 d5>?"     mini-notation: one quoted cycle (one bar)
//! c4:4 e4:8 [c4,e4,g4]:2      classic notes with durations, one after another
//! ```
//!
//! Mini-notation: a quoted sequence divides the bar among its words; `[ ]`
//! divides a word's share again, `~` rests, `*n` repeats within the share,
//! `@n` gives a word n shares, `<a b>` plays one option per bar, `?` plays a
//! word on half the bars, `?0.3` on about 3 in 10 (drawn from a fixed seed, so
//! a run is repeatable), `[c4,e4,g4]` is a chord and `!` accents a note. A trailing `&` makes a note
//! (or chord) run one tick into the next, so a legato synth with glide slides
//! to it (#241). Classic: `c4:4` is a quarter, `:1 :2 :4 :8 :16` with a dot
//! for one and a half, `r:4` rests, and `c4:8&` slides.
//!
//! A number where a note goes is a degree of the scale (#570): `"0 2 [1 3]"`,
//! `-1` below the root. The clip's `.scale(a1 minor)` or the song's `scale`
//! line makes it a note when the song loads; until then an event holds the
//! degree, offset by `DEGREE_ZERO`, so `add(n)` moves it by n degrees.
//! `0 .. 12` (or `0..12`) is every degree from one to the other, sharing the
//! word's time as a `[ ]` group would, downward when the second is lower (#575).
//!
//! A third form, timed notes, says exactly where each note is (#173, the
//! form MIDI import writes): `d5@0:6 f#5@6:6:90 a4@12:24` is a note at tick
//! 0 for 6 ticks, one at 6 with velocity 90 of 127, one at 12 for a half bar.
//! Notes may overlap and run past the line's end; the clip line may say how
//! many bars the line is (`bars 8`), else it ends at the bar of its last start.
//!
//! Parsing and compiling allocate and happen when a song is loaded, never in
//! `render`. Both are total: bad text is an error with a column.

mod chord;
mod generate;
pub mod pattern;

pub use chord::{NUMERALS, QUALITIES};
use generate::Source;
pub use generate::{ArpMode, CALLS, Gen};

use crate::algo::{Euclid, Rng, Scale, mix};

/// Ticks in a bar of four quarters.
pub const TICKS_PER_BAR: u32 = 48;
/// Most events a clip may compile to, and most bars it may run before it
/// repeats.
pub const MAX_EVENTS: usize = 512;
pub const MAX_BARS: u32 = 64;
/// Most words (slots and notes) in one line.
const MAX_ITEMS: usize = 1100;
/// A line with `?` repeats after this many bars.
const CHANCE_BARS: u32 = 8;
/// Nesting of `[ ]` and `< >`.
const MAX_DEPTH: usize = 4;
const MAX_FACTOR: u32 = 16;
/// A degree is held in a note number this far up, so a negative one fits.
pub const DEGREE_ZERO: u8 = 64;
/// Degrees reach this far above and below the root.
const MAX_DEGREE: i32 = 24;
/// The octave of a scale root written without one, as in Strudel: `a` is a3.
pub const ROOT_OCTAVE: u8 = 3;
/// Velocities, as drums have them.
pub const HIT_VELOCITY: f32 = 0.75;
pub const ACCENT_VELOCITY: f32 = 1.0;

/// A note of the line: a MIDI number and whether it is accented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pitch {
    pub note: u8,
    pub accent: bool,
    /// `note` is a scale degree plus `DEGREE_ZERO`, not yet a note.
    pub degree: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Rest,
    Note(Pitch),
    Chord(Vec<Pitch>),
    /// `c:m7` or `VI`: a chord by name, kept as written (#103).
    Symbol(Symbol),
    /// `[ ... ]`: its words share the slot.
    Group(Vec<Slot>),
    /// `< ... >`: one option per bar.
    Alt(Vec<Slot>),
    /// `0 .. 12`: every degree from the first to the last, sharing the slot.
    Range(i32, i32),
}

/// A chord written by name and the notes it names in the song's key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub text: String,
    pub pitches: Vec<Pitch>,
}

/// A word of a mini-notation line with its suffixes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub item: Item,
    /// `*n`: repeats inside the share.
    pub times: u32,
    /// `@n`: shares of the bar (or group) it takes.
    pub weight: u32,
    /// `(k,n[,rot])`: plays on k hits spread over n steps of its share, as
    /// in Strudel and Tidal (#601).
    pub euclid: Option<Euclid>,
    /// `?…`: plays on some bars only.
    pub cond: Option<Cond>,
    /// `&`: the note runs one tick into the next, so a legato synth slides.
    pub slide: bool,
}

/// The bars a word plays in, after its `?`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    /// `?p`: a seeded chance in thousandths, `None` for `?` alone, a half
    /// (#602).
    Chance(Option<u16>),
    /// An Elektron trig condition (#603), true when written `?not…`.
    Trig(Trig, bool),
}

/// The trig conditions after a `?` (#603).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trig {
    /// `a:b`: bar a of every b of the line.
    Ratio(u8, u8),
    /// `pre`: as the last condition before it in the bar went.
    Pre,
    /// `first`: in the clip's first bar (`When::first`).
    First,
    /// `last`: in the last bar of its scene (`When::last`).
    Last,
}

/// The bars an event plays in that only the engine knows (#603): with
/// `Some(b)`, only where being the first (last) bar is `b`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct When {
    /// The first bar of its scene (a launched one's first time round), or
    /// the bar a new song takes over on.
    pub first: Option<bool>,
    /// The last bar of its scene; never outside a scene.
    pub last: Option<bool>,
}

impl When {
    /// Both at once, or `None` when they can never both hold.
    fn and(self, o: When) -> Option<When> {
        let one = |a: Option<bool>, b: Option<bool>| match (a, b) {
            (Some(x), Some(y)) if x != y => Err(()),
            _ => Ok(a.or(b)),
        };
        Some(When {
            first: one(self.first, o.first).ok()?,
            last: one(self.last, o.last).ok()?,
        })
    }

    /// The opposite of a single condition.
    fn not(self) -> When {
        When {
            first: self.first.map(|b| !b),
            last: self.last.map(|b| !b),
        }
    }

    /// Does it play in a bar that is (or is not) `first` and `last`?
    pub fn holds(self, first: bool, last: bool) -> bool {
        self.first.is_none_or(|b| b == first) && self.last.is_none_or(|b| b == last)
    }
}

/// How the last condition in a bar went, for `?pre`: known when the line
/// compiles, or as the engine finds the bar.
#[derive(Clone, Copy)]
enum Prev {
    Known(bool),
    Runtime(When),
}

impl Cond {
    /// Does a chance play, `r` the bar's next draw? A half plays when `r` is
    /// odd, as `?` always has, and every chance reads the same bit first, so
    /// `?` alone plays exactly as before.
    fn plays(p: Option<u16>, r: u32) -> bool {
        let p = u64::from(p.unwrap_or(500));
        u64::from((!r).reverse_bits()) * 1000 < p << 32
    }

    fn print(self) -> String {
        match self {
            Cond::Chance(None) => "?".to_string(),
            Cond::Chance(Some(0)) => "?0".to_string(),
            Cond::Chance(Some(1000)) => "?1".to_string(),
            Cond::Chance(Some(m)) => format!(
                "?{}",
                format!("{:.3}", f64::from(m) / 1000.0).trim_end_matches('0')
            ),
            Cond::Trig(t, not) => {
                let what = match t {
                    Trig::Ratio(a, b) => format!("{a}:{b}"),
                    Trig::Pre => "pre".to_string(),
                    Trig::First => "first".to_string(),
                    Trig::Last => "last".to_string(),
                };
                format!("?{}{what}", if not { "not" } else { "" })
            }
        }
    }
}

/// A classic note: pitches (none for a rest) and a length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Beat {
    pub pitches: Vec<Pitch>,
    /// The chord's name when written as one (`c:m7:2`), printed as written.
    pub symbol: Option<String>,
    /// 1, 2, 4, 8 or 16: a whole, half, quarter, eighth or sixteenth note.
    pub div: u8,
    pub dot: bool,
    /// `&`: as a mini word's, the note runs one tick into the next.
    pub slide: bool,
}

/// What the hits of a generator play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    /// Every hit plays this note.
    Pitch(Pitch),
    /// Hit by hit up the song's scale from this note.
    Walk(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seq {
    Mini(Vec<Slot>),
    Classic(Vec<Beat>),
    /// `euclid(5,8) c4`: hits spread over a bar.
    Euclid(Euclid, Fill),
    /// `arp(...)`, `walk(...)`, `markov(...)` or `mutate(...)`.
    Generated(Gen),
    /// `d5@0:6:90 …`: notes at their ticks (#173).
    Timed(Vec<Event>),
}

/// One note to play: where it starts in the loop, how long it lasts, in ticks.
#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub start: u32,
    pub len: u32,
    pub note: u8,
    pub accent: bool,
    /// A velocity of 1 to 127 from a timed note; 0 takes it from `accent`.
    pub vel: u8,
    /// The word of the line it came from, in `Notes::word_spans` order, so a
    /// view can light it while it plays (#205); 0 for a generated line.
    pub word: u16,
    /// `note` is a scale degree plus `DEGREE_ZERO`, until a scale resolves it.
    pub degree: bool,
    /// The bars of its scene it plays in (#603): every one by default.
    pub when: When,
}

/// Equal as music: which word an event came from is not part of it.
impl PartialEq for Event {
    fn eq(&self, o: &Event) -> bool {
        sort_key(self) == sort_key(o)
    }
}

impl Eq for Event {}

impl Event {
    pub fn velocity(&self) -> f32 {
        if self.vel > 0 {
            f32::from(self.vel.min(127)) / 127.0
        } else if self.accent {
            ACCENT_VELOCITY
        } else {
            HIT_VELOCITY
        }
    }
}

/// A compiled line: its text as parsed, the events it makes, sorted by start,
/// and how many bars before it repeats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notes {
    pub seq: Seq,
    /// `print()` of `seq`, kept so a view can read it without allocating.
    pub text: String,
    pub events: Vec<Event>,
    pub bars: u32,
}

/// Where and why a line is not notes; `col` counts chars from 1 in the text given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteError {
    pub col: usize,
    pub msg: &'static str,
}

fn err<T>(col: usize, msg: &'static str) -> Result<T, NoteError> {
    Err(NoteError { col, msg })
}

pub(crate) const NAMES: [&str; 12] = [
    "c", "c#", "d", "d#", "e", "f", "f#", "g", "g#", "a", "a#", "b",
];

/// The name of MIDI note `n`: `c4` is 60.
pub fn note_name(n: u8) -> String {
    let name = NAMES.get(usize::from(n % 12)).copied().unwrap_or("c");
    format!("{name}{}", i32::from(n / 12) - 1)
}

// --- Parsing ---------------------------------------------------------------

struct Cursor<'a> {
    c: &'a [char],
    i: usize,
    /// Column of `c[0]`.
    base: usize,
    items: usize,
    /// The song's key, for roman numerals.
    scale: Option<&'a Scale>,
}

impl Cursor<'_> {
    /// The end of the word at the cursor: the next space, or a char in `stop`.
    fn word_end(&self, stop: &[char]) -> usize {
        let mut j = self.i;
        while self
            .c
            .get(j)
            .is_some_and(|c| !c.is_whitespace() && !stop.contains(c))
        {
            j += 1;
        }
        j
    }

    /// A chord by name from the cursor to `end`.
    fn symbol(&mut self, end: usize) -> Result<Symbol, NoteError> {
        let w = self.c.get(self.i..end).unwrap_or(&[]);
        let pitches = chord::notes_of(w, self.col(), self.scale)?;
        let text = w.iter().collect();
        self.i = end;
        Ok(Symbol { text, pitches })
    }

    fn col(&self) -> usize {
        self.base + self.i
    }

    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }

    fn number(&mut self) -> Result<u32, NoteError> {
        let at = self.col();
        let mut n: u32 = 0;
        let mut any = false;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            n = n.saturating_mul(10).saturating_add(d);
            self.i += 1;
            any = true;
        }
        if !any {
            return err(at, "a number goes here");
        }
        Ok(n)
    }

    fn count_item(&mut self) -> Result<(), NoteError> {
        self.items += 1;
        if self.items > MAX_ITEMS {
            return err(self.col(), "a line has too many words");
        }
        Ok(())
    }

    /// `c4`, `c#4`, `eb3` or a degree `2`, `-1`, then an optional `!` for an
    /// accent.
    fn pitch(&mut self) -> Result<Pitch, NoteError> {
        let at = self.col();
        if self.peek().is_some_and(|c| c == '-' || c.is_ascii_digit()) {
            return self.degree();
        }
        let bad = "a note is a letter a to g, maybe # or b, and an octave 0 to 9, as c4";
        let letter = self.peek().ok_or(NoteError { col: at, msg: bad })?;
        let base: i32 = match letter {
            'c' => 0,
            'd' => 2,
            'e' => 4,
            'f' => 5,
            'g' => 7,
            'a' => 9,
            'b' => 11,
            _ => return err(at, bad),
        };
        self.i += 1;
        let mut semi = base;
        match self.peek() {
            Some('#') => {
                semi += 1;
                self.i += 1;
            }
            Some('b') => {
                semi -= 1;
                self.i += 1;
            }
            _ => {}
        }
        let Some(oct) = self.peek().and_then(|c| c.to_digit(10)) else {
            return err(self.col(), bad);
        };
        self.i += 1;
        let n = (i32::try_from(oct).unwrap_or(0) + 1) * 12 + semi;
        let Some(note) = u8::try_from(n).ok().filter(|n| *n <= 127) else {
            return err(at, "this note is out of range");
        };
        let accent = self.accent();
        Ok(Pitch {
            note,
            accent,
            degree: false,
        })
    }

    /// An optional `!` after a note.
    fn accent(&mut self) -> bool {
        let accent = self.peek() == Some('!');
        if accent {
            self.i += 1;
        }
        accent
    }

    /// A scale degree, `2` or `-1`, kept as a degree until a scale resolves it.
    fn degree(&mut self) -> Result<Pitch, NoteError> {
        let at = self.col();
        let minus = self.peek() == Some('-');
        if minus {
            self.i += 1;
        }
        let n = i32::try_from(self.number()?).unwrap_or(i32::MAX);
        if n > MAX_DEGREE {
            return err(at, "a degree is -24 to 24");
        }
        let d = if minus { -n } else { n };
        let note = u8::try_from(i32::from(DEGREE_ZERO) + d).unwrap_or(DEGREE_ZERO);
        let accent = self.accent();
        Ok(Pitch {
            note,
            accent,
            degree: true,
        })
    }

    /// `d5@0:6` or `d5!@0:6` or `d5@0:6:90`: a note at a tick, its length in
    /// ticks and maybe a velocity of 1 to 127.
    fn timed(&mut self) -> Result<Event, NoteError> {
        let p = self.pitch()?;
        let expect = |cur: &mut Cursor<'_>, c: char, msg: &'static str| {
            if cur.peek() == Some(c) {
                cur.i += 1;
                Ok(())
            } else {
                err(cur.col(), msg)
            }
        };
        expect(self, '@', "a timed note is note@tick:length, as d5@0:6")?;
        let at = self.col();
        let start = self.number()?;
        if start >= MAX_BARS * TICKS_PER_BAR {
            return err(at, "a timed note starts within 64 bars");
        }
        expect(self, ':', "a length in ticks goes after :, as d5@0:6")?;
        let at = self.col();
        let len = self.number()?;
        if len == 0 || len > MAX_BARS * TICKS_PER_BAR {
            return err(at, "a length is 1 to 3072 ticks");
        }
        let mut vel = 0;
        if self.peek() == Some(':') {
            self.i += 1;
            let at = self.col();
            vel = self.number()?;
            if !(1..=127).contains(&vel) {
                return err(at, "a velocity is 1 to 127");
            }
        }
        if self.peek().is_some_and(|c| !c.is_whitespace()) {
            return err(self.col(), "a word ends at a space");
        }
        Ok(Event {
            start,
            len,
            note: p.note,
            accent: p.accent,
            vel: u8::try_from(vel).unwrap_or(0),
            word: 0,
            degree: p.degree,
            when: Default::default(),
        })
    }

    /// The inside of `[ ]` that holds commas, after the `[`: a chord.
    fn chord(&mut self) -> Result<Vec<Pitch>, NoteError> {
        let mut out = vec![self.pitch()?];
        loop {
            match self.peek() {
                Some(',') => {
                    self.i += 1;
                    out.push(self.pitch()?);
                }
                Some(']') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some('(') => {
                    return err(self.col(), "euclid goes on the whole chord: [c4,e4](3,8)");
                }
                _ => return err(self.col(), "a chord is notes with commas, then ]"),
            }
        }
    }

    /// Is the `[` at the cursor a chord (a comma before its `]`)?
    fn is_chord(&self) -> bool {
        let mut depth = 0usize;
        let mut parens = 0usize;
        for ch in self.c.iter().skip(self.i) {
            match ch {
                '(' => parens += 1,
                ')' => parens = parens.saturating_sub(1),
                _ if parens > 0 => {}
                '[' | '<' => depth += 1,
                ']' | '>' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return false;
                    }
                }
                ',' if depth == 1 => return true,
                _ => {}
            }
        }
        false
    }

    /// Words up to `close` (or the end when `None`).
    fn slots(&mut self, close: Option<char>, depth: usize) -> Result<Vec<Slot>, NoteError> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            match (self.peek(), close) {
                (None, None) => return Ok(out),
                (None, Some(_)) => return err(self.col(), "a [ or < is not closed"),
                (Some(c), Some(want)) if c == want => {
                    self.i += 1;
                    if out.is_empty() {
                        return err(self.col() - 1, "nothing between the brackets");
                    }
                    return Ok(out);
                }
                _ if self.at_range() => {
                    let at = self.col();
                    let from = out.pop().and_then(|s| bare_degree(&s));
                    let Some(from) = from else {
                        return err(at, "a range runs from one degree to another: 0 .. 12");
                    };
                    self.i += 2;
                    self.skip_ws();
                    let end = self.slot(depth)?;
                    let Some(to) = bare_degree(&Slot {
                        times: 1,
                        weight: 1,
                        euclid: None,
                        cond: None,
                        slide: false,
                        ..end.clone()
                    }) else {
                        return err(at, "a range runs from one degree to another: 0 .. 12");
                    };
                    self.count_range(from, to, at)?;
                    out.push(Slot {
                        item: Item::Range(from, to),
                        ..end
                    });
                }
                _ => out.push(self.slot(depth)?),
            }
        }
    }

    /// Is the cursor at a `..` of its own, as in `0 .. 12`?
    fn at_range(&self) -> bool {
        self.c.get(self.i..self.i + 2) == Some(&['.', '.'][..])
            && self.c.get(self.i + 2).is_none_or(|c| c.is_whitespace())
    }

    /// A range of `from` to `to` counts a word for each of its degrees.
    fn count_range(&mut self, from: i32, to: i32, at: usize) -> Result<(), NoteError> {
        for _ in 0..from.abs_diff(to) {
            self.count_item()?;
        }
        if from.abs() > MAX_DEGREE || to.abs() > MAX_DEGREE {
            return err(at, "a degree is -24 to 24");
        }
        Ok(())
    }

    fn slot(&mut self, depth: usize) -> Result<Slot, NoteError> {
        self.count_item()?;
        let at = self.col();
        let item = match self.peek() {
            Some('~') => {
                self.i += 1;
                Item::Rest
            }
            Some('[') => {
                if depth >= MAX_DEPTH {
                    return err(at, "brackets nest at most 4 deep");
                }
                let chord = self.is_chord();
                self.i += 1;
                if chord {
                    Item::Chord(self.chord()?)
                } else {
                    Item::Group(self.slots(Some(']'), depth + 1)?)
                }
            }
            Some('<') => {
                if depth >= MAX_DEPTH {
                    return err(at, "brackets nest at most 4 deep");
                }
                self.i += 1;
                Item::Alt(self.slots(Some('>'), depth + 1)?)
            }
            Some(']' | '>') => return err(at, "this bracket closes nothing"),
            Some(':') => return err(at, "durations go outside the quotes, as c4:4"),
            _ => {
                let end = self.word_end(&['(', '*', '@', '?', '&', ']', '>']);
                let word = self.c.get(self.i..end).unwrap_or(&[]);
                if let Some(k) = word.windows(2).position(|w| w == ['.', '.']) {
                    // `0..12`, one word
                    let (a, b) = (
                        word.get(..k).unwrap_or(&[]),
                        word.get(k + 2..).unwrap_or(&[]),
                    );
                    let num = |w: &[char]| {
                        (is_degree(w) && w.last() != Some(&'!'))
                            .then(|| w.iter().collect::<String>().parse::<i32>().ok())
                            .flatten()
                    };
                    let (Some(from), Some(to)) = (num(a), num(b)) else {
                        return err(at, "a range runs from one degree to another: 0 .. 12");
                    };
                    self.count_range(from, to, at)?;
                    self.i = end;
                    Item::Range(from, to)
                } else if chord::is_note(word) || is_degree(word) {
                    Item::Note(self.pitch()?)
                } else {
                    Item::Symbol(self.symbol(end)?)
                }
            }
        };
        let euclid = self.inline_euclid()?;
        let (mut times, mut weight, mut cond, mut slide) = (1, 1, None, false);
        let (mut got_times, mut got_weight) = (false, false);
        loop {
            let here = self.col();
            match self.peek() {
                Some('*') if !got_times => {
                    self.i += 1;
                    times = self.factor(here, MAX_FACTOR)?;
                    got_times = true;
                }
                Some('@') if !got_weight => {
                    self.i += 1;
                    weight = self.factor(here, TICKS_PER_BAR)?;
                    got_weight = true;
                }
                Some('?') if cond.is_none() => {
                    self.i += 1;
                    cond = Some(self.cond()?);
                }
                Some('&') if !slide => {
                    if !matches!(item, Item::Note(_) | Item::Chord(_) | Item::Symbol(_)) {
                        return err(here, "only a note or a chord slides");
                    }
                    self.i += 1;
                    slide = true;
                }
                Some(':') => return err(here, "durations go outside the quotes, as c4:4"),
                Some('(') => return err(here, "euclid comes right after the word: c2(3,8)*2"),
                Some(c) if !c.is_whitespace() && !matches!(c, ']' | '>') => {
                    return err(here, "a word ends at a space");
                }
                _ => break,
            }
        }
        Ok(Slot {
            item,
            times,
            weight,
            euclid,
            cond,
            slide,
        })
    }

    /// `(3,8)` or `(3,8,2)` right after a word, if there.
    fn inline_euclid(&mut self) -> Result<Option<Euclid>, NoteError> {
        if self.peek() != Some('(') {
            return Ok(None);
        }
        let at = self.col();
        let close = self.c.iter().skip(self.i).position(|c| *c == ')');
        let Some(close) = close.map(|k| self.i + k) else {
            return err(at, "a ( is not closed: c2(3,8)");
        };
        let args: String = self
            .c
            .get(self.i + 1..=close)
            .unwrap_or(&[])
            .iter()
            .collect();
        let e = Euclid::args(&args).map_err(|msg| NoteError { col: at, msg })?;
        self.i = close + 1;
        Ok(Some(e))
    }

    /// What follows a `?`: nothing (a half), a chance `0.3` from 0 to 1, or
    /// a trig condition `1:4`, `pre`, `first`, `last`, maybe after `not`.
    fn cond(&mut self) -> Result<Cond, NoteError> {
        let at = self.col();
        let start = self.i;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '.' || c == ':')
        {
            self.i += 1;
        }
        let w: String = self.c.get(start..self.i).unwrap_or(&[]).iter().collect();
        if w.is_empty() {
            return Ok(Cond::Chance(None));
        }
        if w.starts_with(|c: char| c.is_ascii_digit() || c == '.') && !w.contains(':') {
            let decimals = w.split_once('.').map_or(0, |(_, d)| d.len());
            return match w.parse::<f64>() {
                Ok(p) if (0.0..=1.0).contains(&p) && decimals <= 3 => {
                    Ok(Cond::Chance(Some((p * 1000.0).round() as u16)))
                }
                _ => err(at, "a chance is 0 to 1, as ?0.3"),
            };
        }
        let (not, t) = w
            .strip_prefix("not")
            .map_or((false, w.as_str()), |t| (true, t));
        let trig = match t {
            "pre" => Trig::Pre,
            "first" => Trig::First,
            "last" => Trig::Last,
            _ => {
                let ratio = t.split_once(':').and_then(|(a, b)| {
                    let (a, b) = (a.parse::<u8>().ok()?, b.parse::<u8>().ok()?);
                    (1..=b).contains(&a).then_some(()).filter(|()| b <= 16)?;
                    Some(Trig::Ratio(a, b))
                });
                ratio.ok_or(NoteError {
                    col: at,
                    msg: "a condition is ?0.3, ?1:4 (1 ≤ a ≤ b ≤ 16), ?pre, ?first or ?last, \
                          or ?not before one of the last four",
                })?
            }
        };
        Ok(Cond::Trig(trig, not))
    }

    fn factor(&mut self, at: usize, max: u32) -> Result<u32, NoteError> {
        let n = self.number()?;
        if n == 0 || n > max {
            return err(
                at,
                if max == MAX_FACTOR {
                    "a repeat is 1 to 16"
                } else {
                    "a weight is 1 to 48"
                },
            );
        }
        Ok(n)
    }

    /// A classic note: `c4:4`, `[c4,e4]:2`, `c:m7:2`, `V7:4` or `r:8`, with a
    /// dot for a dotted note. The last `:` is the duration's.
    fn beat(&mut self) -> Result<Beat, NoteError> {
        self.count_item()?;
        let at = self.col();
        let mut symbol = None;
        let pitches = match self.peek() {
            Some('r') => {
                self.i += 1;
                Vec::new()
            }
            Some('[') => {
                self.i += 1;
                self.chord()?
            }
            Some('"' | '~' | '<' | '*' | '@' | '?' | '&') => {
                return err(
                    at,
                    "mini-notation goes inside quotes, classic notes outside",
                );
            }
            _ => {
                let end = self.word_end(&[]);
                let word = self.c.get(self.i..end).unwrap_or(&[]);
                let head = word
                    .iter()
                    .rposition(|c| *c == ':')
                    .map_or(end, |k| self.i + k);
                let what = self.c.get(self.i..head).unwrap_or(&[]);
                if chord::is_note(what) || is_degree(what) || head == self.i {
                    vec![self.pitch()?]
                } else {
                    let s = self.symbol(head)?;
                    symbol = Some(s.text);
                    s.pitches
                }
            }
        };
        if self.peek() != Some(':') {
            return err(self.col(), "a classic note has a duration, as c4:4");
        }
        self.i += 1;
        let dat = self.col();
        let div = match self.number()? {
            1 => 1,
            2 => 2,
            4 => 4,
            8 => 8,
            16 => 16,
            _ => return err(dat, "a duration is 1, 2, 4, 8 or 16"),
        };
        let dot = self.peek() == Some('.');
        if dot {
            self.i += 1;
            if div == 16 {
                return err(dat, "a dotted sixteenth does not fit the grid");
            }
        }
        let slide = self.peek() == Some('&');
        if slide {
            if pitches.is_empty() {
                return err(self.col(), "only a note or a chord slides");
            }
            self.i += 1;
        }
        if self.peek().is_some_and(|c| !c.is_whitespace()) {
            return err(self.col(), "a word ends at a space");
        }
        Ok(Beat {
            pitches,
            symbol,
            div,
            dot,
            slide,
        })
    }
}

/// Parse one line of notes with no scale in force.
pub fn parse(text: &str, base: usize) -> Result<Notes, NoteError> {
    parse_in(text, base, None)
}

/// `euclid(5,8) c4` or `euclid(5,8,2) scale c4`.
fn parse_euclid(cur: &mut Cursor<'_>, scale: Option<&Scale>) -> Result<Option<Seq>, NoteError> {
    let at = cur.col();
    let start = cur.i;
    while cur.peek().is_some_and(|c| !c.is_whitespace()) {
        cur.i += 1;
    }
    let word: String = cur.c.get(start..cur.i).unwrap_or(&[]).iter().collect();
    let Some(call) = Euclid::parse(&word) else {
        cur.i = start;
        return Ok(None);
    };
    let e = call.map_err(|msg| NoteError { col: at, msg })?;
    cur.skip_ws();
    let fill_at = cur.col();
    let walk = cur
        .c
        .get(cur.i..cur.i + 5)
        .is_some_and(|w| w.iter().collect::<String>() == "scale")
        && cur.c.get(cur.i + 5).is_some_and(|c| c.is_whitespace());
    let fill = if walk {
        cur.i += 5;
        cur.skip_ws();
        if scale.is_none() {
            return err(fill_at, "a scale walk needs a scale line before it");
        }
        let p = cur.pitch()?;
        if p.degree {
            return err(fill_at, "a walk starts from a note, as c4");
        }
        Fill::Walk(p.note)
    } else {
        if cur.peek().is_none() {
            return err(fill_at, "a note goes after the call: euclid(3,8) c4");
        }
        Fill::Pitch(cur.pitch()?)
    };
    cur.skip_ws();
    if cur.peek().is_some() {
        return err(cur.col(), "a word ends at a space");
    }
    Ok(Some(Seq::Euclid(e, fill)))
}

/// Parse one line of notes. `base` is the column of the first char of `text`;
/// `scale` is the song's, for walks.
pub fn parse_in(text: &str, base: usize, scale: Option<&Scale>) -> Result<Notes, NoteError> {
    parse_with(text, base, scale, &|_| None)
}

/// As `parse_in`, with `srcs` to find the note clips `markov` and
/// `mutate` read.
pub fn parse_with(
    text: &str,
    base: usize,
    scale: Option<&Scale>,
    srcs: &dyn Fn(&str) -> Option<Source>,
) -> Result<Notes, NoteError> {
    let chars: Vec<char> = text.chars().collect();
    let mut cur = Cursor {
        c: &chars,
        i: 0,
        base,
        items: 0,
        scale,
    };
    cur.skip_ws();
    let seq = if let Some(seq) = parse_euclid(&mut cur, scale)? {
        seq
    } else if let Some(g) = generate::parse_call(&mut cur, scale, srcs)? {
        Seq::Generated(g)
    } else if cur.peek() == Some('"') {
        cur.i += 1;
        let mut inner_end = cur.i;
        while chars.get(inner_end).is_some_and(|c| *c != '"') {
            inner_end += 1;
        }
        if inner_end >= chars.len() {
            return err(base + chars.len(), "the quote is not closed");
        }
        let inner = chars.get(cur.i..inner_end).unwrap_or(&[]);
        let mut sub = Cursor {
            c: inner,
            i: 0,
            base: cur.col(),
            items: 0,
            scale,
        };
        let slots = sub.slots(None, 0)?;
        cur.i = inner_end + 1;
        cur.skip_ws();
        if cur.peek().is_some() {
            return err(cur.col(), "one quoted sequence to a line");
        }
        if slots.is_empty() {
            return err(base + chars.len(), "a sequence needs a word");
        }
        Seq::Mini(slots)
    } else if chars.contains(&'@') {
        let mut events = Vec::new();
        loop {
            cur.skip_ws();
            if cur.peek().is_none() {
                break;
            }
            if events.len() >= MAX_EVENTS {
                return err(cur.col(), "a line has at most 512 notes");
            }
            events.push(cur.timed()?);
        }
        Seq::Timed(events)
    } else {
        let mut beats = Vec::new();
        loop {
            cur.skip_ws();
            if cur.peek().is_none() {
                break;
            }
            beats.push(cur.beat()?);
        }
        if beats.is_empty() {
            return err(base, "a line of notes goes here");
        }
        Seq::Classic(beats)
    };
    compile(seq, scale).map_err(|msg| NoteError { col: base, msg })
}

impl Notes {
    /// A timed line made `bars` bars long (`clip … bars N`): at least as long
    /// as its last start, at most 64 bars. Other lines keep their own length.
    pub fn with_bars(mut self, bars: u32) -> Result<Notes, &'static str> {
        if !matches!(self.seq, Seq::Timed(_)) {
            return Err("bars N is for a line of timed notes");
        }
        if bars < self.bars || bars > MAX_BARS {
            return Err("the bars hold the line's notes, and are at most 64");
        }
        self.bars = bars;
        Ok(self)
    }

    /// Each chord moved to the inversion nearest the one before (#103): the
    /// clip line's `voicing`. The text stays as written.
    pub fn voiced(mut self) -> Notes {
        chord::voice(&mut self.events);
        self
    }

    fn new(seq: Seq, events: Vec<Event>, bars: u32) -> Notes {
        let mut n = Notes {
            seq,
            text: String::new(),
            events,
            bars,
        };
        n.text = n.print();
        n
    }
}

// --- Printing --------------------------------------------------------------

fn pitch_text(p: &Pitch) -> String {
    let what = if p.degree {
        (i32::from(p.note) - i32::from(DEGREE_ZERO)).to_string()
    } else {
        note_name(p.note)
    };
    format!("{what}{}", if p.accent { "!" } else { "" })
}

/// The degree a slot is when it is only that: no accent, no suffix.
fn bare_degree(s: &Slot) -> Option<i32> {
    match s.item {
        Item::Note(p)
            if p.degree
                && !p.accent
                && s.times == 1
                && s.weight == 1
                && s.euclid.is_none()
                && s.cond.is_none()
                && !s.slide =>
        {
            Some(i32::from(p.note) - i32::from(DEGREE_ZERO))
        }
        _ => None,
    }
}

/// Is `w` a degree: maybe `-`, digits, maybe `!`?
fn is_degree(w: &[char]) -> bool {
    let w = w.strip_prefix(&['-']).unwrap_or(w);
    let w = w.strip_suffix(&['!']).unwrap_or(w);
    !w.is_empty() && w.iter().all(char::is_ascii_digit)
}

fn chord_text(ps: &[Pitch]) -> String {
    let inner: Vec<String> = ps.iter().map(pitch_text).collect();
    format!("[{}]", inner.join(","))
}

fn slots_text(slots: &[Slot]) -> String {
    let parts: Vec<String> = slots.iter().map(slot_text).collect();
    parts.join(" ")
}

fn slot_text(s: &Slot) -> String {
    let mut out = match &s.item {
        Item::Rest => "~".to_string(),
        Item::Note(p) => pitch_text(p),
        Item::Chord(ps) => chord_text(ps),
        Item::Symbol(sym) => sym.text.clone(),
        Item::Group(g) => format!("[{}]", slots_text(g)),
        Item::Alt(g) => format!("<{}>", slots_text(g)),
        Item::Range(a, b) => format!("{a} .. {b}"),
    };
    if let Some(e) = s.euclid {
        out.push_str(e.print().trim_start_matches("euclid"));
    }
    if s.slide {
        out.push('&');
    }
    if s.times != 1 {
        out.push_str(&format!("*{}", s.times));
    }
    if s.weight != 1 {
        out.push_str(&format!("@{}", s.weight));
    }
    if let Some(c) = s.cond {
        out.push_str(&c.print());
    }
    out
}

/// Where each word of `slots` sits in `slots_text(slots)` from `at`, in print
/// order: the word itself, without its suffixes.
fn mini_spans(slots: &[Slot], mut at: usize, out: &mut Vec<(usize, usize)>) {
    for (k, s) in slots.iter().enumerate() {
        if k > 0 {
            at += 1;
        }
        match &s.item {
            Item::Group(g) | Item::Alt(g) => mini_spans(g, at + 1, out),
            Item::Rest => out.push((at, 1)),
            Item::Note(p) => out.push((at, pitch_text(p).len())),
            Item::Chord(ps) => out.push((at, chord_text(ps).len())),
            Item::Symbol(sym) => out.push((at, sym.text.len())),
            Item::Range(a, b) => out.push((at, format!("{a} .. {b}").len())),
        }
        at += slot_text(s).len();
    }
}

impl Notes {
    /// Where each word of `text` is, as (byte offset, length), in the order
    /// `Event::word` counts them (#205). A generated line is one word: all of it.
    pub fn word_spans(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        match &self.seq {
            Seq::Mini(slots) => mini_spans(slots, 1, &mut out),
            Seq::Classic(_) | Seq::Timed(_) => {
                let mut at = 0;
                for word in self.text.split(' ') {
                    // A classic word lights up to its length, `c4` of `c4:8`.
                    let what = word
                        .find(':')
                        .filter(|_| matches!(self.seq, Seq::Classic(_)));
                    out.push((at, what.unwrap_or(word.len())));
                    at += word.len() + 1;
                }
            }
            Seq::Generated(_) | Seq::Euclid(..) => out.push((0, self.text.len())),
        }
        out
    }

    /// The canonical text of the line: parsing it gives these notes back.
    pub fn print(&self) -> String {
        match &self.seq {
            Seq::Mini(slots) => format!("\"{}\"", slots_text(slots)),
            Seq::Generated(g) => g.print(),
            Seq::Euclid(e, Fill::Pitch(p)) => format!("{} {}", e.print(), pitch_text(p)),
            Seq::Euclid(e, Fill::Walk(n)) => format!("{} scale {}", e.print(), note_name(*n)),
            Seq::Timed(events) => {
                let words: Vec<String> = events
                    .iter()
                    .map(|e| {
                        let p = pitch_text(&Pitch {
                            note: e.note,
                            accent: e.accent,
                            degree: e.degree,
                        });
                        if e.vel > 0 {
                            format!("{p}@{}:{}:{}", e.start, e.len, e.vel)
                        } else {
                            format!("{p}@{}:{}", e.start, e.len)
                        }
                    })
                    .collect();
                words.join(" ")
            }
            Seq::Classic(beats) => {
                let parts: Vec<String> = beats
                    .iter()
                    .map(|b| {
                        let what = match (&b.symbol, b.pitches.as_slice()) {
                            (Some(sym), _) => sym.clone(),
                            (None, []) => "r".to_string(),
                            (None, [p]) => pitch_text(p),
                            (None, ps) => chord_text(ps),
                        };
                        format!(
                            "{what}:{}{}{}",
                            b.div,
                            if b.dot { "." } else { "" },
                            if b.slide { "&" } else { "" }
                        )
                    })
                    .collect();
                parts.join(" ")
            }
        }
    }
}

// --- Compiling -------------------------------------------------------------

/// A point or span of a bar as a fraction.
#[derive(Clone, Copy)]
struct Frac {
    n: u128,
    d: u128,
}

fn gcd(a: u128, b: u128) -> u128 {
    if b == 0 { a } else { gcd(b, a % b) }
}

impl Frac {
    fn new(n: u128, d: u128) -> Frac {
        let g = gcd(n, d).max(1);
        Frac { n: n / g, d: d / g }
    }

    fn add(self, o: Frac) -> Frac {
        Frac::new(self.n * o.d + o.n * self.d, self.d * o.d)
    }

    fn scale(self, num: u128, den: u128) -> Frac {
        Frac::new(self.n * num, self.d * den)
    }

    /// The tick it falls on, within a bar.
    fn tick(self) -> u32 {
        u32::try_from(self.n * u128::from(TICKS_PER_BAR) / self.d.max(1)).unwrap_or(u32::MAX)
    }
}

struct Compiler<'a> {
    events: Vec<Event>,
    rng: Rng,
    /// The last condition of the bar, for `?pre`.
    prev: Prev,
    /// The engine's conditions on the words being compiled.
    when: When,
    full: bool,
    /// The line's words in print order, to tag each event with its own.
    leaves: Vec<&'a Item>,
    /// The word being compiled.
    word: u16,
}

/// The words of `slots` (notes, chords, names, rests), in the order they print.
fn leaves_of<'a>(slots: &'a [Slot], out: &mut Vec<&'a Item>) {
    for s in slots {
        match &s.item {
            Item::Group(g) | Item::Alt(g) => leaves_of(g, out),
            leaf => out.push(leaf),
        }
    }
}

impl Compiler<'_> {
    fn slots(&mut self, slots: &[Slot], at: Frac, span: Frac, bar: u32) {
        let total: u128 = slots.iter().map(|s| u128::from(s.weight)).sum();
        let mut before = 0u128;
        for s in slots {
            let w = u128::from(s.weight);
            let start = at.add(span.scale(before, total.max(1)));
            self.slot(s, start, span.scale(w, total.max(1)), bar);
            before += w;
        }
    }

    fn slot(&mut self, s: &Slot, at: Frac, span: Frac, bar: u32) {
        let outer = self.when;
        let plays = match s.cond {
            None => Prev::Known(true),
            Some(Cond::Chance(p)) => Prev::Known(Cond::plays(p, self.rng.next_u32())),
            Some(Cond::Trig(t, not)) => {
                let went = match t {
                    Trig::Ratio(a, b) => Prev::Known(bar % u32::from(b) + 1 == u32::from(a)),
                    Trig::Pre => self.prev,
                    Trig::First => Prev::Runtime(When {
                        first: Some(true),
                        last: None,
                    }),
                    Trig::Last => Prev::Runtime(When {
                        first: None,
                        last: Some(true),
                    }),
                };
                match went {
                    Prev::Known(b) => Prev::Known(b != not),
                    Prev::Runtime(w) => Prev::Runtime(if not { w.not() } else { w }),
                }
            }
        };
        // `?pre` reads the conditions before it and leaves them be.
        if s.cond.is_some() && !matches!(s.cond, Some(Cond::Trig(Trig::Pre, _))) {
            self.prev = plays;
        }
        match plays {
            Prev::Known(false) => return,
            Prev::Known(true) => {}
            Prev::Runtime(w) => match outer.and(w) {
                Some(both) => self.when = both,
                None => return,
            },
        }
        let times = u128::from(s.times);
        for r in 0..times {
            let (start, share) = (at.add(span.scale(r, times)), span.scale(1, times));
            match s.euclid {
                None => self.item(&s.item, start, share, bar, s.slide),
                Some(e) => {
                    let n = u128::from(e.n);
                    for (i, hit) in (0..n).zip(e.pattern()) {
                        if hit {
                            let step = start.add(share.scale(i, n));
                            self.item(&s.item, step, share.scale(1, n), bar, s.slide);
                        }
                    }
                }
            }
        }
        self.when = outer;
    }

    fn item(&mut self, item: &Item, at: Frac, span: Frac, bar: u32, slide: bool) {
        if let Some(w) = self.leaves.iter().position(|l| std::ptr::eq(*l, item)) {
            self.word = u16::try_from(w).unwrap_or(u16::MAX);
        }
        match item {
            Item::Rest => {}
            Item::Note(p) => self.note(*p, at, span, bar, slide),
            Item::Chord(ps) | Item::Symbol(Symbol { pitches: ps, .. }) => {
                for p in ps {
                    self.note(*p, at, span, bar, slide);
                }
            }
            Item::Group(g) => self.slots(g, at, span, bar),
            Item::Range(a, b) => {
                let n = u128::from(a.abs_diff(*b)) + 1;
                let step = if b < a { -1 } else { 1 };
                for k in 0..n {
                    let d = a + step * i32::try_from(k).unwrap_or(0);
                    let p = Pitch {
                        note: u8::try_from(i32::from(DEGREE_ZERO) + d).unwrap_or(DEGREE_ZERO),
                        accent: false,
                        degree: true,
                    };
                    self.note(p, at.add(span.scale(k, n)), span.scale(1, n), bar, slide);
                }
            }
            Item::Alt(g) => {
                let n = u32::try_from(g.len()).unwrap_or(1).max(1);
                let pick = usize::try_from(bar % n).unwrap_or(0);
                if let Some(s) = g.get(pick) {
                    self.slot(s, at, span, bar);
                }
            }
        }
    }

    fn note(&mut self, p: Pitch, at: Frac, span: Frac, bar: u32, slide: bool) {
        if self.events.len() >= MAX_EVENTS {
            self.full = true;
            return;
        }
        let start = at.tick();
        let end = at.add(span).tick().max(start + 1);
        self.events.push(Event {
            start: bar * TICKS_PER_BAR + start,
            len: end - start + u32::from(slide),
            note: p.note,
            accent: p.accent,
            vel: 0,
            word: self.word,
            degree: p.degree,
            when: self.when,
        });
    }
}

/// Each degree of `events` made a note of `scale`, degree 0 being `root`.
pub fn resolve(events: &mut [Event], scale: &Scale, root: u8) {
    for e in events.iter_mut().filter(|e| e.degree) {
        e.note = scale.degree_note(root, i32::from(e.note) - i32::from(DEGREE_ZERO));
        e.degree = false;
    }
}

/// The order events are kept in: a total order, so an unstable sort (which
/// allocates nothing) gives the same list every time.
pub(crate) fn sort_key(e: &Event) -> (u32, bool, u8, u32, bool, u8, When) {
    (e.start, e.degree, e.note, e.len, e.accent, e.vel, e.when)
}

/// A line of mini-notation that plays exactly `events` (sorted, within
/// `bars` bars): every word weighs its length in ticks, a bar to a group when
/// there is more than one. `None` when the events do not fit that way: a
/// note over a bar line, overlapping notes, or a chord of different lengths.
pub fn freeze(events: &[Event], bars: u32) -> Option<Notes> {
    let bar = TICKS_PER_BAR;
    if events.iter().any(|e| e.when != When::default()) {
        return None;
    }
    let mut texts = Vec::new();
    for b in 0..bars {
        let (lo, hi) = (b * bar, (b + 1) * bar);
        let mut words: Vec<String> = Vec::new();
        let mut cursor = lo;
        let mut i = 0;
        let in_bar: Vec<&Event> = events
            .iter()
            .filter(|e| e.start >= lo && e.start < hi)
            .collect();
        while let Some(first) = in_bar.get(i) {
            let same = in_bar.iter().skip(i).take_while(|e| e.start == first.start);
            let chord: Vec<&&Event> = same.collect();
            if chord.iter().any(|e| e.len != first.len)
                || first.start < cursor
                || first.start + first.len > hi
            {
                return None;
            }
            if first.start > cursor {
                words.push(weighted("~".into(), first.start - cursor));
            }
            let ps: Vec<String> = chord
                .iter()
                .map(|e| {
                    pitch_text(&Pitch {
                        note: e.note,
                        accent: e.accent,
                        degree: e.degree,
                    })
                })
                .collect();
            let item = if let [one] = ps.as_slice() {
                one.clone()
            } else {
                format!("[{}]", ps.join(","))
            };
            words.push(weighted(item, first.len));
            cursor = first.start + first.len;
            i += chord.len();
        }
        if cursor < hi {
            words.push(weighted("~".into(), hi - cursor));
        }
        texts.push(words.join(" "));
    }
    if events.iter().any(|e| e.start >= bars * bar) {
        return None;
    }
    let line = match texts.as_slice() {
        [one] => format!("\"{one}\""),
        many => {
            let groups: Vec<String> = many.iter().map(|t| format!("[{t}]")).collect();
            format!("\"<{}>\"", groups.join(" "))
        }
    };
    let notes = parse(&line, 1).ok()?;
    (notes.events == events && notes.bars == bars).then_some(notes)
}

/// An edit from the composer's note view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// A note of the default length (a sixteenth) at `tick`.
    Add {
        tick: u32,
        note: u8,
    },
    Remove {
        tick: u32,
        note: u8,
    },
    /// Set the length of the notes that start with this one.
    Len {
        tick: u32,
        note: u8,
        len: u32,
    },
}

/// A sixteenth, the length of a note added by a click.
pub const DEFAULT_LEN: u32 = 3;

/// `events` with `op` done, made writable: notes that start together are a
/// chord of one length, and a note ends where the next one begins or at its
/// bar line, so an edit shortens what is in its way. `None` when the edit
/// names no note or a tick past `bars`.
/// `events` as a line of timed notes `bars` bars long (#173): what an edit of
/// a timed line gives back, overlaps and velocities kept.
pub fn timed(events: Vec<Event>, bars: u32) -> Option<Notes> {
    if events.len() > MAX_EVENTS {
        return None;
    }
    compile(Seq::Timed(events), None).ok()?.with_bars(bars).ok()
}

/// An edit of a line of timed notes: notes may overlap, so nothing else is
/// shortened to make room, and a length changes only the note named. `None`
/// when the edit names no note or falls outside the line.
pub fn edit_timed(events: &[Event], bars: u32, op: Edit) -> Option<Vec<Event>> {
    let mut out = events.to_vec();
    match op {
        Edit::Add { tick, note } => {
            if tick >= bars * TICKS_PER_BAR || note > 127 {
                return None;
            }
            if !out.iter().any(|e| e.start == tick && e.note == note) {
                out.push(Event {
                    start: tick,
                    len: DEFAULT_LEN,
                    note,
                    accent: false,
                    vel: 0,
                    word: 0,
                    degree: false,
                    when: Default::default(),
                });
            }
        }
        Edit::Remove { tick, note } => {
            let at = out.iter().position(|e| e.start == tick && e.note == note)?;
            out.remove(at);
        }
        Edit::Len { tick, note, len } => {
            let e = out.iter_mut().find(|e| e.start == tick && e.note == note)?;
            if len == 0 || len > MAX_BARS * TICKS_PER_BAR {
                return None;
            }
            e.len = len;
        }
    }
    Some(out)
}

pub fn edit(events: &[Event], bars: u32, op: Edit) -> Option<Vec<Event>> {
    let mut out = events.to_vec();
    let end = bars * TICKS_PER_BAR;
    match op {
        Edit::Add { tick, note } => {
            if tick >= end || note > 127 {
                return None;
            }
            if !out.iter().any(|e| e.start == tick && e.note == note) {
                let len = out
                    .iter()
                    .find(|e| e.start == tick)
                    .map_or(DEFAULT_LEN, |e| e.len);
                out.push(Event {
                    start: tick,
                    len,
                    note,
                    accent: false,
                    vel: 0,
                    word: 0,
                    degree: false,
                    when: Default::default(),
                });
            }
        }
        Edit::Remove { tick, note } => {
            let at = out.iter().position(|e| e.start == tick && e.note == note)?;
            out.remove(at);
        }
        Edit::Len { tick, note, len } => {
            if len == 0 || !out.iter().any(|e| e.start == tick && e.note == note) {
                return None;
            }
            for e in out.iter_mut().filter(|e| e.start == tick) {
                e.len = len;
            }
        }
    }
    out.sort_unstable_by_key(sort_key);
    let starts: Vec<u32> = out.iter().map(|e| e.start).collect();
    for e in out.iter_mut() {
        let bar_end = (e.start / TICKS_PER_BAR + 1) * TICKS_PER_BAR;
        let next = starts
            .iter()
            .copied()
            .find(|s| *s > e.start)
            .unwrap_or(bar_end);
        e.len = e.len.min(next.min(bar_end) - e.start).max(1);
    }
    Some(out)
}

fn weighted(item: String, ticks: u32) -> String {
    if ticks == 1 {
        item
    } else {
        format!("{item}@{ticks}")
    }
}

fn lcm(a: u32, b: u32) -> u32 {
    let g = u32::try_from(gcd(u128::from(a), u128::from(b)))
        .unwrap_or(1)
        .max(1);
    a / g * b
}

/// How many bars before an alternation or a chance repeats.
fn period(slots: &[Slot]) -> u32 {
    let mut p = 1;
    for s in slots {
        let inner = match &s.item {
            Item::Group(g) => period(g),
            Item::Alt(g) => lcm(u32::try_from(g.len()).unwrap_or(1), period(g)),
            _ => 1,
        };
        p = lcm(p, inner);
        match s.cond {
            Some(Cond::Chance(_)) => p = lcm(p, CHANCE_BARS),
            Some(Cond::Trig(Trig::Ratio(_, b), _)) => p = lcm(p, u32::from(b)),
            _ => {}
        }
        if p > MAX_BARS {
            return p;
        }
    }
    p
}

fn classic_ticks(b: &Beat) -> u32 {
    let base = TICKS_PER_BAR / u32::from(b.div).max(1);
    if b.dot { base * 3 / 2 } else { base }
}

fn compile(seq: Seq, scale: Option<&Scale>) -> Result<Notes, &'static str> {
    // A timed line is kept in event order, so it prints canonically.
    let seq = match seq {
        Seq::Timed(mut events) => {
            events.sort_unstable_by_key(sort_key);
            Seq::Timed(events)
        }
        other => other,
    };
    let (events, bars) = match &seq {
        Seq::Timed(events) => {
            let last = events.iter().map(|e| e.start).max().unwrap_or(0);
            let words = events.iter().enumerate().map(|(w, e)| Event {
                word: u16::try_from(w).unwrap_or(u16::MAX),
                ..*e
            });
            (words.collect(), last / TICKS_PER_BAR + 1)
        }
        Seq::Mini(slots) => {
            let bars = period(slots);
            if bars > MAX_BARS {
                return Err("this line takes more than 64 bars to repeat");
            }
            let mut leaves = Vec::new();
            leaves_of(slots, &mut leaves);
            let mut c = Compiler {
                events: Vec::new(),
                rng: Rng::new(0),
                prev: Prev::Known(false),
                when: When::default(),
                full: false,
                leaves,
                word: 0,
            };
            for bar in 0..bars {
                c.rng = Rng::new(mix(bar, 0x5EED));
                c.prev = Prev::Known(false);
                c.slots(slots, Frac::new(0, 1), Frac::new(1, 1), bar);
            }
            if c.full {
                return Err("this line has too many notes");
            }
            (c.events, bars)
        }
        Seq::Generated(g) => (g.events(g.seed(), scale), g.bars()),
        Seq::Euclid(e, fill) => {
            let mut events = Vec::new();
            let n = u128::from(e.n);
            let mut hit = 0u32;
            for (i, on) in e.pattern().into_iter().enumerate() {
                if !on {
                    continue;
                }
                let start = Frac::new(i as u128, n).tick();
                let end = Frac::new(i as u128 + 1, n).tick().max(start + 1);
                let (note, accent, degree) = match fill {
                    Fill::Pitch(p) => (p.note, p.accent, p.degree),
                    Fill::Walk(from) => (scale.map_or(*from, |s| s.walk(*from, hit)), false, false),
                };
                hit += 1;
                events.push(Event {
                    start,
                    len: end - start,
                    note,
                    accent,
                    vel: 0,
                    word: 0,
                    degree,
                    when: Default::default(),
                });
            }
            (events, 1)
        }
        Seq::Classic(beats) => {
            let mut events = Vec::new();
            let mut at = 0u32;
            for (w, b) in beats.iter().enumerate() {
                let len = classic_ticks(b);
                for p in &b.pitches {
                    events.push(Event {
                        start: at,
                        len: len + u32::from(b.slide),
                        note: p.note,
                        accent: p.accent,
                        vel: 0,
                        word: u16::try_from(w).unwrap_or(u16::MAX),
                        degree: p.degree,
                        when: Default::default(),
                    });
                }
                at += len;
                if events.len() > MAX_EVENTS {
                    return Err("this line has too many notes");
                }
            }
            let bars = at.div_ceil(TICKS_PER_BAR).max(1);
            if bars > MAX_BARS {
                return Err("a line is at most 64 bars long");
            }
            (events, bars)
        }
    };
    let mut events = events;
    events.sort_unstable_by_key(sort_key);
    Ok(Notes::new(seq, events, bars))
}

#[cfg(test)]
mod tests;
