//! Standard MIDI File parser (spec 002, Req 8).
//!
//! Total: any byte string gives `Ok` or an `Error`, never a panic (ADR-0002).
//! Reads formats 0, 1 and 2 (2 is played like 1), ticks-per-quarter
//! division, running status, note on/off, tempo and track names. Everything
//! else (controllers, sysex, other meta events) is skipped.

/// Why a file was rejected. `code` is what crosses the C ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Doesn't start with `MThd`.
    NotMidi,
    /// A chunk, event or length runs past the end of the file.
    Truncated,
    /// SMPTE time division, or a zero division.
    Unsupported,
    /// A status byte that can't start an event, or a malformed number.
    BadEvent,
}

impl Error {
    /// Negative codes for `midi_load` (spec 002, Req 8).
    pub fn code(self) -> i32 {
        match self {
            Error::NotMidi => -1,
            Error::Truncated => -2,
            Error::Unsupported => -3,
            Error::BadEvent => -4,
        }
    }
}

/// A parsed file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Smf {
    pub format: u16,
    /// Ticks per quarter note.
    pub division: u16,
    pub tracks: Vec<Track>,
}

/// One `MTrk` chunk.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// The first track-name meta event, as raw bytes (often Latin-1).
    pub name: Vec<u8>,
    pub events: Vec<Event>,
}

/// An event at an absolute tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub tick: u64,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    /// Microseconds per quarter note.
    Tempo(u32),
}

/// Parse a Standard MIDI File.
pub fn parse(bytes: &[u8]) -> Result<Smf, Error> {
    if !bytes.starts_with(b"MThd") {
        return Err(Error::NotMidi);
    }
    let mut r = Reader::new(bytes);
    r.take(4)?;
    let len = r.u32()? as usize;
    let mut header = Reader::new(r.take(len)?);
    let format = header.u16()?;
    let _declared_tracks = header.u16()?;
    let division = header.u16()?;
    if division & 0x8000 != 0 || division == 0 {
        return Err(Error::Unsupported);
    }
    let mut tracks = Vec::new();
    // Trailing bytes shorter than a chunk header are padding some writers add.
    while r.remaining() >= 8 {
        let id = r.take(4)?;
        let len = r.u32()? as usize;
        let body = r.take(len)?;
        if id == b"MTrk" {
            tracks.push(parse_track(body)?);
        }
    }
    Ok(Smf {
        format,
        division,
        tracks,
    })
}

fn parse_track(body: &[u8]) -> Result<Track, Error> {
    let mut r = Reader::new(body);
    let mut track = Track::default();
    let mut tick: u64 = 0;
    let mut running: Option<u8> = None;
    while !r.at_end() {
        tick = tick.saturating_add(u64::from(r.vlq()?));
        let first = r.peek()?;
        let status = if first & 0x80 != 0 {
            r.u8()?;
            first
        } else {
            running.ok_or(Error::BadEvent)?
        };
        match status {
            0xFF => {
                running = None;
                let kind = r.u8()?;
                let len = r.vlq()? as usize;
                let data = r.take(len)?;
                match (kind, data) {
                    (0x2F, _) => break,
                    (0x51, [a, b, c]) => track.events.push(Event {
                        tick,
                        kind: Kind::Tempo(u32::from_be_bytes([0, *a, *b, *c])),
                    }),
                    (0x03, name) if track.name.is_empty() => track.name = name.to_vec(),
                    _ => {}
                }
            }
            0xF0 | 0xF7 => {
                running = None;
                let len = r.vlq()? as usize;
                r.take(len)?;
            }
            0x80..=0xEF => {
                running = Some(status);
                let channel = status & 0x0F;
                match status & 0xF0 {
                    0xC0 | 0xD0 => {
                        r.u8()?;
                    }
                    high => {
                        let note = r.u8()? & 0x7F;
                        let velocity = r.u8()? & 0x7F;
                        let kind = match high {
                            0x90 if velocity > 0 => Some(Kind::NoteOn {
                                channel,
                                note,
                                velocity,
                            }),
                            0x80 | 0x90 => Some(Kind::NoteOff { channel, note }),
                            _ => None,
                        };
                        if let Some(kind) = kind {
                            track.events.push(Event { tick, kind });
                        }
                    }
                }
            }
            _ => return Err(Error::BadEvent),
        }
    }
    Ok(track)
}

/// A bounds-checked cursor: every read is `Result`, nothing indexes.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn at_end(&self) -> bool {
        self.remaining() == 0
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).ok_or(Error::Truncated)?;
        let slice = self.bytes.get(self.pos..end).ok_or(Error::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn peek(&self) -> Result<u8, Error> {
        self.bytes.get(self.pos).copied().ok_or(Error::Truncated)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        let b = self.peek()?;
        self.pos = self.pos.saturating_add(1);
        Ok(b)
    }

    fn u16(&mut self) -> Result<u16, Error> {
        let s = self.take(2)?;
        <[u8; 2]>::try_from(s)
            .map(u16::from_be_bytes)
            .map_err(|_| Error::Truncated)
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let s = self.take(4)?;
        <[u8; 4]>::try_from(s)
            .map(u32::from_be_bytes)
            .map_err(|_| Error::Truncated)
    }

    /// A variable-length quantity: at most four bytes, 28 bits.
    fn vlq(&mut self) -> Result<u32, Error> {
        let mut value: u32 = 0;
        for _ in 0..4 {
            let b = self.u8()?;
            value = (value << 7) | u32::from(b & 0x7F);
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(Error::BadEvent)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Build a file: header plus one `MTrk` per body.
    pub(crate) fn file(format: u16, division: u16, tracks: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"MThd".to_vec();
        out.extend(6u32.to_be_bytes());
        out.extend(format.to_be_bytes());
        out.extend((tracks.len() as u16).to_be_bytes());
        out.extend(division.to_be_bytes());
        for t in tracks {
            out.extend(b"MTrk");
            out.extend((t.len() as u32).to_be_bytes());
            out.extend(t);
        }
        out
    }

    const END: [u8; 4] = [0x00, 0xFF, 0x2F, 0x00];

    #[test]
    fn note_on_and_off() {
        let mut t = vec![0x00, 0x90, 60, 100, 0x83, 0x60, 0x80, 60, 0];
        t.extend(END);
        let smf = parse(&file(0, 480, &[t])).expect("parses");
        assert_eq!(smf.division, 480);
        let ev = &smf.tracks[0].events;
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[1].tick, 480);
        assert_eq!(
            ev[1].kind,
            Kind::NoteOff {
                channel: 0,
                note: 60
            }
        );
    }

    #[test]
    fn running_status_and_zero_velocity_off() {
        let mut t = vec![0x00, 0x91, 60, 100, 0x10, 62, 90, 0x10, 60, 0];
        t.extend(END);
        let smf = parse(&file(0, 96, &[t])).expect("parses");
        let kinds: Vec<Kind> = smf.tracks[0].events.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                Kind::NoteOn {
                    channel: 1,
                    note: 60,
                    velocity: 100
                },
                Kind::NoteOn {
                    channel: 1,
                    note: 62,
                    velocity: 90
                },
                Kind::NoteOff {
                    channel: 1,
                    note: 60
                },
            ]
        );
    }

    #[test]
    fn tempo_and_name() {
        let mut t = vec![0x00, 0xFF, 0x03, 3, b'V', b'i', b'v'];
        t.extend([0x00, 0xFF, 0x51, 3, 0x07, 0xA1, 0x20]);
        t.extend(END);
        let smf = parse(&file(1, 480, &[t])).expect("parses");
        assert_eq!(smf.tracks[0].name, b"Viv");
        assert_eq!(smf.tracks[0].events[0].kind, Kind::Tempo(500_000));
    }

    #[test]
    fn rejects_what_it_cannot_play() {
        assert_eq!(parse(b"RIFF...."), Err(Error::NotMidi));
        assert_eq!(parse(b"MThd\0\0"), Err(Error::Truncated));
        assert_eq!(parse(&file(0, 0xE728, &[])), Err(Error::Unsupported));
        // A data byte with no running status to apply it to.
        assert_eq!(
            parse(&file(0, 96, &[vec![0x00, 0x40, 0x40]])),
            Err(Error::BadEvent)
        );
        // A note that runs off the end of its chunk.
        assert_eq!(
            parse(&file(0, 96, &[vec![0x00, 0x90, 60]])),
            Err(Error::Truncated)
        );
    }

    /// Totality: random bytes and mutations of a valid file never panic.
    #[test]
    fn never_panics_on_garbage() {
        let mut x: u32 = 0x9E37_79B9;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x
        };
        let mut valid = vec![
            0x00, 0x90, 60, 100, 0x10, 62, 90, 0x10, 0xFF, 0x51, 3, 1, 2, 3,
        ];
        valid.extend(END);
        let valid = file(1, 480, &[valid.clone(), valid]);
        for round in 0..20_000 {
            let mut bytes = if round % 2 == 0 {
                valid.clone()
            } else {
                (0..(next() % 64)).map(|_| next() as u8).collect()
            };
            if round % 2 == 0 {
                for _ in 0..(next() % 4 + 1) {
                    let i = next() as usize % bytes.len();
                    bytes[i] = next() as u8;
                }
                bytes.truncate(next() as usize % (bytes.len() + 1));
            }
            let _parsed = parse(&bytes);
        }
    }
}
