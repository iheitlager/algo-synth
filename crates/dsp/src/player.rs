//! The MIDI file player: a parsed file compiled to sample-timed events.
//!
//! Ticks become samples once, at load, through the tempo map; `render` then
//! only compares integers (ADR-0002, ADR-0005). The engine owns the voices
//! and the channel routing; this module owns the timeline and the cursor.

use crate::smf::{Kind, Smf};

/// Default tempo when a file has none: 120 BPM.
const DEFAULT_TEMPO: u32 = 500_000;

/// A note on or off at an absolute sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayEvent {
    pub sample: u64,
    pub channel: u8,
    pub note: u8,
    pub velocity: f32,
    pub on: bool,
}

/// What the UI shows per MIDI channel that has notes.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub channel: u8,
    pub notes: u32,
    /// First note-on and last event, in samples.
    pub start: u64,
    pub end: u64,
    /// The name of the first track that plays on this channel.
    pub name: Vec<u8>,
}

/// The compiled file and the transport over it.
#[derive(Clone, Debug, Default)]
pub struct Sequence {
    events: Vec<PlayEvent>,
    parts: Vec<Part>,
    /// Length of a 4/4 bar at the opening tempo, in samples.
    bar: u64,
    cursor: usize,
    pos: u64,
    playing: bool,
}

impl Sequence {
    /// Compile a parsed file for playback at `sample_rate`.
    pub fn compile(smf: &Smf, sample_rate: f32) -> Sequence {
        struct Raw {
            tick: u64,
            seq: usize,
            track: usize,
            channel: u8,
            note: u8,
            velocity: u8,
            on: bool,
        }
        let mut raw = Vec::new();
        let mut tempos: Vec<(u64, u32)> = Vec::new();
        for (track, t) in smf.tracks.iter().enumerate() {
            for e in &t.events {
                let seq = raw.len();
                match e.kind {
                    Kind::NoteOn {
                        channel,
                        note,
                        velocity,
                    } => raw.push(Raw {
                        tick: e.tick,
                        seq,
                        track,
                        channel,
                        note,
                        velocity,
                        on: true,
                    }),
                    Kind::NoteOff { channel, note } => raw.push(Raw {
                        tick: e.tick,
                        seq,
                        track,
                        channel,
                        note,
                        velocity: 0,
                        on: false,
                    }),
                    Kind::Tempo(t) => tempos.push((e.tick, t.max(1))),
                }
            }
        }
        // Offs before ons at the same tick, so a repeated note retriggers.
        raw.sort_by_key(|r| (r.tick, r.on, r.seq));
        tempos.sort_by_key(|t| t.0);

        let rate = f64::from(sample_rate);
        let division = f64::from(smf.division.max(1));
        let opening = tempos
            .first()
            .filter(|t| t.0 == 0)
            .map_or(DEFAULT_TEMPO, |t| t.1);
        let bar = (4.0 * f64::from(opening) / 1.0e6 * rate).round() as u64;

        let mut events = Vec::with_capacity(raw.len());
        let mut parts: Vec<Part> = Vec::new();
        let (mut next_tempo, mut at_tick, mut at_sec, mut tempo) = (0, 0u64, 0.0f64, DEFAULT_TEMPO);
        for r in &raw {
            while let Some(&(tick, t)) = tempos.get(next_tempo)
                && tick <= r.tick
            {
                at_sec += seconds(tick.saturating_sub(at_tick), tempo, division);
                at_tick = tick;
                tempo = t;
                next_tempo += 1;
            }
            let sec = at_sec + seconds(r.tick.saturating_sub(at_tick), tempo, division);
            let sample = (sec * rate).round() as u64;
            events.push(PlayEvent {
                sample,
                channel: r.channel,
                note: r.note,
                velocity: f32::from(r.velocity) / 127.0,
                on: r.on,
            });
            match parts.iter_mut().find(|p| p.channel == r.channel) {
                Some(p) => {
                    p.end = sample;
                    if r.on {
                        p.notes = p.notes.saturating_add(1);
                    }
                }
                None if r.on => parts.push(Part {
                    channel: r.channel,
                    notes: 1,
                    start: sample,
                    end: sample,
                    name: smf
                        .tracks
                        .get(r.track)
                        .map(|t| t.name.clone())
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| format!("Channel {}", r.channel + 1).into_bytes()),
                }),
                None => {}
            }
        }
        parts.sort_by_key(|p| p.channel);
        Sequence {
            events,
            parts,
            bar,
            ..Sequence::default()
        }
    }

    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    pub fn events(&self) -> &[PlayEvent] {
        &self.events
    }

    /// Sample of the last event.
    pub fn length(&self) -> u64 {
        self.events.last().map_or(0, |e| e.sample)
    }

    pub fn bar(&self) -> u64 {
        self.bar
    }

    pub fn position(&self) -> u64 {
        self.pos
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    /// Start (from the top if the end was reached). No-op with no events.
    pub fn play(&mut self) {
        if self.events.is_empty() {
            return;
        }
        if self.cursor >= self.events.len() {
            self.seek(0);
        }
        self.playing = true;
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Jump to `sample`; the next event is the first at or after it.
    pub fn seek(&mut self, sample: u64) {
        self.pos = sample.min(self.length());
        self.cursor = self.events.partition_point(|e| e.sample < self.pos);
    }

    /// The next event due at the current position, advancing the cursor.
    pub(crate) fn due(&mut self) -> Option<PlayEvent> {
        if !self.playing {
            return None;
        }
        let ev = *self.events.get(self.cursor)?;
        if ev.sample > self.pos {
            return None;
        }
        self.cursor += 1;
        Some(ev)
    }

    /// Frames to render before the next event, at least 1, at most `remaining`.
    pub(crate) fn frames_until_next(&self, remaining: usize) -> usize {
        if !self.playing {
            return remaining;
        }
        match self.events.get(self.cursor) {
            Some(ev) => {
                let gap = usize::try_from(ev.sample.saturating_sub(self.pos)).unwrap_or(remaining);
                gap.clamp(1, remaining.max(1))
            }
            None => remaining,
        }
    }

    pub(crate) fn advance(&mut self, frames: usize) {
        if self.playing {
            self.pos = self.pos.saturating_add(frames as u64);
        }
    }

    /// Playing, and every event has fired.
    pub(crate) fn finished(&self) -> bool {
        self.playing && self.cursor >= self.events.len()
    }
}

fn seconds(ticks: u64, tempo: u32, division: f64) -> f64 {
    ticks as f64 * f64::from(tempo) / 1.0e6 / division
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smf::{self, tests::file};

    /// Quarter notes at 120 BPM (division 480) on channel 0, then a tempo
    /// change to 60 BPM before the third note.
    fn quarters() -> Smf {
        let mut t = vec![0x00, 0xFF, 0x03, 4, b'L', b'e', b'a', b'd'];
        for i in 0..4u8 {
            if i == 2 {
                t.extend([0x00, 0xFF, 0x51, 3, 0x0F, 0x42, 0x40]); // 1_000_000 µs
            }
            t.extend([0x00, 0x90, 60 + i, 100, 0x83, 0x60, 0x80, 60 + i, 0]);
        }
        t.extend([0x00, 0xFF, 0x2F, 0x00]);
        smf::parse(&file(0, 480, &[t])).expect("parses")
    }

    #[test]
    fn ticks_become_samples_through_the_tempo_map() {
        let s = Sequence::compile(&quarters(), 48_000.0);
        let ons: Vec<u64> = s
            .events()
            .iter()
            .filter(|e| e.on)
            .map(|e| e.sample)
            .collect();
        // 0.5 s per quarter, then 1 s per quarter after the change.
        assert_eq!(ons, vec![0, 24_000, 48_000, 96_000]);
        assert_eq!(s.length(), 144_000);
        assert_eq!(s.bar(), 96_000);
    }

    #[test]
    fn parts_summarise_channels() {
        let s = Sequence::compile(&quarters(), 48_000.0);
        assert_eq!(s.parts().len(), 1);
        let p = &s.parts()[0];
        assert_eq!((p.channel, p.notes, p.start, p.end), (0, 4, 0, 144_000));
        assert_eq!(p.name, b"Lead");
    }

    #[test]
    fn offs_sort_before_ons_at_the_same_sample() {
        let s = Sequence::compile(&quarters(), 48_000.0);
        let at: Vec<bool> = s
            .events()
            .iter()
            .filter(|e| e.sample == 24_000)
            .map(|e| e.on)
            .collect();
        assert_eq!(at, vec![false, true]);
    }

    #[test]
    fn transport_fires_each_event_once() {
        let mut s = Sequence::compile(&quarters(), 48_000.0);
        s.play();
        let mut fired = 0;
        while !s.finished() {
            while s.due().is_some() {
                fired += 1;
            }
            let n = s.frames_until_next(128);
            s.advance(n);
        }
        assert_eq!(fired, 8);
        s.play(); // from the top again
        assert_eq!(s.position(), 0);
    }

    #[test]
    fn seek_lands_on_the_next_event() {
        let mut s = Sequence::compile(&quarters(), 48_000.0);
        s.seek(30_000);
        s.play();
        s.advance(s.frames_until_next(usize::MAX));
        assert_eq!(s.position(), 48_000);
        s.seek(u64::MAX);
        assert_eq!(s.position(), s.length());
    }
}
