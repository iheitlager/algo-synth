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
//! - **Cymbal**: the same six oscillators through a low and a high band with
//!   their own decays; tone moves between them.
//! - **Toms and congas**: a sine with a small pitch drop; the congas higher and
//!   shorter.
//! - **Cowbell**: two band-limited squares at 540 and 800 Hz under a fast and a
//!   slow decay, through a band-pass near 850 Hz with a Q of about 4, as the
//!   808's circuit has it (the squares come from the Mono voice's BLEP
//!   oscillator, ADR-0007, so nothing aliases).
//! - **Rimshot and claves**: the 808's bridged-T resonators, here a resonant
//!   band-pass struck by an impulse: two (about 455 Hz and 1.7 kHz) through a
//!   high-pass for the rimshot, one at about 2.5 kHz for the claves. A
//!   resonator's Q is worked out from its decay, and the impulse scaled so it
//!   rings at about unit level whatever its tune and decay. (The hardware's
//!   1 ms trigger pulse has nulls at every kHz, which would silence a clave
//!   tuned onto one.)
//! - **Maracas**: high-passed noise that swells for a few milliseconds and falls.

use super::{Machine, Pad, PadParams};
use crate::mono::noise::Noise;
use crate::mono::osc::{Blep, Osc as BlOsc, Waveform};
use crate::voice::{lookup, wrap};

/// The 808's six hi-hat and cymbal oscillators, in Hz.
const HAT_HZ: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
/// The 909's metal: six inharmonic squares under its hats and cymbals.
const METAL_909: [f32; 6] = [263.0, 400.0, 421.0, 474.0, 587.0, 845.0];
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

    /// A resonator at `hz` that rings −60 dB down in `seconds`, cleared;
    /// returns the impulse that makes it ring at about unit level. (Its
    /// impulse response peaks at ω/Q per unit area, and ω/Q is 2·6.91/t60.)
    fn ring(&mut self, hz: f32, seconds: f32, sr: f32) -> f32 {
        let q = (std::f32::consts::PI * hz * seconds / 6.907_755).max(0.5);
        self.set(hz, q, sr);
        self.ic1 = 0.0;
        self.ic2 = 0.0;
        seconds * sr / 13.815_51
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

/// One pad sounding: a pool voice of a TR-808 slot, or one of a `Kit`'s.
#[derive(Clone, Copy)]
pub struct PadVoice {
    pad: Pad,
    /// The machine it plays (#148) and the sound it makes for `pad` there.
    machine: Machine,
    sound: Pad,
    active: bool,
    /// Samples since the trigger.
    n: u32,
    amp: Decay,
    amp2: Decay,
    pitch: Decay,
    /// The hats and cymbal use all six oscillators, the other pads the first one or two.
    osc: [Osc; 6],
    /// The cowbell's band-limited squares.
    bell: [BlOsc; 2],
    /// How far above its base the pitch starts, as a factor minus one.
    sweep: f32,
    bp: Svf,
    hp: Svf,
    /// A second resonator: the rimshot's upper one, the cymbal's high band.
    res: Svf,
    /// The impulses that strike the two resonators (`bp`, `res`).
    ping: [f32; 2],
    /// The cymbal's high band and its share.
    hi: Decay,
    mix: f32,
    noise: Noise,
    /// The clap's burst spacing in samples, and its burst and tail times.
    burst: u32,
    burst_s: f32,
    tail_s: f32,
    /// How many bursts the clap has before its tail: 3 on the 808, 4 on the 909.
    bursts: u32,
    /// The kicks' drive into their soft clip (the 808's off at 0); the level
    /// of the 909's metal voices.
    drive: f32,
    /// The kicks' gain after their clip, back to their undriven peak.
    makeup: f32,
    gain: f32,
    sr: f32,
}

impl PadVoice {
    pub(crate) fn new(pad: Pad, seed: u32) -> PadVoice {
        PadVoice {
            pad,
            machine: Machine::Tr808,
            sound: pad,
            active: false,
            n: 0,
            amp: Decay::default(),
            amp2: Decay::default(),
            pitch: Decay::default(),
            osc: [Osc::default(); 6],
            bell: [BlOsc::default(); 2],
            sweep: 0.0,
            bp: Svf::default(),
            hp: Svf::default(),
            res: Svf::default(),
            ping: [0.0; 2],
            hi: Decay::default(),
            mix: 0.0,
            noise: Noise::new(seed),
            burst: 1,
            burst_s: 0.0,
            tail_s: 0.0,
            bursts: 3,
            drive: 0.0,
            makeup: 0.0,
            gain: 0.0,
            sr: 48_000.0,
        }
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }

    pub(crate) fn pad(&self) -> Pad {
        self.pad
    }

    /// Play `pad` from the next trigger on (a pool voice plays any pad).
    pub(crate) fn set_pad(&mut self, pad: Pad) {
        self.pad = pad;
    }

    /// Play `machine`'s sounds from the next trigger on.
    pub(crate) fn set_machine(&mut self, machine: Machine) {
        self.machine = machine;
    }

    pub(crate) fn choke(&mut self) {
        self.active = false;
    }

    /// Start a hit: everything the samples need is computed here.
    pub(crate) fn trigger(&mut self, p: &PadParams, gain: f32, sr: f32) {
        let ratio = (p.tune / 12.0).exp2();
        let d = p.decay;
        let inc = |hz: f32| hz * ratio / sr;
        self.active = gain > 0.0;
        self.n = 0;
        self.gain = gain;
        self.sr = sr;
        self.sound = self.machine.voice(self.pad);
        if self.machine == Machine::Tr909 {
            self.trigger_909(p, ratio, sr);
            return;
        }
        self.bursts = 3;
        match self.sound {
            Pad::Bd => {
                let [o, ..] = &mut self.osc;
                *o = Osc {
                    phase: 0.0,
                    inc: inc(50.0),
                };
                self.sweep = 1.0 + 4.0 * p.tone;
                self.pitch.start(1.0, 0.06, sr);
                self.amp.start(0.42, 0.5 * d, sr);
                self.amp2.start(0.12 * p.tone, 0.004, sr);
                // Drive (#264): the body, at 1 on its peak, into `k·x/(1 + |k·x|)`
                // and back to its peak of 0.42, so the tail holds up and the hit
                // is louder for the same headroom.
                self.drive = 10.0 * p.drive;
                self.makeup = 0.42 * (1.0 + self.drive) / self.drive.max(1.0e-6);
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
                let decay = if self.sound == Pad::Ch { 0.06 } else { 0.5 };
                self.bp.set(7000.0 + 3000.0 * p.tone, 1.0, sr);
                self.hp.set(6000.0, 0.7, sr);
                self.amp.start(0.5, decay * d, sr);
            }
            // The 808 has no crash or ride: its cymbal plays them.
            Pad::Cy | Pad::Cr | Pad::Rd => {
                for (o, hz) in self.osc.iter_mut().zip(HAT_HZ) {
                    o.inc = inc(hz);
                }
                // A low band that fades first and a high band that rings on.
                self.bp.set(3500.0, 1.2, sr);
                self.res.set(8000.0, 1.0, sr);
                self.hp.set(2500.0, 0.7, sr);
                self.mix = p.tone;
                self.amp.start(0.5, 0.4 * d, sr);
                self.hi.start(0.5, 0.7 * d, sr);
            }
            Pad::Lt | Pad::Mt | Pad::Ht | Pad::Lc | Pad::Mc | Pad::Hc => {
                let (hz, sweep, decay) = match self.sound {
                    Pad::Lt => (95.0, 0.1 + 0.5 * p.tone, 0.4),
                    Pad::Mt => (125.0, 0.1 + 0.5 * p.tone, 0.35),
                    Pad::Ht => (160.0, 0.1 + 0.5 * p.tone, 0.3),
                    // The congas: the toms' switch on the hardware, higher and shorter.
                    Pad::Lc => (165.0, 0.05 + 0.2 * p.tone, 0.2),
                    Pad::Mc => (250.0, 0.05 + 0.2 * p.tone, 0.18),
                    _ => (370.0, 0.05 + 0.2 * p.tone, 0.15),
                };
                let [o, ..] = &mut self.osc;
                *o = Osc {
                    phase: 0.0,
                    inc: inc(hz),
                };
                self.sweep = sweep;
                self.pitch.start(1.0, 0.1, sr);
                self.amp.start(0.475, decay * d, sr);
            }
            Pad::Cb => {
                for (o, hz) in self.bell.iter_mut().zip([540.0, 800.0]) {
                    o.wave = Waveform::Pulse;
                    o.set_increment(inc(hz));
                }
                // Tone moves the band half an octave either way of 850 Hz.
                let centre = 850.0 * ratio * (p.tone - 0.5).exp2();
                self.bp.set(centre, 4.25, sr);
                self.amp.start(0.5, 0.05, sr);
                self.amp2.start(0.2, 0.4 * d, sr);
            }
            Pad::Rs => {
                self.ping = [
                    self.bp.ring(455.0 * ratio, 0.04 * d, sr),
                    self.res.ring(1667.0 * ratio, 0.03 * d, sr),
                ];
                self.hp.set(400.0 + 600.0 * p.tone, 0.7, sr);
                self.amp.start(1.0, 0.03 * d, sr);
            }
            Pad::Cl => {
                self.ping = [0.0, self.res.ring(2500.0 * ratio, 0.025 * d, sr)];
                self.amp.start(1.0, 0.03 * d, sr);
            }
            Pad::Ma => {
                self.hp.set(4000.0 + 4000.0 * p.tone, 0.7, sr);
                // The swell: `amp2` falls from 1, so `1 − amp2` rises.
                self.amp.start(0.5, 0.07 * d, sr);
                self.amp2.start(1.0, 0.006, sr);
            }
        }
    }

    /// Add this pad's next `out.len()` samples into `out`.
    pub(crate) fn render(&mut self, sine: &[f32], blep: &Blep, out: &mut [f32]) {
        for o in out.iter_mut() {
            if !self.active {
                return;
            }
            let [ping_lo, ping_hi] = if self.n == 0 { self.ping } else { [0.0; 2] };
            let s = if self.machine == Machine::Tr909 {
                self.tick_909(sine, ping_lo, ping_hi)
            } else {
                match self.sound {
                    Pad::Bd => {
                        let [o, ..] = &mut self.osc;
                        let ph = o.tick(1.0 + self.sweep * self.pitch.next());
                        let mut body = lookup(sine, ph) * self.amp.next();
                        if self.drive > 0.0 {
                            let x = self.drive * body / 0.42;
                            body = self.makeup * x / (1.0 + x.abs());
                        }
                        body + self.noise.white() * self.amp2.next()
                    }
                    Pad::Sn => {
                        let [a, b, ..] = &mut self.osc;
                        let body = lookup(sine, a.tick(1.0)) + 0.6 * lookup(sine, b.tick(1.0));
                        let (_, snap) = self.hp.process(self.noise.white());
                        0.6 * body * self.amp.next() + snap * self.amp2.next()
                    }
                    Pad::Cp => {
                        if self.n > 0
                            && self.n % self.burst == 0
                            && self.n <= self.bursts * self.burst
                        {
                            let secs = if self.n == self.bursts * self.burst {
                                self.tail_s
                            } else {
                                self.burst_s
                            };
                            self.amp.start(0.5, secs, self.sr);
                        }
                        let (b, _) = self.bp.process(self.noise.white());
                        1.5 * b * self.amp.next()
                    }
                    Pad::Ch | Pad::Oh => {
                        let sq: f32 = self.osc.iter_mut().map(Osc::square).sum();
                        let (b, _) = self.bp.process(sq / 6.0);
                        let (_, h) = self.hp.process(b);
                        3.0 * h * self.amp.next()
                    }
                    Pad::Cy | Pad::Cr | Pad::Rd => {
                        let sq: f32 = self.osc.iter_mut().map(Osc::square).sum::<f32>() / 6.0;
                        let (low, _) = self.bp.process(sq);
                        let (high, _) = self.res.process(sq);
                        let mixed = (1.0 - self.mix) * low * self.amp.next()
                            + (0.5 + self.mix) * high * self.hi.next();
                        let (_, h) = self.hp.process(mixed);
                        1.4 * h
                    }
                    Pad::Lt | Pad::Mt | Pad::Ht | Pad::Lc | Pad::Mc | Pad::Hc => {
                        let [o, ..] = &mut self.osc;
                        let ph = o.tick(1.0 + self.sweep * self.pitch.next());
                        lookup(sine, ph) * self.amp.next()
                    }
                    Pad::Cb => {
                        let [a, b] = &mut self.bell;
                        let sq = a.step(blep, sine, 0.5, None).0 + b.step(blep, sine, 0.5, None).0;
                        let (band, _) = self.bp.process(0.5 * sq);
                        1.2 * band * (self.amp.next() + self.amp2.next())
                    }
                    Pad::Rs => {
                        let (low, _) = self.bp.process(ping_lo);
                        let (high, _) = self.res.process(ping_hi);
                        let (_, h) = self.hp.process(low + high);
                        0.25 * h * self.amp.next()
                    }
                    Pad::Cl => {
                        let (ring, _) = self.res.process(ping_hi);
                        0.45 * ring * self.amp.next()
                    }
                    Pad::Ma => {
                        let (_, h) = self.hp.process(self.noise.white());
                        let swell = 1.0 - self.amp2.next();
                        0.8 * h * swell * self.amp.next()
                    }
                }
            };
            *o += s * self.gain;
            self.n = self.n.saturating_add(1);
            let clap_bursting = self.sound == Pad::Cp && self.n <= self.bursts * self.burst;
            let ringing = self.machine == Machine::Tr808
                && matches!(self.sound, Pad::Cy | Pad::Cr | Pad::Rd)
                && !self.hi.silent();
            // The maracas' swell envelope falls to 0 while the sound rises.
            let swelling = self.sound == Pad::Ma && !self.amp.silent();
            if self.amp.silent()
                && (self.amp2.silent() || self.sound == Pad::Ma)
                && !clap_bursting
                && !ringing
                && !swelling
            {
                self.active = false;
            }
        }
    }

    /// Start a hit on the 909 (#148). Its own pads only: `Machine::voice`
    /// maps the others onto these.
    fn trigger_909(&mut self, p: &PadParams, ratio: f32, sr: f32) {
        let d = p.decay;
        let inc = |hz: f32| hz * ratio / sr;
        match self.sound {
            Pad::Bd => {
                // A sweep from five times the tune in a few milliseconds, a
                // click, and drive into a soft clip: the 909's punch.
                let [o, ..] = &mut self.osc;
                *o = Osc {
                    phase: 0.0,
                    inc: inc(52.0),
                };
                self.sweep = 4.0;
                self.pitch.start(1.0, 0.03, sr);
                self.amp.start(0.5, 0.45 * d, sr);
                self.amp2.start(0.13 + 0.2 * p.tone, 0.003, sr);
                self.hp.set(3_000.0, 0.7, sr);
                // Drive (#264) on top of the tone's, back to the tone's peak.
                let sat = |k: f32| 0.5 * k / (1.0 + 0.5 * k);
                let toned = 1.0 + 4.0 * p.tone;
                self.drive = toned + 8.0 * p.drive;
                self.makeup = sat(toned) / sat(self.drive);
            }
            Pad::Sn => {
                let [a, b, ..] = &mut self.osc;
                *a = Osc {
                    phase: 0.0,
                    inc: inc(180.0),
                };
                *b = Osc {
                    phase: 0.0,
                    inc: inc(330.0),
                };
                self.sweep = 0.5;
                self.pitch.start(1.0, 0.02, sr);
                self.amp.start(0.16, 0.12 * d, sr);
                self.amp2.start(0.1 + 0.17 * p.tone, 0.2 * d, sr);
                self.hp.set(2_500.0 * ratio, 0.7, sr);
            }
            Pad::Lt | Pad::Mt | Pad::Ht => {
                let (hz, decay) = match self.sound {
                    Pad::Lt => (90.0, 0.35),
                    Pad::Mt => (125.0, 0.3),
                    _ => (170.0, 0.25),
                };
                let [o, ..] = &mut self.osc;
                *o = Osc {
                    phase: 0.0,
                    inc: inc(hz),
                };
                self.sweep = 0.6 + 0.8 * p.tone;
                self.pitch.start(1.0, 0.06, sr);
                self.amp.start(0.42, decay * d, sr);
                self.amp2.start(0.04, 0.05, sr);
            }
            Pad::Rs => {
                self.ping = [
                    self.bp.ring(800.0 * ratio, 0.025 * d, sr),
                    self.res.ring(2_400.0 * ratio, 0.018 * d, sr),
                ];
                self.hp.set(1_000.0 + 1_000.0 * p.tone, 0.7, sr);
                self.amp.start(1.0, 0.025 * d, sr);
            }
            Pad::Cp => {
                self.bp.set((1_200.0 + 1_000.0 * p.tone) * ratio, 1.2, sr);
                self.burst = (0.008 * sr) as u32;
                self.bursts = 4;
                self.burst_s = 0.01;
                self.tail_s = 0.18 * d;
                self.amp.start(0.5, self.burst_s, sr);
            }
            Pad::Ch | Pad::Oh | Pad::Cr | Pad::Rd => {
                for (o, hz) in self.osc.iter_mut().zip(METAL_909) {
                    o.inc = inc(hz);
                }
                // Centre, Q, high-pass and decay; `drive` is each one's level.
                let (centre, q, high, decay, level) = match self.sound {
                    Pad::Ch => (9_000.0, 1.2, 7_000.0, 0.05, 2.6),
                    Pad::Oh => (9_000.0, 1.2, 7_000.0, 0.4, 2.3),
                    Pad::Cr => (5_500.0, 0.8, 4_000.0, 0.7, 2.0),
                    _ => (6_500.0, 2.0, 5_000.0, 0.7, 3.6),
                };
                self.drive = level;
                self.bp.set(centre + 2_000.0 * p.tone, q, sr);
                self.hp.set(high, 0.7, sr);
                // The ride rings with little noise; hats and crash are noisier.
                self.mix = if self.sound == Pad::Rd { 0.15 } else { 0.45 };
                self.amp.start(0.5, decay * d, sr);
            }
            // `Machine::voice` never gives the 909 these.
            _ => self.active = false,
        }
    }

    /// One sample of a 909 voice.
    fn tick_909(&mut self, sine: &[f32], ping_lo: f32, ping_hi: f32) -> f32 {
        match self.sound {
            Pad::Bd => {
                let [o, ..] = &mut self.osc;
                let ph = o.tick(1.0 + self.sweep * self.pitch.next());
                let body = lookup(sine, ph) * self.amp.next();
                let driven = self.drive * body;
                let (_, click) = self.hp.process(self.noise.white());
                0.4 * self.makeup * driven / (1.0 + driven.abs()) + click * self.amp2.next()
            }
            Pad::Sn => {
                let drop = 1.0 + self.sweep * self.pitch.next();
                let [a, b, ..] = &mut self.osc;
                let body = lookup(sine, a.tick(drop)) + 0.6 * lookup(sine, b.tick(drop));
                let (_, snap) = self.hp.process(self.noise.white());
                0.6 * body * self.amp.next() + snap * self.amp2.next()
            }
            Pad::Lt | Pad::Mt | Pad::Ht => {
                let [o, ..] = &mut self.osc;
                let ph = o.tick(1.0 + self.sweep * self.pitch.next());
                lookup(sine, ph) * self.amp.next() + self.noise.white() * self.amp2.next()
            }
            Pad::Rs => {
                let (low, _) = self.bp.process(ping_lo);
                let (high, _) = self.res.process(ping_hi);
                let (_, h) = self.hp.process(low + high);
                0.25 * h * self.amp.next()
            }
            Pad::Cp => {
                if self.n > 0 && self.n % self.burst == 0 && self.n <= self.bursts * self.burst {
                    let secs = if self.n == self.bursts * self.burst {
                        self.tail_s
                    } else {
                        self.burst_s
                    };
                    self.amp.start(0.5, secs, self.sr);
                }
                let (b, _) = self.bp.process(self.noise.white());
                1.28 * b * self.amp.next()
            }
            Pad::Ch | Pad::Oh | Pad::Cr | Pad::Rd => {
                let sq: f32 = self.osc.iter_mut().map(Osc::square).sum::<f32>() / 6.0;
                let metal = (1.0 - self.mix) * sq + self.mix * self.noise.white();
                let (b, _) = self.bp.process(metal);
                let (_, h) = self.hp.process(b);
                self.drive * h * self.amp.next()
            }
            _ => 0.0,
        }
    }
}
