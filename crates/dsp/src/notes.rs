//! Note fragments (spec 002 Req 3, ADR-0016): the text of a pitched line,
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
//! word on half the bars (drawn from a fixed seed, so a run is repeatable),
//! `[c4,e4,g4]` is a chord and `!` accents a note. Classic: `c4:4` is a
//! quarter, `:1 :2 :4 :8 :16` with a dot for one and a half, and `r:4` rests.
//!
//! A third form, timed notes, says exactly where each note is (#173, the
//! form MIDI import writes): `d5@0:6 f#5@6:6:90 a4@12:24` is a note at tick
//! 0 for 6 ticks, one at 6 with velocity 90 of 127, one at 12 for a half bar.
//! Notes may overlap and run past the line's end; the frag line may say how
//! many bars the line is (`bars 8`), else it ends at the bar of its last start.
//!
//! Parsing and compiling allocate and happen when a song is loaded, never in
//! `render`. Both are total: bad text is an error with a column.

mod chord;
mod generate;

use generate::Source;
pub use generate::{ArpMode, Gen};

use crate::algo::{Euclid, Rng, Scale, mix};

/// Ticks in a bar of four quarters.
pub const TICKS_PER_BAR: u32 = 48;
/// Most events a fragment may compile to, and most bars it may run before it
/// repeats.
pub const MAX_EVENTS: usize = 512;
pub const MAX_BARS: u32 = 32;
/// Most words (slots and notes) in one line.
const MAX_ITEMS: usize = 1100;
/// A line with `?` repeats after this many bars.
const CHANCE_BARS: u32 = 8;
/// Nesting of `[ ]` and `< >`.
const MAX_DEPTH: usize = 4;
const MAX_FACTOR: u32 = 16;
/// Velocities, as drums have them.
pub const HIT_VELOCITY: f32 = 0.75;
pub const ACCENT_VELOCITY: f32 = 1.0;

/// A note of the line: a MIDI number and whether it is accented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pitch {
    pub note: u8,
    pub accent: bool,
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
    /// `?`: plays on some bars only.
    pub chance: bool,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub start: u32,
    pub len: u32,
    pub note: u8,
    pub accent: bool,
    /// A velocity of 1 to 127 from a timed note; 0 takes it from `accent`.
    pub vel: u8,
}

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

    /// `c4`, `c#4`, `eb3`, then an optional `!` for an accent.
    fn pitch(&mut self) -> Result<Pitch, NoteError> {
        let at = self.col();
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
        let accent = self.peek() == Some('!');
        if accent {
            self.i += 1;
        }
        Ok(Pitch { note, accent })
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
            return err(at, "a timed note starts within 32 bars");
        }
        expect(self, ':', "a length in ticks goes after :, as d5@0:6")?;
        let at = self.col();
        let len = self.number()?;
        if len == 0 || len > MAX_BARS * TICKS_PER_BAR {
            return err(at, "a length is 1 to 1536 ticks");
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
                _ => return err(self.col(), "a chord is notes with commas, then ]"),
            }
        }
    }

    /// Is the `[` at the cursor a chord (a comma before its `]`)?
    fn is_chord(&self) -> bool {
        let mut depth = 0usize;
        for ch in self.c.iter().skip(self.i) {
            match ch {
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
                _ => out.push(self.slot(depth)?),
            }
        }
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
                let end = self.word_end(&['*', '@', '?', ']', '>']);
                if chord::is_note(self.c.get(self.i..end).unwrap_or(&[])) {
                    Item::Note(self.pitch()?)
                } else {
                    Item::Symbol(self.symbol(end)?)
                }
            }
        };
        let (mut times, mut weight, mut chance) = (1, 1, false);
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
                Some('?') if !chance => {
                    self.i += 1;
                    chance = true;
                }
                Some(':') => return err(here, "durations go outside the quotes, as c4:4"),
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
            chance,
        })
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
            Some('"' | '~' | '<' | '*' | '@' | '?') => {
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
                if chord::is_note(self.c.get(self.i..head).unwrap_or(&[])) || head == self.i {
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
        if self.peek().is_some_and(|c| !c.is_whitespace()) {
            return err(self.col(), "a word ends at a space");
        }
        Ok(Beat {
            pitches,
            symbol,
            div,
            dot,
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

/// As `parse_in`, with `srcs` to find the note fragments `markov` and
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
    /// A timed line made `bars` bars long (`frag … bars N`): at least as long
    /// as its last start, at most 32 bars. Other lines keep their own length.
    pub fn with_bars(mut self, bars: u32) -> Result<Notes, &'static str> {
        if !matches!(self.seq, Seq::Timed(_)) {
            return Err("bars N is for a line of timed notes");
        }
        if bars < self.bars || bars > MAX_BARS {
            return Err("the bars hold the line's notes, and are at most 32");
        }
        self.bars = bars;
        Ok(self)
    }

    /// Each chord moved to the inversion nearest the one before (#103): the
    /// frag line's `voicing`. The text stays as written.
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
    format!("{}{}", note_name(p.note), if p.accent { "!" } else { "" })
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
    };
    if s.times != 1 {
        out.push_str(&format!("*{}", s.times));
    }
    if s.weight != 1 {
        out.push_str(&format!("@{}", s.weight));
    }
    if s.chance {
        out.push('?');
    }
    out
}

impl Notes {
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
                        format!("{what}:{}{}", b.div, if b.dot { "." } else { "" })
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

struct Compiler {
    events: Vec<Event>,
    rng: Rng,
    full: bool,
}

impl Compiler {
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
        if s.chance && self.rng.next_u32() % 2 == 0 {
            return;
        }
        let times = u128::from(s.times);
        for r in 0..times {
            let start = at.add(span.scale(r, times));
            self.item(&s.item, start, span.scale(1, times), bar);
        }
    }

    fn item(&mut self, item: &Item, at: Frac, span: Frac, bar: u32) {
        match item {
            Item::Rest => {}
            Item::Note(p) => self.note(*p, at, span, bar),
            Item::Chord(ps) | Item::Symbol(Symbol { pitches: ps, .. }) => {
                for p in ps {
                    self.note(*p, at, span, bar);
                }
            }
            Item::Group(g) => self.slots(g, at, span, bar),
            Item::Alt(g) => {
                let n = u32::try_from(g.len()).unwrap_or(1).max(1);
                let pick = usize::try_from(bar % n).unwrap_or(0);
                if let Some(s) = g.get(pick) {
                    self.slot(s, at, span, bar);
                }
            }
        }
    }

    fn note(&mut self, p: Pitch, at: Frac, span: Frac, bar: u32) {
        if self.events.len() >= MAX_EVENTS {
            self.full = true;
            return;
        }
        let start = at.tick();
        let end = at.add(span).tick().max(start + 1);
        self.events.push(Event {
            start: bar * TICKS_PER_BAR + start,
            len: end - start,
            note: p.note,
            accent: p.accent,
            vel: 0,
        });
    }
}

/// The order events are kept in: a total order, so an unstable sort (which
/// allocates nothing) gives the same list every time.
pub(crate) fn sort_key(e: &Event) -> (u32, u8, u32, bool, u8) {
    (e.start, e.note, e.len, e.accent, e.vel)
}

/// A line of mini-notation that plays exactly `events` (sorted, within
/// `bars` bars): every word weighs its length in ticks, a bar to a group when
/// there is more than one. `None` when the events do not fit that way: a
/// note over a bar line, overlapping notes, or a chord of different lengths.
pub fn freeze(events: &[Event], bars: u32) -> Option<Notes> {
    let bar = TICKS_PER_BAR;
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
        if s.chance {
            p = lcm(p, CHANCE_BARS);
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
            (events.clone(), last / TICKS_PER_BAR + 1)
        }
        Seq::Mini(slots) => {
            let bars = period(slots);
            if bars > MAX_BARS {
                return Err("this line takes more than 32 bars to repeat");
            }
            let mut c = Compiler {
                events: Vec::new(),
                rng: Rng::new(0),
                full: false,
            };
            for bar in 0..bars {
                c.rng = Rng::new(mix(bar, 0x5EED));
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
                let (note, accent) = match fill {
                    Fill::Pitch(p) => (p.note, p.accent),
                    Fill::Walk(from) => (scale.map_or(*from, |s| s.walk(*from, hit)), false),
                };
                hit += 1;
                events.push(Event {
                    start,
                    len: end - start,
                    note,
                    accent,
                    vel: 0,
                });
            }
            (events, 1)
        }
        Seq::Classic(beats) => {
            let mut events = Vec::new();
            let mut at = 0u32;
            for b in beats {
                let len = classic_ticks(b);
                for p in &b.pitches {
                    events.push(Event {
                        start: at,
                        len,
                        note: p.note,
                        accent: p.accent,
                        vel: 0,
                    });
                }
                at += len;
                if events.len() > MAX_EVENTS {
                    return Err("this line has too many notes");
                }
            }
            let bars = at.div_ceil(TICKS_PER_BAR).max(1);
            if bars > MAX_BARS {
                return Err("a line is at most 32 bars long");
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
