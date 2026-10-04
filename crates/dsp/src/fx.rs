//! Effects: the drive insert on each synth's bus and the send effects. Their
//! buffers are allocated up front and `process` never allocates (ADR-0002).

pub mod chorus;
pub mod compressor;
pub mod drive;
pub mod echo;
pub mod ensemble;
pub mod eq;
pub mod flanger;
pub mod insert;
pub mod limiter;
pub mod processor;
pub mod reverb;
pub mod vocoder;

/// A ring buffer read at a distance behind the write position.
pub(crate) struct Delay {
    buf: Vec<f32>,
    /// Where the next sample goes.
    pos: usize,
}

impl Delay {
    /// Holds at least `max` samples of history (and one more to interpolate).
    pub fn new(max: usize) -> Delay {
        Delay {
            buf: vec![0.0; max + 2],
            pos: 0,
        }
    }

    pub fn clear(&mut self) {
        self.buf.fill(0.0);
    }

    pub fn write(&mut self, x: f32) {
        if let Some(slot) = self.buf.get_mut(self.pos) {
            *slot = x;
        }
        self.pos += 1;
        if self.pos >= self.buf.len() {
            self.pos = 0;
        }
    }

    /// The sample written `n` writes ago (`n` ≥ 1).
    pub fn at(&self, n: usize) -> f32 {
        let len = self.buf.len();
        let i = (self.pos + len - n % len) % len;
        self.buf.get(i).copied().unwrap_or(0.0)
    }

    /// Linear interpolation between the samples `d` and `d + 1` back.
    pub fn read(&self, d: f32) -> f32 {
        let i = d as usize;
        let frac = d - i as f32;
        self.at(i) * (1.0 - frac) + self.at(i + 1) * frac
    }
}

/// A sine-like wave for a modulator, from a phase in cycles (any real number),
/// without a transcendental function: the parabolic approximation
/// `4x(1 − |x|)` of a sine over a saw `x` in −1..=1. Peaks at ±1, within about
/// 6% of a sine, which a delay modulator does not need better than.
pub(crate) fn lfo(phase: f32) -> f32 {
    let x = (phase - phase.floor()) * 2.0 - 1.0;
    4.0 * x * (1.0 - x.abs())
}

/// One-pole low-pass coefficient for a corner at `hz`.
pub(crate) fn lowpass(hz: f32, sample_rate: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * hz / sample_rate).exp()
}
