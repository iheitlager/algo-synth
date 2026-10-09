//! MIDI input (#257 stage 1, #10): the bytes of one message, as Web MIDI hands
//! them over, read into an event; `Engine::midi_in` plays it. JavaScript only
//! forwards (ADR-0001). Decoding is total: any three bytes give an event or
//! `None`, without allocating or panicking (ADR-0002).
//!
//! Web MIDI delivers whole messages, so there is no running status here.
//! System messages (SysEx, clock, transport) are not read yet.
//!
//! One map serves every controller (#422): knobs on CC 70–77, the MIDI sound
//! controllers (cutoff on 74, resonance on 71, the envelope on 72, 73 and
//! 75) and the Akai MPK mini Plus's default; pads on channel 10, General
//! MIDI's drums; transport on CC 115–118, the MPK's.

use crate::mono::model::Model;
use crate::params::Param;

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

/// The drum channel, General MIDI's 10: its notes hit the kit.
pub const DRUMS: u8 = 9;

/// Knobs 1–8 send CC 70–77.
pub const KNOBS: usize = 8;
pub const FIRST_KNOB: u8 = 70;

/// The transport buttons, which send 127 on press (#420).
pub const REWIND: u8 = 115;
pub const FORWARD: u8 = 116;
pub const STOP: u8 = 117;
pub const PLAY: u8 = 118;

/// What knobs 1–8 set on a synth of `model`; `None` for a Modular synth
/// (its knobs are its code's) and the pad sampler (nothing to turn yet).
/// The voices with a filter follow the sound controllers: 70 drive,
/// 71 resonance, 72 release, 73 attack, 74 cutoff, 75 decay, then
/// envelope amount and sustain.
pub fn knob_params(model: Model) -> Option<[Param; KNOBS]> {
    use Param::*;
    if model.uses_graph() || model.uses_pads() {
        None
    } else if model.uses_drums() {
        Some([
            BdTune, BdDecay, SnTune, SnDecay, ChDecay, OhDecay, CpLevel, DrumAccent,
        ])
    } else if model.uses_fm() {
        Some([
            Feedback,
            Op2Level,
            Op4Level,
            Op6Level,
            Op1Level,
            Op3Level,
            Op5Level,
            LfoPitchDepth,
        ])
    } else {
        Some([
            Drive,
            Resonance,
            AdsrRelease,
            AdsrAttack,
            Cutoff,
            AdsrDecay,
            EnvCutoff,
            AdsrSustain,
        ])
    }
}

/// The range a knob turns through: `lo` to `hi`, exponentially when `exp`
/// (a frequency, a time), so each step is the same ratio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub lo: f32,
    pub hi: f32,
    pub exp: bool,
}

impl Span {
    /// A parameter's whole range; exponential when it spans 100:1 or more
    /// above zero.
    pub fn of(param: Param) -> Span {
        let (lo, hi) = param.range();
        Span::new(lo, hi, lo > 0.0 && hi >= 100.0 * lo)
    }

    /// `exp` only holds above zero.
    pub fn new(lo: f32, hi: f32, exp: bool) -> Span {
        Span {
            lo,
            hi,
            exp: exp && lo > 0.0 && hi > lo,
        }
    }

    /// The value at `pos`, 0..=1 along the knob.
    pub fn value(self, pos: f32) -> f32 {
        let pos = pos.clamp(0.0, 1.0);
        if self.exp {
            self.lo * (self.hi / self.lo).powf(pos)
        } else {
            self.lo + (self.hi - self.lo) * pos
        }
    }

    /// Where `value` sits along the knob, 0..=1.
    pub fn pos(self, value: f32) -> f32 {
        let pos = if self.exp {
            (value.max(self.lo) / self.lo).ln() / (self.hi / self.lo).ln()
        } else if self.hi > self.lo {
            (value - self.lo) / (self.hi - self.lo)
        } else {
            0.0
        };
        if pos.is_nan() {
            0.0
        } else {
            pos.clamp(0.0, 1.0)
        }
    }
}

/// Soft takeover: whether a knob turned from `last` to `now` takes the
/// parameter sitting at `at` (all 0..=1). It does once it is within a step
/// of it, or moved across it; a knob that has not moved before takes it
/// only within a step, so nothing jumps.
pub fn takes_over(last: Option<f32>, now: f32, at: f32) -> bool {
    const STEP: f32 = 1.0 / 127.0;
    if (now - at).abs() <= STEP {
        return true;
    }
    last.is_some_and(|l| (l - at).abs() <= 0.5 * STEP || (l < at) != (now < at))
}

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
/// key or pad started on, so its release reaches it after the target
/// changes; and where each knob was last, for soft takeover.
#[derive(Clone, Debug)]
pub struct MidiIn {
    pub target: usize,
    /// Keys, then pads (the drum channel).
    held: [Option<u8>; 256],
    pub knobs: [Option<f32>; KNOBS],
}

impl Default for MidiIn {
    fn default() -> Self {
        Self {
            target: 0,
            held: [None; 256],
            knobs: [None; KNOBS],
        }
    }
}

impl MidiIn {
    fn slot(drum: bool, note: u8) -> usize {
        usize::from(drum) * 128 + usize::from(note & 0x7F)
    }

    /// Remember that `note`, a pad when `drum`, started on `synth`.
    pub fn press(&mut self, drum: bool, note: u8, synth: usize) {
        if let Some(h) = self.held.get_mut(Self::slot(drum, note)) {
            *h = u8::try_from(synth).ok();
        }
    }

    /// The synth `note` started on, forgotten; the target if it is not held.
    pub fn release(&mut self, drum: bool, note: u8) -> usize {
        self.held
            .get_mut(Self::slot(drum, note))
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
        m.press(false, 60, 2);
        m.target = 5;
        assert_eq!(m.release(false, 60), 2, "where it started");
        assert_eq!(m.release(false, 60), 5, "then the target");
    }

    /// The MPK's pads (36–51) and its lowest keys share note numbers.
    #[test]
    fn a_pad_and_a_key_on_the_same_note_are_held_apart() {
        let mut m = MidiIn::default();
        m.press(false, 36, 1);
        m.press(true, 36, 9);
        assert_eq!(m.release(true, 36), 9);
        assert_eq!(m.release(false, 36), 1);
    }

    #[test]
    fn a_span_turns_frequencies_and_times_exponentially() {
        let cutoff = Span::of(Param::Cutoff);
        assert!(cutoff.exp);
        assert!((cutoff.value(0.0) - 20.0).abs() < 1.0e-3);
        assert!(
            (cutoff.value(0.5) - 632.46).abs() < 0.1,
            "the geometric middle"
        );
        assert!((cutoff.value(1.0) - 20_000.0).abs() < 0.1);
        assert!(Span::of(Param::AdsrAttack).exp);
        let res = Span::of(Param::Resonance);
        assert!(!res.exp);
        assert_eq!(res.value(0.25), 0.25);
        assert!(
            !Span::of(Param::EnvCutoff).exp,
            "a range through zero is linear"
        );
        for span in [
            cutoff,
            res,
            Span::of(Param::EnvCutoff),
            Span::new(0.0, 0.0, true),
        ] {
            for k in 0..=127u8 {
                let pos = f32::from(k) / 127.0;
                let back = span.pos(span.value(pos));
                assert!(back.is_finite() && (0.0..=1.0).contains(&back));
                if span.hi > span.lo {
                    assert!((back - pos).abs() < 1.0e-3, "{span:?} {k}");
                }
            }
        }
    }

    #[test]
    fn a_knob_takes_over_only_when_it_reaches_the_value() {
        let step = 1.0 / 127.0;
        assert!(!takes_over(None, 0.2, 0.6), "far off, never moved: no jump");
        assert!(takes_over(None, 0.6 + 0.5 * step, 0.6), "within a step");
        assert!(!takes_over(Some(0.2), 0.3, 0.6), "on its way");
        assert!(takes_over(Some(0.55), 0.65, 0.6), "across it");
        assert!(takes_over(Some(0.7), 0.5, 0.6), "across it from above");
        assert!(takes_over(Some(0.6), 0.9, 0.6), "held: it follows");
    }

    /// Every synth but Modular and the pad sampler has eight knobs, each a
    /// parameter that turns.
    #[test]
    fn every_kind_of_synth_has_eight_knobs() {
        for &(model, _) in Model::ALL.iter() {
            let Some(params) = knob_params(model) else {
                assert!(model.uses_graph() || model.uses_pads(), "{model:?}");
                continue;
            };
            for p in params {
                let (lo, hi) = p.range();
                assert!(hi > lo, "{model:?} {p:?}");
            }
        }
    }
}
