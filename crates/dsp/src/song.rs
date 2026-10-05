//! The song as text (ADR-0012, spec 002 Req 3 and 6): a parser and a
//! canonical printer, first cut. Drum tracks and fragments of lanes:
//!
//! ```text
//! tempo 124
//! swing 56
//! track kit drums
//!
//! frag beat = kit /16
//!   bd x...x...x...x...
//!   sn ....X.......X..x
//!
//! section intro 4: beat
//! section main 8: beat fill
//! arrange intro main main
//! loop 5 12
//! ```
//!
//! A `synth` track holds note fragments instead (spec 002 Req 3, ADR-0016):
//!
//! ```text
//! track lead synth
//!
//! frag riff = lead
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
//! Sections and the arrangement (ADR-0015, spec 002 Req 4): a section is a
//! number of bars and the fragments that play in it, each from the section's
//! first bar and looping inside it; `arrange` plays sections in order, and
//! `loop` repeats a range of the arrangement's bars (from 1, inclusive).
//! Without `arrange` every fragment loops, as before.
//!
//! Scenes and automation (ADR-0015): any parameter, by its registry name, on a
//! target: a track (its synth), `strip1`–`strip16`, `group1`–`group8` or
//! `master` (the global parameters):
//!
//! ```text
//! auto sweep = kit.Cutoff ramp 300 4000 /8
//! auto duck = strip3.Level 1 0.5 0.25 1 /1
//! scene drop: strip1.Mute 1, master.P2Return 0.4
//! section main 8: beat sweep duck [drop]
//! ```
//!
//! A lane of values (spread evenly over its bars) or a ramp is placed in
//! sections like a fragment and loops inside them; a scene sets its values on
//! the first step of a section that lists it. Without `arrange` every lane
//! loops and no scene is applied.
//!
//! A modulation (ADR-0019) writes a signal to a parameter for the whole song,
//! once per block and after lanes and scenes; see `signal` for the language:
//!
//! ```text
//! mod lead.cutoff = lfo(1).exprange(100, 2000) + lfo(3).range(0, 300)
//! ```
//!
//! Parsing and printing allocate, so they run when a song is loaded or a step
//! edited, never in `render`; the engine plays the parsed song in place. Both
//! are total: whatever the text, the parser returns a song or an error with a
//! line and a column, and never panics. Comments and layout are not kept: a
//! printed song is the canonical form of what was parsed.

use crate::algo::{Euclid, Mode, Scale};
use crate::drums::Pad;
use crate::fx::insert::InsertType;
use crate::fx::processor::ProcType;
use crate::mixer::OUT_NONE;
use crate::mono::model::Model;
use crate::mono::preset::Preset;
use crate::notes::{self, Notes};
use crate::params::Param;

pub use comments::Comments;
pub use signal::Signal;

/// Most tracks, fragments, lanes per fragment and steps per lane a song may have.
pub const MAX_TRACKS: usize = 16;
pub const MAX_FRAGS: usize = 256;
pub const MAX_STEPS: usize = 64;
/// Most sections, entries in the arrangement and bars in a section.
pub const MAX_SECTIONS: usize = 256;
pub const MAX_ARRANGE: usize = 256;
pub const MAX_BARS: u32 = 256;
/// Most automation lanes, values in a lane, scenes and settings in a scene.
pub const MAX_AUTOS: usize = 32;
pub const MAX_VALUES: usize = 64;
pub const MAX_SCENES: usize = 32;
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
/// Longest name of a track or fragment.
const MAX_NAME: usize = 32;
/// Tempo and swing ranges, as the clock has them.
const TEMPO: (f32, f32) = crate::clock::TEMPO;
const SWING: (f32, f32) = crate::clock::SWING;

/// One step of a lane: a rest, a hit or an accented hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Off = 0,
    Hit = 1,
    Accent = 2,
}

impl Step {
    fn from_char(c: char) -> Option<Step> {
        match c {
            '.' => Some(Step::Off),
            'x' => Some(Step::Hit),
            'X' => Some(Step::Accent),
            _ => None,
        }
    }

    fn char(self) -> char {
        match self {
            Step::Off => '.',
            Step::Hit => 'x',
            Step::Accent => 'X',
        }
    }

    /// The step for a level from the view: 0 off, 1 hit, 2 accent.
    pub fn from_level(level: u32) -> Option<Step> {
        match level {
            0 => Some(Step::Off),
            1 => Some(Step::Hit),
            2 => Some(Step::Accent),
            _ => None,
        }
    }

    /// The velocity it plays at; an accent is at or above `ACCENT_VELOCITY`.
    pub fn velocity(self) -> Option<f32> {
        match self {
            Step::Off => None,
            Step::Hit => Some(0.75),
            Step::Accent => Some(1.0),
        }
    }
}

/// A pad's row of steps; it loops on its own length (polymeter).
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub pad: Pad,
    pub steps: Vec<Step>,
    /// The call that made the steps (`euclid(3,8)`); editing a step drops it.
    pub call: Option<Euclid>,
}

/// A loop on a track: one lane per pad on a drum track, one line of notes on
/// a synth track (the other is then empty).
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    pub name: String,
    pub track: usize,
    pub lanes: Vec<Lane>,
    pub notes: Option<Notes>,
    /// A generator call that makes new events every cycle (`frag a = t live`).
    pub live: bool,
    /// Chords moved to the inversion nearest the one before (`frag a = t voicing`, #103).
    pub voicing: bool,
}

/// What a track's fragments hold.
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

/// Bars of the song and the fragments that play in them.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub bars: u32,
    /// Indices into `Song::frags`.
    pub frags: Vec<usize>,
    /// Indices into `Song::autos` and `Song::scenes`.
    pub autos: Vec<usize>,
    pub scenes: Vec<usize>,
}

/// What a scene or lane sets: a track's synth, a strip (synths 0–15, groups
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
        }
    }
}

/// A modulation (ADR-0019): a signal written to one parameter of one target
/// for the whole song, while it plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Mod {
    pub target: Target,
    pub param: Param,
    pub signal: Signal,
}

/// Values set together on the first step of a section.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
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
    /// No arrangement: every fragment loops on the clock's step.
    Free(u64),
    /// In arrangement entry `entry`, which plays `section`, `local` steps in.
    In {
        entry: usize,
        section: usize,
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
}

/// Two tracks are equal by what they play; whether the preset was picked or
/// written is not part of the song, so a printed song parses back equal.
impl PartialEq for Track {
    fn eq(&self, other: &Track) -> bool {
        (&self.name, self.kind, self.preset, self.setting)
            == (&other.name, other.kind, other.preset, other.setting)
    }
}

/// A synth patch that lives in the song (#210): a factory preset and the
/// changes made to it, `setting nile = Minimoog MiniLead: Cutoff 0.4, …`.
#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub name: String,
    pub preset: Preset,
    pub sets: Vec<(Param, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub tempo: f32,
    pub swing: f32,
    /// The key generators walk (`scale c minor`); it goes before its first use.
    pub scale: Option<Scale>,
    pub settings: Vec<Setting>,
    pub tracks: Vec<Track>,
    /// Starting values for strips, groups and the master (ADR-0018).
    pub mix: Vec<MixLine>,
    pub frags: Vec<Fragment>,
    pub autos: Vec<Auto>,
    pub scenes: Vec<Scene>,
    pub mods: Vec<Mod>,
    pub sections: Vec<Section>,
    /// The order sections play in, by index; empty means no arrangement.
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
            settings: Vec::new(),
            tracks: Vec::new(),
            mix: Vec::new(),
            frags: Vec::new(),
            autos: Vec::new(),
            scenes: Vec::new(),
            mods: Vec::new(),
            sections: Vec::new(),
            arrange: Vec::new(),
            loop_bars: None,
            comments: Comments::default(),
        }
    }
}

/// Why a text is not a song, and where: line and column count from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SongError {
    pub line: usize,
    pub col: usize,
    pub msg: &'static str,
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

/// `c`, `c#` or `eb`: a pitch class, 0 for c.
fn pitch_class(s: &str) -> Option<u8> {
    let mut chars = s.chars();
    let base = match chars.next()? {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    let semi = match (chars.next(), chars.next()) {
        (None, _) => base,
        (Some('#'), None) => base + 1,
        (Some('b'), None) => base + 11,
        _ => return None,
    };
    Some(semi % 12)
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
        // The fragment that indented lines add lanes to, and the line it began on.
        let mut open: Option<(usize, usize)> = None;
        // `frag … bars N`: the length the open frag's line of timed notes is given.
        let mut open_bars: Option<(u32, usize, usize)> = None;
        // The line of the `loop`, checked against the arrangement at the end.
        let mut loop_at: Option<usize> = None;
        // Tracks given a model and no preset.
        let mut models: Vec<(usize, Model)> = Vec::new();
        // Nodes in the song's signals so far.
        let mut nodes = 0;
        for (i, raw) in text.lines().enumerate() {
            let line = i + 1;
            let err = |col: usize, msg: &'static str| SongError { line, col, msg };
            let body = strip_comment(raw);
            let mut ws = words(body);
            // `frag a = t … voicing` (#103): the last word, taken off before the rest is read.
            let mut voicing = None;
            if ws.len() > 4
                && ws.first().is_some_and(|w| w.text == "frag")
                && ws.last().is_some_and(|w| w.text == "voicing")
            {
                voicing = ws.pop().map(|w| w.col);
            }
            let Some(first) = ws.first() else {
                continue;
            };
            if body.starts_with([' ', '\t']) {
                let Some((f, _)) = open else {
                    return Err(err(first.col, "a lane goes under a frag"));
                };
                let kind = song
                    .frags
                    .get(f)
                    .and_then(|fr| song.tracks.get(fr.track))
                    .map(|t| t.kind);
                // A sampler line is a lane unless it is written as notes: quoted,
                // a chord or with a duration (a bad pad name is then a lane error).
                let as_notes = match kind {
                    Some(Kind::Synth) => true,
                    Some(Kind::Sampler) => {
                        first.text.starts_with(['"', '[']) || first.text.contains(':')
                    }
                    _ => false,
                };
                if kind == Some(Kind::Sampler) {
                    let has = song
                        .frags
                        .get(f)
                        .map(|fr| (fr.notes.is_some(), !fr.lanes.is_empty()));
                    if (as_notes && has.is_some_and(|h| h.1))
                        || (!as_notes && has.is_some_and(|h| h.0))
                    {
                        return Err(err(first.col, "a frag holds lanes or notes, not both"));
                    }
                }
                if as_notes {
                    if song.frags.get(f).is_some_and(|fr| fr.notes.is_some()) {
                        return Err(err(first.col, "a note frag is one line of notes"));
                    }
                    let srcs = |name: &str| {
                        song.frags
                            .iter()
                            .find(|f| f.name == name)
                            .and_then(|f| f.notes.as_ref())
                            .map(|n| (n.events.clone(), n.bars))
                    };
                    let live = song.frags.get(f).is_some_and(|fr| fr.live);
                    let voiced = song.frags.get(f).is_some_and(|fr| fr.voicing);
                    let mut n =
                        notes::parse_with(body.trim_start(), first.col, song.scale.as_ref(), &srcs)
                            .map_err(|e| err(e.col, e.msg))?;
                    if let Some((bars, col, at)) = open_bars.take() {
                        n = n
                            .with_bars(bars)
                            .map_err(|msg| SongError { line: at, col, msg })?;
                    }
                    if live && !matches!(n.seq, notes::Seq::Generated(_)) {
                        return Err(err(
                            first.col,
                            "a live frag is a call: arp, walk, markov, mutate, root or prog",
                        ));
                    }
                    let frag = song
                        .frags
                        .get_mut(f)
                        .ok_or(err(first.col, "a lane goes under a frag"))?;
                    frag.notes = Some(if voiced { n.voiced() } else { n });
                    continue;
                }
                let lane = parse_lane(&ws, line)?;
                let frag = song
                    .frags
                    .get_mut(f)
                    .ok_or(err(first.col, "a lane goes under a frag"))?;
                if frag.lanes.iter().any(|l| l.pad == lane.pad) {
                    return Err(err(first.col, "this pad already has a lane"));
                }
                frag.lanes.push(lane);
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
            match first.text {
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
                    if first.text == "tempo" {
                        song.tempo = v;
                    } else {
                        song.swing = v;
                    }
                }
                "scale" => {
                    let root = arg(1, "a root and a mode go here: scale c minor")?;
                    let pc = pitch_class(root.text)
                        .ok_or(err(root.col, "a root is a letter a to g, maybe # or b"))?;
                    let mode = arg(2, "a mode goes here: scale c minor")?;
                    let m = Mode::from_name(mode.text).ok_or(err(
                        mode.col,
                        "a mode is major, minor, dorian, phrygian, lydian, mixolydian, locrian, pentatonic, blues, phrygian-dominant or harmonic-minor",
                    ))?;
                    expect_end(3)?;
                    if song.scale.is_some() {
                        return Err(err(first.col, "a song has one scale"));
                    }
                    if !song.frags.is_empty() {
                        return Err(err(first.col, "the scale goes before the frags"));
                    }
                    song.scale = Some(Scale { root: pc, mode: m });
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
                    // `<setting>`, `<model> <preset>` or `<model>`; nothing picks both.
                    let mut setting = None;
                    let mut model = None;
                    let mut preset = None;
                    if let Some(w) = ws.get(3) {
                        if let Some(i) = song.settings.iter().position(|st| st.name == w.text) {
                            setting = Some(i);
                            preset = song.settings.get(i).map(|st| st.preset);
                            expect_end(4)?;
                        } else {
                            let m = model_named(w.text).ok_or(err(
                                w.col,
                                "a model (as Minimoog or Tr808) or a setting goes here",
                            ))?;
                            model = Some(m);
                            if let Some(p) = ws.get(4) {
                                let pr = preset_named(p.text).ok_or(err(
                                    p.col,
                                    "a preset is a factory preset, as MiniBass",
                                ))?;
                                if pr.model() != m {
                                    return Err(err(p.col, "this preset is for another model"));
                                }
                                preset = Some(pr);
                            }
                            expect_end(5)?;
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
                    });
                    // A model without a preset: one is picked once the frags are in.
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
                    });
                }
                "frag" => {
                    let name = arg(1, "a fragment name goes here")?;
                    if !is_name(name.text) {
                        return Err(err(
                            name.col,
                            "a name is a letter, then letters, digits or _",
                        ));
                    }
                    if song.frags.iter().any(|f| f.name == name.text)
                        || song.autos.iter().any(|a| a.name == name.text)
                    {
                        return Err(err(
                            name.col,
                            "there is already a frag or auto with this name",
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
                    if let Some(col) = voicing {
                        if kind == Some(Kind::Drums) {
                            return Err(err(col, "voicing is for a frag of notes"));
                        }
                        if live {
                            return Err(err(col, "a live frag is not voiced"));
                        }
                    }
                    if live && kind == Some(Kind::Drums) {
                        let col = ws.get(4).map_or(first.col, |w| w.col);
                        return Err(err(col, "only a note frag can be live"));
                    }
                    // `bars N` for a line of timed notes (#173).
                    open_bars = None;
                    if let Some(w) = ws.get(4).filter(|w| w.text == "bars") {
                        if kind == Some(Kind::Drums) {
                            return Err(err(w.col, "bars N is for a line of timed notes"));
                        }
                        let n = arg(5, "a number of bars goes here")?;
                        let bars: u32 = n
                            .text
                            .parse()
                            .ok()
                            .filter(|b| (1..=notes::MAX_BARS).contains(b))
                            .ok_or(err(n.col, "a line is 1 to 32 bars"))?;
                        open_bars = Some((bars, w.col, line));
                    }
                    if let Some(w) = ws.get(4).filter(|_| !live && open_bars.is_none()) {
                        if synth {
                            return Err(err(w.col, "a note frag has no step grid"));
                        }
                        if w.text != "/16" {
                            return Err(err(w.col, "only /16 steps for now"));
                        }
                    }
                    expect_end(if open_bars.is_some() { 6 } else { 5 })?;
                    if song.frags.len() >= MAX_FRAGS {
                        return Err(err(first.col, "a song has at most 256 frags"));
                    }
                    song.frags.push(Fragment {
                        name: name.text.to_string(),
                        track: t,
                        lanes: Vec::new(),
                        notes: None,
                        live,
                        voicing: voicing.is_some(),
                    });
                    open = Some((song.frags.len() - 1, line));
                }
                "section" => {
                    let name = arg(1, "a section name goes here")?;
                    if !is_name(name.text) {
                        return Err(err(
                            name.col,
                            "a name is a letter, then letters, digits or _",
                        ));
                    }
                    if song.sections.iter().any(|s| s.name == name.text) {
                        return Err(err(name.col, "there is already a section with this name"));
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
                        .ok_or(err(bars_word.col, "a section is 1 to 256 bars"))?;
                    let (mut frags, mut autos, mut scenes) = (Vec::new(), Vec::new(), Vec::new());
                    while let Some(w) = ws.get(k) {
                        k += 1;
                        if let Some(name) =
                            w.text.strip_prefix('[').and_then(|n| n.strip_suffix(']'))
                        {
                            let Some(s) = song.scenes.iter().position(|s| s.name == name) else {
                                return Err(err(w.col, "no scene has this name"));
                            };
                            if scenes.contains(&s) {
                                return Err(err(w.col, "this scene is already in the section"));
                            }
                            scenes.push(s);
                        } else if let Some(a) = song.autos.iter().position(|a| a.name == w.text) {
                            if autos.contains(&a) {
                                return Err(err(w.col, "this auto is already in the section"));
                            }
                            autos.push(a);
                        } else {
                            let Some(f) = song.frags.iter().position(|f| f.name == w.text) else {
                                return Err(err(w.col, "no frag, auto or [scene] has this name"));
                            };
                            if frags.contains(&f) {
                                return Err(err(w.col, "this frag is already in the section"));
                            }
                            frags.push(f);
                        }
                    }
                    if song.sections.len() >= MAX_SECTIONS {
                        return Err(err(first.col, "a song has at most 256 sections"));
                    }
                    song.sections.push(Section {
                        name: name.text.to_string(),
                        bars,
                        frags,
                        autos,
                        scenes,
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
                        || song.frags.iter().any(|f| f.name == name.text)
                    {
                        return Err(err(
                            name.col,
                            "there is already a frag or auto with this name",
                        ));
                    }
                    let eq = arg(2, "= and a target.Param go here")?;
                    if eq.text != "=" {
                        return Err(err(eq.col, "= and a target.Param go here"));
                    }
                    let tp = arg(3, "a target.Param goes here")?;
                    let (target, param) =
                        target_param(&song, tp.text).map_err(|m| err(tp.col, m))?;
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
                "strip" | "group" | "master" | "master:" => {
                    let line = parse_mix(&song, &ws, line, body)?;
                    if song.mix.iter().any(|m| m.at == line.at) {
                        return Err(err(first.col, "this strip already has a line"));
                    }
                    song.mix.push(line);
                }
                "scene" => {
                    let w = arg(1, "a scene name and : go here")?;
                    let name = w.text.strip_suffix(':').unwrap_or(w.text);
                    if !is_name(name) {
                        return Err(err(w.col, "a name is a letter, then letters, digits or _"));
                    }
                    if song.scenes.iter().any(|s| s.name == name) {
                        return Err(err(w.col, "there is already a scene with this name"));
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
                            target_param(&song, tp.text).map_err(|m| err(tp.col, m))?;
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
                            return Err(err(tp.col, "a scene has at most 32 settings"));
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
                    if song.scenes.len() >= MAX_SCENES {
                        return Err(err(first.col, "a song has at most 32 scenes"));
                    }
                    song.scenes.push(Scene {
                        name: name.to_string(),
                        sets,
                    });
                }
                "mod" => {
                    let tp = arg(1, "a target.param goes here")?;
                    let (target, param) =
                        target_param(&song, tp.text).map_err(|m| err(tp.col, m))?;
                    if song
                        .mods
                        .iter()
                        .any(|m| m.target == target && m.param == param)
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
                    if song.mods.len() >= MAX_MODS {
                        return Err(err(first.col, "a song has at most 32 mods"));
                    }
                    song.mods.push(Mod {
                        target,
                        param,
                        signal,
                    });
                }
                "arrange" => {
                    if !song.arrange.is_empty() {
                        return Err(err(first.col, "a song has one arrange line"));
                    }
                    let mut order = Vec::new();
                    for w in ws.iter().skip(1) {
                        let Some(s) = song.sections.iter().position(|s| s.name == w.text) else {
                            return Err(err(w.col, "no section has this name"));
                        };
                        if order.len() >= MAX_ARRANGE {
                            return Err(err(w.col, "an arrangement has at most 256 sections"));
                        }
                        order.push(s);
                    }
                    if order.is_empty() {
                        return Err(err(
                            body.trim_end().chars().count() + 1,
                            "sections go here, in the order they play",
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
                        "a line starts with tempo, swing, scale, setting, track, strip, group, master, frag, auto, scene, mod, section, arrange or loop",
                    ));
                }
            }
        }
        if let Some((f, at)) = open {
            check_lanes(&song, f, at)?;
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
                return Err(SongError { line, col: 1, msg });
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
        song.comments = Comments::collect(text, &song);
        Ok(song)
    }

    /// Bars in the arrangement; 0 without one.
    pub fn bars(&self) -> u64 {
        self.arrange
            .iter()
            .filter_map(|s| self.sections.get(*s))
            .map(|s| u64::from(s.bars))
            .sum()
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
            let Some(section) = self.sections.get(*sec) else {
                continue;
            };
            let len = u64::from(section.bars) * STEPS_PER_BAR;
            if s < begin + len {
                return At::In {
                    entry,
                    section: *sec,
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
                "scale {} {}",
                crate::notes::NAMES
                    .get(usize::from(s.root))
                    .copied()
                    .unwrap_or("c"),
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
        }
        lines.extend(self.tracks.iter().map(|t| {
            let setting = t.setting.and_then(|i| self.settings.get(i));
            match (setting, t.preset) {
                (Some(st), _) => format!("track {} {} {}", t.name, t.kind.name(), st.name),
                (None, Some(p)) => format!(
                    "track {} {} {} {}",
                    t.name,
                    t.kind.name(),
                    model_name(p.model()),
                    preset_name(p)
                ),
                (None, None) => format!("track {} {}", t.name, t.kind.name()),
            }
        }));
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
            lines.push(format!("{at}: {}", sets.join(", ")));
        }
        for f in &self.frags {
            let track = self.tracks.get(f.track).map_or("", |t| t.name.as_str());
            lines.push(String::new());
            if let Some(n) = &f.notes {
                let live = if f.live { " live" } else { "" };
                let bars = if matches!(n.seq, notes::Seq::Timed(_)) {
                    format!(" bars {}", n.bars)
                } else {
                    String::new()
                };
                let voicing = if f.voicing { " voicing" } else { "" };
                lines.push(format!("frag {} = {}{live}{bars}{voicing}", f.name, track));
                lines.push(format!("  {}", n.print()));
                continue;
            }
            lines.push(format!("frag {} = {} /16", f.name, track));
            for l in &f.lanes {
                let steps: String = match &l.call {
                    Some(e) => e.print(),
                    None => l.steps.iter().map(|st| st.char()).collect(),
                };
                lines.push(format!("  {} {}", l.pad.name(), steps));
            }
        }
        if !self.autos.is_empty() || !self.scenes.is_empty() || !self.mods.is_empty() {
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
            };
            lines.push(format!(
                "auto {} = {}.{} {} /{}",
                a.name,
                self.target_name(a.target),
                param_name(a.param),
                shape,
                a.bars
            ));
        }
        for s in &self.scenes {
            let sets: Vec<String> = s
                .sets
                .iter()
                .map(|(t, p, v)| format!("{}.{} {}", self.target_name(*t), param_name(*p), v))
                .collect();
            lines.push(format!("scene {}: {}", s.name, sets.join(", ")));
        }
        for m in &self.mods {
            lines.push(format!(
                "mod {}.{} = {}",
                self.target_name(m.target),
                param_name(m.param).to_ascii_lowercase(),
                m.signal
            ));
        }
        if !self.sections.is_empty() {
            lines.push(String::new());
        }
        for s in &self.sections {
            let mut line = format!("section {} {}:", s.name, s.bars);
            let names = s
                .frags
                .iter()
                .filter_map(|f| self.frags.get(*f).map(|x| x.name.clone()))
                .chain(
                    s.autos
                        .iter()
                        .filter_map(|a| self.autos.get(*a).map(|x| x.name.clone())),
                )
                .chain(
                    s.scenes
                        .iter()
                        .filter_map(|c| self.scenes.get(*c).map(|x| format!("[{}]", x.name))),
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
                .filter_map(|s| self.sections.get(*s).map(|s| s.name.as_str()))
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

    /// Replace the generator call of note frag `frag` with the events it
    /// plays (`events`, or its own when `None`), written as mini-notation, and
    /// make it a fixed frag. False when it is not a generated frag or the
    /// events cannot be written back exactly.
    pub fn freeze(&mut self, frag: usize, events: Option<&[notes::Event]>) -> bool {
        let Some(f) = self.frags.get_mut(frag) else {
            return false;
        };
        let Some(n) = f.notes.as_ref() else {
            return false;
        };
        if !matches!(n.seq, notes::Seq::Generated(_) | notes::Seq::Euclid(..)) {
            return false;
        }
        match notes::freeze(events.unwrap_or(&n.events), n.bars) {
            Some(frozen) => {
                f.notes = Some(frozen);
                f.live = false;
                true
            }
            None => false,
        }
    }

    // --- Arranger edits (#171): each keeps the song valid, false when refused.

    /// Put fragment (`kind` 0), lane (1) or scene (2) `item` in section `s`,
    /// or take it out.
    pub fn toggle(&mut self, s: usize, kind: u32, item: usize) -> bool {
        let exists = match kind {
            0 => item < self.frags.len(),
            1 => item < self.autos.len(),
            2 => item < self.scenes.len(),
            _ => false,
        };
        let Some(sec) = self.sections.get_mut(s) else {
            return false;
        };
        let list = match kind {
            0 => &mut sec.frags,
            1 => &mut sec.autos,
            _ => &mut sec.scenes,
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

    /// A new empty section of `bars` bars named `partN`, added to the end of
    /// the arrangement; its index.
    pub fn add_section(&mut self, bars: u32) -> Option<usize> {
        if self.sections.len() >= MAX_SECTIONS
            || self.arrange.len() >= MAX_ARRANGE
            || !(1..=MAX_BARS).contains(&bars)
        {
            return None;
        }
        let name = (1..)
            .map(|n| format!("part{n}"))
            .find(|n| !self.sections.iter().any(|s| &s.name == n))?;
        // The first section holds every frag, so turning the arrangement on
        // keeps the music that was looping instead of silencing it.
        let frags = if self.arrange.is_empty() {
            (0..self.frags.len()).collect()
        } else {
            Vec::new()
        };
        self.sections.push(Section {
            name,
            bars,
            frags,
            autos: Vec::new(),
            scenes: Vec::new(),
        });
        let s = self.sections.len() - 1;
        self.arrange.push(s);
        Some(s)
    }

    pub fn set_bars(&mut self, s: usize, bars: u32) -> bool {
        match self.sections.get_mut(s) {
            Some(sec) if (1..=MAX_BARS).contains(&bars) => {
                sec.bars = bars;
                self.fit_loop();
                true
            }
            _ => false,
        }
    }

    /// Play section `s` at place `at` of the arrangement (`at` may be its length).
    pub fn arrange_insert(&mut self, at: usize, s: usize) -> bool {
        if s >= self.sections.len() || at > self.arrange.len() || self.arrange.len() >= MAX_ARRANGE
        {
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

    /// Whether section `s` holds fragment (`kind` 0), lane (1) or scene (2) `item`.
    pub fn section_has(&self, s: usize, kind: u32, item: usize) -> bool {
        self.sections.get(s).is_some_and(|sec| match kind {
            0 => sec.frags.contains(&item),
            1 => sec.autos.contains(&item),
            2 => sec.scenes.contains(&item),
            _ => false,
        })
    }

    /// Do `op` on note frag `frag` (from the composer's note view) and write
    /// it back as notes, whatever notation it was in. False when it is not a
    /// written note frag (a generated one is frozen first), the edit names no
    /// note, or the result cannot be written.
    pub fn edit_note(&mut self, frag: usize, op: notes::Edit) -> bool {
        let Some(f) = self.frags.get_mut(frag) else {
            return false;
        };
        let Some(n) = f.notes.as_ref() else {
            return false;
        };
        if matches!(n.seq, notes::Seq::Generated(_) | notes::Seq::Euclid(..)) {
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
        match edited {
            Some(edited) => {
                f.notes = Some(edited);
                true
            }
            None => false,
        }
    }

    /// Set one step; false when there is no such step.
    pub fn set_step(&mut self, frag: usize, lane: usize, step: usize, to: Step) -> bool {
        let slot = self
            .frags
            .get_mut(frag)
            .and_then(|f| f.lanes.get_mut(lane))
            .and_then(|l| l.steps.get_mut(step));
        match slot {
            Some(s) => {
                *s = to;
                if let Some(l) = self.frags.get_mut(frag).and_then(|f| f.lanes.get_mut(lane)) {
                    l.call = None;
                }
                true
            }
            None => false,
        }
    }
}

impl Song {
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

    /// Track `t` plays factory preset `preset`, without a setting; refused when
    /// the preset's model does not play the track's kind.
    pub fn set_track_preset(&mut self, t: usize, preset: Preset) -> bool {
        match self.tracks.get_mut(t) {
            Some(tr) if fits(tr.kind, Some(preset.model())) => {
                tr.preset = Some(preset);
                tr.setting = None;
                tr.picked = false;
                true
            }
            _ => false,
        }
    }

    /// Track `t` plays the song's setting `i`.
    pub fn set_track_setting(&mut self, t: usize, i: usize) -> bool {
        let Some(preset) = self.settings.get(i).map(|st| st.preset) else {
            return false;
        };
        match self.tracks.get_mut(t) {
            Some(tr) if fits(tr.kind, Some(preset.model())) => {
                tr.preset = Some(preset);
                tr.setting = Some(i);
                tr.picked = false;
                true
            }
            _ => false,
        }
    }

    /// A new setting of track `t`'s preset and `sets`, named after the track
    /// (`bass`, else `bass2`, …), which the track then plays. `None` when the
    /// track has no preset, the song has 16 settings or `sets` is more than a
    /// setting holds.
    pub fn add_setting(&mut self, t: usize, sets: Vec<(Param, f32)>) -> Option<usize> {
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
        self.settings.push(Setting { name, preset, sets });
        let i = self.settings.len() - 1;
        let tr = self.tracks.get_mut(t)?;
        tr.setting = Some(i);
        tr.picked = false;
        Some(i)
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
        Kind::Drums => m.uses_drums() || m.uses_pads(),
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
const DRUMS: &[Preset] = &[P::Kit808, P::Kit909, P::TightKit, P::Hard909, P::PadsLoud];
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
        .frags
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
fn target_param(song: &Song, text: &str) -> Result<(Target, Param), &'static str> {
    let (t, p) = text
        .split_once('.')
        .ok_or("target.Param goes here, e.g. strip1.Level")?;
    let param = Param::by_name(p).ok_or("no parameter has this name")?;
    if matches!(param, Param::Model | Param::Out) {
        return Err("the model and the routing can't be automated");
    }
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
        return Err("a target is a track, strip1–16, group1–8 or master");
    };
    let ok = match target {
        Target::Master => param.is_global(),
        Target::Strip(s) if s >= STRIPS => param.is_strip(),
        _ => !param.is_global(),
    };
    if ok {
        Ok((target, param))
    } else {
        Err("this parameter does not belong to this target")
    }
}

fn check_lanes(song: &Song, f: usize, line: usize) -> Result<(), SongError> {
    match song.frags.get(f) {
        Some(frag) if frag.lanes.is_empty() && frag.notes.is_none() => Err(SongError {
            line,
            col: 1,
            msg: match song.tracks.get(frag.track).map(|t| t.kind) {
                Some(Kind::Synth) => "a frag needs a line of notes",
                Some(Kind::Sampler) => "a frag needs lanes or a line of notes",
                _ => "a frag needs at least one lane",
            },
        }),
        _ => Ok(()),
    }
}

/// `pad steps...`: the steps may be split by spaces for reading.
fn parse_lane(ws: &[Word<'_>], line: usize) -> Result<Lane, SongError> {
    let err = |col: usize, msg: &'static str| SongError { line, col, msg };
    let Some(first) = ws.first() else {
        return Err(err(1, "a lane is a pad and its steps"));
    };
    let pad = Pad::from_name(first.text).ok_or(err(
        first.col,
        "a pad is bd sn cp ch oh lt mt ht rs cl ma cb cy lc mc or hc",
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
        });
    }
    let mut steps = Vec::new();
    for w in ws.iter().skip(1) {
        for (k, c) in w.text.chars().enumerate() {
            let step = Step::from_char(c).ok_or(err(w.col + k, "a step is x, X or ."))?;
            if steps.len() >= MAX_STEPS {
                return Err(err(w.col + k, "a lane has at most 64 steps"));
            }
            steps.push(step);
        }
    }
    if steps.is_empty() {
        return Err(err(first.col, "a lane needs its steps: x, X or ."));
    }
    Ok(Lane {
        pad,
        steps,
        call: None,
    })
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
            return Err("an insert is Off, Overdrive, Distortion, Fuzz, Eq, Comp or Vocoder");
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
    let err = |col: usize, msg: &'static str| SongError { line, col, msg };
    let end = body.trim_end().chars().count() + 1;
    let first = ws.first().ok_or(err(1, "a mixer line goes here"))?;
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
}

pub mod lex;
pub mod signal;

#[cfg(test)]
mod tests;
