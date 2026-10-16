//! The song as text (ADR-0012, spec 002 Req 3 and 6): a parser and a
//! canonical printer, first cut. Drum tracks and clips of lanes:
//!
//! ```text
//! tempo 124
//! swing 56
//! track kit drums
//!
//! clip beat = kit /16
//!   bd x...x...x...x...
//!   sn ....X.......X..x
//!
//! scene intro 4: beat
//! scene main 8: beat fill
//! arrange intro main main
//! loop 5 12
//! ```
//!
//! A `synth` track holds note clips instead (spec 002 Req 3, ADR-0016):
//!
//! ```text
//! track lead synth
//!
//! clip riff = lead
//!   "c4 [e4 g4] ~ <c5 d5>"      # or classic: c4:4 e4:8 g4:8 c5:2
//! ```
//!
//! A track names its synth after its kind (#210): a model and a preset, or a
//! setting, a patch that lives in the song. Left out, one is picked from the
//! track's role and printed:
//!
//! ```text
//! setting nile = Minimoog MiniLead: Cutoff 1200, Resonance 0.5
//! track lead synth nile
//! track bass synth Sh101 AcidBass
//! track pad synth              # prints as: track pad synth Juno106 JunoPad
//! ```
//!
//! Scenes and the arrangement (ADR-0015, spec 002 Req 4): a scene is a
//! number of bars and the clips that play in it, each from the scene's
//! first bar and looping inside it; `arrange` plays scenes in order, and
//! `loop` repeats a range of the arrangement's bars (from 1, inclusive).
//! Without `arrange` every clip loops, as before.
//!
//! Snapshots and automation (ADR-0015): any parameter, by its registry name, on a
//! target: a track (its synth), `strip1`–`strip16`, `group1`–`group8` or
//! `master` (the global parameters):
//!
//! ```text
//! auto sweep = kit.BdDecay ramp 0.25 2 /8
//! auto duck = strip3.Level 1 0.5 0.25 1 /1
//! snapshot drop: strip1.Mute 1, master.P2Return 0.4
//! scene main 8: beat sweep duck [drop]
//! ```
//!
//! A lane of values (spread evenly over its bars) or a ramp is placed in
//! scenes like a clip and loops inside them; a snapshot sets its values on
//! the first step of a scene that lists it. Without `arrange` every lane
//! loops and no snapshot is applied.
//!
//! A modulation (ADR-0019) writes a signal to a parameter for the whole song,
//! once per block and after lanes and snapshots; see `signal` for the language:
//!
//! ```text
//! mod lead.cutoff = lfo(1).exprange(100, 2000) + lfo(3).range(0, 300)
//! clip acid = bass .cutoff(sine.slow(4).exprange(300, 3000)) .resonance(0.7)
//! ```
//!
//! A method on a clip's line writes to its track while the clip plays
//! (#204).
//!
//! Parsing and printing allocate, so they run when a song is loaded or a step
//! edited, never in `render`; the engine plays the parsed song in place. Both
//! are total: whatever the text, the parser returns a song or an error with a
//! line and a column, and never panics. Comments are kept (#199); layout is
//! not: a printed song is the canonical form of what was parsed.

use std::borrow::Cow;

use crate::algo::{Euclid, Mode, Scale, pitch_class};
use crate::drums::Pad;
use crate::fx::insert::InsertType;
use crate::fx::processor::ProcType;
use crate::mixer::OUT_NONE;
use crate::mono::model::Model;
use crate::mono::preset::Preset;
use crate::notes::pattern::{self, Line, Pattern};
use crate::notes::{self, Notes};
use crate::padsampler;
use crate::params::Param;

pub use comments::Comments;
pub use signal::Signal;

/// Most tracks, clips, lanes per clip and steps per lane a song may have.
pub const MAX_TRACKS: usize = 16;
pub const MAX_CLIPS: usize = 256;
pub const MAX_STEPS: usize = 64;
/// The steps to a bar a drum lane may have (#353): 16ths, their triplets,
/// 32nds and 32nd triplets.
pub const GRIDS: [u32; 5] = [12, 16, 24, 32, 48];
/// Most scenes, entries in the arrangement and bars in a scene.
pub const MAX_SCENES: usize = 256;
pub const MAX_ARRANGE: usize = 256;
pub const MAX_BARS: u32 = 256;
/// Most automation lanes, values in a lane, snapshots and settings in a snapshot.
pub const MAX_AUTOS: usize = 32;
pub const MAX_VALUES: usize = 64;
pub const MAX_SNAPSHOTS: usize = 32;
pub const MAX_SETS: usize = 32;
/// Most modulations (`mod` lines); their signals share `signal::MAX_NODES`.
pub const MAX_MODS: usize = 32;
/// Synth strips and group buses (ADR-0010).
const STRIPS: usize = 16;
const GROUPS: usize = 8;
/// Clock steps (sixteenths) in a bar.
pub const STEPS_PER_BAR: u64 = 16;
/// Longest song text accepted, in bytes.
pub const MAX_TEXT: usize = 1 << 20;
/// Longest name of a track or clip.
const MAX_NAME: usize = 32;
/// The words a line of the song starts with, in the order the parser's error
/// names them. The parser reads no other, and `.openspec/language.md` (or
/// `sound-design.md`) must define each (a test holds it to that).
pub const KEYWORDS: [&str; 20] = [
    "tempo", "swing", "scale", "setting", "track", "samples", "zone", "pad", "table", "attack",
    "strip", "group", "master", "clip", "auto", "snapshot", "mod", "scene", "arrange", "loop",
];
/// A line's keyword in today's words (#486, ADR-0031): a song written
/// before reads on, `frag` as `clip`, `section` as `scene`, and the old
/// `scene <name>: …` (a name and a colon, no bars) as a `snapshot`.
pub fn current_keyword<'a>(first: &'a str, second: Option<&str>, third: Option<&str>) -> &'a str {
    match first {
        "frag" => "clip",
        "section" => "scene",
        "scene" if second.is_some_and(|w| w.ends_with(':')) || third == Some(":") => "snapshot",
        _ => first,
    }
}

/// Tempo and swing ranges, as the clock has them.
const TEMPO: (f32, f32) = crate::clock::TEMPO;
const SWING: (f32, f32) = crate::clock::SWING;

/// A ghost note's velocity: well under a hit's 0.75 (#353).
pub const GHOST_VELOCITY: f32 = 0.35;
/// A flam's or drag's grace stroke velocity (#353).
pub const GRACE_VELOCITY: f32 = 0.4;

/// One step of a lane: a rest, a hit, an accented hit or a ghost note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Off = 0,
    Hit = 1,
    Accent = 2,
    /// A soft stroke under the hits (#353).
    Ghost = 3,
    /// A hit with one soft grace stroke just before it (#353).
    Flam = 4,
    /// A hit with two soft grace strokes just before it (#353).
    Drag = 5,
}

impl Step {
    fn from_char(c: char) -> Option<Step> {
        match c {
            '.' => Some(Step::Off),
            'x' => Some(Step::Hit),
            'X' => Some(Step::Accent),
            'o' => Some(Step::Ghost),
            'f' => Some(Step::Flam),
            'd' => Some(Step::Drag),
            _ => None,
        }
    }

    fn char(self) -> char {
        match self {
            Step::Off => '.',
            Step::Hit => 'x',
            Step::Accent => 'X',
            Step::Ghost => 'o',
            Step::Flam => 'f',
            Step::Drag => 'd',
        }
    }

    /// The step for a level from the view: 0 off, 1 hit, 2 accent, 3 ghost,
    /// 4 flam, 5 drag.
    pub fn from_level(level: u32) -> Option<Step> {
        match level {
            0 => Some(Step::Off),
            1 => Some(Step::Hit),
            2 => Some(Step::Accent),
            3 => Some(Step::Ghost),
            4 => Some(Step::Flam),
            5 => Some(Step::Drag),
            _ => None,
        }
    }

    /// The velocity it plays at; an accent is at or above `ACCENT_VELOCITY`.
    pub fn velocity(self) -> Option<f32> {
        match self {
            Step::Off => None,
            Step::Hit | Step::Flam | Step::Drag => Some(0.75),
            Step::Accent => Some(1.0),
            Step::Ghost => Some(GHOST_VELOCITY),
        }
    }

    /// Whether a ratchet may repeat it (#242): a hit, an accent or a ghost.
    pub fn repeats(self) -> bool {
        matches!(self, Step::Hit | Step::Accent | Step::Ghost)
    }

    /// How long before the hit each grace stroke falls, in milliseconds,
    /// earliest first: a flam's one, a drag's two (#353).
    pub fn graces(self) -> &'static [f32] {
        match self {
            Step::Flam => &[20.0],
            Step::Drag => &[30.0, 15.0],
            _ => &[],
        }
    }
}

/// What a lane plays (#662): a pad by its name, which plays its General MIDI
/// note, or a Pad Sampler's pad by number, `p0` to `p15`, which plays note
/// 36 + n whatever the kit puts there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanePad {
    Named(Pad),
    Number(u8),
}

/// The names of a Pad Sampler's pads by number.
const PAD_NUMBERS: [&str; padsampler::PADS] = [
    "p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8", "p9", "p10", "p11", "p12", "p13", "p14",
    "p15",
];

/// Where the ids of pads by number start, after `Pad`'s; mirrored as
/// `PAD_NUMBER` in `web/src/audio/params.ts`.
pub const PAD_NUMBER: u32 = 32;

impl LanePad {
    pub fn from_name(s: &str) -> Option<LanePad> {
        Pad::from_name(s).map(LanePad::Named).or_else(|| {
            let n = PAD_NUMBERS.iter().position(|x| *x == s)?;
            u8::try_from(n).ok().map(LanePad::Number)
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            LanePad::Named(p) => p.name(),
            LanePad::Number(n) => PAD_NUMBERS.get(usize::from(n)).copied().unwrap_or("p0"),
        }
    }

    /// The note the lane plays.
    pub fn note(self) -> u8 {
        match self {
            LanePad::Named(p) => p.note(),
            LanePad::Number(n) => padsampler::FIRST_NOTE + n,
        }
    }

    /// Its id across the FFI: a `Pad`'s, or `PAD_NUMBER` + n.
    pub fn id(self) -> u32 {
        match self {
            LanePad::Named(p) => p as u32,
            LanePad::Number(n) => PAD_NUMBER + u32::from(n),
        }
    }

    /// Whether `model` plays it: a pad by number only on a Pad Sampler, and
    /// a named pad on one only where a pad answers its note.
    pub fn plays_on(self, model: Model) -> bool {
        match self {
            LanePad::Number(_) => model.uses_pads(),
            LanePad::Named(p) => !model.uses_pads() || padsampler::pad_of(p.note()).is_some(),
        }
    }
}

/// A pad's row of steps; it loops on its own length (polymeter).
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub pad: LanePad,
    pub steps: Vec<Step>,
    /// The call that made the steps (`euclid(3,8)`); editing a step drops it.
    pub call: Option<Euclid>,
    /// Ratchets (#242): how often each step plays inside its span, 1–4.
    /// Empty when no step repeats, else as long as `steps`.
    pub ratchets: Vec<u8>,
}

/// The most hits a ratchet plays in one step (#242).
pub const MAX_RATCHET: u8 = 4;

impl Lane {
    /// How often step `n` plays inside its span: 1 unless it is ratcheted.
    pub fn ratchet(&self, n: usize) -> u8 {
        self.ratchets.get(n).copied().unwrap_or(1).max(1)
    }

    /// Step `n` with its ratchet, as the text writes it: `x`, `x3`.
    pub(crate) fn step_text(&self, n: usize) -> String {
        let st = self.steps.get(n).map_or('.', |s| s.char());
        match self.ratchet(n) {
            1 => st.to_string(),
            r => format!("{st}{r}"),
        }
    }

    /// Set step `n` to play `r` times (1–4); only a hit, accent or ghost
    /// repeats. False when there is no such step or it can't.
    pub fn set_ratchet(&mut self, n: usize, r: u8) -> bool {
        let Some(st) = self.steps.get(n) else {
            return false;
        };
        if !(1..=MAX_RATCHET).contains(&r) || (r > 1 && !st.repeats()) {
            return false;
        }
        if self.ratchets.is_empty() {
            if r == 1 {
                return true;
            }
            self.ratchets = vec![1; self.steps.len()];
        }
        if let Some(slot) = self.ratchets.get_mut(n) {
            *slot = r;
        }
        if self.ratchets.iter().all(|r| *r == 1) {
            self.ratchets.clear();
        }
        true
    }
}

/// A loop on a track: one lane per pad on a drum track, one line of notes on
/// a synth track (the other is then empty).
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub name: String,
    pub track: usize,
    pub lanes: Vec<Lane>,
    pub notes: Option<Notes>,
    /// A generator call that makes new events every cycle (`clip a = t live`).
    pub live: bool,
    /// Chords voiced near the one before (`clip a = t voicing`, #103), in a
    /// style and under an anchor (`voicing(drop2,c5)`, #606).
    pub voicing: Option<notes::Voicing>,
    /// Pattern methods (`.fast(2) .rev()`, ADR-0019), applied to its notes in order.
    pub pattern: Vec<Pattern>,
    /// A drum clip's steps to a bar (`/16`, #353): one of `GRIDS`.
    pub grid: u32,
    /// `clip b = t a .rev()` (#649): the earlier clip of notes this one is
    /// made from, through its methods; it has no line of its own.
    pub from: Option<String>,
    /// `"bd sn [ch oh] ch"` (#659): the beat line the lanes were made from,
    /// printed in their place until a step is edited.
    pub beat: Option<Beat>,
}

/// A drum clip's beat line as written, and whether the clip line wrote its
/// grid (else the line picked it).
#[derive(Clone, Debug, PartialEq)]
pub struct Beat {
    pub text: String,
    pub grid: bool,
}

/// The grids a beat line tries, straight before triplets, when the clip
/// line writes none (#659); /32 takes the hits /16 does.
const BEAT_GRIDS: [u32; 4] = [16, 12, 24, 48];

/// Strudel's names for pads, read as ours (#659).
const PAD_ALIASES: [(&str, &str); 3] = [("sd", "sn"), ("hh", "ch"), ("rim", "rs")];

impl Clip {
    /// A drum clip's hits as events, a step long on the 48-tick grid, any
    /// pad, sorted, over the bars its lanes take to come round together
    /// (#600); none when that is past 64 bars. `None` for a clip of notes.
    /// Each event's `word` holds its step and ratchet (`step_word`), so a
    /// clip made from this one keeps them (#653).
    pub fn hits(&self) -> Option<(Vec<notes::Event>, u32)> {
        if self.notes.is_some() || self.lanes.is_empty() {
            return None;
        }
        let grid = u64::from(self.grid.max(1));
        let gcd = |mut a: u64, mut b: u64| {
            while b != 0 {
                (a, b) = (b, a % b);
            }
            a.max(1)
        };
        let mut steps = grid;
        for lane in &self.lanes {
            let n = u64::try_from(lane.steps.len()).unwrap_or(1).max(1);
            steps = (steps / gcd(steps, n)).saturating_mul(n);
        }
        let bars = u32::try_from(steps / grid).unwrap_or(u32::MAX);
        if bars > notes::MAX_BARS {
            return Some((Vec::new(), bars));
        }
        let tick = |n: u64| u32::try_from(n * u64::from(notes::TICKS_PER_BAR) / grid).unwrap_or(0);
        let mut out = Vec::new();
        for n in 0..steps {
            for lane in &self.lanes {
                let len = lane.steps.len().max(1);
                let k = usize::try_from(n).unwrap_or(0) % len;
                if let Some(&st) = lane.steps.get(k).filter(|s| **s != Step::Off) {
                    let start = tick(n);
                    out.push(notes::Event {
                        start,
                        len: (tick(n + 1) - start).max(1),
                        note: lane.pad.note(),
                        accent: st == Step::Accent,
                        vel: 0,
                        word: step_word(st, lane.ratchet(k)),
                        degree: false,
                        when: notes::When::default(),
                        nudge: 0,
                    });
                }
            }
        }
        Some((out, bars))
    }
}

/// A hit's step and ratchet in an event's `word` (#653).
fn step_word(st: Step, ratchet: u8) -> u16 {
    u16::from(st as u8) | u16::from(ratchet) << 8
}

/// The lanes of a drum clip made from a clip (#653): the hits of `line`
/// back on `grid` steps a bar, each in the step it starts in, one lane for each of `pads`, each `bars`
/// long. Where two hits share a step, the stronger plays.
fn lanes_of(line: &Line, grid: u32, pads: &[LanePad]) -> Result<Vec<Lane>, &'static str> {
    let steps = usize::try_from(line.bars * grid).unwrap_or(usize::MAX);
    if steps > MAX_STEPS {
        return Err("a lane has at most 64 steps");
    }
    let mut lanes: Vec<Lane> = pads
        .iter()
        .map(|&pad| Lane {
            pad,
            steps: vec![Step::Off; steps],
            call: None,
            ratchets: vec![1; steps],
        })
        .collect();
    let bar = notes::TICKS_PER_BAR;
    for e in &line.events {
        // A hit lands in the step it starts in. It starts on a step, or,
        // made shorter (`fast`) and turned round (`rev`), ends on one.
        let on = |t: u32| t * grid % bar == 0;
        if !on(e.start) && !on(e.start + e.len) {
            return Err("a method moved a hit off the clip's step grid");
        }
        let n = usize::try_from(e.start * grid / bar).unwrap_or(usize::MAX);
        let st = Step::from_level(u32::from(e.word & 0xFF)).unwrap_or(Step::Hit);
        let ratchet = u8::try_from(e.word >> 8).unwrap_or(1).clamp(1, MAX_RATCHET);
        let Some(lane) = lanes.iter_mut().find(|l| l.pad.note() == e.note) else {
            continue;
        };
        if let (Some(slot), Some(r)) = (lane.steps.get_mut(n), lane.ratchets.get_mut(n)) {
            if strength(st) > strength(*slot) {
                (*slot, *r) = (st, ratchet);
            }
        }
    }
    for lane in &mut lanes {
        if lane.ratchets.iter().all(|r| *r == 1) {
            lane.ratchets.clear();
        }
    }
    Ok(lanes)
}

/// Which of two steps on one place plays: an accent over a hit over the
/// graced strokes over a ghost.
fn strength(st: Step) -> u8 {
    match st {
        Step::Off => 0,
        Step::Ghost => 1,
        Step::Flam | Step::Drag => 2,
        Step::Hit => 3,
        Step::Accent => 4,
    }
}

/// What a track's clips hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Drums,
    Synth,
    /// Lanes of pads on a pad sampler, notes on a multisampler.
    Sampler,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Drums => "drums",
            Kind::Synth => "synth",
            Kind::Sampler => "sampler",
        }
    }
}

/// Bars of the song and the clips that play in them.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub name: String,
    pub bars: u32,
    /// Indices into `Song::clips`.
    pub clips: Vec<usize>,
    /// Indices into `Song::autos` and `Song::snapshots`.
    pub autos: Vec<usize>,
    pub snapshots: Vec<usize>,
}

/// What a snapshot or lane sets: a track's synth, a strip (synths 0–15, groups
/// 16–23) or the global parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Track(usize),
    Strip(usize),
    Master,
}

/// How a lane moves over its length.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// Values spread evenly over the lane, each held for its share.
    Steps(Vec<f32>),
    /// From the first value to the second, linearly.
    Ramp(f32, f32),
    /// A quoted sequence, `"<300 [800 1200]>"`, one cycle a bar from the
    /// lane's start, nesting as a mod's does (#683).
    Seq(Signal),
}

/// An automation lane: one parameter of one target over `bars` bars.
#[derive(Clone, Debug, PartialEq)]
pub struct Auto {
    pub name: String,
    pub target: Target,
    pub param: Param,
    pub shape: Shape,
    pub bars: u32,
}

impl Auto {
    /// The value `steps` (fractional) into the lane; it loops on its length.
    pub fn value_at(&self, steps: f64) -> f32 {
        let len = f64::from(self.bars) * STEPS_PER_BAR as f64;
        let phase = (steps.rem_euclid(len) / len).clamp(0.0, 1.0);
        match &self.shape {
            Shape::Steps(v) => {
                let i = ((phase * v.len() as f64) as usize).min(v.len().saturating_sub(1));
                v.get(i).copied().unwrap_or(0.0)
            }
            Shape::Ramp(a, b) => a + (b - a) * phase as f32,
            // NaN for a `~`: the lane holds its last value.
            Shape::Seq(s) => {
                let mut ctx = signal::Ctx {
                    cps: 1.0,
                    dt: 0.0,
                    state: &mut [],
                    voice: None,
                };
                s.eval(steps.rem_euclid(len) / STEPS_PER_BAR as f64, &mut ctx)
            }
        }
    }

    /// The values in force `steps` into the lane (#683): `f(value, ahead)`,
    /// the value's index (in `Steps`, or in the sequence's numbers) and how
    /// many steps on it ends. Nothing for a ramp. Never allocates.
    pub fn lit(&self, steps: f64, f: &mut dyn FnMut(usize, f64)) {
        let len = f64::from(self.bars) * STEPS_PER_BAR as f64;
        let local = steps.rem_euclid(len);
        match &self.shape {
            Shape::Steps(v) if !v.is_empty() => {
                let n = v.len() as f64;
                let k = (local / len * n).floor().min(n - 1.0);
                f(k as usize, (k + 1.0) / n * len - local);
            }
            Shape::Seq(s) => {
                let t = local / STEPS_PER_BAR as f64;
                s.lit(t, &mut |v, until| {
                    f(v, (until - t) * STEPS_PER_BAR as f64);
                });
            }
            _ => {}
        }
    }
}

/// A modulation (ADR-0019): a signal written to one parameter of one target
/// while the song plays: a `mod` line for the whole song, or a parameter
/// method on a clip (`clip a = t .cutoff(…)`, #204) while it plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Mod {
    pub target: Target,
    pub param: Param,
    pub signal: Signal,
    /// The clip a method belongs to; `None` for a `mod` line.
    pub clip: Option<usize>,
    /// The Strudel name it was written with (#298), printed back as written.
    pub alias: Option<&'static str>,
}

/// Strudel's names for parameters (#298), for parameter methods and `mod`
/// lines: each stands for one registry parameter, at our scale (a resonance
/// 0..1, not a Q; a pan −1..1, not 0..1).
pub const ALIASES: [(&str, Param); 16] = [
    ("lpf", Param::Cutoff),
    ("ctf", Param::Cutoff),
    ("lpq", Param::Resonance),
    ("hpf", Param::HpCutoff),
    ("hpq", Param::HpResonance),
    ("attack", Param::AdsrAttack),
    ("att", Param::AdsrAttack),
    ("decay", Param::AdsrDecay),
    ("dec", Param::AdsrDecay),
    ("sustain", Param::AdsrSustain),
    ("sus", Param::AdsrSustain),
    ("release", Param::AdsrRelease),
    ("rel", Param::AdsrRelease),
    ("gain", Param::Level),
    ("room", Param::Send2),
    ("delay", Param::Send1),
];

/// The alias `name` and its parameter, if it is one.
fn alias(name: &str) -> Option<(&'static str, Param)> {
    ALIASES.iter().find(|(a, _)| *a == name).copied()
}

/// Values set together on the first step of a scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub name: String,
    pub sets: Vec<(Target, Param, f32)>,
}

/// What a mixer line sets (ADR-0018): the strip of a track's synth, a strip
/// by number, a group bus or the master's global parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mix {
    Track(usize),
    /// A synth strip, 0–15.
    Strip(usize),
    /// A group bus, 0–7.
    Group(usize),
    Master,
}

/// A mixer line: starting values for one strip, group or the master,
/// `strip bass: Level 0.8, I1Type Overdrive`. A group may carry a name.
#[derive(Clone, Debug, PartialEq)]
pub struct MixLine {
    pub at: Mix,
    pub name: Option<String>,
    pub sets: Vec<(Param, f32)>,
}

/// Most values on one mixer line: every parameter of a strip or the master.
pub const MAX_MIX: usize = 48;

/// Where clock step `k` falls in the song.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At {
    /// No arrangement: every clip loops on the clock's step.
    Free(u64),
    /// In arrangement entry `entry`, which plays `scene`, `local` steps in.
    In {
        entry: usize,
        scene: usize,
        local: u64,
    },
    /// Past the end of the arrangement.
    End,
}

/// A track of the song; the engine routes it to a synth.
#[derive(Clone, Debug)]
pub struct Track {
    pub name: String,
    pub kind: Kind,
    /// The synth's model and settings (#210): written as `<model> <preset>`
    /// after the kind, or picked from the track's role when left out. `None`
    /// only on a sampler track, which plays the samples already loaded.
    pub preset: Option<Preset>,
    /// The song's own setting the track plays, by index into `Song::settings`.
    pub setting: Option<usize>,
    /// Neither a model nor a setting was written: the preset was picked from
    /// the role, and the engine may pick again from the synths it has.
    pub picked: bool,
    /// Muted, or soloed (#355): written `mute` / `solo` at the end of its
    /// line. A clip plays when its track is heard (`Song::heard`).
    pub mute: bool,
    pub solo: bool,
}

/// Two tracks are equal by what they play; whether the preset was picked or
/// written is not part of the song, so a printed song parses back equal.
impl PartialEq for Track {
    fn eq(&self, other: &Track) -> bool {
        (
            &self.name,
            self.kind,
            self.preset,
            self.setting,
            self.mute,
            self.solo,
        ) == (
            &other.name,
            other.kind,
            other.preset,
            other.setting,
            other.mute,
            other.solo,
        )
    }
}

/// A synth patch that lives in the song (#210): a factory preset and the
/// changes made to it, `setting nile = Minimoog MiniLead: Cutoff 0.4, …`.
/// A Modular setting may hold its SynthDef on the indented lines under it
/// (ADR-0024).
#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub name: String,
    pub preset: Preset,
    pub sets: Vec<(Param, f32)>,
    pub code: Option<String>,
    /// Its code's knobs by `ctl` (#674): an argument's name, else ''.
    pub knobs: Vec<String>,
}

/// A Modular setting whose code lines are still being read: its index, the
/// line of its first code line and the lines so far.
struct OpenCode {
    setting: usize,
    line: usize,
    lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub tempo: f32,
    pub swing: f32,
    /// The key generators walk (`scale c minor`); it goes before its first use.
    pub scale: Option<Scale>,
    /// The octave of the scale's root when written (`scale c1 minor`, #575):
    /// where degree 0 is for clips without `.scale`; octave 3 when `None`.
    pub scale_octave: Option<u8>,
    pub settings: Vec<Setting>,
    pub tracks: Vec<Track>,
    /// The samples each sampler or drums track wants, by pack or kit id from
    /// the samples manifest (#214, ADR-0018): the view fetches and loads them,
    /// since the engine can't fetch (ADR-0013).
    pub samples: Vec<(usize, String)>,
    /// The Sampler zones a sampler track without a `samples` line plays
    /// (#522): the view loads each source and lays the zones out.
    pub zones: Vec<zone::SongZone>,
    /// The pads of a pad sampler a drums or sampler track without a
    /// `samples` line plays (#527), laid out as its zones are.
    pub pads: Vec<pad::SongPad>,
    /// The recipes of the user wavetables and attacks (ADR-0033): the view
    /// rebuilds them when the song loads, one per slot of each kind.
    pub recipes: Vec<recipe::Recipe>,
    /// Starting values for strips, groups and the master (ADR-0018).
    pub mix: Vec<MixLine>,
    pub clips: Vec<Clip>,
    pub autos: Vec<Auto>,
    pub snapshots: Vec<Snapshot>,
    pub mods: Vec<Mod>,
    pub scenes: Vec<Scene>,
    /// The order scenes play in, by index; empty means no arrangement.
    pub arrange: Vec<usize>,
    /// Bars of the arrangement to repeat, from 1 and inclusive.
    pub loop_bars: Option<(u32, u32)>,
    /// The comments of the text, kept to print back (#199).
    pub comments: Comments,
}

impl Default for Song {
    fn default() -> Song {
        Song {
            tempo: 120.0,
            swing: 50.0,
            scale: None,
            scale_octave: None,
            settings: Vec::new(),
            tracks: Vec::new(),
            samples: Vec::new(),
            zones: Vec::new(),
            pads: Vec::new(),
            recipes: Vec::new(),
            mix: Vec::new(),
            clips: Vec::new(),
            autos: Vec::new(),
            snapshots: Vec::new(),
            mods: Vec::new(),
            scenes: Vec::new(),
            arrange: Vec::new(),
            loop_bars: None,
            comments: Comments::default(),
        }
    }
}

/// Why a text is not a song, and where: line and column count from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SongError {
    pub line: usize,
    pub col: usize,
    pub msg: Cow<'static, str>,
}

/// A word of a line and its column.
struct Word<'a> {
    col: usize,
    text: &'a str,
}

/// The words of `line`, split at whitespace, each with its column (in chars, from 1).
fn words(line: &str) -> Vec<Word<'_>> {
    let mut out = Vec::new();
    let mut start: Option<(usize, usize)> = None;
    for (col, (byte, c)) in line.char_indices().enumerate() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some((byte, col + 1)),
            (true, Some((b, k))) => {
                out.push(Word {
                    col: k,
                    text: line.get(b..byte).unwrap_or(""),
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some((b, k)) = start {
        out.push(Word {
            col: k,
            text: line.get(b..).unwrap_or(""),
        });
    }
    out
}

/// The line without its comment: a `#` starts one at the line's start or after
/// a space, so the sharp in `c#4` stays a sharp.
fn strip_comment(raw: &str) -> &str {
    let mut prev_space = true;
    for (i, c) in raw.char_indices() {
        if c == '#' && prev_space {
            return raw.get(..i).unwrap_or(raw);
        }
        prev_space = c.is_whitespace();
    }
    raw
}

/// A pack or kit id of the samples manifest: `upright-piano-kw`.
fn is_sample_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    s.len() <= MAX_NAME
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl Song {
    /// Parse a song; the first problem found is the error.
    pub fn parse(text: &str) -> Result<Song, SongError> {
        let mut song = Song::default();
        // The clip that indented lines add lanes to, and the line it began on.
        let mut open: Option<(usize, usize)> = None;
        // `clip … bars N`: the length the open clip's line of timed notes is given.
        let mut open_bars: Option<(u32, usize, usize)> = None;
        // `clip … /N` other than /16 on a sampler: its column and line, refused
        // if the clip turns out to hold notes (#395).
        let mut open_grid: Option<(usize, usize)> = None;
        // Whether the open clip's line writes its grid, which a beat line keeps (#659).
        let mut open_fixed = false;
        // The line of the `loop`, checked against the arrangement at the end.
        let mut loop_at: Option<usize> = None;
        // Tracks given a model and no preset.
        let mut models: Vec<(usize, Model)> = Vec::new();
        // Parameters on a track whose model is picked after the clips (#673).
        let mut later: Vec<(SongError, usize, Param)> = Vec::new();
        // Nodes in the song's signals so far.
        let mut nodes = 0;
        // The Modular setting whose code lines come next.
        let mut open_code: Option<OpenCode> = None;
        // Whether a `tempo` and a `swing` line were read: one of each (#251).
        let (mut has_tempo, mut has_swing) = (false, false);
        for (i, raw) in text.lines().enumerate() {
            let line = i + 1;
            let err = |col: usize, msg: &'static str| SongError {
                line,
                col,
                msg: msg.into(),
            };
            // A Modular setting's code is its indented lines as written,
            // blank lines and all, `#` included (ADR-0024).
            if let Some(c) = open_code.as_mut() {
                if raw.starts_with([' ', '\t']) || raw.trim().is_empty() {
                    c.lines.push(dedent(raw).to_string());
                    continue;
                }
            }
            if let Some(c) = open_code.take() {
                close_code(&mut song, c)?;
            }
            let body = strip_comment(raw);
            // `clip … .cutoff(…) .resonance(…)` (#204): parameter methods from
            // the first word that starts with a dot, taken off and read with
            // the clip.
            let (body, methods) = match ["clip ", "frag "]
                .iter()
                .any(|k| body.trim_start().starts_with(k))
            {
                true => split_methods(body),
                false => (body, None),
            };
            let mut ws = words(body);
            // `clip a = t … voicing` (#103) or `voicing(drop2, c5)` (#606):
            // the last words, taken off before the rest is read.
            let mut voicing = None;
            if ws.len() > 4
                && ws
                    .first()
                    .is_some_and(|w| matches!(w.text, "clip" | "frag"))
                && ws
                    .last()
                    .is_some_and(|w| w.text == "voicing" || w.text.ends_with(')'))
                && let Some(k) = ws
                    .iter()
                    .skip(4)
                    .rposition(|w| w.text.starts_with("voicing"))
            {
                let col = ws.get(k + 4).map_or(0, |w| w.col);
                let text: String = ws.drain(k + 4..).map(|w| w.text).collect();
                let v = notes::Voicing::parse(&text).map_err(|m| err(col, m))?;
                voicing = Some((col, v));
            }
            let Some(first) = ws.first() else {
                continue;
            };
            if body.starts_with([' ', '\t']) {
                let Some((f, _)) = open else {
                    return Err(err(first.col, "a lane goes under a clip"));
                };
                if song.clips.get(f).is_some_and(|c| c.from.is_some()) {
                    return Err(err(
                        first.col,
                        "a clip made from a clip has no line of its own",
                    ));
                }
                let kind = song
                    .clips
                    .get(f)
                    .and_then(|fr| song.tracks.get(fr.track))
                    .map(|t| t.kind);
                // A sampler line is a lane unless it is written as notes: quoted,
                // a chord, with a duration or a call such as `arp(` or `euclid(`
                // (#251); no pad has a `(`. A bad pad name is then a lane error.
                if kind == Some(Kind::Drums) && first.text.starts_with('"') {
                    let model = song
                        .clips
                        .get(f)
                        .and_then(|c| track_model(&song.tracks, &models, c.track));
                    let clip = song
                        .clips
                        .get_mut(f)
                        .ok_or(err(first.col, "a lane goes under a clip"))?;
                    if !clip.lanes.is_empty() {
                        return Err(err(first.col, "a drum clip is lanes or one beat line"));
                    }
                    if !clip.pattern.is_empty() {
                        return Err(err(first.col, "pattern methods are for a clip of notes"));
                    }
                    let fixed = [clip.grid];
                    let grids: &[u32] = if open_fixed { &fixed } else { &BEAT_GRIDS };
                    let text = body.trim();
                    let (events, bars, grid) =
                        notes::parse_beat(text, first.col, &|w| beat_pad(w, model), grids)
                            .map_err(|e| err(e.col, e.msg))?;
                    clip.lanes = beat_lanes(events, bars, grid).map_err(|m| err(first.col, m))?;
                    clip.grid = grid;
                    clip.beat = Some(Beat {
                        text: text.to_string(),
                        grid: open_fixed,
                    });
                    continue;
                }
                let as_notes = match kind {
                    Some(Kind::Synth) => true,
                    Some(Kind::Sampler) => {
                        first.text.starts_with(['"', '[']) || first.text.contains([':', '('])
                    }
                    _ => false,
                };
                if kind == Some(Kind::Sampler) {
                    let has = song
                        .clips
                        .get(f)
                        .map(|fr| (fr.notes.is_some(), !fr.lanes.is_empty()));
                    if (as_notes && has.is_some_and(|h| h.1))
                        || (!as_notes && has.is_some_and(|h| h.0))
                    {
                        return Err(err(first.col, "a clip holds lanes or notes, not both"));
                    }
                }
                if as_notes {
                    if song.clips.get(f).is_some_and(|fr| fr.notes.is_some()) {
                        return Err(err(first.col, "a note clip is one line of notes"));
                    }
                    // A grid is for lanes: a sampler clip that holds notes has
                    // none, as on a synth track (#395).
                    if let Some((col, at)) = open_grid.take() {
                        return Err(SongError {
                            line: at,
                            col,
                            msg: "a note clip has no step grid".into(),
                        });
                    }
                    let srcs = |name: &str| {
                        song.clips
                            .iter()
                            .find(|f| f.name == name)
                            .and_then(|f| f.notes.as_ref())
                            .map(|n| (n.events.clone(), n.bars))
                    };
                    let live = song.clips.get(f).is_some_and(|fr| fr.live);
                    let voiced = song.clips.get(f).and_then(|fr| fr.voicing);
                    let mut n =
                        notes::parse_with(body.trim_start(), first.col, song.scale.as_ref(), &srcs)
                            .map_err(|e| err(e.col, e.msg))?;
                    if let Some((bars, col, at)) = open_bars.take() {
                        n = n.with_bars(bars).map_err(|msg| SongError {
                            line: at,
                            col,
                            msg: msg.into(),
                        })?;
                    }
                    if live && !matches!(n.seq, notes::Seq::Generated(_)) {
                        return Err(err(
                            first.col,
                            "a live clip is a call: arp, walk, markov, mutate, root, prog or bass",
                        ));
                    }
                    let clip = song
                        .clips
                        .get_mut(f)
                        .ok_or(err(first.col, "a lane goes under a clip"))?;
                    // Degrees (#570): the clip's `.scale` makes them notes after
                    // the methods before it; without one, the song's scale does,
                    // at once, so `add` and voicing see notes.
                    let keyed = clip.pattern.iter().any(|p| {
                        matches!(
                            p,
                            pattern::Pattern::Scale(_)
                                | pattern::Pattern::ScaleOf(..)
                                | pattern::Pattern::ChordOf(_)
                        )
                    });
                    if !keyed {
                        song_degrees(&mut n.events, song.scale.as_ref(), song.scale_octave)
                            .map_err(|m| err(first.col, m))?;
                    }
                    if voiced.is_some() && n.events.iter().any(|e| e.degree) {
                        return Err(err(first.col, "voicing takes notes, not degrees"));
                    }
                    let mut n = match voiced {
                        Some(v) => n.voiced(v, song.scale.as_ref()),
                        None => n,
                    };
                    if live {
                        // The first bar kept for a source, as `step_live`
                        // plays it (#593).
                        pattern::pitch_all(&clip.pattern, &mut n.events, 0);
                    } else if !clip.pattern.is_empty() {
                        let line = Line {
                            events: std::mem::take(&mut n.events),
                            bars: n.bars,
                        };
                        let line = pattern::apply_all(&clip.pattern, line)
                            .map_err(|m| err(first.col, m))?;
                        (n.events, n.bars) = (line.events, line.bars);
                    }
                    song_degrees(&mut n.events, song.scale.as_ref(), song.scale_octave)
                        .map_err(|m| err(first.col, m))?;
                    clip.notes = Some(n);
                    continue;
                }
                if let Some((_, col, at)) = open_bars {
                    return Err(SongError {
                        line: at,
                        col,
                        msg: notes::BARS_MSG.into(),
                    });
                }
                let lane = parse_lane(&ws, line)?;
                let clip = song
                    .clips
                    .get_mut(f)
                    .ok_or(err(first.col, "a lane goes under a clip"))?;
                if !clip.pattern.is_empty() {
                    return Err(err(first.col, "pattern methods are for a clip of notes"));
                }
                if clip.beat.is_some() {
                    return Err(err(first.col, "a drum clip is lanes or one beat line"));
                }
                if clip.lanes.iter().any(|l| l.pad.note() == lane.pad.note()) {
                    return Err(err(first.col, "this pad already has a lane"));
                }
                // A lane its track's model has no pad for is an error, where
                // the track names the model (#662).
                let model = track_model(&song.tracks, &models, clip.track);
                if model.is_some_and(|m| !lane.pad.plays_on(m)) {
                    return Err(err(
                        first.col,
                        match lane.pad {
                            LanePad::Number(_) => "a pad by number is for a Pad Sampler",
                            LanePad::Named(_) => "a Pad Sampler has no such pad: name it p0 to p15",
                        },
                    ));
                }
                clip.lanes.push(lane);
                continue;
            }
            if let Some((f, at)) = open.take() {
                check_lanes(&song, f, at)?;
            }
            let arg = |k: usize, msg: &'static str| {
                ws.get(k)
                    .ok_or(err(body.trim_end().chars().count() + 1, msg))
            };
            let expect_end = |k: usize| match ws.get(k) {
                Some(w) => Err(err(w.col, "unexpected text")),
                None => Ok(()),
            };
            let keyword = current_keyword(
                first.text,
                ws.get(1).map(|w| w.text),
                ws.get(2).map(|w| w.text),
            );
            let keyword = keyword.strip_suffix(':').unwrap_or(keyword);
            if !KEYWORDS.contains(&keyword) {
                return Err(err(
                    first.col,
                    "a line starts with tempo, swing, scale, setting, track, samples, zone, pad, table, attack, strip, group, master, clip, auto, snapshot, mod, scene, arrange or loop",
                ));
            }
            match current_keyword(
                first.text,
                ws.get(1).map(|w| w.text),
                ws.get(2).map(|w| w.text),
            ) {
                "tempo" | "swing" => {
                    let w = arg(1, "a number goes here")?;
                    let (range, msg) = if first.text == "tempo" {
                        (TEMPO, "tempo is 20 to 300")
                    } else {
                        (SWING, "swing is 50 to 75")
                    };
                    let v: f32 = w.text.parse().map_err(|_| err(w.col, msg))?;
                    if !(v.is_finite() && v >= range.0 && v <= range.1) {
                        return Err(err(w.col, msg));
                    }
                    expect_end(2)?;
                    let (has, field, once) = if first.text == "tempo" {
                        (&mut has_tempo, &mut song.tempo, "a song has one tempo")
                    } else {
                        (&mut has_swing, &mut song.swing, "a song has one swing")
                    };
                    if *has {
                        return Err(err(first.col, once));
                    }
                    *has = true;
                    *field = v;
                }
                "scale" => {
                    let root = arg(1, "a root and a mode go here: scale c minor")?;
                    // Strudel's `c:minor:pentatonic` is one word (#576).
                    let (root_text, mode, end) = match root.text.split_once(':') {
                        Some((r, m)) => (
                            r,
                            Word {
                                col: root.col + r.chars().count() + 1,
                                text: m,
                            },
                            2,
                        ),
                        None => {
                            let m = arg(2, "a mode goes here: scale c minor")?;
                            (
                                root.text,
                                Word {
                                    col: m.col,
                                    text: m.text,
                                },
                                3,
                            )
                        }
                    };
                    // `c1`: a root and its octave (#575)
                    let (root_text, octave) = match root_text.char_indices().last() {
                        Some((i, d)) if d.is_ascii_digit() => {
                            (root_text.get(..i).unwrap_or(""), d.to_digit(10))
                        }
                        _ => (root_text, None),
                    };
                    let pc = pitch_class(root_text).ok_or(err(
                        root.col,
                        "a root is a letter a to g, maybe # or b, maybe an octave",
                    ))?;
                    let m = Mode::from_name(mode.text)
                        .ok_or(err(mode.col, Mode::unknown(mode.text)))?;
                    expect_end(end)?;
                    if song.scale.is_some() {
                        return Err(err(first.col, "a song has one scale"));
                    }
                    if !song.clips.is_empty() {
                        return Err(err(first.col, "the scale goes before the clips"));
                    }
                    song.scale = Some(Scale { root: pc, mode: m });
                    song.scale_octave = octave.and_then(|o| u8::try_from(o).ok());
                }
                "track" => {
                    let name = arg(1, "a track name goes here")?;
                    if !is_name(name.text) {
                        return Err(err(
                            name.col,
                            "a name is a letter, then letters, digits or _",
                        ));
                    }
                    if song.tracks.iter().any(|t| t.name == name.text) {
                        return Err(err(name.col, "there is already a track with this name"));
                    }
                    let kind = arg(2, "a track kind goes here: drums, synth or sampler")?;
                    let kind = match kind.text {
                        "drums" => Kind::Drums,
                        "synth" => Kind::Synth,
                        "sampler" => Kind::Sampler,
                        _ => return Err(err(kind.col, "a track kind is drums, synth or sampler")),
                    };
                    // `mute` and `solo` end the line (#355); before them,
                    // `<setting>`, `<model> <preset>` or `<model>`.
                    let flags = ws
                        .iter()
                        .skip(3)
                        .rev()
                        .take_while(|w| matches!(w.text, "mute" | "solo"))
                        .count();
                    let base = ws.len() - flags;
                    let flag = |f: &str| ws.iter().skip(base).any(|w| w.text == f);
                    let (mute, solo) = (flag("mute"), flag("solo"));
                    let mut setting = None;
                    let mut model = None;
                    let mut preset = None;
                    if let Some(w) = ws.get(3).filter(|_| base > 3) {
                        if let Some(i) = song.settings.iter().position(|st| st.name == w.text) {
                            setting = Some(i);
                            preset = song.settings.get(i).map(|st| st.preset);
                            if base > 4 {
                                expect_end(4)?;
                            }
                        } else {
                            let m = model_named(w.text).ok_or(err(
                                w.col,
                                "a model (as Minimoog or Tr808) or a setting goes here",
                            ))?;
                            model = Some(m);
                            if let Some(p) = ws.get(4).filter(|_| base > 4) {
                                let pr = preset_named(p.text).ok_or(err(
                                    p.col,
                                    "a preset is a factory preset, as MiniBass",
                                ))?;
                                if pr.model() != m {
                                    return Err(err(p.col, "this preset is for another model"));
                                }
                                preset = Some(pr);
                            }
                            if base > 5 {
                                expect_end(5)?;
                            }
                        }
                        if !fits(kind, preset.map_or(model, |p| Some(p.model()))) {
                            return Err(err(w.col, "this model does not play this kind of track"));
                        }
                    }
                    if song.tracks.len() >= MAX_TRACKS {
                        return Err(err(first.col, "a song has at most 16 tracks"));
                    }
                    song.tracks.push(Track {
                        name: name.text.to_string(),
                        kind,
                        preset,
                        setting,
                        picked: false,
                        mute,
                        solo,
                    });
                    // A model without a preset: one is picked once the clips are in.
                    if let (Some(m), None) = (model, preset) {
                        models.push((song.tracks.len() - 1, m));
                    }
                }
                "setting" => {
                    let name = arg(1, "a setting name goes here")?;
                    if !is_name(name.text) || model_named(name.text).is_some() {
                        return Err(err(
                            name.col,
                            "a name is a letter, then letters, digits or _, and not a model's",
                        ));
                    }
                    if song.settings.iter().any(|st| st.name == name.text) {
                        return Err(err(name.col, "there is already a setting with this name"));
                    }
                    if !song.tracks.is_empty() {
                        return Err(err(first.col, "a setting goes before the tracks"));
                    }
                    let eq = arg(2, "= and a model go here")?;
                    if eq.text != "=" {
                        return Err(err(eq.col, "= and a model go here"));
                    }
                    let m = arg(3, "a model goes here, as Minimoog")?;
                    let model = model_named(m.text)
                        .ok_or(err(m.col, "a model is a synth's, as Minimoog or Tr808"))?;
                    let p = arg(4, "a preset of the model goes here")?;
                    let preset = preset_named(p.text.strip_suffix(':').unwrap_or(p.text))
                        .ok_or(err(p.col, "a preset is a factory preset, as MiniBass"))?;
                    if preset.model() != model {
                        return Err(err(p.col, "this preset is for another model"));
                    }
                    let mut k = 5;
                    if !p.text.ends_with(':') {
                        if let Some(colon) = ws.get(5) {
                            if colon.text != ":" {
                                return Err(err(colon.col, ": and the changes go here"));
                            }
                            k = 6;
                        }
                    }
                    let mut sets = Vec::new();
                    while let Some(pw) = ws.get(k) {
                        let param = Param::by_name(pw.text)
                            .ok_or(err(pw.col, "no parameter has this name"))?;
                        if param == Param::Model || param.is_global() || param.is_strip() {
                            return Err(err(
                                pw.col,
                                "a setting changes the synth's own parameters",
                            ));
                        }
                        if !model.reads(param) {
                            return Err(err(pw.col, model.unread(param)));
                        }
                        let v = ws.get(k + 1).ok_or(err(
                            body.trim_end().chars().count() + 1,
                            "a value goes here",
                        ))?;
                        let text = v.text.strip_suffix(',').unwrap_or(v.text);
                        let value = text
                            .parse::<f32>()
                            .ok()
                            .filter(|x| x.is_finite())
                            .ok_or(err(v.col, "a value is a number"))?;
                        if sets.len() >= MAX_SETS {
                            return Err(err(pw.col, "a setting has at most 32 changes"));
                        }
                        sets.push((param, value));
                        k += 2;
                        if !v.text.ends_with(',') {
                            if let Some(extra) = ws.get(k) {
                                return Err(err(extra.col, "a comma goes between changes"));
                            }
                        }
                    }
                    if k > 5 && sets.is_empty() {
                        return Err(err(
                            body.trim_end().chars().count() + 1,
                            "changes go here: Param value, …",
                        ));
                    }
                    if song.settings.len() >= MAX_TRACKS {
                        return Err(err(first.col, "a song has at most 16 settings"));
                    }
                    song.settings.push(Setting {
                        name: name.text.to_string(),
                        preset,
                        sets,
                        code: None,
                        knobs: Vec::new(),
                    });
                    if model.uses_graph() {
                        open_code = Some(OpenCode {
                            setting: song.settings.len() - 1,
                            line: line + 1,
                            lines: Vec::new(),
                        });
                    }
                }
                "clip" => {
                    let name = arg(1, "a clip name goes here")?;
                    if !is_name(name.text) {
                        return Err(err(
                            name.col,
                            "a name is a letter, then letters, digits or _",
                        ));
                    }
                    if song.clips.iter().any(|f| f.name == name.text)
                        || song.autos.iter().any(|a| a.name == name.text)
                    {
                        return Err(err(
                            name.col,
                            "there is already a clip or auto with this name",
                        ));
                    }
                    let eq = arg(2, "= and a track go here")?;
                    if eq.text != "=" {
                        return Err(err(eq.col, "= and a track go here"));
                    }
                    let track = arg(3, "a track goes here")?;
                    let Some(t) = song.tracks.iter().position(|t| t.name == track.text) else {
                        return Err(err(track.col, "no track has this name"));
                    };
                    let kind = song.tracks.get(t).map(|t| t.kind);
                    let synth = kind == Some(Kind::Synth);
                    let live = ws.get(4).is_some_and(|w| w.text == "live");
                    if let Some((col, _)) = voicing {
                        if kind == Some(Kind::Drums) {
                            return Err(err(col, "voicing is for a clip of notes"));
                        }
                        if live {
                            return Err(err(col, "a live clip is not voiced"));
                        }
                    }
                    if live && kind == Some(Kind::Drums) {
                        let col = ws.get(4).map_or(first.col, |w| w.col);
                        return Err(err(col, "only a note clip can be live"));
                    }
                    // `clip b = t a .rev()` (#649): an earlier clip of notes,
                    // played through this clip's methods.
                    let from = ws
                        .get(4)
                        .filter(|w| !matches!(w.text, "live" | "bars") && is_name(w.text));
                    // Made from a drum clip: lanes, which take methods of time
                    // only (#653).
                    let mut from_lanes = false;
                    if let Some(w) = from {
                        let Some(src) = song.clips.iter().find(|c| c.name == w.text) else {
                            return Err(err(w.col, "no clip with this name comes before this one"));
                        };
                        from_lanes = src.notes.is_none();
                        if from_lanes && kind == Some(Kind::Synth) {
                            return Err(err(w.col, "a clip of notes is made from a clip of notes"));
                        }
                        if !from_lanes && kind == Some(Kind::Drums) {
                            return Err(err(w.col, "a drum clip is made from a drum clip"));
                        }
                        if let Some((col, _)) = voicing {
                            return Err(err(
                                col,
                                "a clip made from a clip is voiced as its source",
                            ));
                        }
                    }
                    // `bars N` for a line of timed notes (#173).
                    open_bars = None;
                    if let Some(w) = ws.get(4).filter(|w| w.text == "bars") {
                        if kind == Some(Kind::Drums) {
                            return Err(err(w.col, notes::BARS_MSG));
                        }
                        let n = arg(5, "a number of bars goes here")?;
                        let bars: u32 = n
                            .text
                            .parse()
                            .ok()
                            .filter(|b| (1..=notes::MAX_BARS).contains(b))
                            .ok_or(err(n.col, "a line is 1 to 64 bars"))?;
                        open_bars = Some((bars, w.col, line));
                    }
                    let mut grid = 16;
                    open_grid = None;
                    open_fixed = false;
                    if let Some(w) = ws
                        .get(4)
                        .filter(|_| !live && open_bars.is_none() && from.is_none())
                    {
                        if synth {
                            return Err(err(w.col, "a note clip has no step grid"));
                        }
                        grid = w
                            .text
                            .strip_prefix('/')
                            .and_then(|g| g.parse().ok())
                            .filter(|g| GRIDS.contains(g))
                            .ok_or(err(w.col, "a drum grid is /12, /16, /24, /32 or /48"))?;
                        if grid != 16 {
                            open_grid = Some((w.col, line));
                        }
                        open_fixed = true;
                    }
                    expect_end(if open_bars.is_some() { 6 } else { 5 })?;
                    if song.clips.len() >= MAX_CLIPS {
                        return Err(err(first.col, "a song has at most 256 clips"));
                    }
                    song.clips.push(Clip {
                        name: name.text.to_string(),
                        track: t,
                        lanes: Vec::new(),
                        notes: None,
                        live,
                        voicing: voicing.map(|(_, v)| v),
                        pattern: Vec::new(),
                        grid,
                        from: from.map(|w| w.text.to_string()),
                        beat: None,
                    });
                    let f = song.clips.len() - 1;
                    if let Some((at, text)) = methods {
                        for (name, ncol, sig, scol) in
                            parse_methods(text).map_err(|(c, m)| err(at + c - 1, m))?
                        {
                            if Pattern::NAMES.contains(&name) {
                                let msg = if kind == Some(Kind::Drums) && from.is_none() {
                                    Some("pattern methods are for a clip of notes")
                                } else {
                                    None
                                };
                                // The earlier clips, for a method that reads one (ADR-0035).
                                let srcs = |name: &str| {
                                    song.clips.iter().take(f).find(|c| c.name == name).and_then(
                                        |c| match &c.notes {
                                            Some(n) => Some((n.events.clone(), n.bars, false)),
                                            None => c.hits().map(|(h, bars)| (h, bars, true)),
                                        },
                                    )
                                };
                                let p = match msg {
                                    Some(m) => Err(m),
                                    None => Pattern::parse_in(name, sig, &srcs),
                                }
                                .and_then(|p| {
                                    // A live clip changes pitches only, per note (#593).
                                    if live && !p.pitch_only() {
                                        Err("a live clip takes only pitch methods: scale, chord, oct, add(n) and sub(n)")
                                    } else if from_lanes && p.moves_pitch() {
                                        Err("a drum clip takes methods of time, not pitch")
                                    } else {
                                        Ok(p)
                                    }
                                })
                                .map_err(|m| err(at + ncol - 1, m))?;
                                if let Some(clip) = song.clips.get_mut(f) {
                                    clip.pattern.push(p);
                                }
                                continue;
                            }
                            let target = Target::Track(t);
                            let param =
                                param_for(&song, target, name).map_err(|msg| SongError {
                                    line,
                                    col: at + ncol - 1,
                                    msg,
                                })?;
                            check_reads(
                                &song.tracks,
                                &models,
                                t,
                                param,
                                err(at + ncol - 1, ""),
                                &mut later,
                            )?;
                            if song
                                .mods
                                .iter()
                                .any(|m| m.clip == Some(f) && m.param == param)
                            {
                                return Err(err(
                                    at + ncol - 1,
                                    "this parameter already has a method",
                                ));
                            }
                            let signal = Signal::parse(sig, &mut nodes)
                                .map_err(|(c, m)| err(at + scol + c - 2, m))?;
                            per_voice_fits(&song, target, param, &signal)
                                .map_err(|m| err(at + scol - 1, m))?;
                            if song.mods.len() >= MAX_MODS {
                                return Err(err(at + ncol - 1, "a song has at most 32 mods"));
                            }
                            song.mods.push(Mod {
                                // Not when a knob has the alias's name (#674).
                                alias: alias(name).filter(|(_, p)| *p == param).map(|(a, _)| a),
                                target,
                                param,
                                signal,
                                clip: Some(f),
                            });
                        }
                    }
                    if let Some(w) = from {
                        song.derive(f).map_err(|m| err(w.col, m))?;
                    }
                    open = Some((f, line));
                }
                "scene" => {
                    let name = arg(1, "a scene name goes here")?;
                    if !is_name(name.text) {
                        return Err(err(
                            name.col,
                            "a name is a letter, then letters, digits or _",
                        ));
                    }
                    if song.scenes.iter().any(|s| s.name == name.text) {
                        return Err(err(name.col, "there is already a scene with this name"));
                    }
                    let bars_word = arg(2, "a number of bars and : go here")?;
                    // `8:` or `8 :`.
                    let (count, mut k) = match bars_word.text.strip_suffix(':') {
                        Some(n) => (n, 3),
                        None => {
                            let colon = arg(3, ": goes here, after the bars")?;
                            if colon.text != ":" {
                                return Err(err(colon.col, ": goes here, after the bars"));
                            }
                            (bars_word.text, 4)
                        }
                    };
                    let bars: u32 = count
                        .parse()
                        .ok()
                        .filter(|b| (1..=MAX_BARS).contains(b))
                        .ok_or(err(bars_word.col, "a scene is 1 to 256 bars"))?;
                    let (mut clips, mut autos, mut snapshots) =
                        (Vec::new(), Vec::new(), Vec::new());
                    while let Some(w) = ws.get(k) {
                        k += 1;
                        if let Some(name) =
                            w.text.strip_prefix('[').and_then(|n| n.strip_suffix(']'))
                        {
                            let Some(s) = song.snapshots.iter().position(|s| s.name == name) else {
                                return Err(err(w.col, "no snapshot has this name"));
                            };
                            if snapshots.contains(&s) {
                                return Err(err(w.col, "this snapshot is already in the scene"));
                            }
                            snapshots.push(s);
                        } else if let Some(a) = song.autos.iter().position(|a| a.name == w.text) {
                            if autos.contains(&a) {
                                return Err(err(w.col, "this auto is already in the scene"));
                            }
                            autos.push(a);
                        } else {
                            let Some(f) = song.clips.iter().position(|f| f.name == w.text) else {
                                return Err(err(
                                    w.col,
                                    "no clip, auto or [snapshot] has this name",
                                ));
                            };
                            if clips.contains(&f) {
                                return Err(err(w.col, "this clip is already in the scene"));
                            }
                            clips.push(f);
                        }
                    }
                    if song.scenes.len() >= MAX_SCENES {
                        return Err(err(first.col, "a song has at most 256 scenes"));
                    }
                    song.scenes.push(Scene {
                        name: name.text.to_string(),
                        bars,
                        clips,
                        autos,
                        snapshots,
                    });
                }
                "auto" => {
                    let name = arg(1, "an auto name goes here")?;
                    if !is_name(name.text) {
                        return Err(err(
                            name.col,
                            "a name is a letter, then letters, digits or _",
                        ));
                    }
                    if song.autos.iter().any(|a| a.name == name.text)
                        || song.clips.iter().any(|f| f.name == name.text)
                    {
                        return Err(err(
                            name.col,
                            "there is already a clip or auto with this name",
                        ));
                    }
                    let eq = arg(2, "= and a target.Param go here")?;
                    if eq.text != "=" {
                        return Err(err(eq.col, "= and a target.Param go here"));
                    }
                    let tp = arg(3, "a target.Param goes here")?;
                    let (target, param) =
                        target_param(&song, tp.text).map_err(|msg| SongError {
                            line,
                            col: tp.col,
                            msg,
                        })?;
                    if let Target::Track(t) = target {
                        let at = err(param_col(tp.col, tp.text), "");
                        check_reads(&song.tracks, &models, t, param, at, &mut later)?;
                    }
                    let Some(last) = ws.last() else {
                        return Err(err(first.col, "a length in bars goes here: /4"));
                    };
                    let bars: u32 = last
                        .text
                        .strip_prefix('/')
                        .and_then(|n| n.parse().ok())
                        .filter(|b| (1..=MAX_BARS).contains(b))
                        .ok_or(err(last.col, "a length in bars goes last: /1 to /256"))?;
                    let values = ws.get(4..ws.len().saturating_sub(1)).unwrap_or(&[]);
                    let number = |w: &Word<'_>| {
                        w.text
                            .parse::<f32>()
                            .ok()
                            .filter(|v| v.is_finite())
                            .ok_or(err(w.col, "a value is a number"))
                    };
                    let shape = match values.first() {
                        // A quoted sequence, spaces and all (#683).
                        Some(w) if w.text.starts_with('"') => {
                            let text: String = body
                                .chars()
                                .skip(w.col - 1)
                                .take(last.col - w.col)
                                .collect();
                            let text = text.trim_end();
                            if !text.ends_with('"') || text.matches('"').count() != 2 {
                                return Err(err(
                                    w.col,
                                    "a sequence is one quoted text: \"<300 [800 1200]>\"",
                                ));
                            }
                            let signal = Signal::parse(text, &mut nodes)
                                .map_err(|(c, m)| err(w.col + c - 1, m))?;
                            Shape::Seq(signal)
                        }
                        Some(w) if w.text == "ramp" => {
                            let (Some(a), Some(b)) = (values.get(1), values.get(2)) else {
                                return Err(err(w.col, "a ramp goes from one value to another"));
                            };
                            if let Some(extra) = values.get(3) {
                                return Err(err(extra.col, "unexpected text"));
                            }
                            Shape::Ramp(number(a)?, number(b)?)
                        }
                        Some(_) => {
                            if values.len() > MAX_VALUES {
                                return Err(err(tp.col, "a lane has at most 64 values"));
                            }
                            Shape::Steps(values.iter().map(number).collect::<Result<_, _>>()?)
                        }
                        None => return Err(err(last.col, "values or a ramp go before the length")),
                    };
                    if song.autos.len() >= MAX_AUTOS {
                        return Err(err(first.col, "a song has at most 32 autos"));
                    }
                    song.autos.push(Auto {
                        name: name.text.to_string(),
                        target,
                        param,
                        shape,
                        bars,
                    });
                }
                "samples" => {
                    let track = arg(1, "a track goes here, then a pack or kit id")?;
                    let Some(t) = song.tracks.iter().position(|t| t.name == track.text) else {
                        return Err(err(track.col, "no track has this name"));
                    };
                    if song.tracks.get(t).is_some_and(|t| t.kind == Kind::Synth) {
                        return Err(err(track.col, "samples go on a sampler or drums track"));
                    }
                    if song.samples.iter().any(|(u, _)| *u == t) {
                        return Err(err(track.col, "this track already has its samples"));
                    }
                    if song.zones.iter().any(|z| z.track == t) {
                        return Err(err(
                            track.col,
                            "this track has zone lines: a track has a pack or its zones, not both",
                        ));
                    }
                    if song.pads.iter().any(|p| p.track == t) {
                        return Err(err(
                            track.col,
                            "this track has pad lines: a track has a kit or its pads, not both",
                        ));
                    }
                    let id = arg(2, "a pack or kit id goes here, as upright-piano-kw")?;
                    if !is_sample_id(id.text) {
                        return Err(err(
                            id.col,
                            "an id is lower-case letters, digits and -, as upright-piano-kw",
                        ));
                    }
                    if let Some(extra) = ws.get(3) {
                        return Err(err(extra.col, "unexpected text"));
                    }
                    song.samples.push((t, id.text.to_string()));
                }
                "zone" => {
                    let track = arg(1, "a track goes here, then a zone number, 1 to 64")?;
                    let Some(t) = song.tracks.iter().position(|t| t.name == track.text) else {
                        return Err(err(track.col, "no track has this name"));
                    };
                    if song.tracks.get(t).is_some_and(|t| t.kind != Kind::Sampler) {
                        return Err(err(track.col, "zones go on a sampler track"));
                    }
                    if song.samples.iter().any(|(u, _)| *u == t) {
                        return Err(err(
                            track.col,
                            "this track has a samples line: a track has a pack or its zones, not both",
                        ));
                    }
                    let n = arg(2, "a zone number goes here, 1 to 64")?;
                    let number = n
                        .text
                        .parse::<usize>()
                        .ok()
                        .filter(|n| zone::is_zone_number(*n))
                        .ok_or(err(n.col, "a zone number is 1 to 64"))?;
                    if song
                        .zones
                        .iter()
                        .any(|z| z.track == t && z.zone + 1 == number)
                    {
                        return Err(err(n.col, "this track already has this zone"));
                    }
                    let eq = arg(3, "= and a source go here")?;
                    if eq.text != "=" {
                        return Err(err(eq.col, "= and a source go here"));
                    }
                    let src = arg(4, "a source goes here, as upright-piano-kw/C4vH")?;
                    let source = src.text.strip_suffix(':').unwrap_or(src.text);
                    if !zone::is_zone_source(source) {
                        return Err(err(
                            src.col,
                            "a zone's source is a pack's sample, as upright-piano-kw/C4vH, or @ and a file's hash",
                        ));
                    }
                    let mut k = 5;
                    if !src.text.ends_with(':') {
                        if let Some(colon) = ws.get(k) {
                            if colon.text != ":" {
                                return Err(err(colon.col, ": and the zone's fields go here"));
                            }
                            k += 1;
                        }
                    }
                    let mut fields = crate::sampler::Zone::default();
                    while let Some(name) = ws.get(k) {
                        let v = ws.get(k + 1).ok_or(err(
                            body.trim_end().chars().count() + 1,
                            "a value goes here",
                        ))?;
                        let value = v.text.strip_suffix(',').unwrap_or(v.text);
                        if !zone::FIELD_NAMES.contains(&name.text) {
                            return Err(err(
                                name.col,
                                "a zone field: keylo keyhi vello velhi root tune level loop loopstart loopend seqlen seqpos release",
                            ));
                        }
                        zone::set_field(&mut fields, name.text, value)
                            .map_err(|msg| err(v.col, msg))?;
                        k += 2;
                    }
                    song.zones.push(zone::SongZone {
                        track: t,
                        zone: number - 1,
                        source: source.to_string(),
                        fields,
                    });
                }
                "pad" => {
                    let track = arg(1, "a track goes here, then a pad number, 1 to 16")?;
                    let Some(t) = song.tracks.iter().position(|t| t.name == track.text) else {
                        return Err(err(track.col, "no track has this name"));
                    };
                    // A NES synth plays its pads through the DMC (#112).
                    let nes = song.tracks.get(t).is_some_and(|tr| {
                        tr.setting
                            .and_then(|i| song.settings.get(i))
                            .map(|st| st.preset)
                            .or(tr.preset)
                            .is_some_and(|p| p.model().uses_nes())
                    });
                    if song.tracks.get(t).is_some_and(|t| t.kind == Kind::Synth) && !nes {
                        return Err(err(
                            track.col,
                            "pads go on a drums or sampler track, or a Nes synth",
                        ));
                    }
                    if song.samples.iter().any(|(u, _)| *u == t) {
                        return Err(err(
                            track.col,
                            "this track has a samples line: a track has a kit or its pads, not both",
                        ));
                    }
                    let n = arg(2, "a pad number goes here, 1 to 16")?;
                    let number = n
                        .text
                        .parse::<usize>()
                        .ok()
                        .filter(|n| pad::is_pad_number(*n))
                        .ok_or(err(n.col, "a pad number is 1 to 16"))?;
                    if song
                        .pads
                        .iter()
                        .any(|p| p.track == t && p.pad + 1 == number)
                    {
                        return Err(err(n.col, "this track already has this pad"));
                    }
                    let eq = arg(3, "= and a source go here")?;
                    if eq.text != "=" {
                        return Err(err(eq.col, "= and a source go here"));
                    }
                    let src = arg(4, "a source goes here, as audiophob/kick")?;
                    let source = src.text.strip_suffix(':').unwrap_or(src.text);
                    if !zone::is_zone_source(source) {
                        return Err(err(
                            src.col,
                            "a pad's source is a pack's sample, as audiophob/kick, or @ and a file's hash",
                        ));
                    }
                    let mut k = 5;
                    if !src.text.ends_with(':') {
                        if let Some(colon) = ws.get(k) {
                            if colon.text != ":" {
                                return Err(err(colon.col, ": and the pad's fields go here"));
                            }
                            k += 1;
                        }
                    }
                    let mut fields = crate::padsampler::PadCfg::default();
                    while let Some(name) = ws.get(k) {
                        let v = ws.get(k + 1).ok_or(err(
                            body.trim_end().chars().count() + 1,
                            "a value goes here",
                        ))?;
                        let value = v.text.strip_suffix(',').unwrap_or(v.text);
                        if !pad::FIELD_NAMES.contains(&name.text) {
                            return Err(err(
                                name.col,
                                "a pad field: tune level pan decay choke vellevel velstart oneshot out",
                            ));
                        }
                        pad::set_field(&mut fields, name.text, value)
                            .map_err(|msg| err(v.col, msg))?;
                        k += 2;
                    }
                    song.pads.push(pad::SongPad {
                        track: t,
                        pad: number - 1,
                        source: source.to_string(),
                        fields,
                    });
                }
                "table" | "attack" => {
                    let kind = if keyword == "table" {
                        recipe::Kind::Table
                    } else {
                        recipe::Kind::Attack
                    };
                    let n = arg(1, "a user slot goes here, 1 to 8")?;
                    let slot = n
                        .text
                        .parse::<usize>()
                        .ok()
                        .filter(|s| (1..=kind.slots()).contains(s))
                        .ok_or(err(n.col, "a user slot is 1 to 8"))?;
                    if song
                        .recipes
                        .iter()
                        .any(|r| r.kind == kind && r.slot == slot)
                    {
                        return Err(err(n.col, "this user slot already has its recipe"));
                    }
                    let eq = arg(2, "= and a source go here")?;
                    if eq.text != "=" {
                        return Err(err(eq.col, "= and a source go here"));
                    }
                    let src = arg(3, "a source goes here, as upright-piano-kw/C4vH")?;
                    let source = src.text.strip_suffix(':').unwrap_or(src.text);
                    if !recipe::is_source(source) {
                        return Err(err(
                            src.col,
                            "a source is a pack's sample, as upright-piano-kw/C4vH, @ and a file's hash, synth:Preset:note or plaits:h:t:m",
                        ));
                    }
                    let mut r = recipe::Recipe::new(kind, slot, source);
                    let mut k = 4;
                    let mut last = src.text;
                    // `A ~ B`: a second sound to morph with.
                    if !src.text.ends_with(':') && ws.get(4).is_some_and(|w| w.text == "~") {
                        let b = arg(5, "a second source goes here, to morph with")?;
                        let other = b.text.strip_suffix(':').unwrap_or(b.text);
                        if !recipe::is_source(other) {
                            return Err(err(
                                b.col,
                                "a source is a pack's sample, @ and a file's hash, synth:Preset:note or plaits:h:t:m",
                            ));
                        }
                        r.other = Some(other.to_string());
                        last = b.text;
                        k = 6;
                    }
                    if !last.ends_with(':') {
                        if let Some(colon) = ws.get(k) {
                            if colon.text != ":" {
                                return Err(err(colon.col, ": and the transforms go here"));
                            }
                            k += 1;
                        }
                    }
                    while let Some(name) = ws.get(k) {
                        let v = ws.get(k + 1).ok_or(err(
                            body.trim_end().chars().count() + 1,
                            "a value goes here",
                        ))?;
                        let value = v
                            .text
                            .strip_suffix(',')
                            .unwrap_or(v.text)
                            .parse::<f32>()
                            .ok()
                            .filter(|x| x.is_finite())
                            .ok_or(err(v.col, "a value is a number"))?;
                        if !r.set(name.text, value) {
                            return Err(err(
                                name.col,
                                "a transform of the lab (as inharmonic), a morph position (mamp …, sweep), or window or hop, with a value it takes",
                            ));
                        }
                        k += 2;
                    }
                    song.recipes.push(r);
                }
                "strip" | "group" | "master" | "master:" => {
                    let line = parse_mix(&song, &ws, line, body)?;
                    if song.mix.iter().any(|m| m.at == line.at) {
                        return Err(err(first.col, "this strip already has a line"));
                    }
                    song.mix.push(line);
                }
                "snapshot" => {
                    let w = arg(1, "a snapshot name and : go here")?;
                    let name = w.text.strip_suffix(':').unwrap_or(w.text);
                    if !is_name(name) {
                        return Err(err(w.col, "a name is a letter, then letters, digits or _"));
                    }
                    if song.snapshots.iter().any(|s| s.name == name) {
                        return Err(err(w.col, "there is already a snapshot with this name"));
                    }
                    let mut k = 2;
                    if !w.text.ends_with(':') {
                        let colon = arg(2, ": goes here, after the name")?;
                        if colon.text != ":" {
                            return Err(err(colon.col, ": goes here, after the name"));
                        }
                        k = 3;
                    }
                    let mut sets = Vec::new();
                    while let Some(tp) = ws.get(k) {
                        let (target, param) =
                            target_param(&song, tp.text).map_err(|msg| SongError {
                                line,
                                col: tp.col,
                                msg,
                            })?;
                        if let Target::Track(t) = target {
                            let at = err(param_col(tp.col, tp.text), "");
                            check_reads(&song.tracks, &models, t, param, at, &mut later)?;
                        }
                        let v = ws.get(k + 1).ok_or(err(
                            body.trim_end().chars().count() + 1,
                            "a value goes here",
                        ))?;
                        let text = v.text.strip_suffix(',').unwrap_or(v.text);
                        let value = text
                            .parse::<f32>()
                            .ok()
                            .filter(|x| x.is_finite())
                            .ok_or(err(v.col, "a value is a number"))?;
                        if sets.len() >= MAX_SETS {
                            return Err(err(tp.col, "a snapshot has at most 32 settings"));
                        }
                        sets.push((target, param, value));
                        k += 2;
                        if !v.text.ends_with(',') {
                            if let Some(extra) = ws.get(k) {
                                return Err(err(extra.col, "a comma goes between settings"));
                            }
                        }
                    }
                    if sets.is_empty() {
                        return Err(err(
                            body.trim_end().chars().count() + 1,
                            "settings go here: target.Param value, …",
                        ));
                    }
                    if song.snapshots.len() >= MAX_SNAPSHOTS {
                        return Err(err(first.col, "a song has at most 32 snapshots"));
                    }
                    song.snapshots.push(Snapshot {
                        name: name.to_string(),
                        sets,
                    });
                }
                "mod" => {
                    let tp = arg(1, "a target.param goes here")?;
                    let (target, param) =
                        target_param(&song, tp.text).map_err(|msg| SongError {
                            line,
                            col: tp.col,
                            msg,
                        })?;
                    if let Target::Track(t) = target {
                        let at = err(param_col(tp.col, tp.text), "");
                        check_reads(&song.tracks, &models, t, param, at, &mut later)?;
                    }
                    if song
                        .mods
                        .iter()
                        .any(|m| m.clip.is_none() && m.target == target && m.param == param)
                    {
                        return Err(err(tp.col, "this parameter already has a mod"));
                    }
                    let eq = arg(2, "= and a signal go here")?;
                    if eq.text != "=" {
                        return Err(err(eq.col, "= and a signal go here"));
                    }
                    let at = ws
                        .get(3)
                        .map_or(body.trim_end().chars().count() + 1, |w| w.col);
                    let text = body
                        .char_indices()
                        .nth(at - 1)
                        .and_then(|(b, _)| body.get(b..))
                        .unwrap_or("");
                    let signal =
                        Signal::parse(text, &mut nodes).map_err(|(c, m)| err(at + c - 1, m))?;
                    per_voice_fits(&song, target, param, &signal).map_err(|m| err(at, m))?;
                    if song.mods.len() >= MAX_MODS {
                        return Err(err(first.col, "a song has at most 32 mods"));
                    }
                    song.mods.push(Mod {
                        target,
                        param,
                        signal,
                        clip: None,
                        alias: tp
                            .text
                            .split_once('.')
                            .and_then(|(_, p)| alias(p))
                            .filter(|(_, p)| *p == param)
                            .map(|(a, _)| a),
                    });
                }
                "arrange" => {
                    if !song.arrange.is_empty() {
                        return Err(err(first.col, "a song has one arrange line"));
                    }
                    let mut order = Vec::new();
                    for w in ws.iter().skip(1) {
                        let Some(s) = song.scenes.iter().position(|s| s.name == w.text) else {
                            return Err(err(w.col, "no scene has this name"));
                        };
                        if order.len() >= MAX_ARRANGE {
                            return Err(err(w.col, "an arrangement has at most 256 scenes"));
                        }
                        order.push(s);
                    }
                    if order.is_empty() {
                        return Err(err(
                            body.trim_end().chars().count() + 1,
                            "scenes go here, in the order they play",
                        ));
                    }
                    song.arrange = order;
                }
                "loop" => {
                    let bar = |k: usize| -> Result<u32, SongError> {
                        let w = arg(k, "the first and last bar go here")?;
                        w.text
                            .parse()
                            .ok()
                            .filter(|b| *b >= 1)
                            .ok_or(err(w.col, "a bar counts from 1"))
                    };
                    let (from, to) = (bar(1)?, bar(2)?);
                    expect_end(3)?;
                    if to < from {
                        return Err(err(first.col, "the last bar comes after the first"));
                    }
                    loop_at = Some(line);
                    song.loop_bars = Some((from, to));
                }
                _ => {
                    return Err(err(
                        first.col,
                        "a line starts with tempo, swing, scale, setting, track, samples, zone, pad, table, attack, strip, group, master, clip, auto, snapshot, mod, scene, arrange or loop",
                    ));
                }
            }
        }
        if let Some((f, at)) = open {
            check_lanes(&song, f, at)?;
        }
        if let Some(c) = open_code {
            close_code(&mut song, c)?;
        }
        if let (Some(line), Some((_, to))) = (loop_at, song.loop_bars) {
            let msg = if song.arrange.is_empty() {
                Some("a loop needs an arrange line")
            } else if u64::from(to) > song.bars() {
                Some("the loop ends after the arrangement")
            } else {
                None
            };
            if let Some(msg) = msg {
                return Err(SongError {
                    line,
                    col: 1,
                    msg: msg.into(),
                });
            }
        }
        for t in 0..song.tracks.len() {
            let model = models.iter().find(|(i, _)| *i == t).map(|(_, m)| *m);
            if song.tracks.get(t).is_some_and(|tr| tr.preset.is_none()) {
                let picked = pick(&song, t, model);
                if let Some(tr) = song.tracks.get_mut(t) {
                    tr.preset = picked;
                    tr.picked = model.is_none() && tr.setting.is_none();
                }
            }
        }
        for (at, t, param) in later {
            let model = song
                .tracks
                .get(t)
                .and_then(|tr| tr.preset)
                .map(|p| p.model());
            if let Some(m) = model.filter(|m| !m.reads(param)) {
                return Err(SongError {
                    msg: m.unread(param).into(),
                    ..at
                });
            }
        }
        song.comments = Comments::collect(text, &song);
        Ok(song)
    }

    /// Bars in the arrangement; 0 without one.
    pub fn bars(&self) -> u64 {
        self.arrange
            .iter()
            .filter_map(|s| self.scenes.get(*s))
            .map(|s| u64::from(s.bars))
            .sum()
    }

    /// Whether track `t`'s clips play (#355): while any track is soloed only
    /// the soloed ones, muted or not; else every track not muted.
    pub fn heard(&self, t: usize) -> bool {
        let Some(track) = self.tracks.get(t) else {
            return false;
        };
        if self.tracks.iter().any(|x| x.solo) {
            track.solo
        } else {
            !track.mute
        }
    }

    /// Mute or solo track `t`; false for an unknown track.
    pub fn set_track_flags(&mut self, t: usize, mute: bool, solo: bool) -> bool {
        let Some(track) = self.tracks.get_mut(t) else {
            return false;
        };
        track.mute = mute;
        track.solo = solo;
        true
    }

    /// Where clock step `k` falls: inside the loop region the song wraps.
    /// Scans the arrangement (at most `MAX_ARRANGE` entries); never allocates.
    pub fn at(&self, k: u64) -> At {
        if self.arrange.is_empty() {
            return At::Free(k);
        }
        let mut s = k;
        if let Some((from, to)) = self.loop_bars {
            let start = u64::from(from - 1) * STEPS_PER_BAR;
            let end = u64::from(to) * STEPS_PER_BAR;
            if s >= end && end > start {
                s = start + (s - start) % (end - start);
            }
        }
        let mut begin = 0;
        for (entry, sec) in self.arrange.iter().enumerate() {
            let Some(scene) = self.scenes.get(*sec) else {
                continue;
            };
            let len = u64::from(scene.bars) * STEPS_PER_BAR;
            if s < begin + len {
                return At::In {
                    entry,
                    scene: *sec,
                    local: s - begin,
                };
            }
            begin += len;
        }
        At::End
    }

    /// The canonical text: parsing it gives this song back.
    pub fn print(&self) -> String {
        let mut lines = vec![
            format!("tempo {}", self.tempo),
            format!("swing {}", self.swing),
        ];
        if let Some(s) = &self.scale {
            lines.push(format!(
                "scale {}{} {}",
                crate::notes::NAMES
                    .get(usize::from(s.root))
                    .copied()
                    .unwrap_or("c"),
                self.scale_octave.map_or(String::new(), |o| o.to_string()),
                s.mode.name()
            ));
        }
        for st in &self.settings {
            let mut line = format!(
                "setting {} = {} {}",
                st.name,
                model_name(st.preset.model()),
                preset_name(st.preset)
            );
            let sets: Vec<String> = st
                .sets
                .iter()
                .map(|(p, v)| format!("{} {}", param_name(*p), v))
                .collect();
            if !sets.is_empty() {
                line.push_str(": ");
                line.push_str(&sets.join(", "));
            }
            lines.push(line);
            if let Some(code) = &st.code {
                lines.extend(code.lines().map(|l| {
                    if l.is_empty() {
                        String::new()
                    } else {
                        format!("  {l}")
                    }
                }));
            }
        }
        lines.extend(self.tracks.iter().map(|t| {
            let setting = t.setting.and_then(|i| self.settings.get(i));
            let flags = match (t.mute, t.solo) {
                (true, true) => " mute solo",
                (true, false) => " mute",
                (false, true) => " solo",
                (false, false) => "",
            };
            let line = match (setting, t.preset) {
                (Some(st), _) => format!("track {} {} {}", t.name, t.kind.name(), st.name),
                (None, Some(p)) => format!(
                    "track {} {} {} {}",
                    t.name,
                    t.kind.name(),
                    model_name(p.model()),
                    preset_name(p)
                ),
                (None, None) => format!("track {} {}", t.name, t.kind.name()),
            };
            line + flags
        }));
        for (t, id) in &self.samples {
            let name = self.tracks.get(*t).map_or("", |x| x.name.as_str());
            lines.push(format!("samples {name} {id}"));
        }
        for z in &self.zones {
            let name = self.tracks.get(z.track).map_or("", |x| x.name.as_str());
            lines.push(z.print(name));
        }
        for p in &self.pads {
            let name = self.tracks.get(p.track).map_or("", |x| x.name.as_str());
            lines.push(p.print(name));
        }
        lines.extend(self.recipes.iter().map(recipe::Recipe::print));
        for m in &self.mix {
            let at = match m.at {
                Mix::Track(t) => format!(
                    "strip {}",
                    self.tracks.get(t).map_or("", |x| x.name.as_str())
                ),
                Mix::Strip(i) => format!("strip strip{}", i + 1),
                Mix::Group(g) => match &m.name {
                    Some(n) => format!("group {} {n}", g + 1),
                    None => format!("group {}", g + 1),
                },
                Mix::Master => "master".to_string(),
            };
            let sets: Vec<String> = m
                .sets
                .iter()
                .map(|(p, v)| format!("{} {}", param_name(*p), mix_text(*p, *v)))
                .collect();
            if sets.is_empty() {
                // A group with only its name (#214).
                lines.push(at);
            } else {
                lines.push(format!("{at}: {}", sets.join(", ")));
            }
        }
        for (fi, f) in self.clips.iter().enumerate() {
            let track = self.tracks.get(f.track).map_or("", |t| t.name.as_str());
            lines.push(String::new());
            if let Some(src) = &f.from {
                lines.push(format!(
                    "clip {} = {} {src}{}",
                    f.name,
                    track,
                    self.methods_of(fi)
                ));
                continue;
            }
            if let Some(n) = &f.notes {
                let live = if f.live { " live" } else { "" };
                let bars = if matches!(n.seq, notes::Seq::Timed(_))
                    || matches!(
                        n.seq,
                        notes::Seq::Generated(notes::Gen::Arp { bars: 2.., .. })
                    ) {
                    format!(" bars {}", n.bars)
                } else {
                    String::new()
                };
                let voicing = f
                    .voicing
                    .map_or(String::new(), |v| format!(" {}", v.print()));
                lines.push(format!(
                    "clip {} = {}{live}{bars}{voicing}{}",
                    f.name,
                    track,
                    self.methods_of(fi)
                ));
                lines.push(format!("  {}", n.print()));
                continue;
            }
            if let Some(b) = &f.beat {
                let grid = if b.grid {
                    format!(" /{}", f.grid)
                } else {
                    String::new()
                };
                lines.push(format!(
                    "clip {} = {}{grid}{}",
                    f.name,
                    track,
                    self.methods_of(fi)
                ));
                lines.push(format!("  {}", b.text));
                continue;
            }
            lines.push(format!(
                "clip {} = {} /{}{}",
                f.name,
                track,
                f.grid,
                self.methods_of(fi)
            ));
            for l in &f.lanes {
                let steps: String = match &l.call {
                    Some(e) => e.print(),
                    None => (0..l.steps.len()).map(|n| l.step_text(n)).collect(),
                };
                lines.push(format!("  {} {}", l.pad.name(), steps));
            }
        }
        let mod_lines = self.mods.iter().any(|m| m.clip.is_none());
        if !self.autos.is_empty() || !self.snapshots.is_empty() || mod_lines {
            lines.push(String::new());
        }
        for a in &self.autos {
            let shape = match &a.shape {
                Shape::Steps(v) => v
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
                Shape::Ramp(x, y) => format!("ramp {x} {y}"),
                Shape::Seq(s) => s.to_string(),
            };
            lines.push(format!(
                "auto {} = {}.{} {} /{}",
                a.name,
                self.target_name(a.target),
                self.param_label(a.target, a.param, false),
                shape,
                a.bars
            ));
        }
        for s in &self.snapshots {
            let sets: Vec<String> = s
                .sets
                .iter()
                .map(|(t, p, v)| {
                    format!(
                        "{}.{} {}",
                        self.target_name(*t),
                        self.param_label(*t, *p, false),
                        v
                    )
                })
                .collect();
            lines.push(format!("snapshot {}: {}", s.name, sets.join(", ")));
        }
        for m in self.mods.iter().filter(|m| m.clip.is_none()) {
            lines.push(format!(
                "mod {}.{} = {}",
                self.target_name(m.target),
                m.alias
                    .map_or_else(|| self.param_label(m.target, m.param, true), str::to_string),
                m.signal
            ));
        }
        if !self.scenes.is_empty() {
            lines.push(String::new());
        }
        for s in &self.scenes {
            let mut line = format!("scene {} {}:", s.name, s.bars);
            let names = s
                .clips
                .iter()
                .filter_map(|f| self.clips.get(*f).map(|x| x.name.clone()))
                .chain(
                    s.autos
                        .iter()
                        .filter_map(|a| self.autos.get(*a).map(|x| x.name.clone())),
                )
                .chain(
                    s.snapshots
                        .iter()
                        .filter_map(|c| self.snapshots.get(*c).map(|x| format!("[{}]", x.name))),
                );
            for n in names {
                line.push(' ');
                line.push_str(&n);
            }
            lines.push(line);
        }
        if !self.arrange.is_empty() {
            let names: Vec<&str> = self
                .arrange
                .iter()
                .filter_map(|s| self.scenes.get(*s).map(|s| s.name.as_str()))
                .collect();
            lines.push(format!("arrange {}", names.join(" ")));
        }
        if let Some((from, to)) = self.loop_bars {
            lines.push(format!("loop {from} {to}"));
        }
        let mut lines = self.comments.apply(lines, self);
        lines.push(String::new());
        lines.join("\n")
    }

    /// Replace the generator call of note clip `clip` with the events it
    /// plays (`events`, or its own when `None`), written as mini-notation, and
    /// make it a fixed clip. False when it is not a generated clip or the
    /// events cannot be written back exactly.
    pub fn freeze(&mut self, clip: usize, events: Option<&[notes::Event]>) -> bool {
        let Some(f) = self.clips.get_mut(clip) else {
            return false;
        };
        let Some(n) = f.notes.as_ref() else {
            return false;
        };
        if !matches!(n.seq, notes::Seq::Generated(_) | notes::Seq::Euclid(..)) || f.from.is_some() {
            return false;
        }
        let Some(frozen) = notes::freeze(events.unwrap_or(&n.events), n.bars) else {
            return false;
        };
        let was = f.clone();
        f.notes = Some(frozen);
        f.live = false;
        self.rederive(clip, was)
    }

    // --- Arranger edits (#171): each keeps the song valid, false when refused.

    /// Put clip (`kind` 0), lane (1) or snapshot (2) `item` in scene `s`,
    /// or take it out.
    pub fn toggle(&mut self, s: usize, kind: u32, item: usize) -> bool {
        let exists = match kind {
            0 => item < self.clips.len(),
            1 => item < self.autos.len(),
            2 => item < self.snapshots.len(),
            _ => false,
        };
        let Some(sec) = self.scenes.get_mut(s) else {
            return false;
        };
        let list = match kind {
            0 => &mut sec.clips,
            1 => &mut sec.autos,
            _ => &mut sec.snapshots,
        };
        if !exists {
            return false;
        }
        match list.iter().position(|i| *i == item) {
            Some(at) => {
                list.remove(at);
            }
            None => list.push(item),
        }
        true
    }

    /// A new empty scene of `bars` bars named `partN`, added to the end of
    /// the arrangement; its index.
    pub fn add_scene(&mut self, bars: u32) -> Option<usize> {
        if self.scenes.len() >= MAX_SCENES
            || self.arrange.len() >= MAX_ARRANGE
            || !(1..=MAX_BARS).contains(&bars)
        {
            return None;
        }
        let name = (1..)
            .map(|n| format!("part{n}"))
            .find(|n| !self.scenes.iter().any(|s| &s.name == n))?;
        // The first scene holds every clip, so turning the arrangement on
        // keeps the music that was looping instead of silencing it.
        let clips = if self.arrange.is_empty() {
            (0..self.clips.len()).collect()
        } else {
            Vec::new()
        };
        self.scenes.push(Scene {
            name,
            bars,
            clips,
            autos: Vec::new(),
            snapshots: Vec::new(),
        });
        let s = self.scenes.len() - 1;
        self.arrange.push(s);
        Some(s)
    }

    pub fn set_bars(&mut self, s: usize, bars: u32) -> bool {
        match self.scenes.get_mut(s) {
            Some(sec) if (1..=MAX_BARS).contains(&bars) => {
                sec.bars = bars;
                self.fit_loop();
                true
            }
            _ => false,
        }
    }

    /// Play scene `s` at place `at` of the arrangement (`at` may be its length).
    pub fn arrange_insert(&mut self, at: usize, s: usize) -> bool {
        if s >= self.scenes.len() || at > self.arrange.len() || self.arrange.len() >= MAX_ARRANGE {
            return false;
        }
        self.arrange.insert(at, s);
        true
    }

    pub fn arrange_remove(&mut self, at: usize) -> bool {
        if at >= self.arrange.len() {
            return false;
        }
        self.arrange.remove(at);
        self.fit_loop();
        true
    }

    /// Move the entry at `from` to `to`, shifting the ones between.
    pub fn arrange_move(&mut self, from: usize, to: usize) -> bool {
        if from >= self.arrange.len() || to >= self.arrange.len() {
            return false;
        }
        let s = self.arrange.remove(from);
        self.arrange.insert(to, s);
        true
    }

    /// Loop bars `from` to `to` (from 1, inclusive); `0, 0` clears the loop.
    pub fn set_loop(&mut self, from: u32, to: u32) -> bool {
        if from == 0 && to == 0 {
            self.loop_bars = None;
            return true;
        }
        if from == 0 || to < from || u64::from(to) > self.bars() {
            return false;
        }
        self.loop_bars = Some((from, to));
        true
    }

    /// A loop that no longer fits the arrangement is dropped.
    fn fit_loop(&mut self) {
        if let Some((_, to)) = self.loop_bars {
            if u64::from(to) > self.bars() {
                self.loop_bars = None;
            }
        }
    }

    /// Whether scene `s` holds clip (`kind` 0), lane (1) or snapshot (2) `item`.
    pub fn scene_has(&self, s: usize, kind: u32, item: usize) -> bool {
        self.scenes.get(s).is_some_and(|sec| match kind {
            0 => sec.clips.contains(&item),
            1 => sec.autos.contains(&item),
            2 => sec.snapshots.contains(&item),
            _ => false,
        })
    }

    /// Do `op` on note clip `clip` (from the composer's note view) and write
    /// it back as notes, whatever notation it was in. False when it is not a
    /// written note clip (a generated one is frozen first), the edit names no
    /// note, or the result cannot be written.
    pub fn edit_note(&mut self, clip: usize, op: notes::Edit) -> bool {
        let Some(f) = self.clips.get_mut(clip) else {
            return false;
        };
        let Some(n) = f.notes.as_ref() else {
            return false;
        };
        // A patterned line's events are not its text: an edit would bake the
        // pattern in (ADR-0019).
        if matches!(n.seq, notes::Seq::Generated(_) | notes::Seq::Euclid(..))
            || !f.pattern.is_empty()
            || f.from.is_some()
        {
            return false;
        }
        // A timed line stays timed: its overlaps and velocities have no
        // mini-notation (#173).
        let timed = matches!(n.seq, notes::Seq::Timed(_));
        let edited = if timed {
            notes::edit_timed(&n.events, n.bars, op).and_then(|ev| notes::timed(ev, n.bars))
        } else {
            notes::edit(&n.events, n.bars, op).and_then(|ev| notes::freeze(&ev, n.bars))
        };
        let Some(edited) = edited else {
            return false;
        };
        let was = f.clone();
        f.notes = Some(edited);
        self.rederive(clip, was)
    }

    /// Clip `f`'s notes, or lanes, from the clip it is made from, through
    /// its methods (#649, #653).
    fn derive(&mut self, f: usize) -> Result<(), &'static str> {
        let Some(clip) = self.clips.get(f) else {
            return Ok(());
        };
        let Some(name) = &clip.from else {
            return Ok(());
        };
        let src = self
            .clips
            .iter()
            .take(f)
            .find(|c| &c.name == name)
            .ok_or("no clip with this name comes before this one")?;
        if let Some(n) = &src.notes {
            let line = Line {
                events: n.events.clone(),
                bars: n.bars,
            };
            let line = pattern::apply_all(&clip.pattern, line)?;
            let notes = Notes {
                events: line.events,
                bars: line.bars,
                ..n.clone()
            };
            if let Some(c) = self.clips.get_mut(f) {
                c.notes = Some(notes);
            }
            return Ok(());
        }
        let (events, bars) = src.hits().ok_or("this clip has no hits")?;
        if bars > notes::MAX_BARS {
            return Err("a lane has at most 64 steps");
        }
        let line = pattern::apply_all(&clip.pattern, Line { events, bars })?;
        let pads: Vec<LanePad> = src.lanes.iter().map(|l| l.pad).collect();
        let lanes = lanes_of(&line, src.grid, &pads)?;
        let grid = src.grid;
        if let Some(c) = self.clips.get_mut(f) {
            (c.lanes, c.grid) = (lanes, grid);
        }
        Ok(())
    }

    /// After clip `clip` changed from `was`: the clips made from clips
    /// again, in order. False, with `was` put back, when one no longer
    /// takes its methods.
    fn rederive(&mut self, clip: usize, was: Clip) -> bool {
        let all = |song: &mut Song| (0..song.clips.len()).try_for_each(|f| song.derive(f));
        if all(self).is_ok() {
            return true;
        }
        if let Some(f) = self.clips.get_mut(clip) {
            *f = was;
        }
        // They took their methods before the edit, so they take them again.
        let restored = all(self);
        debug_assert!(restored.is_ok(), "{restored:?}");
        false
    }

    /// Set one step; false when there is no such step.
    /// Set step `step` of a lane to play `r` times in its span (#242); see
    /// `Lane::set_ratchet`. Editing drops the lane's call, as a step does.
    pub fn set_ratchet(&mut self, clip: usize, lane: usize, step: usize, r: u8) -> bool {
        let Some(was) = self.clips.get(clip).filter(|f| f.from.is_none()).cloned() else {
            return false;
        };
        let Some(l) = self.clips.get_mut(clip).and_then(|f| f.lanes.get_mut(lane)) else {
            return false;
        };
        if !l.set_ratchet(step, r) {
            return false;
        }
        l.call = None;
        // The clip is lanes from now on (#659).
        if let Some(f) = self.clips.get_mut(clip) {
            f.beat = None;
        }
        self.rederive(clip, was)
    }

    pub fn set_step(&mut self, clip: usize, lane: usize, step: usize, to: Step) -> bool {
        // A clip made from a clip has no steps of its own (#653).
        let Some(was) = self.clips.get(clip).filter(|f| f.from.is_none()).cloned() else {
            return false;
        };
        let slot = self
            .clips
            .get_mut(clip)
            .and_then(|f| f.lanes.get_mut(lane))
            .and_then(|l| l.steps.get_mut(step));
        match slot {
            Some(s) => {
                *s = to;
                if let Some(f) = self.clips.get_mut(clip) {
                    f.beat = None;
                }
                if let Some(l) = self.clips.get_mut(clip).and_then(|f| f.lanes.get_mut(lane)) {
                    l.call = None;
                    // A step that can't repeat loses its ratchet (#242).
                    if !to.repeats() {
                        l.set_ratchet(step, 1);
                    }
                }
                self.rederive(clip, was)
            }
            None => false,
        }
    }
}

impl Song {
    /// The SuperCollider code of the setting track `t` plays, if it has one.
    pub fn code(&self, t: usize) -> Option<&str> {
        let i = self.tracks.get(t)?.setting?;
        self.settings.get(i)?.code.as_deref()
    }

    /// The patch track `t` plays: its preset, then its setting's changes.
    pub fn patch(&self, t: usize) -> Option<(Preset, &[(Param, f32)])> {
        let track = self.tracks.get(t)?;
        let sets = track
            .setting
            .and_then(|i| self.settings.get(i))
            .map_or(&[][..], |st| st.sets.as_slice());
        track.preset.map(|p| (p, sets))
    }

    /// Pick track `t`'s preset again, on `model`: the engine found a synth of
    /// that model to play a track whose preset was picked (a drum kit already
    /// in the rack). The text then names what plays.
    pub fn pick_on(&mut self, t: usize, model: Model) -> Option<Preset> {
        let p = pick(self, t, Some(model))?;
        self.tracks.get_mut(t)?.preset = Some(p);
        Some(p)
    }

    // --- Track edits from the composer (#213) ---------------------------------

    /// Does `model` take everything the song names on track `t`: its
    /// parameter methods' and `mod`, `auto` and snapshot parameters (#673)
    /// and its lanes' pads (#662)? A switch to a model that doesn't would
    /// print text that no longer parses (#680).
    pub fn takes_model(&self, t: usize, model: Model) -> bool {
        let on = |target: &Target| *target == Target::Track(t);
        self.mods
            .iter()
            .all(|m| !on(&m.target) || model.reads(m.param))
            && self
                .autos
                .iter()
                .all(|a| !on(&a.target) || model.reads(a.param))
            && self
                .snapshots
                .iter()
                .flat_map(|s| &s.sets)
                .all(|(target, p, _)| !on(target) || model.reads(*p))
            && self
                .clips
                .iter()
                .filter(|c| c.track == t)
                .flat_map(|c| &c.lanes)
                .all(|l| l.pad.plays_on(model))
    }

    /// Track `t` plays factory preset `preset`, without a setting; refused when
    /// the preset's model does not play the track's kind or take what the
    /// song names on it (#680).
    pub fn set_track_preset(&mut self, t: usize, preset: Preset) -> bool {
        let takes = self.takes_model(t, preset.model());
        match self.tracks.get_mut(t) {
            Some(tr) if takes && fits(tr.kind, Some(preset.model())) => {
                tr.preset = Some(preset);
                tr.setting = None;
                tr.picked = false;
                true
            }
            _ => false,
        }
    }

    /// Track `t` plays the song's setting `i`; refused as `set_track_preset` is.
    pub fn set_track_setting(&mut self, t: usize, i: usize) -> bool {
        let Some(preset) = self.settings.get(i).map(|st| st.preset) else {
            return false;
        };
        let takes = self.takes_model(t, preset.model());
        match self.tracks.get_mut(t) {
            Some(tr) if takes && fits(tr.kind, Some(preset.model())) => {
                tr.preset = Some(preset);
                tr.setting = Some(i);
                tr.picked = false;
                true
            }
            _ => false,
        }
    }

    /// A new setting of track `t`'s preset, `sets` and a Modular synth's
    /// `code`, named after the track (`bass`, else `bass2`, …), which the
    /// track then plays. `None` when the track has no preset, the song has 16
    /// settings or `sets` is more than a setting holds.
    pub fn add_setting(
        &mut self,
        t: usize,
        sets: Vec<(Param, f32)>,
        code: Option<String>,
    ) -> Option<usize> {
        if sets.len() > MAX_SETS || self.settings.len() >= MAX_TRACKS {
            return None;
        }
        let track = self.tracks.get(t)?;
        let preset = track.preset?;
        let base: String = track.name.chars().take(MAX_NAME - 2).collect();
        let taken =
            |n: &str| self.settings.iter().any(|st| st.name == n) || model_named(n).is_some();
        let name = std::iter::once(base.clone())
            .chain((2..=MAX_TRACKS + 1).map(|k| format!("{base}{k}")))
            .find(|n| !taken(n))?;
        self.settings.push(Setting {
            name,
            preset,
            sets,
            knobs: knob_names(code.as_deref()),
            code,
        });
        let i = self.settings.len() - 1;
        let tr = self.tracks.get_mut(t)?;
        tr.setting = Some(i);
        tr.picked = false;
        Some(i)
    }

    /// A track for a synth shown on screen (ADR-0027): named `base`, or
    /// `base_2`… when taken, of the kind `preset`'s model plays, on that
    /// preset. Its index, or `None` past the song's room.
    pub fn add_track(&mut self, base: &str, preset: Preset) -> Option<usize> {
        if self.tracks.len() >= MAX_TRACKS {
            return None;
        }
        let model = preset.model();
        let kind = if model.uses_drums() || model.uses_pads() {
            Kind::Drums
        } else if model.uses_sampler() {
            Kind::Sampler
        } else {
            Kind::Synth
        };
        let taken = |n: &str| self.tracks.iter().any(|t| t.name == n);
        let name = std::iter::once(base.to_string())
            .chain((2..=MAX_TRACKS + 1).map(|k| format!("{base}_{k}")))
            .find(|n| !taken(n))?;
        self.tracks.push(Track {
            name,
            kind,
            preset: Some(preset),
            setting: None,
            picked: false,
            mute: false,
            solo: false,
        });
        Some(self.tracks.len() - 1)
    }

    /// Name group bus `g` (0–7), or take its name away with `None` (#214):
    /// its `group` line, which goes when it has neither a name nor values.
    /// False for no such group or a name that isn't one.
    pub fn set_group_name(&mut self, g: usize, name: Option<&str>) -> bool {
        if g >= crate::mixer::GROUPS || name.is_some_and(|n| !is_name(n)) {
            return false;
        }
        match self.mix.iter_mut().find(|m| m.at == Mix::Group(g)) {
            Some(line) => line.name = name.map(str::to_string),
            None if name.is_some() => self.mix.push(MixLine {
                at: Mix::Group(g),
                name: name.map(str::to_string),
                sets: Vec::new(),
            }),
            None => {}
        }
        self.mix
            .retain(|m| m.at != Mix::Group(g) || m.name.is_some() || !m.sets.is_empty());
        true
    }

    /// The samples each track wants (#214): its `samples` line, else, for a
    /// General MIDI preset on a track without zone lines, the preset's pack
    /// (#652). The written lines come first, in order.
    pub fn sample_ids(&self) -> impl Iterator<Item = (usize, &str)> {
        let written = self.samples.iter().map(|(t, id)| (*t, id.as_str()));
        let implied = self.tracks.iter().enumerate().filter_map(|(t, track)| {
            let pack = track.preset?.pack()?;
            let free = !self.samples.iter().any(|(u, _)| *u == t)
                && !self.zones.iter().any(|z| z.track == t);
            free.then_some((t, pack))
        });
        written.chain(implied)
    }

    /// Track `t` wants the samples of pack or kit `id` (#214): its `samples`
    /// line, replaced or added. False for no such track, a synth track, or an
    /// id that isn't one.
    pub fn set_samples(&mut self, t: usize, id: &str) -> bool {
        if !is_sample_id(id) || self.tracks.get(t).is_none_or(|x| x.kind == Kind::Synth) {
            return false;
        }
        match self.samples.iter_mut().find(|(u, _)| *u == t) {
            Some((_, have)) => id.clone_into(have),
            None => self.samples.push((t, id.to_string())),
        }
        // A pack or its zones, a kit or its pads, not both (#522, #527).
        self.zones.retain(|z| z.track != t);
        self.pads.retain(|p| p.track != t);
        true
    }

    /// Track `t`'s pad lines (#527), replacing any, in pad order; they take
    /// the place of its `samples` line. None takes both away. False for no
    /// such track or a synth track.
    pub fn set_pads(&mut self, t: usize, mut pads: Vec<pad::SongPad>) -> bool {
        if self.tracks.get(t).is_none_or(|x| x.kind == Kind::Synth) {
            return false;
        }
        pads.sort_by_key(|p| p.pad);
        pads.dedup_by_key(|p| p.pad);
        self.samples.retain(|(u, _)| *u != t);
        let at = self
            .pads
            .iter()
            .position(|p| p.track == t)
            .unwrap_or(self.pads.len());
        self.pads.retain(|p| p.track != t);
        let at = at.min(self.pads.len());
        self.pads.splice(
            at..at,
            pads.into_iter().map(|p| pad::SongPad { track: t, ..p }),
        );
        true
    }

    /// Track `t`'s zone lines (#522), replacing any, in zone order; they
    /// take the place of its `samples` line. None takes both away. False for
    /// no such track or one that isn't a sampler.
    pub fn set_zones(&mut self, t: usize, mut zones: Vec<zone::SongZone>) -> bool {
        if self.tracks.get(t).is_none_or(|x| x.kind != Kind::Sampler) {
            return false;
        }
        zones.sort_by_key(|z| z.zone);
        zones.dedup_by_key(|z| z.zone);
        self.samples.retain(|(u, _)| *u != t);
        let at = self
            .zones
            .iter()
            .position(|z| z.track == t)
            .unwrap_or(self.zones.len());
        self.zones.retain(|z| z.track != t);
        let at = at.min(self.zones.len());
        self.zones.splice(
            at..at,
            zones.into_iter().map(|z| zone::SongZone { track: t, ..z }),
        );
        true
    }

    /// The recipe of a user slot (ADR-0033): replaced or added.
    pub fn set_recipe(&mut self, r: recipe::Recipe) {
        match self
            .recipes
            .iter_mut()
            .find(|have| have.kind == r.kind && have.slot == r.slot)
        {
            Some(have) => *have = r,
            None => self.recipes.push(r),
        }
    }

    /// Take track `t` out (ADR-0027), when nothing of the song plays or moves
    /// it: no clip, lane, modulation or snapshot names it. Its mixer line
    /// goes; later tracks move down one. False when it has music.
    pub fn remove_track(&mut self, t: usize) -> bool {
        let names = |target: Target| target == Target::Track(t);
        if t >= self.tracks.len()
            || self.clips.iter().any(|f| f.track == t)
            || self.autos.iter().any(|a| names(a.target))
            || self.mods.iter().any(|m| names(m.target))
            || self
                .snapshots
                .iter()
                .any(|s| s.sets.iter().any(|(tg, _, _)| names(*tg)))
        {
            return false;
        }
        self.tracks.remove(t);
        self.mix.retain(|m| m.at != Mix::Track(t));
        self.samples.retain(|(u, _)| *u != t);
        self.zones.retain(|z| z.track != t);
        self.pads.retain(|p| p.track != t);
        let down = |u: &mut usize| {
            if *u > t {
                *u -= 1;
            }
        };
        for f in &mut self.clips {
            down(&mut f.track);
        }
        for (u, _) in &mut self.samples {
            down(u);
        }
        for z in &mut self.zones {
            down(&mut z.track);
        }
        for p in &mut self.pads {
            down(&mut p.track);
        }
        for m in &mut self.mix {
            if let Mix::Track(u) = &mut m.at {
                down(u);
            }
        }
        let target = |tg: &mut Target| {
            if let Target::Track(u) = tg {
                down(u);
            }
        };
        self.autos.iter_mut().for_each(|a| target(&mut a.target));
        self.mods.iter_mut().for_each(|m| target(&mut m.target));
        for s in &mut self.snapshots {
            s.sets.iter_mut().for_each(|(tg, _, _)| target(tg));
        }
        true
    }

    /// Track `t`'s sound as the hands left it (ADR-0027): the setting only it
    /// plays takes `sets` and `code` in place; without one, a new setting
    /// named after the track does, unless nothing differs from the preset.
    /// False without a preset or past a setting's room.
    pub fn fold_sound(&mut self, t: usize, sets: Vec<(Param, f32)>, code: Option<String>) -> bool {
        let Some(tr) = self.tracks.get(t) else {
            return false;
        };
        let Some(preset) = tr.preset else {
            return false;
        };
        if sets.len() > MAX_SETS {
            return false;
        }
        let shared = |i: usize| {
            self.tracks
                .iter()
                .enumerate()
                .any(|(u, other)| u != t && other.setting == Some(i))
        };
        match tr.setting.filter(|&i| !shared(i)) {
            Some(i) => match self.settings.get_mut(i) {
                Some(st) => {
                    st.preset = preset;
                    st.sets = sets;
                    st.knobs = knob_names(code.as_deref());
                    st.code = code;
                    true
                }
                None => false,
            },
            None if sets.is_empty() && code.is_none() => true,
            None => self.add_setting(t, sets, code).is_some(),
        }
    }

    /// ` .fast(2) .cutoff(…)`: the pattern methods of clip `f`, then its
    /// parameter methods.
    fn methods_of(&self, f: usize) -> String {
        let patterns = self
            .clips
            .get(f)
            .map(|fr| fr.pattern.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(|p| format!(" .{}", p.print()));
        let params = self.mods.iter().filter(|m| m.clip == Some(f)).map(|m| {
            format!(
                " .{}({})",
                m.alias
                    .map_or_else(|| self.param_label(m.target, m.param, true), str::to_string),
                m.signal
            )
        });
        patterns.chain(params).collect()
    }

    /// A parameter as the text names it: a knob by its name in the code of
    /// the track's setting (#674), else its registry name (lower case where
    /// `lower`).
    fn param_label(&self, target: Target, p: Param, lower: bool) -> String {
        let knob = knobbed(self, target)
            .zip(p.ctl())
            .and_then(|(st, i)| st.knobs.get(i));
        if let Some(k) = knob.filter(|k| !k.is_empty()) {
            k.clone()
        } else if lower {
            param_name(p).to_ascii_lowercase()
        } else {
            param_name(p).to_string()
        }
    }

    fn target_name(&self, t: Target) -> String {
        match t {
            Target::Track(i) => self
                .tracks
                .get(i)
                .map_or_else(String::new, |t| t.name.clone()),
            Target::Strip(s) if s < STRIPS => format!("strip{}", s + 1),
            Target::Strip(s) => format!("group{}", s - STRIPS + 1),
            Target::Master => "master".to_string(),
        }
    }
}

fn model_named(text: &str) -> Option<Model> {
    Model::ALL.iter().find(|(_, n)| *n == text).map(|(m, _)| *m)
}

fn preset_named(text: &str) -> Option<Preset> {
    Preset::ALL
        .iter()
        .find(|(_, n)| *n == text)
        .map(|(p, _)| *p)
}

fn model_name(m: Model) -> &'static str {
    Model::ALL
        .iter()
        .find(|(q, _)| *q == m)
        .map_or("", |(_, n)| n)
}

fn preset_name(p: Preset) -> &'static str {
    Preset::ALL
        .iter()
        .find(|(q, _)| *q == p)
        .map_or("", |(_, n)| n)
}

/// Whether `model` (if one is given) plays a track of `kind`.
pub(crate) fn fits(kind: Kind, model: Option<Model>) -> bool {
    let Some(m) = model else {
        return true;
    };
    match kind {
        // The NES plays drums from its keys with `NesKit` (#112).
        Kind::Drums => m.uses_drums() || m.uses_pads() || m.uses_nes(),
        Kind::Sampler => m.uses_sampler() || m.uses_pads(),
        Kind::Synth => !m.uses_drums() && !m.uses_pads() && !m.uses_sampler(),
    }
}

/// What a track plays, for picking its preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Drums,
    Bass,
    Lead,
    Pad,
    Arp,
    Keys,
}

use Preset as P;

/// The presets for each role, the default first.
const DRUMS: &[Preset] = &[
    P::Kit808,
    P::Kit909,
    P::TightKit,
    P::Hard909,
    P::PadsLoud,
    P::Heavy808,
    P::Heavy909,
];
const BASS: &[Preset] = &[
    P::MiniBass,
    P::AcidBass,
    P::Sh101Bass,
    P::ProBass,
    P::Bass,
    P::FunkBass,
    P::P5Bass,
    P::JunoBass,
    P::JupiterBass,
    P::PpgPulseBass,
    P::LaThumpBass,
    P::FmBass,
];
const LEAD: &[Preset] = &[
    P::ProLead,
    P::MiniLead,
    P::Sh101Lead,
    P::Lead,
    P::SyncLead,
    P::Ms20Lead,
    P::Cs15Lead,
    P::CurrieLead,
    P::OdysseySync,
    P::P5SyncLead,
    P::MatrixLead,
    P::JupiterSync,
    P::LuckyMan,
];
const PAD: &[Preset] = &[
    P::JunoPad,
    P::P5Pad,
    P::JupiterPad,
    P::MatrixPad,
    P::PpgSweepPad,
    P::FmPad,
    P::LaFantasia,
    P::PolyStrings,
    P::SolinaStrings,
    P::MoogStrings,
    P::ProStrings,
    P::Ms20Strings,
    P::Cs15Strings,
    P::Sh101Strings,
    P::P5Strings,
    P::JunoStrings,
    P::JupiterStrings,
    P::LaChoir,
    P::VoxHumana,
    P::SamplerPad,
];
const ARP: &[Preset] = &[
    P::ShArp,
    P::JunoPluck,
    P::PpgDigitalPluck,
    P::SubPluck,
    P::FmMarimba,
    P::LaPluckPad,
];
const KEYS: &[Preset] = &[P::FmElectricPiano, P::JunoPoly, P::PolyFunk, P::SamplerKeys];

impl Role {
    fn presets(self) -> &'static [Preset] {
        match self {
            Role::Drums => DRUMS,
            Role::Bass => BASS,
            Role::Lead => LEAD,
            Role::Pad => PAD,
            Role::Arp => ARP,
            Role::Keys => KEYS,
        }
    }
}

/// A track's role: its name says it (`bass`, `pad`, `arp`, `keys`, `lead`),
/// else its notes do: only arps, chords, or mostly below C3 (MIDI 48).
fn role(song: &Song, t: usize) -> Role {
    let Some(track) = song.tracks.get(t) else {
        return Role::Lead;
    };
    if track.kind == Kind::Drums {
        return Role::Drums;
    }
    let name = track.name.to_ascii_lowercase();
    let says = |words: &[&str]| words.iter().any(|w| name.contains(w));
    if says(&["bass", "sub"]) {
        return Role::Bass;
    }
    if says(&["pad", "chord", "string", "choir"]) {
        return Role::Pad;
    }
    if says(&["arp", "pluck", "seq"]) {
        return Role::Arp;
    }
    if says(&["key", "piano", "organ"]) {
        return Role::Keys;
    }
    if says(&["lead", "melod", "solo"]) {
        return Role::Lead;
    }
    let notes: Vec<&Notes> = song
        .clips
        .iter()
        .filter(|f| f.track == t)
        .filter_map(|f| f.notes.as_ref())
        .collect();
    let arp = |n: &&Notes| {
        matches!(
            n.seq,
            notes::Seq::Generated(notes::Gen::Arp { .. } | notes::Gen::ArpProg { .. })
        )
    };
    if !notes.is_empty() && notes.iter().all(arp) {
        return Role::Arp;
    }
    let chords = notes.iter().any(|n| {
        n.events
            .windows(2)
            .any(|w| matches!(w, [a, b] if a.start == b.start))
    });
    if chords {
        return Role::Pad;
    }
    let mut pitches: Vec<u8> = notes
        .iter()
        .flat_map(|n| n.events.iter().map(|e| e.note))
        .collect();
    pitches.sort_unstable();
    match pitches.get(pitches.len() / 2) {
        Some(n) if *n < 48 => Role::Bass,
        _ => Role::Lead,
    }
}

/// The preset a track without one plays: its role's, on `model` when one is
/// given (else that model's first preset). A sampler track keeps the samples
/// it has unless it names a model.
fn pick(song: &Song, t: usize, model: Option<Model>) -> Option<Preset> {
    let kind = song.tracks.get(t)?.kind;
    if kind == Kind::Sampler && model.is_none() {
        return None;
    }
    let presets = role(song, t).presets();
    let named_909 = kind == Kind::Drums && song.tracks.get(t)?.name.contains("909");
    match model {
        None if named_909 => Some(Preset::Kit909),
        None => presets.first().copied(),
        Some(m) => presets
            .iter()
            .copied()
            .find(|p| p.model() == m)
            .or_else(|| Preset::ALL.iter().map(|(p, _)| *p).find(|p| p.model() == m)),
    }
}

/// The degrees left in `events` made notes of the song's scale, from its
/// root in `octave` or octave 3; an error when there are some and the song
/// has no scale.
fn song_degrees(
    events: &mut [notes::Event],
    scale: Option<&Scale>,
    octave: Option<u8>,
) -> Result<(), &'static str> {
    if !events.iter().any(|e| e.degree) {
        return Ok(());
    }
    let sc =
        scale.ok_or("a degree needs a scale: a scale line, or .scale(a1 minor) on the clip")?;
    let octave = octave.unwrap_or(notes::ROOT_OCTAVE);
    notes::resolve(events, sc, (octave + 1) * 12 + sc.root);
    events.sort_unstable_by_key(notes::sort_key);
    Ok(())
}

/// A parameter's registry name (ADR-0004).
pub fn param_name(p: Param) -> &'static str {
    Param::ALL
        .iter()
        .find(|(q, _)| *q == p)
        .map_or("", |(_, n)| n)
}

/// `target.Param`, checked: a track or strip takes its synth's and its strip's
/// parameters, a group its strip's, `master` the global ones. The model and
/// the routing can't be automated: they rebuild voices and the mix graph.
fn target_param(song: &Song, text: &str) -> Result<(Target, Param), Cow<'static, str>> {
    let (t, p) = text
        .split_once('.')
        .ok_or("target.Param goes here, e.g. strip1.Level")?;
    let numbered = |prefix: &str, count: usize| {
        t.strip_prefix(prefix)
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|n| (1..=count).contains(n))
    };
    let target = if t == "master" {
        Target::Master
    } else if let Some(n) = numbered("strip", STRIPS) {
        Target::Strip(n - 1)
    } else if let Some(n) = numbered("group", GROUPS) {
        Target::Strip(STRIPS + n - 1)
    } else if let Some(i) = song.tracks.iter().position(|tr| tr.name == t) {
        Target::Track(i)
    } else {
        return Err("a target is a track, strip1–16, group1–8 or master".into());
    };
    Ok((target, param_for(song, target, p)?))
}

/// A code line under a Modular setting without its indent of two spaces
/// or a tab.
fn dedent(raw: &str) -> &str {
    raw.strip_prefix("  ")
        .or_else(|| raw.strip_prefix('\t'))
        .unwrap_or_else(|| raw.trim_start())
        .trim_end()
}

/// The code of a Modular setting, now that its lines are read: built once
/// here so a mistake is the song's, at its line and column (ADR-0024).
fn close_code(song: &mut Song, mut c: OpenCode) -> Result<(), SongError> {
    while c.lines.last().is_some_and(String::is_empty) {
        c.lines.pop();
    }
    if c.lines.is_empty() {
        return Ok(());
    }
    let code = c.lines.join("\n");
    let patch = crate::modular::sc::compile(&code).map_err(|e| SongError {
        line: c.line + e.line.saturating_sub(1),
        col: e.col + 2,
        msg: e.msg.into(),
    })?;
    if let Some(st) = song.settings.get_mut(c.setting) {
        st.code = Some(code);
        st.knobs = patch.arg_names();
    }
    Ok(())
}

/// The model of track `t`, where the song names it: its preset's, a
/// setting's, or a model written without a preset (#662).
fn track_model(tracks: &[Track], models: &[(usize, Model)], t: usize) -> Option<Model> {
    tracks.get(t).and_then(|tr| {
        tr.preset
            .map(|p| p.model())
            .or_else(|| models.iter().find(|(i, _)| *i == t).map(|(_, m)| *m))
    })
}

/// `param` on track `t`, checked against its model where the song names
/// it, or kept in `later` until the model is picked (#673). `at` is where
/// the parameter is written.
fn check_reads(
    tracks: &[Track],
    models: &[(usize, Model)],
    t: usize,
    param: Param,
    at: SongError,
    later: &mut Vec<(SongError, usize, Param)>,
) -> Result<(), SongError> {
    match track_model(tracks, models, t) {
        Some(m) if !m.reads(param) => Err(SongError {
            msg: m.unread(param).into(),
            ..at
        }),
        Some(_) => Ok(()),
        None => {
            later.push((at, t, param));
            Ok(())
        }
    }
}

/// The column of the parameter in `target.Param` written at `col`.
fn param_col(col: usize, text: &str) -> usize {
    text.split_once('.')
        .map_or(col, |(t, _)| col + t.chars().count() + 1)
}

/// The knobs of a setting's `code` by `ctl`, named as `Setting::knobs`.
fn knob_names(code: Option<&str>) -> Vec<String> {
    code.and_then(|c| crate::modular::sc::compile(c).ok())
        .map(|p| p.arg_names())
        .unwrap_or_default()
}

/// The Modular setting a track plays, when its code names knobs (#674).
fn knobbed(song: &Song, target: Target) -> Option<&Setting> {
    let Target::Track(t) = target else {
        return None;
    };
    let st = song.settings.get(song.tracks.get(t)?.setting?)?;
    st.knobs.iter().any(|k| !k.is_empty()).then_some(st)
}

/// The parameter named `name` (in any case) of `target`, checked as
/// `target_param` says. On a track whose setting names its knobs, a knob's
/// name comes first (#674).
fn param_for(song: &Song, target: Target, name: &str) -> Result<Param, Cow<'static, str>> {
    let setting = knobbed(song, target);
    let knob = setting
        .and_then(|st| st.knobs.iter().position(|k| k.eq_ignore_ascii_case(name)))
        .and_then(Param::ctl_param);
    let param = knob
        .or_else(|| alias(name).map(|(_, p)| p))
        .or_else(|| Param::by_name(name))
        .ok_or_else(|| match setting {
            Some(st) => {
                let names: Vec<&str> = st
                    .knobs
                    .iter()
                    .filter(|k| !k.is_empty())
                    .map(String::as_str)
                    .collect();
                Cow::Owned(format!("{}'s knobs are {}", st.name, names.join(", ")))
            }
            None => Cow::Borrowed("no parameter has this name"),
        })?;
    if matches!(param, Param::Model | Param::Out) {
        return Err("the model and the routing can't be automated".into());
    }
    let ok = match target {
        Target::Master => param.is_global(),
        Target::Strip(s) if s >= STRIPS => param.is_strip(),
        _ => !param.is_global(),
    };
    if ok {
        Ok(param)
    } else {
        Err("this parameter does not belong to this target".into())
    }
}

/// A clip line split before its parameter methods: the line up to the first
/// word that starts with a dot, and the methods with their column.
fn split_methods(body: &str) -> (&str, Option<(usize, &str)>) {
    let mut prev_space = false;
    for (col, (i, c)) in body.char_indices().enumerate() {
        if c == '.' && prev_space {
            return (
                body.get(..i).unwrap_or(body),
                body.get(i..).map(|m| (col + 1, m)),
            );
        }
        prev_space = c.is_whitespace();
    }
    (body, None)
}

/// `.name(signal) .name(signal) …`: each method's name and the text of its
/// signal, with their columns in `text` (from 1).
#[allow(clippy::type_complexity)]
fn parse_methods(text: &str) -> Result<Vec<(&str, usize, &str, usize)>, (usize, &'static str)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let byte = |j: usize| chars.get(j).map_or(text.len(), |&(b, _)| b);
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&(_, c)) = chars.get(i) {
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c != '.' {
            return Err((i + 1, "a parameter method goes here, e.g. .cutoff(800)"));
        }
        let start = i + 1;
        let mut j = start;
        while chars
            .get(j)
            .is_some_and(|&(_, c)| c.is_ascii_alphanumeric())
        {
            j += 1;
        }
        if j == start {
            return Err((start + 1, "a parameter name goes here, e.g. .cutoff(800)"));
        }
        if chars.get(j).map(|&(_, c)| c) != Some('(') {
            return Err((j + 1, "( and a value go here, e.g. .cutoff(800)"));
        }
        let (open, mut depth, mut k) = (j, 0usize, j);
        loop {
            match chars.get(k).map(|&(_, c)| c) {
                Some('(') => depth += 1,
                Some(')') => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                None => return Err((open + 1, "this ( is not closed")),
                _ => {}
            }
            k += 1;
        }
        out.push((
            text.get(byte(start)..byte(j)).unwrap_or(""),
            start + 1,
            text.get(byte(open + 1)..byte(k)).unwrap_or(""),
            open + 2,
        ));
        i = k + 1;
    }
    Ok(out)
}

fn check_lanes(song: &Song, f: usize, line: usize) -> Result<(), SongError> {
    match song.clips.get(f) {
        Some(clip) if clip.lanes.is_empty() && clip.notes.is_none() => Err(SongError {
            line,
            col: 1,
            msg: match song.tracks.get(clip.track).map(|t| t.kind) {
                Some(Kind::Synth) => "a clip needs a line of notes",
                Some(Kind::Sampler) => "a clip needs lanes or a line of notes",
                _ => "a clip needs at least one lane",
            }
            .into(),
        }),
        _ => Ok(()),
    }
}

/// `pad steps...`: the steps may be split by spaces for reading.
fn parse_lane(ws: &[Word<'_>], line: usize) -> Result<Lane, SongError> {
    let err = |col: usize, msg: &'static str| SongError {
        line,
        col,
        msg: msg.into(),
    };
    let Some(first) = ws.first() else {
        return Err(err(1, "a lane is a pad and its steps"));
    };
    let pad = LanePad::from_name(first.text).ok_or(err(
        first.col,
        "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr rd, or p0 to p15",
    ))?;
    if let Some(call) = ws
        .get(1)
        .and_then(|w| Euclid::parse(w.text).map(|c| (w, c)))
    {
        let (w, call) = call;
        let e = call.map_err(|msg| err(w.col, msg))?;
        if let Some(x) = ws.get(2) {
            return Err(err(x.col, "unexpected text"));
        }
        let steps = e
            .pattern()
            .into_iter()
            .map(|h| if h { Step::Hit } else { Step::Off })
            .collect();
        return Ok(Lane {
            pad,
            steps,
            call: Some(e),
            ratchets: Vec::new(),
        });
    }
    let mut steps: Vec<Step> = Vec::new();
    let mut ratchets: Vec<u8> = Vec::new();
    for w in ws.iter().skip(1) {
        for (k, c) in w.text.chars().enumerate() {
            // A digit after a hit, accent or ghost repeats it in its step (#242).
            if let Some(d) = c.to_digit(10) {
                let Some(last) = steps.last().copied() else {
                    return Err(err(w.col + k, "a ratchet follows a step: x3"));
                };
                if !last.repeats() {
                    return Err(err(w.col + k, "only x, X and o take a ratchet"));
                }
                if !(2..=u32::from(MAX_RATCHET)).contains(&d) || ratchets.last() != Some(&1) {
                    return Err(err(w.col + k, "a ratchet is 2, 3 or 4"));
                }
                if let Some(r) = ratchets.last_mut() {
                    *r = d as u8;
                }
                continue;
            }
            let step = Step::from_char(c).ok_or(err(w.col + k, "a step is x, X, o, f, d or ."))?;
            if steps.len() >= MAX_STEPS {
                return Err(err(w.col + k, "a lane has at most 64 steps"));
            }
            steps.push(step);
            ratchets.push(1);
        }
    }
    if steps.is_empty() {
        return Err(err(first.col, "a lane needs its steps: x, X, o, f, d or ."));
    }
    if ratchets.iter().all(|r| *r == 1) {
        ratchets.clear();
    }
    Ok(Lane {
        pad,
        steps,
        call: None,
        ratchets,
    })
}

/// Whether `w` names a pad in a beat line (#659), for the highlighter.
pub(crate) fn is_pad_word(w: &str) -> bool {
    PAD_ALIASES.iter().any(|(a, _)| *a == w) || LanePad::from_name(w).is_some()
}

/// The note a beat line's word plays on `model` (#659): a pad's name (or
/// Strudel's), `p0` to `p15`, or a pad's number, 0 to 15.
fn beat_pad(w: &str, model: Option<Model>) -> Result<u8, &'static str> {
    let w = PAD_ALIASES
        .iter()
        .find(|(a, _)| *a == w)
        .map_or(w, |(_, ours)| ours);
    let pad = match LanePad::from_name(w) {
        Some(p) => p,
        None => match w.parse::<u8>() {
            Ok(n) if n.to_string() == w && usize::from(n) < padsampler::PADS => LanePad::Number(n),
            Ok(_) => return Err("a pad's number is 0 to 15"),
            Err(_) => {
                return Err(
                    "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc hc cr rd, or p0 to p15",
                );
            }
        },
    };
    match model {
        Some(m) if !pad.plays_on(m) => Err(match pad {
            LanePad::Number(_) => "a pad by number is for a Pad Sampler",
            LanePad::Named(_) => "a Pad Sampler has no such pad: name it p0 to p15",
        }),
        _ => Ok(pad.note()),
    }
}

/// A beat line's hits as lanes on `grid` (#659): a lane for each pad, in
/// the order they first play, a named pad's name where it has one.
fn beat_lanes(
    mut events: Vec<notes::Event>,
    bars: u32,
    grid: u32,
) -> Result<Vec<Lane>, &'static str> {
    let mut pads: Vec<LanePad> = Vec::new();
    for e in &mut events {
        let st = if e.accent { Step::Accent } else { Step::Hit };
        e.word = step_word(st, 1);
        let pad = Pad::from_note(e.note).map_or_else(
            || LanePad::Number(e.note.saturating_sub(padsampler::FIRST_NOTE)),
            LanePad::Named,
        );
        if !pads.contains(&pad) {
            pads.push(pad);
        }
    }
    lanes_of(&Line { events, bars }, grid, &pads)
}

mod comments;
// --- Mixer lines (ADR-0018) ------------------------------------------------

/// The value a mixer line writes for `param`: a name for an insert or a
/// processor type and for `Out`, else the number.
fn mix_text(param: Param, v: f32) -> String {
    let id = v.round().max(0.0) as u32;
    if param
        .insert()
        .is_some_and(|(_, k)| k == crate::params::InsertField::Type)
    {
        if let Some((_, n)) = InsertType::ALL.iter().find(|(t, _)| *t as u32 == id) {
            return (*n).to_string();
        }
    }
    if param
        .processor()
        .is_some_and(|(_, k)| k == crate::params::ProcField::Type)
    {
        if let Some((_, n)) = ProcType::ALL.iter().find(|(t, _)| *t as u32 == id) {
            return (*n).to_string();
        }
    }
    if param == Param::Out {
        return match id as usize {
            0 => "master".to_string(),
            OUT_NONE => "none".to_string(),
            g => format!("group{g}"),
        };
    }
    format!("{v}")
}

/// A mixer value as written: a number, or a type or route by name.
fn mix_value(param: Param, text: &str) -> Result<f32, &'static str> {
    if param
        .insert()
        .is_some_and(|(_, k)| k == crate::params::InsertField::Type)
    {
        if let Some((t, _)) = InsertType::ALL.iter().find(|(_, n)| *n == text) {
            return Ok(*t as u32 as f32);
        }
        if text.parse::<f32>().is_err() {
            return Err(
                "an insert is Off, Overdrive, Distortion, Fuzz, Eq, Comp, Vocoder or Bitcrush",
            );
        }
    }
    if param
        .processor()
        .is_some_and(|(_, k)| k == crate::params::ProcField::Type)
    {
        if let Some((t, _)) = ProcType::ALL.iter().find(|(_, n)| *n == text) {
            return Ok(*t as u32 as f32);
        }
        if text.parse::<f32>().is_err() {
            return Err("a processor is Off, Echo, Reverb, Chorus or Flanger");
        }
    }
    if param == Param::Out {
        let g = text
            .strip_prefix("group")
            .and_then(|n| n.parse::<usize>().ok());
        return match (text, g) {
            ("master", _) => Ok(0.0),
            ("none", _) => Ok(OUT_NONE as f32),
            (_, Some(g)) if (1..=GROUPS).contains(&g) => Ok(g as f32),
            _ => Err("an out is master, group1 to group8 or none"),
        };
    }
    text.parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or("a value is a number")
}

/// `strip <track|stripN>: …`, `group <n> [name]: …` or `master: …`.
fn parse_mix(song: &Song, ws: &[Word<'_>], line: usize, body: &str) -> Result<MixLine, SongError> {
    let err = |col: usize, msg: &'static str| SongError {
        line,
        col,
        msg: msg.into(),
    };
    let end = body.trim_end().chars().count() + 1;
    let first = ws.first().ok_or(err(1, "a mixer line goes here"))?;
    // A group with only a name needs no values (#214): `group 2 keys`.
    if let [g, n, name] = ws {
        if g.text == "group" && !name.text.ends_with(':') && is_name(name.text) {
            let g = n
                .text
                .parse::<usize>()
                .ok()
                .filter(|g| (1..=GROUPS).contains(g))
                .ok_or(err(n.col, "a group is 1 to 8, then maybe its name"))?;
            return Ok(MixLine {
                at: Mix::Group(g - 1),
                name: Some(name.text.to_string()),
                sets: Vec::new(),
            });
        }
    }
    // The words up to the one that ends with `:` (or a lone `:`) name the target.
    let colon = ws
        .iter()
        .position(|w| w.text.ends_with(':'))
        .ok_or(err(end, ": goes here, then Param value, …"))?;
    let head: Vec<&str> = ws
        .get(1..=colon)
        .unwrap_or(&[])
        .iter()
        .map(|w| w.text.trim_end_matches(':'))
        .filter(|t| !t.is_empty())
        .collect();
    let at_col = ws.get(1).map_or(end, |w| w.col);
    let (at, name) = match (first.text.trim_end_matches(':'), head.as_slice()) {
        ("master", []) => (Mix::Master, None),
        ("strip", [t]) => {
            let n = t
                .strip_prefix("strip")
                .and_then(|n| n.parse::<usize>().ok());
            match (n, song.tracks.iter().position(|x| x.name == *t)) {
                (Some(n), _) if (1..=STRIPS).contains(&n) => (Mix::Strip(n - 1), None),
                (_, Some(i)) => (Mix::Track(i), None),
                _ => {
                    return Err(err(
                        at_col,
                        "a strip is a track's name or strip1 to strip16",
                    ));
                }
            }
        }
        ("group", [n, rest @ ..]) => {
            let g = n
                .parse::<usize>()
                .ok()
                .filter(|g| (1..=GROUPS).contains(g))
                .ok_or(err(at_col, "a group is 1 to 8, then maybe its name"))?;
            let name = match rest {
                [] => None,
                [n] if is_name(n) => Some((*n).to_string()),
                _ => return Err(err(at_col, "a group is 1 to 8, then maybe its name")),
            };
            (Mix::Group(g - 1), name)
        }
        ("strip", _) => {
            return Err(err(
                at_col,
                "a strip is a track's name or strip1 to strip16",
            ));
        }
        ("group", _) => return Err(err(at_col, "a group is 1 to 8, then maybe its name")),
        _ => return Err(err(at_col, ": goes here, then Param value, …")),
    };
    let mut sets: Vec<(Param, f32)> = Vec::new();
    let mut k = colon + 1;
    while let Some(pw) = ws.get(k) {
        let param = Param::by_name(pw.text).ok_or(err(pw.col, "no parameter has this name"))?;
        let ok = if at == Mix::Master {
            param.is_global()
        } else {
            param.is_strip()
        };
        if !ok {
            return Err(err(
                pw.col,
                if at == Mix::Master {
                    "the master takes the global parameters"
                } else {
                    "a strip or group takes its strip's parameters"
                },
            ));
        }
        let vw = ws.get(k + 1).ok_or(err(end, "a value goes here"))?;
        let text = vw.text.strip_suffix(',').unwrap_or(vw.text);
        let v = mix_value(param, text).map_err(|m| err(vw.col, m))?;
        if param == Param::Out {
            let strip = match at {
                Mix::Group(g) => STRIPS + g,
                _ => 0,
            };
            if !crate::mixer::Mixer::route_ok(strip, v as usize) {
                return Err(err(
                    vw.col,
                    "a group goes only to a higher group or the master",
                ));
            }
        }
        if sets.iter().any(|(p, _)| *p == param) {
            return Err(err(pw.col, "this parameter is already on the line"));
        }
        if sets.len() >= MAX_MIX {
            return Err(err(pw.col, "a mixer line has at most 48 values"));
        }
        sets.push((param, v));
        k += 2;
        if !vw.text.ends_with(',') {
            if let Some(extra) = ws.get(k) {
                return Err(err(extra.col, "a comma goes between values"));
            }
        }
    }
    if sets.is_empty() {
        return Err(err(end, "values go here: Param value, …"));
    }
    Ok(MixLine { at, name, sets })
}

impl Song {
    /// The value a mixer line gives `param` on `at`, if one does.
    pub fn mix_value(&self, at: Mix, param: Param) -> Option<f32> {
        self.mix
            .iter()
            .find(|m| m.at == at)
            .and_then(|m| m.sets.iter().find(|(p, _)| *p == param))
            .map(|(_, v)| *v)
    }

    /// The group buses the song uses, bit g for group g (#568): those with a
    /// `group` line, and those a mixer line or a pad sends to with `Out`.
    pub fn groups_used(&self) -> u8 {
        let bit = |out: usize| (1..=GROUPS).contains(&out).then(|| 1 << (out - 1));
        let lines = self.mix.iter().map(|m| {
            let named = match m.at {
                Mix::Group(g) if g < GROUPS => 1 << g,
                _ => 0,
            };
            let out = m.sets.iter().filter(|(p, _)| *p == Param::Out);
            named
                | out
                    .filter_map(|(_, v)| bit(*v as usize))
                    .fold(0, |a, b| a | b)
        });
        let pads = self
            .pads
            .iter()
            .filter_map(|p| bit(usize::from(p.fields.out)));
        lines.chain(pads).fold(0, |a, b| a | b)
    }
}

pub mod complete;
pub mod lex;
pub mod pad;
pub mod recipe;
pub mod signal;
pub mod spans;
pub mod splice;
pub mod zone;

/// A signal with `env` or a list has a value per voice (ADR-0023): it goes
/// on a track's Mono or Poly synth, onto a parameter a voice holds, without
/// `.lag`. A per-synth signal always fits.
fn per_voice_fits(
    song: &Song,
    target: Target,
    param: Param,
    signal: &Signal,
) -> Result<(), &'static str> {
    if signal.on_note() && !matches!(target, Target::Track(_)) {
        return Err("onNote samples a track's notes: it modulates a track");
    }
    if !signal.per_voice() {
        return Ok(());
    }
    let Target::Track(t) = target else {
        return Err("env and lists give each voice its own value: they modulate a track");
    };
    if !crate::mono::voice::VOICE_PARAMS.contains(&param) {
        return Err(
            "per voice: cutoff, resonance, vco1level, vco2level, vco3level, noiselevel, ringlevel, sublevel, wt1pos or wt2pos",
        );
    }
    if signal.lags() {
        return Err(".lag follows one value for the song, not one per voice");
    }
    if signal.rests() {
        return Err("a ~ lets go of one value, not one per voice");
    }
    let track = song.tracks.get(t);
    let model = track.and_then(|tr| tr.preset).map(Preset::model);
    // A drum or sampler track never plays Mono voices, whatever it picks later.
    let kind = track.is_some_and(|tr| tr.kind != Kind::Synth);
    if kind || model.is_some_and(|m| !m.uses_mono_voice()) {
        return Err(
            "env and lists need a Mono or Poly synth; a Modular voice writes them in its graph",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
