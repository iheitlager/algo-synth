//! The drum kit: synthesized analog pads after the TR-808 (plan.md MVP 3,
//! spec 002 Req 1).
//!
//! Each pad is one voice, retriggered as the hardware's are, and the closed
//! hat chokes the open hat. `Kit` plays the pads on their own; a synth slot
//! with the TR-808 model plays them as voices of its pool (`poly`, #114).
//! Real-time rules
//! (ADR-0002): a trigger computes its coefficients; a sample costs multiplies,
//! table reads and a filter step.

mod pads;

pub use pads::PadVoice;

/// A hit at this velocity or above is accented (MIDI 115 and up).
pub const ACCENT_VELOCITY: f32 = 0.9;

/// A pad of the kit. The names are the notation's (ADR-0012); the notes are
/// General MIDI's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pad {
    Bd = 0,
    Sn = 1,
    Cp = 2,
    Ch = 3,
    Oh = 4,
    Lt = 5,
    Ht = 6,
    Cb = 7,
}

pub const PADS: usize = 8;

impl Pad {
    pub const ALL: [(Pad, &'static str); PADS] = [
        (Pad::Bd, "bd"),
        (Pad::Sn, "sn"),
        (Pad::Cp, "cp"),
        (Pad::Ch, "ch"),
        (Pad::Oh, "oh"),
        (Pad::Lt, "lt"),
        (Pad::Ht, "ht"),
        (Pad::Cb, "cb"),
    ];

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(p, _)| *p == self)
            .map_or("", |(_, n)| n)
    }

    pub fn from_name(name: &str) -> Option<Pad> {
        Self::ALL.iter().find(|(_, n)| *n == name).map(|(p, _)| *p)
    }

    /// The General MIDI drum note.
    pub fn note(self) -> u8 {
        match self {
            Pad::Bd => 36,
            Pad::Sn => 38,
            Pad::Cp => 39,
            Pad::Ch => 42,
            Pad::Oh => 46,
            Pad::Lt => 45,
            Pad::Ht => 50,
            Pad::Cb => 56,
        }
    }

    pub fn from_note(note: u8) -> Option<Pad> {
        Self::ALL
            .iter()
            .find(|(p, _)| p.note() == note)
            .map(|(p, _)| *p)
    }

    /// The pad a key plays: General MIDI's drum map from 35 to 56 (with its
    /// second kick, snare, pedal hat and the toms between), and every other key
    /// by its place in the octave from 36, so any octave of a keyboard plays
    /// the kit.
    pub fn from_gm(note: u8) -> Pad {
        let gm = match note {
            35 | 36 => Some(Pad::Bd),
            37 | 39 => Some(Pad::Cp),
            38 | 40 => Some(Pad::Sn),
            42 | 44 => Some(Pad::Ch),
            46 => Some(Pad::Oh),
            41 | 43 | 45 => Some(Pad::Lt),
            47 | 48 | 50 => Some(Pad::Ht),
            56 => Some(Pad::Cb),
            _ => None,
        };
        gm.unwrap_or(match note % 12 {
            0 => Pad::Bd,
            1 | 3 => Pad::Cp,
            2 | 4 => Pad::Sn,
            6 | 8 => Pad::Ch,
            10 => Pad::Oh,
            5 | 7 | 9 => Pad::Lt,
            _ => Pad::Ht,
        })
    }
}

/// A pad's knobs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadParams {
    /// Semitones, −12..=12.
    pub tune: f32,
    /// A factor on the pad's own decay, 0.25..=4.
    pub decay: f32,
    /// 0..=1: the pitch sweep of the kick and toms, the snare's snap, the
    /// filters of the clap, hats and cowbell.
    pub tone: f32,
    /// 0..=1.
    pub level: f32,
}

impl Default for PadParams {
    fn default() -> PadParams {
        PadParams {
            tune: 0.0,
            decay: 1.0,
            tone: 0.5,
            level: 0.8,
        }
    }
}

impl PadParams {
    /// The gain of a hit at `velocity` (0..=1), louder by `accent` when accented.
    pub fn gain(&self, velocity: f32, accented: bool, accent: f32) -> f32 {
        let v = if velocity.is_nan() {
            0.0
        } else {
            velocity.clamp(0.0, 1.0)
        };
        v * self.level * if accented { 1.0 + accent } else { 1.0 }
    }

    fn clamped(self) -> PadParams {
        let c = |v: f32, lo: f32, hi: f32, nan: f32| if v.is_nan() { nan } else { v.clamp(lo, hi) };
        PadParams {
            tune: c(self.tune, -12.0, 12.0, 0.0),
            decay: c(self.decay, 0.25, 4.0, 1.0),
            tone: c(self.tone, 0.0, 1.0, 0.5),
            level: c(self.level, 0.0, 1.0, 0.0),
        }
    }
}

/// Eight pads, their knobs and the accent.
#[derive(Clone)]
pub struct Kit {
    sample_rate: f32,
    voices: [PadVoice; PADS],
    params: [PadParams; PADS],
    /// How much louder an accented hit is: 0 none, 1 twice as loud.
    accent: f32,
}

impl Kit {
    pub fn new(sample_rate: f32) -> Kit {
        Kit {
            sample_rate,
            voices: Pad::ALL
                .map(|(pad, _)| PadVoice::new(pad, (pad as u32 + 1).wrapping_mul(0x9E37_79B9))),
            params: [PadParams::default(); PADS],
            accent: 0.5,
        }
    }

    /// Set a pad's knobs, clamped; they apply from its next hit.
    pub fn set_params(&mut self, pad: Pad, p: PadParams) {
        if let Some(slot) = self.params.get_mut(pad as usize) {
            *slot = p.clamped();
        }
    }

    pub fn params(&self, pad: Pad) -> PadParams {
        self.params.get(pad as usize).copied().unwrap_or_default()
    }

    /// The accent amount, 0..=1.
    pub fn set_accent(&mut self, amount: f32) {
        if !amount.is_nan() {
            self.accent = amount.clamp(0.0, 1.0);
        }
    }

    /// Hit `pad` at `velocity` (0..=1), accented or not. A closed hat
    /// silences the open hat.
    pub fn trigger(&mut self, pad: Pad, velocity: f32, accent: bool) {
        let p = self.params(pad);
        let gain = p.gain(velocity, accent, self.accent);
        if pad == Pad::Ch {
            if let Some(oh) = self.voices.get_mut(Pad::Oh as usize) {
                oh.choke();
            }
        }
        if let Some(v) = self.voices.get_mut(pad as usize) {
            v.trigger(&p, gain, self.sample_rate);
        }
    }

    /// Any pad still sounding.
    pub fn active(&self) -> bool {
        self.voices.iter().any(PadVoice::active)
    }

    /// Add the kit's next `out.len()` samples into `out`.
    pub fn render(&mut self, sine: &[f32], out: &mut [f32]) {
        for v in self.voices.iter_mut().filter(|v| v.active()) {
            v.render(sine, out);
        }
    }
}

#[cfg(test)]
mod tests;
