//! The Mono VCO (spec 004 Req 1): saw, pulse, triangle and sine, with hard
//! sync, band-limited by a BLEP table (ADR-0007).
//!
//! Each oscillator writes its naive waveform into a short ring, `LATENCY`
//! samples ahead of the read position, and adds a band-limited correction
//! around every step: a saw wrap, a pulse edge, a sync reset. `render` does
//! only adds, multiplies and table reads (ADR-0002).

use crate::voice::lookup;

/// Zero crossings of the windowed sinc on each side of a step.
const ZEROS: usize = 8;
/// Table entries per sample, linearly interpolated.
const PHASES: usize = 64;
/// Cutoff as a fraction of the sample rate: 18 kHz at 48 kHz.
const CUTOFF: f64 = 0.375;
/// Kaiser window shape: about 80 dB of alias rejection with 16 taps.
const BETA: f64 = 6.0;
/// Samples an oscillator's output lags its phase.
pub const LATENCY: usize = ZEROS;
/// Ring length: a power of two above the `2 * ZEROS + 1` samples in flight.
const RING: usize = 32;
const MASK: usize = RING - 1;
/// Highest phase increment, so a step holds at most one wrap.
const MAX_INC: f32 = 0.45;

/// The band-limited step: a windowed sinc integrated from 0 to 1 over
/// `2 * ZEROS` samples. Built once in `Engine::new`.
pub struct Blep {
    table: Vec<f32>,
}

impl Blep {
    pub fn new() -> Blep {
        let n = 2 * ZEROS * PHASES;
        let i0_beta = bessel_i0(BETA);
        let kernel: Vec<f64> = (0..=n)
            .map(|i| {
                let t = i as f64 / PHASES as f64 - ZEROS as f64;
                let x = 2.0 * CUTOFF * t;
                let sinc = if x == 0.0 {
                    1.0
                } else {
                    (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                };
                let r = t / ZEROS as f64;
                let window = bessel_i0(BETA * (1.0 - r * r).max(0.0).sqrt()) / i0_beta;
                sinc * window
            })
            .collect();
        // Trapezoid integral, normalised so the step ends at exactly 1.
        let mut sum = 0.0;
        let mut table = Vec::with_capacity(n + 1);
        let mut prev = 0.0;
        for k in &kernel {
            sum += 0.5 * (prev + k);
            prev = *k;
            table.push(sum);
        }
        let total = table.last().copied().unwrap_or(1.0);
        Blep {
            table: table.iter().map(|v| (v / total) as f32).collect(),
        }
    }

    /// The step at `t` samples from its edge: 0 well before, 1 well after.
    fn at(&self, t: f32) -> f32 {
        let pos = ((t + ZEROS as f32) * PHASES as f32).max(0.0);
        let i = pos as usize;
        let frac = pos - i as f32;
        let a = self.table.get(i).copied().unwrap_or(1.0);
        let b = self.table.get(i + 1).copied().unwrap_or(1.0);
        a + (b - a) * frac
    }
}

impl Default for Blep {
    fn default() -> Blep {
        Blep::new()
    }
}

/// Modified Bessel function of the first kind, order 0, by its series.
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..40 {
        let q = x / (2.0 * k as f64);
        term *= q * q;
        sum += term;
    }
    sum
}

/// A VCO waveform; the ids are mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Waveform {
    #[default]
    Saw = 0,
    Pulse = 1,
    Triangle = 2,
    Sine = 3,
}

impl Waveform {
    /// Every waveform with the name the TypeScript mirror uses.
    pub const ALL: [(Waveform, &'static str); 4] = [
        (Waveform::Saw, "Saw"),
        (Waveform::Pulse, "Pulse"),
        (Waveform::Triangle, "Triangle"),
        (Waveform::Sine, "Sine"),
    ];

    /// The waveform for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Waveform> {
        Self::ALL
            .iter()
            .find(|(w, _)| *w as u32 == id)
            .map(|(w, _)| *w)
    }
}

#[derive(Clone, Copy, Default)]
pub struct Osc {
    pub wave: Waveform,
    /// Cycles per sample.
    inc: f32,
    phase: f32,
    /// Pulse width for the current cycle, latched at the cycle start so an
    /// edge is never added or dropped mid-cycle.
    pw: f32,
    ring: [f32; RING],
    pos: usize,
}

impl Osc {
    /// Set the frequency in cycles per sample. Cheap: no transcendentals.
    pub fn set_increment(&mut self, inc: f32) {
        self.inc = if inc.is_finite() {
            inc.clamp(0.0, MAX_INC)
        } else {
            0.0
        };
    }

    /// Advance one sample and return the output, `LATENCY` samples late.
    ///
    /// `pw` is the pulse width (0.05..=0.95), taken at the next cycle start.
    /// `sync` is the master's wrap this sample, as the time from the wrap to
    /// the next sample in samples; the phase resets to 0 there. The result
    /// also carries this oscillator's own wrap, for its slaves.
    pub fn step(
        &mut self,
        blep: &Blep,
        sine: &[f32],
        pw: f32,
        sync: Option<f32>,
    ) -> (f32, Option<f32>) {
        if self.pw == 0.0 {
            self.pw = pw;
        }
        let naive = self.naive(self.phase, sine);
        self.add(LATENCY, naive);

        let start = self.phase;
        let end = start + self.inc;
        let wrap = if end >= 1.0 && self.inc > 0.0 {
            Some((end - 1.0) / self.inc)
        } else {
            None
        };
        match sync {
            Some(after) if self.inc > 0.0 => {
                // Run to the reset, jump to phase 0, run on from there.
                let reset = start + self.inc * (1.0 - after);
                self.edges(blep, start, reset, after, pw);
                let before = self.naive(wrap_phase(reset), sine);
                self.pw = pw;
                let jump = self.naive(0.0, sine) - before;
                self.correct(blep, jump, after);
                let to = self.inc * after;
                self.edges(blep, 0.0, to, 0.0, pw);
                self.phase = to;
            }
            _ => {
                self.edges(blep, start, end, 0.0, pw);
                self.phase = wrap_phase(end);
            }
        }

        let slot = self.pos & MASK;
        let out = self.ring.get(slot).copied().unwrap_or(0.0);
        if let Some(s) = self.ring.get_mut(slot) {
            *s = 0.0;
        }
        self.pos = (self.pos + 1) & MASK;
        (out, wrap)
    }

    /// The waveform without band-limiting, at `phase` in `0..1`.
    fn naive(&self, phase: f32, sine: &[f32]) -> f32 {
        match self.wave {
            Waveform::Saw => 2.0 * phase - 1.0,
            Waveform::Pulse => {
                if phase < self.pw {
                    1.0
                } else {
                    -1.0
                }
            }
            // Starts at 0 rising, like the sine.
            Waveform::Triangle => 1.0 - 4.0 * (wrap_phase(phase + 0.25) - 0.5).abs(),
            Waveform::Sine => lookup(sine, phase),
        }
    }

    /// Correct the steps the phase crosses in `(from, to]`; `tail` is the
    /// time from `to` to the next sample. A wrap latches the next pulse width.
    fn edges(&mut self, blep: &Blep, from: f32, to: f32, tail: f32, pw: f32) {
        let after = |q: f32, inc: f32| tail + (to - q) / inc;
        match self.wave {
            Waveform::Saw => {
                if from < 1.0 && to >= 1.0 {
                    self.correct(blep, -2.0, after(1.0, self.inc));
                }
            }
            Waveform::Pulse => {
                if from < self.pw && to >= self.pw {
                    self.correct(blep, -2.0, after(self.pw, self.inc));
                }
                if from < 1.0 && to >= 1.0 {
                    self.correct(blep, 2.0, after(1.0, self.inc));
                    self.pw = pw;
                    let fall = 1.0 + self.pw;
                    if to >= fall {
                        self.correct(blep, -2.0, after(fall, self.inc));
                    }
                }
            }
            Waveform::Triangle | Waveform::Sine => {}
        }
    }

    /// Spread a step of `height`, `after` samples before the next sample,
    /// over the samples around it. The next sample already shows the new
    /// value, the current one the old.
    fn correct(&mut self, blep: &Blep, height: f32, after: f32) {
        let after = after.clamp(0.0, 1.0);
        for j in 0..2 * ZEROS {
            let k = j as f32 - ZEROS as f32;
            let step = if j >= ZEROS { 1.0 } else { 0.0 };
            self.add(j + 1, height * (blep.at(k + after) - step));
        }
    }

    /// Add `v` to the sample `ahead` samples after the current one.
    fn add(&mut self, ahead: usize, v: f32) {
        if let Some(s) = self.ring.get_mut((self.pos + ahead) & MASK) {
            *s += v;
        }
    }
}

/// Wrap a phase that is at most one cycle past 1 back into `0..1`.
fn wrap_phase(p: f32) -> f32 {
    if p >= 1.0 { p - 1.0 } else { p }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::{TABLE, midi_to_hz};

    const SR: f32 = 48_000.0;
    /// Gibbs: a band-limited step overshoots by about 9% of its height, and
    /// a ±1 saw or pulse steps by 2.
    const PEAK: f32 = 1.2;
    /// A synced pulse can put two steps within a sample or two, and their
    /// ringing adds: 1.48 at worst, for a 14 kHz slave.
    const SYNC_PEAK: f32 = 1.5;

    fn sine() -> Vec<f32> {
        (0..=TABLE)
            .map(|i| (i as f32 / TABLE as f32 * std::f32::consts::TAU).sin())
            .collect()
    }

    fn render(wave: Waveform, hz: f32, pw: f32, n: usize) -> Vec<f32> {
        let (blep, table) = (Blep::new(), sine());
        let mut osc = Osc {
            wave,
            ..Osc::default()
        };
        osc.set_increment(hz / SR);
        (0..n)
            .map(|_| osc.step(&blep, &table, pw, None).0)
            .collect()
    }

    /// Frequency from the first and last rising zero crossing.
    fn measured_hz(x: &[f32]) -> f64 {
        let crossings: Vec<f64> = x
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
            .map(|(i, w)| i as f64 + f64::from(w[0] / (w[0] - w[1])))
            .collect();
        let (first, last) = (crossings[0], crossings[crossings.len() - 1]);
        (crossings.len() - 1) as f64 * f64::from(SR) / (last - first)
    }

    /// Magnitude of `x` at `hz`, by a direct DFT.
    fn magnitude(x: &[f32], hz: f64) -> f64 {
        let w = std::f64::consts::TAU * hz / f64::from(SR);
        let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (i, s)| {
            let a = w * i as f64;
            (re + f64::from(*s) * a.cos(), im - f64::from(*s) * a.sin())
        });
        (re * re + im * im).sqrt()
    }

    #[test]
    fn pitch_within_a_cent() {
        for note in 24..=108 {
            let hz = midi_to_hz(note);
            for wave in [Waveform::Saw, Waveform::Sine] {
                let got = measured_hz(&render(wave, hz, 0.5, SR as usize));
                let cents = 1200.0 * (got / f64::from(hz)).log2();
                assert!(cents.abs() < 1.0, "{wave:?} note {note}: {cents} cents");
            }
        }
    }

    /// 5 kHz at 48 kHz repeats every 48 samples, so every component, a
    /// harmonic or an alias, sits on a multiple of 1 kHz.
    #[test]
    fn saw_aliasing_below_60_db() {
        let x = render(Waveform::Saw, 5_000.0, 0.5, 4_800 + 48 * 200);
        let x = &x[4_800..];
        let fundamental = magnitude(x, 5_000.0);
        for k in 1..24 {
            let hz = f64::from(k * 1_000);
            if k % 5 == 0 {
                continue;
            }
            let db = 20.0 * (magnitude(x, hz) / fundamental).log10();
            assert!(db < -60.0, "alias at {hz} Hz: {db:.1} dB");
        }
    }

    #[test]
    fn sync_is_bounded() {
        let (blep, table) = (Blep::new(), sine());
        for master_hz in [55.0, 220.0, 1_760.0] {
            for wave in [
                Waveform::Saw,
                Waveform::Pulse,
                Waveform::Triangle,
                Waveform::Sine,
            ] {
                let mut master = Osc::default();
                master.set_increment(master_hz / SR);
                let mut slave = Osc {
                    wave,
                    ..Osc::default()
                };
                let n = SR as usize;
                for i in 0..n {
                    // Sweep the slave from 1x to 8x the master.
                    let ratio = 1.0 + 7.0 * i as f32 / n as f32;
                    slave.set_increment(master_hz * ratio / SR);
                    let (_, wrap) = master.step(&blep, &table, 0.5, None);
                    let (y, _) = slave.step(&blep, &table, 0.3, wrap);
                    assert!(
                        y.is_finite() && y.abs() <= SYNC_PEAK,
                        "{wave:?} at {master_hz} Hz: {y}"
                    );
                }
            }
        }
    }

    /// Max difference between the output and itself one master period on.
    fn period_error(ratio: f32, sync: bool) -> f32 {
        let (blep, table) = (Blep::new(), sine());
        // 480 Hz at 48 kHz: a period of exactly 100 samples.
        let (hz, period) = (480.0, 100);
        let mut master = Osc::default();
        master.set_increment(hz / SR);
        let mut slave = Osc::default();
        slave.set_increment(hz * ratio / SR);
        let y: Vec<f32> = (0..4_800)
            .map(|_| {
                let (_, wrap) = master.step(&blep, &table, 0.5, None);
                slave.step(&blep, &table, 0.5, wrap.filter(|_| sync)).0
            })
            .collect();
        y[1_000..]
            .iter()
            .zip(&y[1_000 + period..])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max)
    }

    /// Hard sync locks the slave to the master's period, at any ratio.
    #[test]
    fn sync_locks_slave_to_master_period() {
        for ratio in [1.5, 2.5, 7.3] {
            let synced = period_error(ratio, true);
            let free = period_error(ratio, false);
            assert!(
                synced < 0.02,
                "ratio {ratio}: synced repeats within {synced}"
            );
            assert!(free > 0.5, "ratio {ratio}: free-running differs by {free}");
        }
    }

    /// A width sweep keeps exactly one rise and one fall per cycle: the
    /// latch never adds or drops an edge.
    #[test]
    fn pwm_sweep_keeps_one_pair_of_edges_per_cycle() {
        let (blep, table) = (Blep::new(), sine());
        let mut osc = Osc {
            wave: Waveform::Pulse,
            ..Osc::default()
        };
        osc.set_increment(220.0 / SR);
        let n = SR as usize;
        let y: Vec<f32> = (0..n)
            .map(|i| {
                osc.step(&blep, &table, 0.05 + 0.9 * i as f32 / n as f32, None)
                    .0
            })
            .collect();
        let rises = y.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        let falls = y.windows(2).filter(|w| w[0] >= 0.0 && w[1] < 0.0).count();
        assert!(
            rises.abs_diff(220) <= 1 && falls.abs_diff(220) <= 1,
            "{rises} rises, {falls} falls"
        );
    }

    /// The narrowest pulses, where two edges' ringing overlaps, stay bounded.
    #[test]
    fn narrow_pulses_are_bounded() {
        for hz in [2_000.0, 5_000.0] {
            for pw in [0.05, 0.95] {
                let x = render(Waveform::Pulse, hz, pw, 4_800);
                let peak = x.iter().fold(0.0_f32, |m, y| m.max(y.abs()));
                assert!(peak <= PEAK, "{hz} Hz at width {pw}: {peak}");
            }
        }
    }

    /// Sweeping the width moves edges but never adds a step bigger than a
    /// fixed-width pulse already has.
    #[test]
    fn pwm_sweep_has_no_clicks() {
        let max_step = |x: &[f32]| {
            x.windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0_f32, f32::max)
        };
        let fixed = max_step(&render(Waveform::Pulse, 220.0, 0.5, 4_800));
        let (blep, table) = (Blep::new(), sine());
        let mut osc = Osc {
            wave: Waveform::Pulse,
            ..Osc::default()
        };
        osc.set_increment(220.0 / SR);
        let n = SR as usize;
        let swept: Vec<f32> = (0..n)
            .map(|i| {
                let pw = 0.05 + 0.9 * i as f32 / n as f32;
                osc.step(&blep, &table, pw, None).0
            })
            .collect();
        assert!(swept.iter().all(|y| y.is_finite() && y.abs() <= PEAK));
        assert!(
            max_step(&swept) <= fixed * 1.05,
            "{} vs {fixed}",
            max_step(&swept)
        );
    }

    #[test]
    fn output_is_late_by_latency() {
        let x = render(Waveform::Pulse, 440.0, 0.5, 64);
        assert!(x[..LATENCY].iter().all(|y| *y == 0.0));
        assert!(x[LATENCY..LATENCY + 4].iter().all(|y| *y > 0.5));
    }

    #[test]
    fn bad_increments_are_silent_not_nan() {
        for inc in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            let (blep, table) = (Blep::new(), sine());
            let mut osc = Osc::default();
            osc.set_increment(inc);
            assert!((0..256).all(|_| osc.step(&blep, &table, 0.5, None).0.is_finite()));
        }
    }

    #[test]
    fn typescript_mirror_matches() {
        let ts = include_str!("../../../../web/src/audio/params.ts");
        for (w, name) in Waveform::ALL {
            let line = format!("{name}: {},", w as u32);
            assert!(ts.contains(&line), "web/src/audio/params.ts lacks `{line}`");
        }
        assert_eq!(Waveform::from_id(9), None);
    }
}
