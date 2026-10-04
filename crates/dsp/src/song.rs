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
//! Parsing and printing allocate, so they run when a song is loaded or a step
//! edited, never in `render`; the engine plays the parsed song in place. Both
//! are total: whatever the text, the parser returns a song or an error with a
//! line and a column, and never panics. Comments and layout are not kept: a
//! printed song is the canonical form of what was parsed.

use crate::drums::Pad;
use crate::notes::{self, Notes};

/// Most tracks, fragments, lanes per fragment and steps per lane a song may have.
pub const MAX_TRACKS: usize = 16;
pub const MAX_FRAGS: usize = 32;
pub const MAX_STEPS: usize = 64;
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
}

/// A loop on a track: one lane per pad on a drum track, one line of notes on
/// a synth track (the other is then empty).
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    pub name: String,
    pub track: usize,
    pub lanes: Vec<Lane>,
    pub notes: Option<Notes>,
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

/// A track of the song; the engine routes it to a synth.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub name: String,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub tempo: f32,
    pub swing: f32,
    pub tracks: Vec<Track>,
    pub frags: Vec<Fragment>,
}

impl Default for Song {
    fn default() -> Song {
        Song {
            tempo: 120.0,
            swing: 50.0,
            tracks: Vec::new(),
            frags: Vec::new(),
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
        for (i, raw) in text.lines().enumerate() {
            let line = i + 1;
            let err = |col: usize, msg: &'static str| SongError { line, col, msg };
            let body = strip_comment(raw);
            let ws = words(body);
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
                    let frag = song
                        .frags
                        .get_mut(f)
                        .ok_or(err(first.col, "a lane goes under a frag"))?;
                    if frag.notes.is_some() {
                        return Err(err(first.col, "a note frag is one line of notes"));
                    }
                    let n = notes::parse(body.trim_start(), first.col)
                        .map_err(|e| err(e.col, e.msg))?;
                    frag.notes = Some(n);
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
                    expect_end(3)?;
                    if song.tracks.len() >= MAX_TRACKS {
                        return Err(err(first.col, "a song has at most 16 tracks"));
                    }
                    song.tracks.push(Track {
                        name: name.text.to_string(),
                        kind,
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
                    if song.frags.iter().any(|f| f.name == name.text) {
                        return Err(err(name.col, "there is already a frag with this name"));
                    }
                    let eq = arg(2, "= and a track go here")?;
                    if eq.text != "=" {
                        return Err(err(eq.col, "= and a track go here"));
                    }
                    let track = arg(3, "a track goes here")?;
                    let Some(t) = song.tracks.iter().position(|t| t.name == track.text) else {
                        return Err(err(track.col, "no track has this name"));
                    };
                    let synth = song.tracks.get(t).is_some_and(|t| t.kind == Kind::Synth);
                    if let Some(w) = ws.get(4) {
                        if synth {
                            return Err(err(w.col, "a note frag has no step grid"));
                        }
                        if w.text != "/16" {
                            return Err(err(w.col, "only /16 steps for now"));
                        }
                    }
                    expect_end(5)?;
                    if song.frags.len() >= MAX_FRAGS {
                        return Err(err(first.col, "a song has at most 32 frags"));
                    }
                    song.frags.push(Fragment {
                        name: name.text.to_string(),
                        track: t,
                        lanes: Vec::new(),
                        notes: None,
                    });
                    open = Some((song.frags.len() - 1, line));
                }
                _ => {
                    return Err(err(
                        first.col,
                        "a line starts with tempo, swing, track or frag",
                    ));
                }
            }
        }
        if let Some((f, at)) = open {
            check_lanes(&song, f, at)?;
        }
        Ok(song)
    }

    /// The canonical text: parsing it gives this song back.
    pub fn print(&self) -> String {
        let mut lines = vec![
            format!("tempo {}", self.tempo),
            format!("swing {}", self.swing),
        ];
        lines.extend(
            self.tracks
                .iter()
                .map(|t| format!("track {} {}", t.name, t.kind.name())),
        );
        for f in &self.frags {
            let track = self.tracks.get(f.track).map_or("", |t| t.name.as_str());
            lines.push(String::new());
            if let Some(n) = &f.notes {
                lines.push(format!("frag {} = {}", f.name, track));
                lines.push(format!("  {}", n.print()));
                continue;
            }
            lines.push(format!("frag {} = {} /16", f.name, track));
            for l in &f.lanes {
                let steps: String = l.steps.iter().map(|st| st.char()).collect();
                lines.push(format!("  {} {}", l.pad.name(), steps));
            }
        }
        lines.push(String::new());
        lines.join("\n")
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
                true
            }
            None => false,
        }
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
    Ok(Lane { pad, steps })
}

#[cfg(test)]
mod tests;
