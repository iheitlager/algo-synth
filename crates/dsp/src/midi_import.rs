//! MIDI import (#173, ADR-0015): a Standard MIDI File becomes song text, so
//! an imported score is a song like any other, edited and arranged as text.
//!
//! Each MIDI channel with notes becomes a `synth` track, named after the
//! file's track it came from. Times snap to the song's grid of 48 ticks to the
//! bar (12 to the quarter, so triplet sixteenths stay exact); a note-on pairs
//! with the first open note-off of its channel and pitch. The song is cut into
//! sections of 8 bars (or 4, 2, 1 when a chunk would hold too many notes for a
//! line, or the song too many fragments): one timed-note fragment per channel
//! and chunk, identical chunks sharing a fragment and identical sections a
//! section, and `arrange` plays them in order.
//!
//! Limits, as the song has them: bars are 4/4 (a 3/4 file keeps its timing,
//! not its bar lines), the tempo is the file's first, and anything finer than
//! a tick is rounded. Import allocates, so it runs when asked, never in
//! `render`; it is total: whatever the file, a song or an error.

use crate::notes::{self, Event, MAX_EVENTS, TICKS_PER_BAR};
use crate::smf::{Kind, Smf};
use crate::song::{MAX_ARRANGE, MAX_FRAGS, MAX_SECTIONS, MAX_TRACKS};

/// Ticks of the song's grid to a quarter note.
const TICKS_PER_QUARTER: u64 = TICKS_PER_BAR as u64 / 4;
/// Chunk lengths tried, longest first.
const CHUNKS: [u32; 4] = [8, 4, 2, 1];
/// Longest track name kept; fragment names add `_N`.
const NAME_LEN: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// The file has no notes.
    NoNotes,
    /// More notes, fragments or sections than a song can hold.
    TooBig,
}

impl ImportError {
    /// Negative codes for `midi_import`, after the MIDI file's own.
    pub fn code(self) -> i32 {
        match self {
            ImportError::NoNotes => -7,
            ImportError::TooBig => -8,
        }
    }
}

/// The song text and, for each of its tracks in order, the MIDI channel it
/// came from, so the engine can route it as the player did.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    pub text: String,
    pub channels: Vec<u8>,
}

/// A note on the grid: its start and length in ticks, pitch and velocity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Note {
    start: u64,
    len: u64,
    note: u8,
    vel: u8,
}

pub fn import(smf: &Smf) -> Result<Imported, ImportError> {
    let ppq = u64::from(smf.division.max(1));
    let grid = |t: u64| (t.saturating_mul(TICKS_PER_QUARTER) + ppq / 2) / ppq;

    // Every event of every track in time order; at one tick, offs before ons,
    // so a repeated note is not cut by its own next start.
    let mut all: Vec<(u64, u8, usize, Kind)> = Vec::new();
    for (t, track) in smf.tracks.iter().enumerate() {
        for e in &track.events {
            let order = match e.kind {
                Kind::NoteOff { .. } => 0,
                Kind::NoteOn { .. } => 1,
                Kind::Tempo(_) => 2,
            };
            all.push((e.tick, order, t, e.kind));
        }
    }
    all.sort_by_key(|(tick, order, t, _)| (*tick, *order, *t));

    let tempo = all.iter().find_map(|(_, _, _, k)| match k {
        Kind::Tempo(us) => Some(60_000_000.0 / f64::from((*us).max(1))),
        _ => None,
    });

    // Notes per channel; open notes per (channel, pitch), first in first out.
    let mut open: Vec<Vec<(u64, u8)>> = vec![Vec::new(); 16 * 128];
    let mut notes: Vec<Vec<Note>> = vec![Vec::new(); 16];
    let mut names: Vec<Option<usize>> = vec![None; 16];
    let mut last = 0;
    for (tick, _, t, kind) in &all {
        let at = grid(*tick);
        last = last.max(at);
        match *kind {
            Kind::NoteOn {
                channel,
                note,
                velocity,
            } => {
                let (c, n) = (usize::from(channel & 15), usize::from(note & 127));
                if let Some(q) = open.get_mut(c * 128 + n) {
                    q.push((at, velocity.clamp(1, 127)));
                }
                if let Some(slot) = names.get_mut(c) {
                    slot.get_or_insert(*t);
                }
            }
            Kind::NoteOff { channel, note } => {
                let (c, n) = (usize::from(channel & 15), usize::from(note & 127));
                let started = open
                    .get_mut(c * 128 + n)
                    .filter(|q| !q.is_empty())
                    .map(|q| q.remove(0));
                if let (Some((start, vel)), Some(list)) = (started, notes.get_mut(c)) {
                    list.push(Note {
                        start,
                        len: at.saturating_sub(start).max(1),
                        note: note & 127,
                        vel,
                    });
                }
            }
            Kind::Tempo(_) => {}
        }
    }
    // A note never released ends with the file.
    for (i, q) in open.iter().enumerate() {
        for (start, vel) in q {
            if let Some(list) = notes.get_mut(i / 128) {
                let note = u8::try_from(i % 128).unwrap_or(0);
                list.push(Note {
                    start: *start,
                    len: last.saturating_sub(*start).max(1),
                    note,
                    vel: *vel,
                });
            }
        }
    }

    let channels: Vec<u8> = (0..16u8)
        .filter(|c| notes.get(usize::from(*c)).is_some_and(|n| !n.is_empty()))
        .collect();
    if channels.is_empty() {
        return Err(ImportError::NoNotes);
    }
    if channels.len() > MAX_TRACKS {
        return Err(ImportError::TooBig);
    }
    for list in notes.iter_mut() {
        list.sort_by_key(|n| (n.start, n.note, n.len, n.vel));
    }
    let max_start = channels
        .iter()
        .filter_map(|c| notes.get(usize::from(*c))?.iter().map(|n| n.start).max())
        .max()
        .unwrap_or(0);
    let total_bars =
        u32::try_from(max_start / u64::from(TICKS_PER_BAR) + 1).map_err(|_| ImportError::TooBig)?;
    let track_names = unique_names(&channels, &names, smf);

    for chunk in CHUNKS {
        if let Some(text) = layout(&channels, &notes, &track_names, total_bars, chunk, tempo) {
            return Ok(Imported { text, channels });
        }
    }
    Err(ImportError::TooBig)
}

/// The song in chunks of `chunk` bars, or `None` when it does not fit.
fn layout(
    channels: &[u8],
    notes: &[Vec<Note>],
    names: &[String],
    total_bars: u32,
    chunk: u32,
    tempo: Option<f64>,
) -> Option<String> {
    let chunks = total_bars.div_ceil(chunk);
    if usize::try_from(chunks).ok()? > MAX_ARRANGE {
        return None;
    }
    // Fragments by content: (track, bars, line) → name.
    let mut frags: Vec<(usize, u32, String, String)> = Vec::new();
    // Sections by content: (bars, fragment names) → name.
    let mut sections: Vec<(u32, Vec<String>, String)> = Vec::new();
    let mut arrange: Vec<String> = Vec::new();
    for i in 0..chunks {
        let bars = chunk.min(total_bars - i * chunk);
        let from = u64::from(i * chunk) * u64::from(TICKS_PER_BAR);
        let to = from + u64::from(bars) * u64::from(TICKS_PER_BAR);
        let mut playing = Vec::new();
        for (t, c) in channels.iter().enumerate() {
            let list = notes.get(usize::from(*c))?;
            let events: Vec<Event> = list
                .iter()
                .filter(|n| n.start >= from && n.start < to)
                .map(|n| Event {
                    start: u32::try_from(n.start - from).unwrap_or(0),
                    len: u32::try_from(n.len.min(u64::from(notes::MAX_BARS * TICKS_PER_BAR)))
                        .unwrap_or(1),
                    note: n.note,
                    accent: false,
                    vel: n.vel,
                })
                .collect();
            if events.is_empty() {
                continue;
            }
            if events.len() > MAX_EVENTS {
                return None;
            }
            let line = timed_line(&events);
            let name = match frags
                .iter()
                .find(|(ft, fb, fl, _)| *ft == t && *fb == bars && *fl == line)
            {
                Some((_, _, _, n)) => n.clone(),
                None => {
                    let track = names.get(t)?;
                    let n = format!("{track}_{}", frags.iter().filter(|f| f.0 == t).count() + 1);
                    frags.push((t, bars, line, n.clone()));
                    n
                }
            };
            playing.push(name);
        }
        let section = match sections
            .iter()
            .find(|(sb, sf, _)| *sb == bars && *sf == playing)
        {
            Some((_, _, n)) => n.clone(),
            None => {
                let n = format!("s{}", sections.len() + 1);
                sections.push((bars, playing, n.clone()));
                n
            }
        };
        arrange.push(section);
    }
    if frags.len() > MAX_FRAGS || sections.len() > MAX_SECTIONS {
        return None;
    }
    let bpm = tempo.unwrap_or(120.0).clamp(20.0, 300.0);
    let mut out = vec![
        format!("tempo {}", (bpm * 100.0).round() / 100.0),
        "swing 50".to_string(),
    ];
    out.extend(names.iter().map(|n| format!("track {n} synth")));
    for (t, bars, line, name) in &frags {
        out.push(String::new());
        out.push(format!("frag {name} = {} bars {bars}", names.get(*t)?));
        out.push(format!("  {line}"));
    }
    out.push(String::new());
    for (bars, playing, name) in &sections {
        let list = if playing.is_empty() {
            String::new()
        } else {
            format!(" {}", playing.join(" "))
        };
        out.push(format!("section {name} {bars}:{list}"));
    }
    out.push(format!("arrange {}", arrange.join(" ")));
    out.push(String::new());
    Some(out.join("\n"))
}

/// Events as a line of timed notes, the way `notes` prints one.
fn timed_line(events: &[Event]) -> String {
    let words: Vec<String> = events
        .iter()
        .map(|e| {
            format!(
                "{}@{}:{}:{}",
                notes::note_name(e.note),
                e.start,
                e.len,
                e.vel.max(1)
            )
        })
        .collect();
    words.join(" ")
}

/// A song name per channel from the file's track it first appeared in:
/// letters, digits and `_`, starting with a letter, unique; `ch<N>` without one.
fn unique_names(channels: &[u8], tracks: &[Option<usize>], smf: &Smf) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in channels {
        let raw = tracks
            .get(usize::from(*c))
            .copied()
            .flatten()
            .and_then(|t| smf.tracks.get(t))
            .map(|t| String::from_utf8_lossy(&t.name).to_string())
            .unwrap_or_default();
        let mut name: String = raw
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .split('_')
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join("_");
        if !name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        {
            name = if name.is_empty() {
                format!("ch{}", c + 1)
            } else {
                format!("t_{name}")
            };
        }
        name.truncate(NAME_LEN);
        let base = name.clone();
        let mut k = 2;
        while out.contains(&name) {
            name = format!(
                "{}_{k}",
                base.chars().take(NAME_LEN - 3).collect::<String>()
            );
            k += 1;
        }
        out.push(name);
    }
    out
}

#[cfg(test)]
mod tests;
