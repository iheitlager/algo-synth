//! MIDI input (#257 stage 1, #10): the bytes of one message, as Web MIDI hands
//! them over, read into an event; `Engine::midi_in` plays it. JavaScript only
//! forwards (ADR-0001). Decoding is total: any three bytes give an event or
//! `None`, without allocating or panicking (ADR-0002).
//!
//! Web MIDI delivers whole messages, so there is no running status here.
//! System messages (SysEx, clock, transport) are not read yet.

/// One channel message, its channel 0..=15.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A key pressed, velocity 1..=127 as 0..1.
    NoteOn {
        channel: u8,
        note: u8,
        velocity: f32,
    },
    /// A key released: a note-off, or a note-on of velocity 0.
    NoteOff { channel: u8, note: u8 },
    /// The pitch wheel, −1..=1 from all 14 bits, 0 at rest.
    Bend { channel: u8, value: f32 },
    /// A controller, 0..=127.
    Control { channel: u8, number: u8, value: u8 },
    /// Pressure on one key (poly aftertouch), 0..=127.
    KeyPressure { channel: u8, note: u8, value: u8 },
    /// Pressure on the whole keyboard (channel aftertouch), 0..=127.
    Pressure { channel: u8, value: u8 },
    /// A program change, 0..=127.
    Program { channel: u8, number: u8 },
}

/// The mod wheel's controller number.
pub const MOD_WHEEL: u8 = 1;

/// The event in `status`, `d1`, `d2`, or `None` for a system message, a status
/// that is not one, or a data byte with its top bit set.
pub fn decode(status: u8, d1: u8, d2: u8) -> Option<Event> {
    if !(0x80..0xF0).contains(&status) || d1 > 0x7F || d2 > 0x7F {
        return None;
    }
    let channel = status & 0x0F;
    Some(match status & 0xF0 {
        0x80 => Event::NoteOff { channel, note: d1 },
        0x90 if d2 == 0 => Event::NoteOff { channel, note: d1 },
        0x90 => Event::NoteOn {
            channel,
            note: d1,
            velocity: f32::from(d2) / 127.0,
        },
        0xA0 => Event::KeyPressure {
            channel,
            note: d1,
            value: d2,
        },
        0xB0 => Event::Control {
            channel,
            number: d1,
            value: d2,
        },
        0xC0 => Event::Program {
            channel,
            number: d1,
        },
        0xD0 => Event::Pressure { channel, value: d1 },
        // 0xE0: the low seven bits first; the top is +8191 and reads as 1.
        _ => {
            let raw = (i32::from(d2) << 7 | i32::from(d1)) - 8192;
            let value = if raw >= 0 {
                raw as f32 / 8191.0
            } else {
                raw as f32 / 8192.0
            };
            Event::Bend { channel, value }
        }
    })
}

/// Where MIDI notes go: the synth the keys play, and which synth each held
/// note started on, so its release reaches it after the target changes.
#[derive(Clone, Debug)]
pub struct MidiIn {
    pub target: usize,
    held: [Option<u8>; 128],
}

impl Default for MidiIn {
    fn default() -> Self {
        Self {
            target: 0,
            held: [None; 128],
        }
    }
}

impl MidiIn {
    /// Remember that `note` started on `synth`.
    pub fn press(&mut self, note: u8, synth: usize) {
        if let Some(h) = self.held.get_mut(usize::from(note)) {
            *h = u8::try_from(synth).ok();
        }
    }

    /// The synth `note` started on, forgotten; the target if it is not held.
    pub fn release(&mut self, note: u8) -> usize {
        self.held
            .get_mut(usize::from(note))
            .and_then(Option::take)
            .map_or(self.target, usize::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Messages as the Akai MPK mini Plus sent them (#420).
    #[test]
    fn the_mpk_mini_plus_messages_decode() {
        assert_eq!(
            decode(0x90, 60, 100),
            Some(Event::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100.0 / 127.0
            })
        );
        // Its keys release with a real note-off.
        assert_eq!(
            decode(0x80, 60, 0),
            Some(Event::NoteOff {
                channel: 0,
                note: 60
            })
        );
        // A pad on channel 10 and its poly aftertouch.
        assert_eq!(
            decode(0x99, 36, 110),
            Some(Event::NoteOn {
                channel: 9,
                note: 36,
                velocity: 110.0 / 127.0
            })
        );
        assert_eq!(
            decode(0xA9, 36, 74),
            Some(Event::KeyPressure {
                channel: 9,
                note: 36,
                value: 74
            })
        );
        // Knob 1, the mod wheel, Play.
        assert_eq!(
            decode(0xB0, 70, 21),
            Some(Event::Control {
                channel: 0,
                number: 70,
                value: 21
            })
        );
        assert_eq!(
            decode(0xB0, 118, 127),
            Some(Event::Control {
                channel: 0,
                number: 118,
                value: 127
            })
        );
    }

    #[test]
    fn a_note_on_of_velocity_zero_is_a_release() {
        assert_eq!(
            decode(0x93, 64, 0),
            Some(Event::NoteOff {
                channel: 3,
                note: 64
            })
        );
    }

    #[test]
    fn the_bend_reads_all_fourteen_bits_and_ends_at_one() {
        let bend = |lsb, msb| match decode(0xE0, lsb, msb) {
            Some(Event::Bend { value, .. }) => value,
            other => panic!("{other:?}"),
        };
        assert_eq!(bend(0x00, 0x40), 0.0, "centre");
        assert_eq!(bend(0x7F, 0x7F), 1.0, "top");
        assert_eq!(bend(0x00, 0x00), -1.0, "bottom");
        // The MPK's wheel moves the low byte too.
        assert!(bend(0x01, 0x40) > 0.0 && bend(0x01, 0x40) < 1.0e-3);
    }

    #[test]
    fn system_and_broken_messages_are_none() {
        for (s, a, b) in [
            (0xF0, 0x7E, 0x7F),
            (0xF8, 0, 0),
            (0xFA, 0, 0),
            (0x40, 1, 2),
            (0x90, 0x80, 1),
            (0x90, 60, 0xFF),
        ] {
            assert_eq!(decode(s, a, b), None, "{s:02X} {a:02X} {b:02X}");
        }
    }

    /// Every three bytes decode without a panic, and every channel message
    /// reads as the kind its status names.
    #[test]
    fn decoding_is_total() {
        for s in 0..=255u8 {
            for d in [0u8, 1, 64, 127, 128, 255] {
                let e = decode(s, d, d);
                assert_eq!(
                    e.is_some(),
                    (0x80..0xF0).contains(&s) && d < 0x80,
                    "{s:02X} {d}"
                );
            }
        }
    }

    #[test]
    fn a_held_note_releases_where_it_started() {
        let mut m = MidiIn::default();
        m.press(60, 2);
        m.target = 5;
        assert_eq!(m.release(60), 2, "where it started");
        assert_eq!(m.release(60), 5, "then the target");
    }
}
