//! The generators of spec 002 Req 7 that write notes: `arp`, `walk`, `markov`
//! and `mutate` (ADR-0016). Each is a call on a note line, seeded and total:
//! the same call gives the same events, bounded in length and range.
//!
//! ```text
//! arp([c4,e4,g4],up,16)          a chord, a mode (up, converge…), a rate (a sixteenth)
//! arp([c4,e4,g4],up:2,12)        over two octaves, in eighth triplets
//! arp([c4,e4,g4],random,16,7)    random takes a seed
//! walk(c4,8,1)                   8 notes, a random walk on the song's scale
//! markov(1,riff,3)               learn order 1 from the clip riff, seed 3
//! mutate(riff,30,5)              change 30 percent of riff's notes, seed 5
//! prog(4,7)                      four bars of chords in the song's key, seed 7
//! prog(4,7,7th,"4 2 2",jazz)      sevenths, a rhythm in beats and a style
//! root(prog)                     the root of each chord of prog, as a bass line
//! bass(prog,walk,2,5)            a walking bass on prog's chords, octave 2, seed 5
//! arp(prog,up,16)                prog's chords arpeggiated, each from its start
//! ```

use super::{Cursor, Event, MAX_EVENTS, NoteError, Pitch, TICKS_PER_BAR, err};
use crate::algo::{Mode, Rng, Scale, mix};
use crate::arp;
pub use crate::arp::ArpMode;

/// The calls a line of notes may be, `euclid` aside; no other is read.
pub const CALLS: [&str; 7] = ["arp", "walk", "markov", "mutate", "root", "prog", "bass"];

/// Notes of a walk, and of a chord for an arpeggio.
const MAX_WALK: u32 = 32;
pub(super) const MAX_CHORD: usize = 8;
/// Longest Markov context.
const MAX_ORDER: u8 = 3;

/// Notes a bar an arp may play: halves to sixteenths, and the triplets
/// between (#613); each is exact on the 48-tick bar.
const ARP_RATES: [u8; 8] = [2, 3, 4, 6, 8, 12, 16, 24];
const RATE_MSG: &str = "a rate is 2, 3, 4, 6, 8, 12, 16 or 24";
const MODE_MSG: &str = "a mode is up, down, updown, downup, up&down, down&up, converge, diverge, disconverge, pinkyup, pinkyupdown, thumbup, thumbupdown, asplayed or random";

/// `up:2`: an arp's mode and its octaves (#613), as written.
fn mode_text(mode: ArpMode, octaves: u8) -> String {
    if octaves > 1 {
        format!("{}:{octaves}", mode.word())
    } else {
        mode.word().to_string()
    }
}

/// How far `prog` stacks each chord (#608).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Size {
    /// 1 3 5.
    #[default]
    Triad,
    /// 1 3 5 7.
    Seventh,
    /// 1 3 5 7 9.
    Ninth,
    /// 1 4 5, and 1 2 5 on the dominant.
    Sus,
}

impl Size {
    pub const ALL: [(Size, &'static str); 4] = [
        (Size::Triad, "triad"),
        (Size::Seventh, "7th"),
        (Size::Ninth, "9th"),
        (Size::Sus, "sus"),
    ];

    /// The scale steps stacked from a chord's degree.
    fn steps(self, degree: usize) -> &'static [usize] {
        match self {
            Size::Triad => &[0, 2, 4],
            Size::Seventh => &[0, 2, 4, 6],
            Size::Ninth => &[0, 2, 4, 6, 8],
            Size::Sus if degree == 4 => &[0, 1, 4],
            Size::Sus => &[0, 3, 4],
        }
    }

    fn notes(self) -> usize {
        self.steps(0).len()
    }
}

/// The moves `prog` makes between chords, and where it starts and ends (#610).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Common practice: tonic, subdominant, dominant; I to V.
    #[default]
    Classic,
    /// Mostly I, IV, V and vi; I to IV or V.
    Pop,
    /// A strong ii, V, I pull; ii to I.
    Jazz,
    /// The tonic and its neighbours, no dominant; I to I.
    Modal,
    /// Down by step from the tonic to the dominant: i VII VI V.
    Andalusian,
}

impl Style {
    pub const ALL: [(Style, &'static str); 5] = [
        (Style::Classic, "classic"),
        (Style::Pop, "pop"),
        (Style::Jazz, "jazz"),
        (Style::Modal, "modal"),
        (Style::Andalusian, "andalusian"),
    ];
}

/// The colours `prog` may add (#623), each with a seeded chance a chord.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Colours {
    /// A chord from the parallel mode (major and minor) for one of the same
    /// function: bVI or iv for IV, bVII for V, bIII for iii in major; IV for
    /// iv and V for v in minor.
    pub borrow: bool,
    /// The dominant 7th of the next chord in its place, when that is not
    /// the tonic.
    pub secondary: bool,
}

impl Colours {
    pub const WORDS: [&'static str; 2] = ["borrow", "secondary"];

    fn any(self) -> bool {
        self.borrow || self.secondary
    }

    /// `borrow secondary`, as a prog's argument prints.
    fn print(self) -> String {
        let words = [(self.borrow, "borrow"), (self.secondary, "secondary")];
        words
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, w)| *w)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// How `bass` follows the chords (#611).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BassStyle {
    /// The root on a chord's first beat, its fifth two beats on.
    Fifths,
    /// Root and the octave above, alternating in eighths.
    Octave,
    /// The root on a chord's first beat, an approach a step from the next
    /// root on the beat before a change, chord notes or scale steps between.
    Walk,
}

impl BassStyle {
    pub const ALL: [(BassStyle, &'static str); 3] = [
        (BassStyle::Fifths, "fifths"),
        (BassStyle::Octave, "octave"),
        (BassStyle::Walk, "walk"),
    ];

    fn name(self) -> &'static str {
        BassStyle::ALL
            .iter()
            .find(|s| s.0 == self)
            .map_or("", |s| s.1)
    }
}

/// A generator call. `from` and `src` of Markov and Mutate keep the events of
/// the clip they read, so a call can run again without looking it up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gen {
    Arp {
        chord: Vec<Pitch>,
        mode: ArpMode,
        /// 1 to 4: the chord repeated an octave up each time (#613).
        octaves: u8,
        /// Notes a bar, one of `ARP_RATES`.
        rate: u8,
        /// The clip's `bars N`: the pattern runs on across them (#613).
        bars: u32,
        seed: u32,
    },
    Walk {
        start: u8,
        steps: u32,
        seed: u32,
        /// `walk(c4,8,3,chords)` (#617): the clip whose chords it lands on,
        /// its events and bars.
        over: Option<(String, Vec<Event>, u32)>,
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
        /// `mutate(riff,10,5,evolve)` (#619): on a live clip each bar changes
        /// the bar before, not the source.
        evolve: bool,
    },
    /// The chords of an earlier clip, each arpeggiated from where it starts (#103).
    ArpProg {
        from: String,
        src: Vec<Event>,
        bars: u32,
        mode: ArpMode,
        octaves: u8,
        rate: u8,
        seed: u32,
    },
    /// The lowest note of each chord of an earlier clip, moved to `octave` (#103).
    Root {
        from: String,
        src: Vec<Event>,
        bars: u32,
        octave: u8,
    },
    /// A bass line on the chords of an earlier clip, in `octave` (#611).
    Bass {
        from: String,
        src: Vec<Event>,
        bars: u32,
        style: BassStyle,
        octave: u8,
        seed: u32,
    },
    /// A progression in the song's key (#103): a chord a bar unless
    /// `beats` say otherwise (#609), stacked to `size` (#608), moving as
    /// `style` does (#610).
    Prog {
        bars: u32,
        seed: u32,
        size: Size,
        /// Each chord's length in beats, 1 to 16, in turn: `[4]` by default.
        beats: Vec<u8>,
        style: Style,
        /// Borrowed chords and secondary dominants (#623).
        colours: Colours,
    },
}

/// Bars an evolving `mutate` walks from before it starts again (#619).
pub const EVOLVE_BARS: u64 = 64;

/// Bars a progression may have, and the octave `root` plays in by default.
const MAX_PROG: u32 = 16;
/// Lengths a progression's rhythm may list, and the longest, in beats.
const MAX_BEATS: usize = 16;
const BEAT: u32 = TICKS_PER_BAR / 4;
const ROOT_OCTAVE: u8 = 2;

/// A clip a call may read: its events and bars.
pub type Source = (Vec<Event>, u32);

impl Gen {
    pub fn seed(&self) -> u32 {
        match self {
            Gen::Arp { seed, .. }
            | Gen::Walk { seed, .. }
            | Gen::Markov { seed, .. }
            | Gen::Mutate { seed, .. }
            | Gen::ArpProg { seed, .. }
            | Gen::Prog { seed, .. }
            | Gen::Bass { seed, .. } => *seed,
            Gen::Root { .. } => 0,
        }
    }

    /// Bars before the events repeat.
    pub fn bars(&self) -> u32 {
        match self {
            Gen::Walk { over: None, .. } => 1,
            Gen::Walk {
                over: Some((_, _, bars)),
                ..
            } => *bars,
            Gen::Arp { bars, .. }
            | Gen::Markov { bars, .. }
            | Gen::Mutate { bars, .. }
            | Gen::ArpProg { bars, .. }
            | Gen::Root { bars, .. }
            | Gen::Prog { bars, .. }
            | Gen::Bass { bars, .. } => *bars,
        }
    }

    pub fn print(&self) -> String {
        match self {
            Gen::Arp {
                chord,
                mode,
                octaves,
                rate,
                seed,
                ..
            } => {
                let ps: Vec<String> = chord.iter().map(super::pitch_text).collect();
                let seed = if *mode == ArpMode::Random {
                    format!(",{seed}")
                } else {
                    String::new()
                };
                let mode = mode_text(*mode, *octaves);
                format!("arp([{}],{mode},{rate}{seed})", ps.join(","))
            }
            Gen::Walk {
                start,
                steps,
                seed,
                over,
            } => match over {
                None => format!("walk({},{steps},{seed})", super::note_name(*start)),
                Some((from, _, _)) => {
                    format!("walk({},{steps},{seed},{from})", super::note_name(*start))
                }
            },
            Gen::Markov {
                order, from, seed, ..
            } => format!("markov({order},{from},{seed})"),
            Gen::Mutate {
                from,
                amount,
                seed,
                evolve,
                ..
            } => {
                let evolve = if *evolve { ",evolve" } else { "" };
                format!("mutate({from},{amount},{seed}{evolve})")
            }
            Gen::ArpProg {
                from,
                mode,
                octaves,
                rate,
                seed,
                ..
            } => {
                let seed = if *mode == ArpMode::Random {
                    format!(",{seed}")
                } else {
                    String::new()
                };
                format!("arp({from},{},{rate}{seed})", mode_text(*mode, *octaves))
            }
            Gen::Root { from, octave, .. } if *octave == ROOT_OCTAVE => format!("root({from})"),
            Gen::Root { from, octave, .. } => format!("root({from},{octave})"),
            Gen::Bass {
                from,
                style,
                octave,
                seed,
                ..
            } => {
                let octave = if *octave == ROOT_OCTAVE {
                    String::new()
                } else {
                    format!(",{octave}")
                };
                let seed = if *style == BassStyle::Walk {
                    format!(",{seed}")
                } else {
                    String::new()
                };
                format!("bass({from},{}{octave}{seed})", style.name())
            }
            Gen::Prog {
                bars,
                seed,
                size,
                beats,
                style,
                colours,
            } => {
                let mut out = format!("prog({bars},{seed}");
                if *size != Size::Triad {
                    let name = Size::ALL.iter().find(|s| s.0 == *size).map_or("", |s| s.1);
                    out.push_str(&format!(",{name}"));
                }
                match beats.as_slice() {
                    [4] => {}
                    [b] => out.push_str(&format!(",{b}")),
                    bs => {
                        let bs: Vec<String> = bs.iter().map(u8::to_string).collect();
                        out.push_str(&format!(",\"{}\"", bs.join(" ")));
                    }
                }
                if *style != Style::Classic {
                    let name = Style::ALL
                        .iter()
                        .find(|s| s.0 == *style)
                        .map_or("", |s| s.1);
                    out.push_str(&format!(",{name}"));
                }
                if colours.any() {
                    out.push_str(&format!(",{}", colours.print()));
                }
                out.push(')');
                out
            }
        }
    }

    /// The events of the call for `seed` (its own, or a cycle's for a live
    /// clip), sorted.
    pub fn events(&self, seed: u32, scale: Option<&Scale>) -> Vec<Event> {
        let mut out = Vec::with_capacity(self.max_events());
        self.events_into(seed, scale, &mut out);
        out
    }

    /// The most events a call can make: room to reserve for `events_into`.
    pub fn max_events(&self) -> usize {
        match self {
            Gen::Arp { rate, bars, .. } => usize::from(*rate) * *bars as usize,
            Gen::Walk { over, .. } => {
                MAX_WALK as usize * over.as_ref().map_or(1, |(_, _, b)| *b as usize)
            }
            // Over chords, any chord may land on any of the source's slots.
            Gen::Markov { src, .. } => {
                let mut starts = [0u16; MAX_EVENTS + 1];
                let n = groups(src, &mut starts);
                let widest = (0..n)
                    .map(|g| group(src, &starts, g).len())
                    .max()
                    .unwrap_or(0);
                if widest > 1 {
                    (n * widest).min(MAX_EVENTS)
                } else {
                    src.len()
                }
            }
            Gen::Mutate { src, .. } | Gen::Root { src, .. } => src.len(),
            // A note a beat, or an eighth for `octave`.
            Gen::Bass { bars, style, .. } => {
                let per = if *style == BassStyle::Octave { 8 } else { 4 };
                per * *bars as usize
            }
            Gen::ArpProg { rate, bars, .. } => usize::from(*rate) * *bars as usize,
            // A secondary dominant is a 7th, whatever the size.
            Gen::Prog {
                bars,
                size,
                beats,
                colours,
                ..
            } => {
                let notes = if colours.secondary {
                    size.notes().max(4)
                } else {
                    size.notes()
                };
                notes * chords(*bars, beats).count()
            }
        }
    }

    /// Whether a live clip's bar is worked out from the bars before, an
    /// evolving `mutate` (#619).
    pub fn evolves(&self) -> bool {
        matches!(self, Gen::Mutate { evolve: true, .. })
    }

    /// An evolving `mutate`'s bars for cycle `cycle` into `out` (#619): the
    /// source with each cycle's change since the last multiple of
    /// `EVOLVE_BARS` applied in turn, each drawn by `seed_of(cycle)` as the
    /// plain `mutate` of that cycle is; a note that would be dropped goes
    /// back to its source pitch instead, so the line never empties. A
    /// function of the cycle alone, so a seek lands on the same notes; with
    /// room reserved for `max_events` nothing allocates.
    pub fn evolve_into(
        &self,
        cycle: u64,
        seed_of: impl Fn(u64) -> u32,
        scale: Option<&Scale>,
        out: &mut Vec<Event>,
    ) {
        out.clear();
        let Gen::Mutate { amount, src, .. } = self else {
            return;
        };
        out.extend_from_slice(src);
        let from = cycle - cycle % EVOLVE_BARS;
        for c in from..=cycle {
            let mut r = rng(seed_of(c), 0x3A3);
            for (e, home) in out.iter_mut().zip(src) {
                if r.next_u32() % 100 >= u32::from(*amount) {
                    continue;
                }
                let d = r.next_u32() % 5;
                if d == 0 {
                    e.note = home.note;
                    continue;
                }
                let shift = [0i32, -2, -1, 1, 2]
                    .get(usize::try_from(d).unwrap_or(0))
                    .copied()
                    .unwrap_or(0);
                let moved =
                    u8::try_from((i32::from(e.note) + shift).clamp(0, 127)).unwrap_or(e.note);
                e.note = scale.map_or(moved, |s| s.walk(moved, 0));
            }
        }
    }

    /// As `events`, into `out` (cleared first). With room reserved for
    /// `max_events` it never allocates, so the engine can run it on the audio
    /// thread's schedule (ADR-0002).
    pub fn events_into(&self, seed: u32, scale: Option<&Scale>, out: &mut Vec<Event>) {
        out.clear();
        match self {
            Gen::Arp {
                chord,
                mode,
                octaves,
                rate,
                bars,
                ..
            } => arp(chord, (*mode, *octaves), *rate, *bars, seed, out),
            Gen::Walk {
                start,
                steps,
                over: None,
                ..
            } => walk(*start, *steps, seed, scale, out),
            Gen::Walk {
                start,
                steps,
                over: Some((_, src, bars)),
                ..
            } => {
                if let Some(scale) = scale {
                    walk_over(*start, *steps, seed, scale, src, *bars, out);
                }
            }
            Gen::Markov { order, src, .. } => markov(*order, src, seed, out),
            Gen::Mutate { amount, src, .. } => mutate(*amount, src, seed, scale, out),
            Gen::ArpProg {
                src,
                bars,
                mode,
                octaves,
                rate,
                ..
            } => arp_prog(src, *bars, (*mode, *octaves), *rate, seed, out),
            Gen::Root { src, octave, .. } => root(src, *octave, out),
            Gen::Bass {
                src,
                bars,
                style,
                octave,
                ..
            } => bass(src, *bars, *style, *octave, seed, scale, out),
            Gen::Prog {
                bars,
                size,
                beats,
                style,
                colours,
                ..
            } => prog(*bars, beats, (*size, *style, *colours), seed, scale, out),
        }
        out.sort_unstable_by_key(super::sort_key);
        // Past the room reserved, a push would allocate on the audio thread (#233).
        debug_assert!(out.len() <= self.max_events(), "{} events", out.len());
    }
}

fn rng(seed: u32, tag: u32) -> Rng {
    Rng::new(mix(seed, tag))
}

/// A note of `notes` (sorted) drawn by `r`, over `octaves` (#613): one
/// octave draws as it always has.
fn draw(r: &mut Rng, notes: &[u8], octaves: u8) -> Option<u8> {
    let n = notes.len();
    let k = usize::try_from(r.next_u32()).unwrap_or(0) % (n * usize::from(octaves)).max(1);
    let note = u16::from(*notes.get(k % n.max(1))?) + 12 * (k / n.max(1)) as u16;
    Some(note.min(127) as u8)
}

/// `up` is a whole number of octaves below `octaves`: 0 is the note itself.
fn is_octave(up: u8, octaves: u8) -> bool {
    up % 12 == 0 && up / 12 < octaves
}

/// A chord arpeggiated over `bars`: the steps run on across the bar lines,
/// as the live arp's do (#613).
fn arp(
    chord: &[Pitch],
    (mode, octaves): (ArpMode, u8),
    rate: u8,
    bars: u32,
    seed: u32,
    out: &mut Vec<Event>,
) {
    let n = chord.len().min(MAX_CHORD);
    // As written, for asplayed: `arp_note` sorts for the others.
    let mut held = [0u8; MAX_CHORD];
    for (h, p) in held.iter_mut().zip(chord.iter()) {
        *h = p.note;
    }
    let held = held.get(..n).unwrap_or(&[]);
    let mut sorted = [0u8; MAX_CHORD];
    let sorted = sorted.get_mut(..n).unwrap_or(&mut []);
    sorted.copy_from_slice(held);
    sorted.sort_unstable();
    let len = TICKS_PER_BAR / u32::from(rate).max(1);
    let count = bars * (TICKS_PER_BAR / len.max(1));
    let mut r = rng(seed, 0xA4);
    for i in 0..count {
        // The deterministic modes are the live arp's pattern (spec 002 Req 7).
        let note = match mode {
            ArpMode::Random => draw(&mut r, sorted, octaves),
            _ => arp::arp_note(held, mode, octaves, u64::from(i), seed),
        };
        if let Some(note) = note {
            // An accent carries to the note's octaves above.
            let accent = chord
                .iter()
                .any(|p| p.accent && note >= p.note && is_octave(note - p.note, octaves));
            out.push(Event {
                start: i * len,
                len,
                note,
                accent,
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
                hold: 0,
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
            degree: false,
            when: Default::default(),
            nudge: 0,
            cents: 0,
            pan: 0,
            region: crate::notes::Region::WHOLE,
            fit: 0,
            play: crate::notes::Play::SAME,
            hold: 0,
        });
    }
}

/// The degree of `scale` counted from its root's pitch class that `note`
/// is, or is nearest.
fn degree_of(scale: &Scale, note: u8) -> i32 {
    let iv = scale.mode.intervals();
    let len = i32::try_from(iv.len()).unwrap_or(1).max(1);
    let rel = i32::from(scale.snap(note)) - i32::from(scale.root);
    let (oct, pc) = (rel.div_euclid(12), rel.rem_euclid(12));
    let idx = iv.iter().position(|i| i32::from(*i) == pc).unwrap_or(0);
    oct * len + i32::try_from(idx).unwrap_or(0)
}

/// A walk that lands on the chords of `src` (#617): on each quarter, the
/// chord note there nearest a step or two on from the last, within an
/// octave of `start`; between quarters, at most a scale step toward the
/// next one. `bars` bars of `steps` notes, seeded by the bar.
fn walk_over(
    start: u8,
    steps: u32,
    seed: u32,
    scale: &Scale,
    src: &[Event],
    bars: u32,
    out: &mut Vec<Event>,
) {
    let steps = steps.clamp(1, MAX_WALK);
    let bars = bars.max(1);
    let span = bars * TICKS_PER_BAR;
    let (lo, hi) = (start.saturating_sub(12), start.saturating_add(12).min(127));
    // The chord note sounding at `t` nearest `near`, in range.
    let tone = |t: u32, near: i32| -> Option<u8> {
        let mut buf = [0u8; MAX_CHORD];
        let n = chord_at(src, t % span, &mut buf)
            .or_else(|| chord_at(src, u32::MAX, &mut buf))
            .map_or(0, |(_, n)| n);
        let chord = buf.get(..n).filter(|c| !c.is_empty())?;
        (lo..=hi)
            .filter(|m| chord.iter().any(|c| c % 12 == m % 12))
            .min_by_key(|m| ((i32::from(*m) - near).abs(), *m))
    };
    let at_degree = |d: i32| scale.degree_note(scale.root, d);
    let beats = bars * 4;
    let mut r = rng(seed, 0x617);
    // Each quarter's target, in turn.
    let mut targets = [0u8; 4 * super::MAX_BARS as usize];
    let mut last = i32::from(scale.snap(start));
    for (k, slot) in (0..beats).zip(targets.iter_mut()) {
        let moved = degree_of(scale, u8::try_from(last.clamp(0, 127)).unwrap_or(60))
            + i32::try_from(r.next_u32() % 5).unwrap_or(2)
            - 2;
        let near = i32::from(at_degree(moved));
        let t =
            tone(k * BEAT, near).unwrap_or_else(|| u8::try_from(last.clamp(0, 127)).unwrap_or(60));
        *slot = t;
        last = i32::from(t);
    }
    let mut cur = targets.first().copied().unwrap_or(start);
    for b in 0..bars {
        for i in 0..steps {
            let a = b * TICKS_PER_BAR + i * TICKS_PER_BAR / steps;
            let end = b * TICKS_PER_BAR + (i + 1) * TICKS_PER_BAR / steps;
            let on_beat = (i * TICKS_PER_BAR) % steps == 0 && a % BEAT == 0;
            if on_beat {
                cur = targets.get((a / BEAT) as usize).copied().unwrap_or(cur);
            } else {
                // A step toward the next quarter's target, round the walk.
                let next = targets
                    .get(((a / BEAT + 1) % beats) as usize)
                    .copied()
                    .unwrap_or(cur);
                let (here, there) = (degree_of(scale, cur), degree_of(scale, next));
                cur = at_degree(here + (there - here).signum());
            }
            out.push(Event {
                start: a,
                len: (end - a).max(1),
                note: cur,
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
                hold: 0,
            });
        }
    }
}

/// Pitches follow a chain learned from the source's own order of notes; the
/// rhythm is the source's. Every note comes from the source's pitch set.
/// A source with chords (two or more notes starting together) is learned
/// chord by chord (#624).
fn markov(order: u8, src: &[Event], seed: u32, out: &mut Vec<Event>) {
    let m = src.len();
    if m == 0 {
        return;
    }
    let mut starts = [0u16; MAX_EVENTS + 1];
    let n = groups(src, &mut starts);
    if (0..n).any(|g| group(src, &starts, g).len() > 1) {
        markov_chords(order, src, &starts, n, seed, out);
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

/// Where each group of events starting together begins in `src` (sorted by
/// start), into `starts`, and how many groups; `starts[n]` is the end.
fn groups(src: &[Event], starts: &mut [u16; MAX_EVENTS + 1]) -> usize {
    let mut n = 0;
    for (i, e) in src.iter().enumerate().take(MAX_EVENTS) {
        if i == 0 || src.get(i - 1).is_some_and(|p| p.start != e.start) {
            if let Some(s) = starts.get_mut(n) {
                *s = i as u16;
            }
            n += 1;
        }
    }
    if let Some(s) = starts.get_mut(n) {
        *s = src.len().min(MAX_EVENTS) as u16;
    }
    n
}

/// Group `g` of `src`, as `groups` found them.
fn group<'a>(src: &'a [Event], starts: &[u16; MAX_EVENTS + 1], g: usize) -> &'a [Event] {
    let at = |k: usize| usize::from(starts.get(k).copied().unwrap_or(0));
    src.get(at(g)..at(g + 1)).unwrap_or(&[])
}

/// `markov` over a source of chords (#624): the chain's states are whole
/// chords, so each slot of the source's rhythm takes one of its chords,
/// every note of it, as long as it lasted there. Nothing allocates, so a
/// live clip may run it in `render`.
fn markov_chords(
    order: u8,
    src: &[Event],
    starts: &[u16; MAX_EVENTS + 1],
    n: usize,
    seed: u32,
    out: &mut Vec<Event>,
) {
    let k = usize::from(order.clamp(1, MAX_ORDER));
    let mut r = rng(seed, 0x3A4);
    let notes = |g: usize| group(src, starts, g).iter().map(|e| e.note);
    let same = |a: usize, b: usize| notes(a).eq(notes(b));
    // The last k chords played, as groups of the source.
    let first = usize::try_from(r.next_u32()).unwrap_or(0) % n;
    let mut ctx = [0usize; MAX_ORDER as usize];
    for (j, c) in ctx.iter_mut().take(k).enumerate() {
        *c = (first + j) % n;
    }
    let follows = |i: usize, ctx: &[usize]| {
        i + k < n && ctx.iter().take(k).enumerate().all(|(j, c)| same(i + j, *c))
    };
    for slot in 0..n {
        let at = group(src, starts, slot).first().map_or(0, |e| e.start);
        let count = (0..n).filter(|i| follows(*i, &ctx)).count();
        let pick = if count > 0 {
            let nth = usize::try_from(r.next_u32()).unwrap_or(0) % count;
            (0..n)
                .filter(|i| follows(*i, &ctx))
                .nth(nth)
                .map_or(slot, |i| i + k)
        } else {
            usize::try_from(r.next_u32()).unwrap_or(0) % n
        };
        for e in group(src, starts, pick) {
            if out.len() < MAX_EVENTS {
                out.push(Event { start: at, ..*e });
            }
        }
        ctx.rotate_left(1);
        if let Some(last) = ctx.get_mut(k - 1) {
            *last = pick;
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
pub(super) fn chord_at(src: &[Event], t: u32, buf: &mut [u8; MAX_CHORD]) -> Option<(u32, usize)> {
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
fn arp_prog(
    src: &[Event],
    bars: u32,
    (mode, octaves): (ArpMode, u8),
    rate: u8,
    seed: u32,
    out: &mut Vec<Event>,
) {
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
            ArpMode::Random => draw(&mut r, held, octaves),
            _ => arp::arp_note(held, mode, octaves, step, seed),
        };
        if let Some(note) = note {
            out.push(Event {
                start: t,
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
                hold: 0,
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
            degree: false,
            when: Default::default(),
            nudge: 0,
            cents: 0,
            pan: 0,
            region: crate::notes::Region::WHOLE,
            fit: 0,
            play: crate::notes::Play::SAME,
            hold: 0,
        });
        i = j.max(i + 1);
    }
}

fn bass_note(start: u32, len: u32, note: i32) -> Event {
    Event {
        start,
        len,
        note: u8::try_from(note.clamp(0, 127)).unwrap_or(0),
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
        hold: 0,
    }
}

/// The scale note a step from `note`, up or down; two semitones without a
/// scale.
fn scale_step(note: i32, up: bool, scale: Option<&Scale>) -> i32 {
    let by = if up { 1 } else { -1 };
    let Some(scale) = scale else {
        return note + 2 * by;
    };
    let mut n = note + by;
    for _ in 0..12 {
        let pc = u8::try_from(n.rem_euclid(12)).unwrap_or(0);
        if scale
            .mode
            .intervals()
            .iter()
            .any(|i| (scale.root + i) % 12 == pc)
        {
            return n;
        }
        n += by;
    }
    note + 2 * by
}

/// The fifth above `root` of a chord: its own (perfect, else flat, else
/// sharp), else the scale's, else a perfect fifth.
fn fifth(chord: &[u8], root: i32, scale: Option<&Scale>) -> i32 {
    let has = |iv: i32| {
        chord
            .iter()
            .any(|n| (i32::from(*n) - root).rem_euclid(12) == iv)
    };
    if let Some(iv) = [7, 6, 8].into_iter().find(|iv| has(*iv)) {
        return root + iv;
    }
    let mut n = root;
    for _ in 0..4 {
        n = scale_step(n, true, scale);
    }
    if scale.is_some() { n } else { root + 7 }
}

/// `bass(prog,style[,octave][,seed])`: a line on the chords of `src`, beat
/// by beat over `bars`, each chord's root (its lowest note's pitch class, as
/// `root` takes it) in `octave`.
fn bass(
    src: &[Event],
    bars: u32,
    style: BassStyle,
    octave: u8,
    seed: u32,
    scale: Option<&Scale>,
    out: &mut Vec<Event>,
) {
    let end = bars * TICKS_PER_BAR;
    let mut r = rng(seed, 0xBA5);
    let mut buf = [0u8; MAX_CHORD];
    let base = (i32::from(octave) + 1) * 12;
    let root_of = |low: u8| base + i32::from(low % 12);
    // Where the chord sounding at `t` ends: the next start, or the end.
    let next_start = |t: u32| {
        src.iter()
            .map(|e| e.start)
            .filter(|s| *s > t && *s < end)
            .min()
            .unwrap_or(end)
    };
    let mut prev = base;
    let mut t = 0;
    while t < end {
        let Some((start, n)) = chord_at(src, t, &mut buf) else {
            t += BEAT;
            continue;
        };
        let chord = buf.get(..n).unwrap_or(&[]);
        let low = chord.first().copied().unwrap_or(0);
        let root = root_of(low);
        let stop = next_start(t);
        let beat = (t - start) / BEAT;
        match style {
            BassStyle::Fifths => {
                let note = match beat % 4 {
                    0 => Some(root),
                    2 => Some(fifth(chord, root, scale)),
                    _ => None,
                };
                if let Some(note) = note {
                    out.push(bass_note(t, (2 * BEAT).min(stop - t), note));
                }
            }
            BassStyle::Octave => {
                let half = BEAT / 2;
                for (k, note) in [root, root + 12].into_iter().enumerate() {
                    let at = t + half * u32::try_from(k).unwrap_or(0);
                    if at < stop {
                        out.push(bass_note(at, half.min(stop - at), note));
                    }
                }
            }
            BassStyle::Walk => {
                let last = t + BEAT >= stop;
                let note = if beat == 0 {
                    root
                } else if last {
                    // A step above or below the next chord's root, round the
                    // loop at the end.
                    let mut next = [0u8; MAX_CHORD];
                    let to = chord_at(src, stop % end.max(1), &mut next)
                        .map_or(root, |_| root_of(next.first().copied().unwrap_or(low)));
                    scale_step(to, r.next_u32() % 2 == 0, scale)
                } else if r.next_u32() % 2 == 0 {
                    // A chord note in the octave above the root, not the last.
                    let tones = chord
                        .iter()
                        .map(|n| root + (i32::from(*n) - root).rem_euclid(12));
                    let count = tones.clone().filter(|n| *n != prev).count();
                    let k = usize::try_from(r.next_u32()).unwrap_or(0) % count.max(1);
                    tones.filter(|n| *n != prev).nth(k).unwrap_or(root)
                } else {
                    // A scale step, turning back at the edges of the octave.
                    let up = r.next_u32() % 2 == 0;
                    let up = if prev <= root {
                        true
                    } else if prev >= root + 12 {
                        false
                    } else {
                        up
                    };
                    scale_step(prev, up, scale)
                };
                out.push(bass_note(t, BEAT.min(stop - t), note));
                prev = note;
            }
        }
        t += BEAT;
    }
}

/// Degrees by function: tonic (I vi iii), subdominant (IV ii), dominant (V vii).
const FUNCTIONS: [&[usize]; 3] = [&[0, 5, 2], &[3, 1], &[4, 6]];
/// The chance (of 8) of each next function after tonic, subdominant, dominant.
const MOVES: [[u32; 3]; 3] = [[1, 4, 3], [1, 2, 5], [6, 0, 2]];

/// Each chord's start and length in ticks: the beats in turn until `bars`
/// bars are full, the last cut at their end.
fn chords(bars: u32, beats: &[u8]) -> impl Iterator<Item = (u32, u32)> + '_ {
    let end = bars.min(MAX_PROG) * TICKS_PER_BAR;
    let mut at = 0;
    beats
        .iter()
        .cycle()
        .take(if beats.is_empty() { 0 } else { usize::MAX })
        .map_while(move |b| {
            let start = at;
            at += u32::from(*b).max(1) * BEAT;
            (start < end).then(|| (start, at.min(end) - start))
        })
}

/// Weights (of any total) of the next degree after each degree, I to vii.
type Moves = [[u32; 7]; 7];

const POP: Moves = [
    [0, 1, 0, 3, 3, 3, 0],
    [0, 0, 0, 1, 3, 0, 0],
    [0, 0, 0, 1, 0, 2, 0],
    [3, 1, 0, 0, 3, 2, 0],
    [3, 0, 0, 2, 0, 3, 0],
    [1, 1, 0, 4, 2, 0, 0],
    [1, 0, 0, 0, 0, 0, 0],
];
const JAZZ: Moves = [
    [0, 3, 1, 2, 0, 2, 0],
    [0, 0, 0, 0, 6, 0, 1],
    [0, 0, 0, 0, 0, 4, 0],
    [0, 2, 0, 0, 2, 0, 1],
    [5, 0, 0, 0, 0, 2, 0],
    [0, 5, 0, 1, 0, 0, 0],
    [2, 0, 2, 0, 0, 0, 0],
];
/// No V: the tonic, ii, IV and the seventh degree (bVII in a minor mode).
const MODAL: Moves = [
    [0, 2, 0, 3, 0, 0, 3],
    [3, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0],
    [3, 1, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0, 0],
    [4, 0, 0, 1, 0, 0, 0],
];

/// The next degree after `from`, by the weights of its row; a row of
/// nothing goes home. A diminished chord is skipped when `plain`.
fn next(moves: &Moves, from: usize, r: &mut Rng, scale: &Scale, plain: bool) -> usize {
    let row = moves.get(from).copied().unwrap_or_default();
    let weight = |d: usize| {
        let w = row.get(d).copied().unwrap_or(0);
        if plain && diminished(scale, d) { 0 } else { w }
    };
    let total: u32 = (0..7).map(weight).sum();
    if total == 0 {
        return 0;
    }
    let pick = r.next_u32() % total;
    let mut acc = 0;
    for d in 0..7 {
        acc += weight(d);
        if pick < acc {
            return d;
        }
    }
    0
}

/// Is the triad on `degree` diminished in `scale`?
fn diminished(scale: &Scale, degree: usize) -> bool {
    let pc = |k: usize| i32::from(scale.degree((degree + k) % 7).unwrap_or(0));
    (pc(4) - pc(0)).rem_euclid(12) == 6
}

/// The degree of each chord of a progression of `n` in `style`.
fn degree(
    style: Style,
    k: usize,
    n: usize,
    function: &mut usize,
    steps: &[usize],
    r: &mut Rng,
    scale: &Scale,
) -> usize {
    let last = k + 1 == n;
    match style {
        Style::Classic => {
            if k == 0 {
                0
            } else if last {
                *function = 2;
                4
            } else {
                let pick = r.next_u32() % 8;
                let moves = MOVES.get(*function).copied().unwrap_or([8, 0, 0]);
                let mut acc = 0;
                for (f, w) in moves.iter().enumerate() {
                    acc += w;
                    if pick < acc {
                        *function = f;
                        break;
                    }
                }
                let choices = FUNCTIONS.get(*function).copied().unwrap_or(&[0]);
                choices
                    .get(usize::try_from(r.next_u32()).unwrap_or(0) % choices.len().max(1))
                    .copied()
                    .unwrap_or(0)
            }
        }
        // The step of the descent: i VII VI V.
        Style::Andalusian => [0, 6, 5, 4]
            .get(steps.get(k).copied().unwrap_or(0))
            .copied()
            .unwrap_or(0),
        Style::Pop | Style::Jazz | Style::Modal => {
            let (moves, start) = match style {
                Style::Pop => (&POP, 0),
                Style::Jazz => (&JAZZ, 1),
                _ => (&MODAL, 0),
            };
            let d = if k == 0 {
                start
            } else if last {
                match style {
                    Style::Pop if r.next_u32() % 2 == 0 => 3,
                    Style::Pop => 4,
                    _ => 0,
                }
            } else if style == Style::Jazz && k + 2 == n {
                // The V before the last I.
                4
            } else {
                next(moves, *function, r, scale, style == Style::Modal)
            };
            *function = d;
            d
        }
    }
}

/// `prog(bars,seed[,size][,beats][,style])`: chords on the song's scale,
/// a bar each or as `beats` say. `classic` starts on the tonic (I), ends on
/// the dominant (V) and moves between functions as common practice does:
/// tonic to anything, subdominant towards the dominant, the dominant home.
/// Each chord is stacked from its degree to `size`, in octave 4.
/// `colours` may then put a borrowed chord or a secondary dominant on any
/// chord but the first and the last (#623), drawn from a stream of their
/// own, so the same call without them gives the same chords.
fn prog(
    bars: u32,
    beats: &[u8],
    (size, style, colours): (Size, Style, Colours),
    seed: u32,
    scale: Option<&Scale>,
    out: &mut Vec<Event>,
) {
    let Some(scale) = scale.filter(|s| s.degree(0).is_some()) else {
        return;
    };
    let mut r = rng(seed, 0x9C0);
    let n = chords(bars, beats).count();
    // The andalusian descent: which chords move a step down, from i to V.
    let mut steps = [0usize; MAX_PROG as usize * 4];
    if style == Style::Andalusian {
        andalusian(n, &mut r, &mut steps);
    }
    let mut function = 0usize;
    let mut degrees = [0usize; MAX_PROG as usize * 4];
    for (k, d) in degrees.iter_mut().take(n).enumerate() {
        *d = degree(style, k, n, &mut function, &steps, &mut r, scale);
    }
    let parallel = parallel(scale);
    // Each chord's coins, both every chord so one colour doesn't move the
    // other's: a secondary dominant (on an inner chord before one that is
    // not the tonic), a borrowing, and which chord to borrow.
    let mut c = rng(seed, 0x9C1);
    let mut coins = [(false, false, 0u32); MAX_PROG as usize * 4];
    for (k, coin) in coins.iter_mut().take(n).enumerate() {
        let (second, lend, pick) = (c.next_u32() % 3 == 0, c.next_u32() % 3 == 0, c.next_u32());
        let next = degrees.get(k + 1).copied().unwrap_or(0);
        let inner = k > 0 && k + 1 < n;
        let second = colours.secondary && inner && second && next != 0 && !diminished(scale, next);
        *coin = (second, colours.borrow && inner && lend, pick);
    }
    for (k, (start, len)) in chords(bars, beats).enumerate() {
        let degree = degrees.get(k).copied().unwrap_or(0);
        let next = degrees.get(k + 1).copied().unwrap_or(0);
        let (second, lend, pick) = coins.get(k).copied().unwrap_or_default();
        // A secondary dominant leads to the chord played next, so not to
        // another one.
        let second = second && !coins.get(k + 1).is_some_and(|c| c.0);
        let mut push = |note: u8| {
            out.push(Event {
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
                hold: 0,
            });
        };
        if second {
            // V7 of the next chord: a major triad and a minor 7th a fifth up.
            let root = (scale.degree(next).unwrap_or(0) + 7) % 12;
            for i in [0, 4, 7, 10] {
                push(60 + root + i);
            }
            continue;
        }
        let (key, degree) = match parallel.filter(|_| lend) {
            Some((p, major)) => match borrowed(degree, major, pick) {
                Some(d) => (p, d),
                None => (*scale, degree),
            },
            None => (*scale, degree),
        };
        let mut below = 59u8;
        for step in size.steps(degree) {
            let pc = key.degree((degree + step) % 7).unwrap_or(0);
            let mut note = 60 + pc;
            while note <= below {
                note += 12;
            }
            below = note;
            push(note);
        }
    }
}

/// The parallel key of a major or minor `scale` (#623), and whether the
/// song's is major; `None` for another mode.
fn parallel(scale: &Scale) -> Option<(Scale, bool)> {
    let (major, minor) = (Mode::from_name("major")?, Mode::from_name("minor")?);
    let (mode, is_major) = match scale.mode {
        m if m == major => (minor, true),
        m if m == minor => (major, false),
        _ => return None,
    };
    Some((
        Scale {
            root: scale.root,
            mode,
        },
        is_major,
    ))
}

/// The degree of the parallel key borrowed for `degree` (#623), `pick`
/// choosing between bVI and iv; `None` when it has none.
fn borrowed(degree: usize, major: bool, pick: u32) -> Option<usize> {
    match (major, degree) {
        // IV: bVI or iv; V: bVII; iii: bIII.
        (true, 3) => Some(if pick % 2 == 0 { 5 } else { 3 }),
        (true, 4) => Some(6),
        (true, 2) => Some(2),
        // iv: IV; v: V.
        (false, 3 | 4) => Some(degree),
        _ => None,
    }
}

/// The step (0 to 3) of each of `n` chords down i VII VI V: from 0 to 3
/// without going back, the seed choosing which steps repeat or are left out.
fn andalusian(n: usize, r: &mut Rng, steps: &mut [usize]) {
    let Some(last) = n.checked_sub(1) else {
        return;
    };
    // The chords where the line moves down, at most three.
    let mut moves = [false; MAX_PROG as usize * 4];
    let want = last.min(3);
    let mut got = 0;
    while got < want {
        let k = 1 + usize::try_from(r.next_u32()).unwrap_or(0) % last;
        if let Some(m) = moves.get_mut(k).filter(|m| !**m) {
            *m = true;
            got += 1;
        }
    }
    // With fewer than four chords a move skips steps, so the last is V.
    let skip = 3 - want;
    let mut extra = if want > 0 {
        usize::try_from(r.next_u32()).unwrap_or(0) % want
    } else {
        0
    };
    let mut step = 0;
    for k in 0..n {
        if moves.get(k).copied().unwrap_or(false) {
            step += 1;
            if extra == 0 {
                step += skip;
            }
            extra = extra.wrapping_sub(1);
        }
        if let Some(s) = steps.get_mut(k) {
            *s = step.min(3);
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

const PROG_ARGS: &str = "after the seed a prog takes a size (triad, 7th, 9th, sus), beats (2 or \"4 2 2\"), a style (classic, pop, jazz, modal, andalusian) or colours (borrow, secondary)";

/// A prog's rhythm (#609): a number of beats, or up to 16 in quotes, each
/// 1 to 16; a bad one is an error at its own column.
fn prog_beats(cur: &Cursor<'_>, r: (usize, usize)) -> Result<Vec<u8>, NoteError> {
    let bad = "a chord lasts 1 to 16 beats: 2, or a list in quotes as \"4 2 2\"";
    let t = text(cur, r);
    let (body, base) = match t.strip_prefix('"') {
        Some(rest) => match rest.strip_suffix('"') {
            Some(body) => (body, r.0 + 1),
            None => return err(cur.base + r.0, bad),
        },
        None => (t.as_str(), r.0),
    };
    let mut out = Vec::new();
    let mut at = 0;
    for word in body.split(' ') {
        if !word.is_empty() {
            let col = cur.base + base + body.get(..at).map_or(0, |b| b.chars().count());
            let n = word
                .parse::<u8>()
                .ok()
                .filter(|n| (1..=16).contains(n))
                .ok_or(NoteError { col, msg: bad })?;
            out.push(n);
        }
        at += word.len() + 1;
    }
    if out.is_empty() || out.len() > MAX_BEATS {
        return err(cur.base + r.0, bad);
    }
    Ok(out)
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
        pads: None,
        cols: Vec::new(),
    };
    let p = sub.pitch()?;
    if sub.peek().is_some() {
        return err(sub.col(), "a note goes here, as c4");
    }
    if p.degree {
        return err(cur.base + r.0, "a generator takes notes, not degrees");
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
        pads: None,
        cols: Vec::new(),
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
    if chord.iter().any(|p| p.degree) {
        return err(cur.base + r.0, "a generator takes notes, not degrees");
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
            "no note clip with this name comes before this one",
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
            want(&[3, 4], crate::usage::ARP)?;
            // A clip's name: its progression, chord by chord (#103).
            let prog = Some(text(cur, arg(0)))
                .filter(|t| !t.starts_with('['))
                .and_then(|t| srcs(&t).map(|s| (t, s)));
            let chord = if prog.is_some() {
                Vec::new()
            } else {
                chord_arg(cur, arg(0))?
            };
            let m = text(cur, arg(1));
            let (word, octaves) = match m.split_once(':') {
                Some((w, _)) => {
                    // The octaves' column: after the mode and its colon.
                    let at = arg(1).0 + w.chars().count() + 1;
                    let o = number(cur, (at, arg(1).1), 4, "octaves are 1 to 4: up:2")?;
                    if o == 0 {
                        return err(cur.base + at, "octaves are 1 to 4: up:2");
                    }
                    (w, o as u8)
                }
                None => (m.as_str(), 1),
            };
            let mode = ArpMode::from_word(word).ok_or(NoteError {
                col: cur.base + arg(1).0,
                msg: MODE_MSG,
            })?;
            let rate = text(cur, arg(2))
                .parse::<u8>()
                .ok()
                .filter(|r| ARP_RATES.contains(r))
                .ok_or(NoteError {
                    col: cur.base + arg(2).0,
                    msg: RATE_MSG,
                })?;
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
                    octaves,
                    rate,
                    seed,
                },
                None => Gen::Arp {
                    chord,
                    mode,
                    octaves,
                    rate,
                    bars: 1,
                    seed,
                },
            }
        }
        "root" => {
            want(&[1, 2], crate::usage::ROOT)?;
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
        "bass" => {
            want(&[2, 3, 4], crate::usage::BASS)?;
            let (from, (src, bars)) = source(cur, arg(0), srcs)?;
            let style = BassStyle::ALL
                .iter()
                .find(|s| s.1 == text(cur, arg(1)))
                .map(|s| s.0)
                .ok_or(NoteError {
                    col: cur.base + arg(1).0,
                    msg: "a bass is fifths, octave or walk",
                })?;
            let walk = style == BassStyle::Walk;
            // `walk` takes a seed, and must; the others none (as arp's random).
            let (octave, seed) = match (walk, list.len()) {
                (true, 3) => (None, Some(2)),
                (true, 4) => (Some(2), Some(3)),
                (false, 2) => (None, None),
                (false, 3) => (Some(2), None),
                _ => {
                    return err(
                        cur.base + at,
                        "only walk takes a seed, and must: bass(prog,walk,5) or bass(prog,fifths)",
                    );
                }
            };
            let octave = match octave {
                Some(k) => number(cur, arg(k), 7, "an octave is 0 to 7")?,
                None => u32::from(ROOT_OCTAVE),
            };
            Gen::Bass {
                from,
                src,
                bars,
                style,
                octave: u8::try_from(octave).unwrap_or(ROOT_OCTAVE),
                seed: match seed {
                    Some(k) => seed_at(k)?,
                    None => 0,
                },
            }
        }
        "prog" => {
            want(&[2, 3, 4, 5, 6], crate::usage::PROG)?;
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
            let (mut size, mut beats, mut style) = (None, None, None);
            let mut colours: Option<Colours> = None;
            for k in 2..list.len() {
                let r = arg(k);
                let t = text(cur, r);
                let at = cur.base + r.0;
                let twice = || err(at, "a prog takes one size, one rhythm and one style");
                if let Some((s, _)) = Size::ALL.iter().find(|s| s.1 == t) {
                    if size.replace(*s).is_some() {
                        return twice();
                    }
                } else if let Some((s, _)) = Style::ALL.iter().find(|s| s.1 == t) {
                    if style.replace(*s).is_some() {
                        return twice();
                    }
                } else if t.starts_with('"') || t.chars().all(|c| c.is_ascii_digit()) {
                    if beats.replace(prog_beats(cur, r)?).is_some() {
                        return twice();
                    }
                } else if !t.is_empty() && t.split_whitespace().all(|w| Colours::WORDS.contains(&w))
                {
                    // `borrow secondary` or `borrow,secondary` (#623).
                    let mut c = colours.unwrap_or_default();
                    for w in t.split_whitespace() {
                        let on = if w == "borrow" {
                            &mut c.borrow
                        } else {
                            &mut c.secondary
                        };
                        if std::mem::replace(on, true) {
                            return err(at, "a prog takes each colour once");
                        }
                    }
                    colours = Some(c);
                } else {
                    return err(at, PROG_ARGS);
                }
            }
            Gen::Prog {
                bars,
                seed: seed_at(1)?,
                size: size.unwrap_or_default(),
                beats: beats.unwrap_or_else(|| vec![4]),
                style: style.unwrap_or_default(),
                colours: colours.unwrap_or_default(),
            }
        }
        "walk" => {
            want(&[3, 4], crate::usage::WALK)?;
            if scale.is_none() {
                return err(cur.base + at, "a walk needs a scale line before it");
            }
            let start = pitch_arg(cur, arg(0))?.note;
            let steps = number(cur, arg(1), MAX_WALK, "a walk is 1 to 32 notes")?;
            if steps == 0 {
                return err(cur.base + arg(1).0, "a walk is 1 to 32 notes");
            }
            let over = if list.len() == 4 {
                let (from, (src, bars)) = source(cur, arg(3), srcs)?;
                if src.is_empty() {
                    return err(cur.base + arg(3).0, "this clip has no chords to land on");
                }
                Some((from, src, bars))
            } else {
                None
            };
            Gen::Walk {
                start,
                steps,
                seed: seed_at(2)?,
                over,
            }
        }
        "markov" => {
            want(&[3], crate::usage::MARKOV)?;
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
            want(&[3, 4], crate::usage::MUTATE)?;
            let (from, (src, bars)) = source(cur, arg(0), srcs)?;
            let amount = number(cur, arg(1), 100, "a percent is 0 to 100")?;
            let evolve = match list.len() {
                4 if text(cur, arg(3)) == "evolve" => true,
                4 => return err(cur.base + arg(3).0, crate::usage::MUTATE),
                _ => false,
            };
            Gen::Mutate {
                from,
                amount: u8::try_from(amount).unwrap_or(100),
                src,
                bars,
                seed: seed_at(2)?,
                evolve,
            }
        }
    };
    Ok(Some(call))
}
