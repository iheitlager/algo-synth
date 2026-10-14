//! Chip instruments (#112, ADR-0034): the SID and the NES's 2A03, each a
//! model of its own with the parts it is made of in `chip::sid` and
//! `chip::nes`, shared with the Modular's unit generators so both play the
//! same chip.

pub mod nes;
pub mod nes_voice;
pub mod sid;
pub mod sid_voice;

use crate::mono::osc::{Blep, LATENCY};

/// Ring length: a power of two above the `2 * LATENCY + 1` samples in flight.
const RING: usize = 32;
const MASK: usize = RING - 1;

/// A band-limited step ring (ADR-0007), for a waveform whose steps come
/// from the chip's registers: the naive value goes in `LATENCY` samples
/// ahead, each step adds the BLEP residual around it, and the output comes
/// out `LATENCY` samples late.
#[derive(Clone, Copy, Debug)]
pub struct Ring {
    buf: [f32; RING],
    pos: usize,
}

impl Default for Ring {
    fn default() -> Ring {
        Ring {
            buf: [0.0; RING],
            pos: 0,
        }
    }
}

impl Ring {
    /// The naive value of the sample about to be stepped.
    pub fn add_now(&mut self, v: f32) {
        self.add(LATENCY, v);
    }

    /// Spread a step of `height`, `after` samples before the next sample,
    /// over the samples around it.
    pub fn correct(&mut self, blep: &Blep, height: f32, after: f32) {
        if height == 0.0 {
            return;
        }
        let after = after.clamp(0.0, 1.0);
        for j in 0..2 * LATENCY {
            let k = j as f32 - LATENCY as f32;
            let step = if j >= LATENCY { 1.0 } else { 0.0 };
            self.add(j + 1, height * (blep.at(k + after) - step));
        }
    }

    /// The output, `LATENCY` samples late; moves on one sample.
    pub fn take(&mut self) -> f32 {
        let slot = self.pos & MASK;
        let out = self.buf.get(slot).copied().unwrap_or(0.0);
        if let Some(s) = self.buf.get_mut(slot) {
            *s = 0.0;
        }
        self.pos = (self.pos + 1) & MASK;
        out
    }

    fn add(&mut self, ahead: usize, v: f32) {
        if let Some(s) = self.buf.get_mut((self.pos + ahead) & MASK) {
            *s += v;
        }
    }
}
