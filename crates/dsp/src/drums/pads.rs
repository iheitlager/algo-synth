//! The pad voices: one struct for every pad, the pad choosing what its
//! oscillators, envelopes and filters do.
//!
//! - **Kick**: a sine whose pitch falls from a few times its tune to the tune,
//!   the ringing bridged-T of the 808, with a short noise click.
//! - **Snare**: two tuned sines and high-passed noise, each with its decay;
//!   tone sets the snap.
//! - **Clap**: band-passed noise in three quick bursts, then a tail.
//! - **Hats**: six square oscillators at the 808's frequencies through a
//!   band-pass and a high-pass; closed is short, open is long and choked.
//! - **Toms**: a sine with a small pitch drop.
//! - **Cowbell**: two squares at 540 and 800 Hz through a band-pass, a fast
//!   and a slow decay.

use super::{Pad, PadParams};
use crate::mono::noise::Noise;
use crate::voice::{lookup, wrap};

/// The 808's six hi-hat oscillators, in Hz.
const HAT_HZ: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
/// Below this an envelope counts as silent: −80 dB.
const SILENT: f32 = 1.0e-4;

/// A phase in 0..1 and its increment per sample.
#[derive(Clone, Copy, Debug, Default)]
struct Osc {
    phase: f32,
    inc: f32,
}

impl Osc {
    /// Advance by `inc` times `factor` and return the phase.
    fn tick(&mut self, factor: f32) -> f32 {
        self.phase = wrap(self.phase + self.inc * factor);
        self.phase
    }

    fn square(&mut self) -> f32 {
        if self.tick(1.0) < 0.5 { 1.0 } else { -1.0 }
    }
}

/// An exponential decay: −60 dB in the set time.
#[derive(Clone, Copy, Debug, Default)]
struct Decay {
    level: f32,
    k: f32,
}

impl Decay {
    /// Per trigger, so `exp` is fine (ADR-0002).
    fn start(&mut self, peak: f32, seconds: f32, sr: f32) {
        self.level = peak;
        self.k = (-6.907_755 / (seconds * sr).max(1.0)).exp();
    }

    fn next(&mut self) -> f32 {
        let l = self.level;
        self.level *= self.k;
        l
    }

    fn silent(&self) -> bool {
        self.level < SILENT
    }
}

/// A zero-delay-feedback state-variable filter with its coefficients set per
/// trigger: a unity-gain band-pass and a high-pass.
#[derive(Clone, Copy, Debug, Default)]
struct Svf {
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1: f32,
    ic2: f32,
}

impl Svf {
    fn set(&mut self, hz: f32, q: f32, sr: f32) {
        let g = (std::f32::consts::PI * hz.min(0.45 * sr) / sr).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// The band-pass (unity gain at the centre) and the high-pass.
    fn process(&mut self, x: f32) -> (f32, f32) {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (self.k * v1, x - self.k * v1 - v2)
    }
}

#[derive(Clone)]
pub(super) struct PadVoice {
    pad: Pad,
    active: bool,
    /// Samples since the trigger.
    n: u32,
    amp: Decay,
    amp2: Decay,
    pitch: Decay,
    /// The hats use all six oscillators, the other pads the first one or two.
    osc: [Osc; 6],
    /// How far above its base the pitch starts, as a factor minus one.
    sweep: f32,
    bp: Svf,
    hp: Svf,
    noise: Noise,
    /// The clap's burst spacing in samples, and its burst and tail times.
    burst: u32,
    burst_s: f32,
    tail_s: f32,
    gain: f32,
    sr: f32,
}

impl PadVoice {
    pub(super) fn new(pad: Pad, seed: u32) -> PadVoice {
        PadVoice {
            pad,
            active: false,
            n: 0,
            amp: Decay::default(),
            amp2: Decay::default(),
            pitch: Decay::default(),
            osc: [Osc::default(); 6],
            sweep: 0.0,
            bp: Svf::default(),
            hp: Svf::default(),
            noise: Noise::new(seed),
            burst: 1,
            burst_s: 0.0,
            tail_s: 0.0,
            gain: 0.0,
            sr: 48_000.0,
        }
    }

    pub(super) fn active(&self) -> bool {
        self.active
    }

    pub(super) fn choke(&mut self) {
        self.active = false;
    }

    /// Start a hit: everything the samples need is computed here.
    pub(super) fn trigger(&mut self, p: &PadParams, gain: f32, sr: f32) {
        let ratio = (p.tune / 12.0).exp2();
        let d = p.decay;
        let inc = |hz: f32| hz * ratio / sr;
        self.active = gain > 0.0;
        self.n = 0;
        self.gain = gain;
        self.sr = sr;
        match self.pad {
            Pad::Bd => {
                let [o, ..] = &mut self.osc;
                *o = Osc {
                    phase: 0.0,
                    inc: inc(50.0),
                };
                self.sweep = 1.0 + 4.0 * p.tone;
                self.pitch.start(1.0, 0.06, sr);
                self.amp.start(0.44, 0.5 * d, sr);
                self.amp2.start(0.13 * p.tone, 0.004, sr);
            }
            Pad::Sn => {
                let [a, b, ..] = &mut self.osc;
                *a = Osc {
                    phase: 0.0,
                    inc: inc(185.0),
                };
                *b = Osc {
                    phase: 0.0,
                    inc: inc(330.0),
                };
                self.amp.start(0.17 * (1.0 - 0.5 * p.tone), 0.15 * d, sr);
                self.amp2.start(0.11 + 0.17 * p.tone, 0.25 * d, sr);
                self.hp.set(1800.0 * ratio, 0.7, sr);
            }
            Pad::Cp => {
                self.bp.set((800.0 + 800.0 * p.tone) * ratio, 1.5, sr);
                self.burst = (0.01 * sr) as u32;
                self.burst_s = 0.012;
                self.tail_s = 0.25 * d;
                self.amp.start(0.5, self.burst_s, sr);
            }
            Pad::Ch | Pad::Oh => {
                for (o, hz) in self.osc.iter_mut().zip(HAT_HZ) {
                    o.inc = inc(hz);
                }
                let decay = if self.pad == Pad::Ch { 0.06 } else { 0.5 };
                self.bp.set(7000.0 + 3000.0 * p.tone, 1.0, sr);
                self.hp.set(6000.0, 0.7, sr);
                self.amp.start(0.5, decay * d, sr);
            }
            Pad::Lt | Pad::Ht => {
                let hz = if self.pad == Pad::Lt { 95.0 } else { 160.0 };
                let [o, ..] = &mut self.osc;
                *o = Osc {
                    phase: 0.0,
                    inc: inc(hz),
                };
                self.sweep = 0.1 + 0.5 * p.tone;
                self.pitch.start(1.0, 0.1, sr);
                self.amp.start(0.5, 0.4 * d, sr);
            }
            Pad::Cb => {
                let [a, b, ..] = &mut self.osc;
                a.inc = inc(540.0);
                b.inc = inc(800.0);
                self.bp.set((1200.0 + 2000.0 * p.tone) * ratio, 1.5, sr);
                self.amp.start(0.23, 0.03, sr);
                self.amp2.start(0.13, 0.35 * d, sr);
            }
        }
    }

    /// Add this pad's next `out.len()` samples into `out`.
    pub(super) fn render(&mut self, sine: &[f32], out: &mut [f32]) {
        for o in out.iter_mut() {
            if !self.active {
                return;
            }
            let s = match self.pad {
                Pad::Bd => {
                    let [o, ..] = &mut self.osc;
                    let ph = o.tick(1.0 + self.sweep * self.pitch.next());
                    lookup(sine, ph) * self.amp.next() + self.noise.white() * self.amp2.next()
                }
                Pad::Sn => {
                    let [a, b, ..] = &mut self.osc;
                    let body = lookup(sine, a.tick(1.0)) + 0.6 * lookup(sine, b.tick(1.0));
                    let (_, snap) = self.hp.process(self.noise.white());
                    0.6 * body * self.amp.next() + snap * self.amp2.next()
                }
                Pad::Cp => {
                    if self.n > 0 && self.n % self.burst == 0 && self.n <= 3 * self.burst {
                        let secs = if self.n == 3 * self.burst {
                            self.tail_s
                        } else {
                            self.burst_s
                        };
                        self.amp.start(0.5, secs, self.sr);
                    }
                    let (b, _) = self.bp.process(self.noise.white());
                    1.55 * b * self.amp.next()
                }
                Pad::Ch | Pad::Oh => {
                    let sq: f32 = self.osc.iter_mut().map(Osc::square).sum();
                    let (b, _) = self.bp.process(sq / 6.0);
                    let (_, h) = self.hp.process(b);
                    3.0 * h * self.amp.next()
                }
                Pad::Lt | Pad::Ht => {
                    let [o, ..] = &mut self.osc;
                    let ph = o.tick(1.0 + self.sweep * self.pitch.next());
                    lookup(sine, ph) * self.amp.next()
                }
                Pad::Cb => {
                    let [a, b, ..] = &mut self.osc;
                    let sq = 0.5 * (a.square() + b.square());
                    let (b, _) = self.bp.process(sq);
                    2.0 * b * (self.amp.next() + self.amp2.next())
                }
            };
            *o += s * self.gain;
            self.n = self.n.saturating_add(1);
            let clap_bursting = self.pad == Pad::Cp && self.n <= 3 * self.burst;
            if self.amp.silent() && self.amp2.silent() && !clap_bursting {
                self.active = false;
            }
        }
    }
}
