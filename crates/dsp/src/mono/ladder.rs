//! The 4-pole ladder low-pass (spec 004 Req 3): the Moog sound inside the
//! Mono voice.
//!
//! Zero-delay feedback after Zavalishin ("The Art of VA Filter Design"):
//! four trapezoidal one-poles, the feedback loop solved for the input each
//! sample, then the input saturated by a rational `tanh` approximation, which
//! also bounds self-oscillation. ADR-0002: the cutoff is smoothed in pitch
//! and `g` comes from a table built in `Engine::new`, read only while the
//! smoothed cutoff moves; `render` does no transcendental math.

use crate::mono::model::LadderVoicing;
use crate::voice::midi_to_hz;

/// Table range in MIDI notes: 8 Hz to above any cutoff the clamp allows.
const LO: f32 = 0.0;
const HI: f32 = 136.0;
/// Table entries per semitone, linearly interpolated.
const STEPS: f32 = 4.0;
/// Highest cutoff as a fraction of the sample rate.
const MAX_CUTOFF: f64 = 0.45;
/// Cutoff smoothing time constant in seconds.
const SMOOTH_SECONDS: f32 = 0.002;
/// Feedback at full resonance. The ladder self-oscillates from 4 (resonance
/// 0.8); past it the saturator sets the level, which is a usable whistle
/// at 5 and a faint one just above 4.
pub const MAX_K: f32 = 5.0;
/// Initial state, standing in for an analog noise floor, so full resonance
/// rings up even with no input.
const FLOOR: f32 = 1.0e-3;

/// What every ladder shares: the `g` table and the smoothing coefficient.
pub struct LadderTables {
    g: Vec<f32>,
    smooth: f32,
}

impl LadderTables {
    pub fn new(sample_rate: f32) -> LadderTables {
        let n = ((HI - LO) * STEPS) as usize + 1;
        let sr = f64::from(sample_rate);
        let g = (0..n)
            .map(|i| {
                let note = LO + i as f32 / STEPS;
                let hz = f64::from(midi_to_hz_f(note)).min(MAX_CUTOFF * sr);
                (std::f64::consts::PI * hz / sr).tan() as f32
            })
            .collect();
        LadderTables {
            g,
            smooth: 1.0 - (-1.0 / (SMOOTH_SECONDS * sample_rate)).exp(),
        }
    }

    /// Move a smoothed cutoff (a MIDI note, `None` before the first sample)
    /// toward `target`. Returns the note to retune at, or `None` when it
    /// already sits there, so a coefficient is only recomputed while the
    /// cutoff moves (ADR-0002). Non-finite targets go to the lowest note.
    pub(crate) fn follow(&self, state: &mut Option<f32>, target: f32) -> Option<f32> {
        let target = if target.is_finite() { target } else { LO };
        match *state {
            Some(n) if n == target => None,
            Some(n) => {
                let next = n + (target - n) * self.smooth;
                let next = if (target - next).abs() < 0.01 {
                    target
                } else {
                    next
                };
                *state = Some(next);
                Some(next)
            }
            None => {
                *state = Some(target);
                Some(target)
            }
        }
    }

    /// The prewarped one-pole gain for a cutoff given as a MIDI note.
    pub(crate) fn g_at(&self, note: f32) -> f32 {
        let pos = ((note - LO) * STEPS).clamp(0.0, (HI - LO) * STEPS);
        let i = pos as usize;
        let frac = pos - i as f32;
        let last = self.g.last().copied().unwrap_or(1.0);
        let a = self.g.get(i).copied().unwrap_or(last);
        let b = self.g.get(i + 1).copied().unwrap_or(last);
        a + (b - a) * frac
    }
}

/// A fractional MIDI note to Hz; only used to build tables.
fn midi_to_hz_f(note: f32) -> f32 {
    midi_to_hz(0) * (note / 12.0).exp2()
}

/// Hz to a fractional MIDI note; per parameter change, not per sample.
pub fn hz_to_note(hz: f32) -> f32 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

/// Cheap `tanh`: exact slope at 0, ±1 from |x| = 3 on.
pub fn saturate(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

#[derive(Clone, Copy, Default)]
pub struct Ladder {
    s: [f32; 4],
    /// Smoothed cutoff as a MIDI note; `None` until the first sample.
    note: Option<f32>,
    /// One-pole gain g/(1+g) and 1/(1+g) at `note`.
    big_g: f32,
    inv: f32,
}

impl Ladder {
    pub fn new() -> Ladder {
        Ladder {
            s: [FLOOR, 0.0, 0.0, 0.0],
            ..Ladder::default()
        }
    }

    /// Filter one sample. `cutoff` is a MIDI note, `k` the feedback
    /// (0..=`MAX_K`), `drive` the input gain into the saturator.
    pub fn process(&mut self, t: &LadderTables, x: f32, cutoff: f32, k: f32, drive: f32) -> f32 {
        if let Some(note) = t.follow(&mut self.note, cutoff) {
            self.retune(t, note);
        }
        let (g, inv) = (self.big_g, self.inv);
        let [s1, s2, s3, s4] = self.s;
        // Output of the last stage is G^4·u + S; solve the loop for u.
        let big_s = ((g * s1 * inv + s2 * inv) * g + s3 * inv) * g + s4 * inv;
        let g4 = g * g * g * g;
        let u = saturate((x * drive - k * big_s) / (1.0 + k * g4));
        let mut y = u;
        for s in self.s.iter_mut() {
            let v = (y - *s) * g;
            y = v + *s;
            *s = y + v;
        }
        if y.is_finite() {
            y
        } else {
            self.s = [0.0; 4];
            0.0
        }
    }

    /// Filter one sample as `v` voices the ladder: `k` the feedback
    /// (0..=`MAX_K`) before the voicing scales it, `drive` the input gain
    /// before the voicing's. The Mono voice and the modular `ladder` share
    /// it, so a voicing sounds the same in both (#307).
    pub fn voiced(
        &mut self,
        t: &LadderTables,
        v: &LadderVoicing,
        x: f32,
        cutoff: f32,
        k: f32,
        drive: f32,
    ) -> f32 {
        let k = k.clamp(0.0, MAX_K) * v.k_scale;
        let x = x * (1.0 + v.comp * k);
        self.process(t, x, cutoff, k, drive * v.drive)
    }

    fn retune(&mut self, t: &LadderTables, note: f32) {
        let g = t.g_at(note);
        self.inv = 1.0 / (1.0 + g);
        self.big_g = g * self.inv;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// The filter's output for a sine at `hz`, after a second to settle.
    fn settled(cutoff_hz: f32, hz: f32, amp: f32, k: f32, drive: f32) -> Vec<f32> {
        let t = LadderTables::new(SR);
        let mut f = Ladder::default();
        let cutoff = hz_to_note(cutoff_hz);
        let n = SR as usize;
        let w = std::f64::consts::TAU * f64::from(hz) / f64::from(SR);
        (0..2 * n)
            .map(|i| f.process(&t, amp * (w * i as f64).sin() as f32, cutoff, k, drive))
            .skip(n)
            .collect()
    }

    /// Amplitude of the partial at `hz` in `y`.
    fn partial(y: &[f32], hz: f32) -> f64 {
        let w = std::f64::consts::TAU * f64::from(hz) / f64::from(SR);
        let (mut re, mut im) = (0.0, 0.0);
        for (i, s) in y.iter().enumerate() {
            re += f64::from(*s) * (w * i as f64).sin();
            im += f64::from(*s) * (w * i as f64).cos();
        }
        2.0 * (re * re + im * im).sqrt() / y.len() as f64
    }

    /// Gain of the filter for a small sine at `hz`, after it settles.
    fn gain(cutoff_hz: f32, hz: f32) -> f64 {
        partial(&settled(cutoff_hz, hz, 0.01, 0.0, 1.0), hz) / 0.01
    }

    fn db(x: f64) -> f64 {
        20.0 * x.log10()
    }

    /// Rising zero crossings per second.
    fn measured_hz(x: &[f32]) -> f32 {
        let c: Vec<f32> = x
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
            .map(|(i, w)| i as f32 + w[0] / (w[0] - w[1]))
            .collect();
        (c.len() - 1) as f32 * SR / (c[c.len() - 1] - c[0])
    }

    #[test]
    fn falls_24_db_per_octave() {
        assert!(db(gain(1_000.0, 100.0)).abs() < 0.5, "passband");
        // About −12 dB at the cutoff: four poles at −3 dB each.
        assert!((db(gain(1_000.0, 1_000.0)) + 12.0).abs() < 0.5);
        let octave = db(gain(1_000.0, 8_000.0)) - db(gain(1_000.0, 4_000.0));
        assert!((octave + 24.0).abs() <= 2.0, "{octave:.2} dB per octave");
    }

    #[test]
    fn self_oscillation_is_bounded() {
        let t = LadderTables::new(SR);
        // Near the cutoff at fixed settings.
        for cutoff_hz in [110.0, 500.0, 2_000.0, 8_000.0] {
            let mut f = Ladder::new();
            let cutoff = hz_to_note(cutoff_hz);
            let y: Vec<f32> = (0..SR as usize)
                .map(|_| f.process(&t, 0.0, cutoff, MAX_K, 8.0))
                .collect();
            let tail = &y[y.len() / 2..];
            let peak = tail.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.2, "{cutoff_hz} Hz rings at only {peak}");
            let hz = measured_hz(tail);
            assert!(
                (hz / cutoff_hz - 1.0).abs() < 0.03,
                "{cutoff_hz} Hz rings at {hz}"
            );
        }
        // And bounded through a sweep from 20 Hz to 20 kHz.
        let mut f = Ladder::new();
        let n = 2 * SR as usize;
        let (lo, hi) = (hz_to_note(20.0), hz_to_note(20_000.0));
        for i in 0..n {
            let cutoff = lo + (hi - lo) * i as f32 / n as f32;
            let y = f.process(&t, 0.0, cutoff, MAX_K, 8.0);
            assert!(y.is_finite() && y.abs() <= 2.0, "{y} at sample {i}");
        }
    }

    #[test]
    fn any_parameters_stay_finite() {
        let t = LadderTables::new(SR);
        let cutoffs = [f32::NAN, -100.0, 0.0, 60.0, 135.0, 500.0, f32::INFINITY];
        for cutoff in cutoffs {
            for k in [0.0, 2.0, MAX_K] {
                for drive in [1.0, 8.0] {
                    let mut f = Ladder::new();
                    for i in 0..4_800 {
                        // A loud square, well past the saturator's knee.
                        let x = if i % 37 < 18 { 4.0 } else { -4.0 };
                        let y = f.process(&t, x, cutoff, k, drive);
                        assert!(y.is_finite() && y.abs() <= 2.0, "{cutoff} {k} {drive}: {y}");
                    }
                }
            }
        }
    }

    /// A jump in cutoff glides over a few milliseconds instead of stepping.
    #[test]
    fn cutoff_is_smoothed() {
        let t = LadderTables::new(SR);
        let mut f = Ladder::new();
        let (from, to) = (hz_to_note(200.0), hz_to_note(8_000.0));
        f.process(&t, 0.0, from, 0.0, 1.0);
        f.process(&t, 0.0, to, 0.0, 1.0);
        let first = f.note.unwrap_or(0.0) - from;
        assert!(first > 0.0 && first < 1.0, "first step {first} semitones");
        for _ in 0..(0.02 * SR) as usize {
            f.process(&t, 0.0, to, 0.0, 1.0);
        }
        assert_eq!(f.note, Some(to), "settled after 20 ms");
    }

    #[test]
    fn table_matches_tan() {
        let t = LadderTables::new(SR);
        for hz in [30.0, 440.0, 5_000.0, 15_000.0] {
            let exact = (std::f32::consts::PI * hz / SR).tan();
            let got = t.g_at(hz_to_note(hz));
            assert!((got / exact - 1.0).abs() < 1.0e-3, "{hz}: {got} vs {exact}");
        }
    }

    #[test]
    fn table_matches_tan_at_different_sample_rates() {
        for sr in [44_100.0, 96_000.0] {
            let t = LadderTables::new(sr);
            for hz in [30.0, 440.0, 5_000.0, 15_000.0] {
                let exact = (std::f32::consts::PI * hz / sr).tan();
                let got = t.g_at(hz_to_note(hz));
                assert!(
                    (got / exact - 1.0).abs() < 1.0e-3,
                    "{hz} at {sr}: {got} vs {exact}"
                );
            }
        }
    }

    /// Below the k = 4 threshold the peak at the cutoff grows with k.
    #[test]
    fn gain_at_cutoff_rises_with_resonance() {
        let gains: Vec<f64> = (0..=6)
            .map(|i| {
                let k = i as f32 * 0.5;
                partial(&settled(1_000.0, 1_000.0, 0.01, k, 1.0), 1_000.0) / 0.01
            })
            .collect();
        assert!(gains.windows(2).all(|g| g[1] > g[0]), "{gains:?}");
        // Four poles at the cutoff: 1/4 open, 1 at k = 3 (1 / (4 - k)).
        assert!((gains[6] / gains[0] - 4.0).abs() < 0.4, "{gains:?}");
    }

    /// Peak of |y| over `samples` from the start of a ladder hit by an
    /// impulse, and over the same length a second later.
    fn impulse_peaks(k: f32, samples: usize) -> (f32, f32) {
        let t = LadderTables::new(SR);
        let mut f = Ladder::default();
        let cutoff = hz_to_note(2_000.0);
        let y: Vec<f32> = (0..SR as usize + samples)
            .map(|i| f.process(&t, if i == 0 { 1.0 } else { 0.0 }, cutoff, k, 1.0))
            .collect();
        let peak = |s: &[f32]| s.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        (peak(&y[..samples]), peak(&y[SR as usize..]))
    }

    #[test]
    fn an_impulse_decays_below_the_threshold_and_rings_above() {
        let (start, late) = impulse_peaks(3.5, 480);
        assert!(late < 1.0e-3 * start, "k 3.5: {start} then {late}");
        let (start, late) = impulse_peaks(4.5, 480);
        assert!(
            late > 0.1 && late > 0.5 * start,
            "k 4.5: {start} then {late}"
        );
    }

    #[test]
    fn saturate_is_an_odd_monotonic_tanh() {
        let xs: Vec<f32> = (-400..=400).map(|i| i as f32 / 100.0).collect();
        for x in &xs {
            assert_eq!(saturate(-x), -saturate(*x), "odd at {x}");
            if x.abs() <= 3.0 {
                assert!((saturate(*x) - x.tanh()).abs() < 0.03, "near tanh at {x}");
            }
        }
        for w in xs.windows(2) {
            let (a, b) = (saturate(w[0]), saturate(w[1]));
            // Within an f32 ulp: the curve is flat at the knee, where
            // 1 - f(x) = (3 - x)^3 / (27 + 9x^2) rounds either way.
            assert!(
                b >= a - f32::EPSILON,
                "monotonic: {} -> {a}, {} -> {b}",
                w[0],
                w[1]
            );
        }
        assert!(
            xs.windows(2)
                .filter(|w| w[1] <= 2.5 && w[0] >= -2.5)
                .all(|w| saturate(w[1]) > saturate(w[0])),
            "strictly rising inside the knee"
        );
        let h = 1.0e-3;
        let slope = (saturate(h) - saturate(-h)) / (2.0 * h);
        assert!((slope - 1.0).abs() < 1.0e-3, "slope at 0: {slope}");
    }

    #[test]
    fn drive_adds_harmonics() {
        let third = |drive: f32| {
            let y = settled(20_000.0, 200.0, 0.3, 0.0, drive);
            partial(&y, 600.0) / partial(&y, 200.0)
        };
        let (clean, driven) = (third(1.0), third(8.0));
        assert!(
            driven > 0.05 && driven > 10.0 * clean,
            "{clean} -> {driven}"
        );
    }

    #[test]
    fn bad_input_and_k_stay_finite() {
        let t = LadderTables::new(SR);
        let cutoff = hz_to_note(1_000.0);
        for k in [-1.0, 0.0, MAX_K, MAX_K + 2.0, 100.0] {
            let mut f = Ladder::new();
            let mut tail = 0.0_f32;
            for i in 0..4_800 {
                let x = match i {
                    100 => f32::NAN,
                    200 => f32::INFINITY,
                    300 => f32::NEG_INFINITY,
                    _ if i % 96 < 48 => 0.5,
                    _ => -0.5,
                };
                let y = f.process(&t, x, cutoff, k, 1.0);
                assert!(y.is_finite() && y.abs() <= 2.0, "k {k}, sample {i}: {y}");
                if i > 4_000 {
                    tail = tail.max(y.abs());
                }
            }
            // And it still passes sound after the bad samples.
            assert!(tail > 0.01, "k {k} went quiet");
        }
    }

    /// The cutoff glides with a 2 ms time constant: 1/e of the way is left
    /// after 2 ms, up or down.
    #[test]
    fn smoothing_time_constant() {
        let t = LadderTables::new(SR);
        let (lo, hi) = (hz_to_note(200.0), hz_to_note(8_000.0));
        for (from, to) in [(lo, hi), (hi, lo)] {
            let mut f = Ladder::new();
            f.process(&t, 0.0, from, 0.0, 1.0);
            for _ in 0..(SMOOTH_SECONDS * SR) as usize {
                f.process(&t, 0.0, to, 0.0, 1.0);
            }
            let left = (to - f.note.unwrap_or(from)) / (to - from);
            assert!((left - (-1.0_f32).exp()).abs() < 1.0e-3, "{left} left");
        }
    }
}
