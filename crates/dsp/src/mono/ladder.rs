//! The 4-pole ladder low-pass (spec 004 Req 3): the Moog sound inside the
//! Mono voice.
//!
//! Zero-delay feedback after Zavalishin ("The Art of VA Filter Design"):
//! four trapezoidal one-poles, the feedback loop solved for the input each
//! sample, then the input saturated by a rational `tanh` approximation, which
//! also bounds self-oscillation. A voicing may saturate each stage as well
//! (`Stages`, #306): each stage's `tanh(v)` is taken as `v · saturate(v₀)/v₀`
//! at the previous sample's `v₀`, which keeps the loop linear in the input,
//! so it is still solved exactly, with no iteration (Mystran's cheap
//! zero-delay method). ADR-0002: the cutoff is smoothed in pitch
//! and `g` comes from a table built in `Engine::new`, read only while the
//! smoothed cutoff moves; `render` does no transcendental math.

use crate::mono::model::{LadderVoicing, Stages};
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
/// The OTA stages take their difference at this fraction of the ladder's
/// level: their linear range is a little wider than the input saturator's.
/// At 1 a full-resonance whistle runs 4% flat; at 0.7 it stays within 2%,
/// and a hot input's edges are still rounded audibly.
const OTA_GAIN: f32 = 0.7;
/// The SSM2040's cells run cleaner: a wider linear range than the IR3109's.
const SSM_GAIN: f32 = 0.45;

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

/// `saturate(x) / x`, 1 at 0: the saturator as a gain at `x`.
fn sat_gain(x: f32) -> f32 {
    let x2 = x * x;
    if x2 < 9.0 {
        (27.0 + x2) / (27.0 + 9.0 * x2)
    } else {
        1.0 / x.abs()
    }
}

impl Stages {
    /// Each stage's saturator as a gain on its input and on its output, at
    /// last sample's loop input and stage outputs.
    fn gains(self, last: [f32; 5]) -> ([f32; 4], [f32; 4]) {
        let [u0, o1, o2, o3, o4] = last;
        // An OTA saturates the difference of its input and output.
        let ota = |gain: f32| {
            let d = [u0 - o1, o1 - o2, o2 - o3, o3 - o4].map(|v| sat_gain(v * gain));
            (d, d)
        };
        match self {
            Stages::Linear => ([1.0; 4], [1.0; 4]),
            // A stage's input is the last one's output: five gains for eight.
            Stages::Transistor => {
                let [g0, g1, g2, g3, g4] = last.map(sat_gain);
                ([g0, g1, g2, g3], [g1, g2, g3, g4])
            }
            Stages::Ota | Stages::Cem3320 => ota(OTA_GAIN),
            Stages::Ssm2040 => ota(SSM_GAIN),
        }
    }

    /// The loop solved for its input `u`, the last stage being `A·u + B`:
    /// `x` the driven input, `k` the feedback, `o4` last sample's output.
    fn loop_input(self, x: f32, k: f32, big_a: f32, big_b: f32, o4: f32) -> f32 {
        match self {
            // The input clips on its own; the feedback through the VCA, at
            // its gain at last sample's output.
            Stages::Cem3320 => {
                let k = k * sat_gain(k * o4);
                (saturate(x) - k * big_b) / (1.0 + k * big_a)
            }
            _ => saturate((x - k * big_b) / (1.0 + k * big_a)),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Ladder {
    s: [f32; 4],
    /// Smoothed cutoff as a MIDI note; `None` until the first sample.
    note: Option<f32>,
    /// The prewarped gain g, g/(1+g) and 1/(1+g) at `note`.
    g: f32,
    big_g: f32,
    inv: f32,
    /// The loop input and each stage's output last sample, where a
    /// saturating stage takes its gain.
    last: [f32; 5],
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
        self.run(t, Stages::Linear, x, cutoff, k, drive)
    }

    fn run(
        &mut self,
        t: &LadderTables,
        stages: Stages,
        x: f32,
        cutoff: f32,
        k: f32,
        drive: f32,
    ) -> f32 {
        if let Some(note) = t.follow(&mut self.note, cutoff) {
            self.retune(t, note);
        }
        if stages != Stages::Linear {
            return self.saturating(stages, x, k, drive);
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

    /// One sample with saturating stages. Stage `i` is
    /// `y = s + g·(a·x − b·y)`, so `y = α·x + β` with `α = g·a/(1 + g·b)`
    /// and `β = s/(1 + g·b)`; `a` and `b` are the saturator's gain at last
    /// sample's input and output.
    fn saturating(&mut self, stages: Stages, x: f32, k: f32, drive: f32) -> f32 {
        let g = self.g;
        let (a, b) = stages.gains(self.last);
        let [d1, d2, d3, d4] = b.map(|b| 1.0 / (1.0 + g * b));
        let [a1, a2, a3, a4] = a;
        let [s1, s2, s3, s4] = self.s;
        let (a1, a2, a3, a4) = (g * a1 * d1, g * a2 * d2, g * a3 * d3, g * a4 * d4);
        let (b1, b2, b3, b4) = (s1 * d1, s2 * d2, s3 * d3, s4 * d4);
        // The last stage's output is A·u + B; solve the loop for u.
        let big_a = a1 * a2 * a3 * a4;
        let big_b = ((b1 * a2 + b2) * a3 + b3) * a4 + b4;
        let u = stages.loop_input(x * drive, k, big_a, big_b, self.last[4]);
        let y1 = a1 * u + b1;
        let y2 = a2 * y1 + b2;
        let y3 = a3 * y2 + b3;
        let y4 = a4 * y3 + b4;
        self.s = [2.0 * y1 - s1, 2.0 * y2 - s2, 2.0 * y3 - s3, 2.0 * y4 - s4];
        self.last = [u, y1, y2, y3, y4];
        if y4.is_finite() {
            y4
        } else {
            self.s = [0.0; 4];
            self.last = [0.0; 5];
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
        self.run(t, v.stages, x, cutoff, k, drive * v.drive)
    }

    fn retune(&mut self, t: &LadderTables, note: f32) {
        let g = t.g_at(note);
        self.g = g;
        self.inv = 1.0 / (1.0 + g);
        self.big_g = g * self.inv;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;
    /// Every stage type, for the tests every ladder must pass.
    const STAGES: [Stages; 5] = [
        Stages::Linear,
        Stages::Transistor,
        Stages::Ota,
        Stages::Cem3320,
        Stages::Ssm2040,
    ];

    /// The filter's output for a sine at `hz`, after a second to settle.
    fn settled(cutoff_hz: f32, hz: f32, amp: f32, k: f32, drive: f32) -> Vec<f32> {
        settled_as(Stages::Linear, cutoff_hz, hz, amp, k, drive)
    }

    fn settled_as(
        stages: Stages,
        cutoff_hz: f32,
        hz: f32,
        amp: f32,
        k: f32,
        drive: f32,
    ) -> Vec<f32> {
        let t = LadderTables::new(SR);
        let mut f = Ladder::default();
        let cutoff = hz_to_note(cutoff_hz);
        let n = SR as usize;
        let w = std::f64::consts::TAU * f64::from(hz) / f64::from(SR);
        (0..2 * n)
            .map(|i| {
                let x = amp * (w * i as f64).sin() as f32;
                f.run(&t, stages, x, cutoff, k, drive)
            })
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
        gain_as(Stages::Linear, cutoff_hz, hz)
    }

    fn gain_as(stages: Stages, cutoff_hz: f32, hz: f32) -> f64 {
        partial(&settled_as(stages, cutoff_hz, hz, 0.01, 0.0, 1.0), hz) / 0.01
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
        for stages in STAGES {
            // Near the cutoff at fixed settings; saturating stages ring
            // softer.
            let least = if stages == Stages::Linear { 0.2 } else { 0.1 };
            for cutoff_hz in [110.0, 500.0, 2_000.0, 8_000.0] {
                let mut f = Ladder::new();
                let cutoff = hz_to_note(cutoff_hz);
                let y: Vec<f32> = (0..SR as usize)
                    .map(|_| f.run(&t, stages, 0.0, cutoff, MAX_K, 8.0))
                    .collect();
                let tail = &y[y.len() / 2..];
                let peak = tail.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
                assert!(
                    peak > least,
                    "{stages:?} {cutoff_hz} Hz rings at only {peak}"
                );
                let hz = measured_hz(tail);
                assert!(
                    (hz / cutoff_hz - 1.0).abs() < 0.03,
                    "{stages:?} {cutoff_hz} Hz rings at {hz}"
                );
            }
            // And bounded through a sweep from 20 Hz to 20 kHz.
            let mut f = Ladder::new();
            let n = 2 * SR as usize;
            let (lo, hi) = (hz_to_note(20.0), hz_to_note(20_000.0));
            for i in 0..n {
                let cutoff = lo + (hi - lo) * i as f32 / n as f32;
                let y = f.run(&t, stages, 0.0, cutoff, MAX_K, 8.0);
                assert!(
                    y.is_finite() && y.abs() <= 2.0,
                    "{stages:?}: {y} at sample {i}"
                );
            }
        }
    }

    #[test]
    fn any_parameters_stay_finite() {
        let t = LadderTables::new(SR);
        let cutoffs = [f32::NAN, -100.0, 0.0, 60.0, 135.0, 500.0, f32::INFINITY];
        for (stages, cutoff) in STAGES.into_iter().flat_map(|s| cutoffs.map(|c| (s, c))) {
            for k in [0.0, 2.0, MAX_K] {
                for drive in [1.0, 8.0] {
                    let mut f = Ladder::new();
                    for i in 0..4_800 {
                        // A loud square, well past the saturator's knee.
                        let x = if i % 37 < 18 { 4.0 } else { -4.0 };
                        let y = f.run(&t, stages, x, cutoff, k, drive);
                        assert!(
                            y.is_finite() && y.abs() <= 2.0,
                            "{stages:?} {cutoff} {k} {drive}: {y}"
                        );
                    }
                }
            }
        }
    }

    /// A jump in cutoff glides over a few milliseconds instead of stepping.
    /// A small signal does not reach the stages' saturators: every stage
    /// type has the linear ladder's response, 24 dB per octave.
    #[test]
    fn soft_input_matches_the_linear_ladder() {
        for stages in STAGES {
            for hz in [100.0, 1_000.0, 4_000.0, 8_000.0] {
                let (want, got) = (gain(1_000.0, hz), gain_as(stages, 1_000.0, hz));
                assert!(
                    (db(got) - db(want)).abs() < 0.1,
                    "{stages:?} at {hz} Hz: {got} vs {want}"
                );
            }
        }
    }

    /// Odd harmonics against the fundamental of a hot sine at 200 Hz into
    /// a 1 kHz cutoff with resonance.
    fn hot_profile(stages: Stages) -> [f64; 2] {
        let y = settled_as(stages, 1_000.0, 200.0, 0.5, 3.8, 8.0);
        let f1 = partial(&y, 200.0);
        [partial(&y, 600.0) / f1, partial(&y, 1_000.0) / f1]
    }

    /// #306: saturating each stage, the transistor ladder keeps its
    /// distortion under resonance, where the single input saturator's is
    /// filtered away: the 3rd and 5th harmonics stand several times higher.
    #[test]
    fn hot_transistor_stages_saturate_differently() {
        let [h3, h5] = hot_profile(Stages::Linear);
        let [t3, t5] = hot_profile(Stages::Transistor);
        assert!(t3 > 3.0 * h3 && t5 > 3.0 * h5, "{h3} {h5} -> {t3} {t5}");
    }

    /// Odd harmonics of a hot 100 Hz square into a 1 kHz cutoff with some
    /// resonance, against the fundamental: the 3rd, 9th and 15th.
    fn hot_square(stages: Stages) -> [f64; 3] {
        let t = LadderTables::new(SR);
        let mut f = Ladder::new();
        let n = SR as usize;
        let y: Vec<f32> = (0..2 * n)
            .map(|i| {
                let x = if (i / 240) % 2 == 0 { 0.5 } else { -0.5 };
                f.run(&t, stages, x, hz_to_note(1_000.0), 3.0, 8.0)
            })
            .skip(n)
            .collect();
        let f1 = partial(&y, 100.0);
        [3.0, 9.0, 15.0].map(|h| partial(&y, h * 100.0) / f1)
    }

    /// #305: the OTAs slew on a hot input's edges, so the harmonics at and
    /// above the cutoff come out lower than through the linear ladder,
    /// while those well below it stay.
    #[test]
    fn hot_ota_stages_round_the_edges() {
        let [l3, l9, l15] = hot_square(Stages::Linear);
        let [o3, o9, o15] = hot_square(Stages::Ota);
        assert!((o3 / l3 - 1.0).abs() < 0.03, "3rd: {l3} vs {o3}");
        assert!(
            o9 < 0.95 * l9 && o15 < 0.8 * l15,
            "{l9} {l15} -> {o9} {o15}"
        );
    }

    /// The fundamental of a hot 200 Hz sine into a 1 kHz cutoff, at
    /// resonance `k` against none.
    fn hot_resonance_keeps(stages: Stages) -> f64 {
        let f1 = |k| partial(&settled_as(stages, 1_000.0, 200.0, 0.5, k, 8.0), 200.0);
        f1(3.8) / f1(0.0)
    }

    /// #321: where the input saturator takes the feedback with it, a hot
    /// input pushes through the resonance; the CEM3320 clips its input and
    /// its feedback apart, so resonance takes the bass as it does softly.
    #[test]
    fn cem_resonance_takes_the_bass_of_a_hot_input() {
        let moog = hot_resonance_keeps(Stages::Transistor);
        let cem = hot_resonance_keeps(Stages::Cem3320);
        assert!(cem < 0.5 * moog, "{moog} vs {cem}");
    }

    /// #321: the Prophet-5's SSM2040 (Rev 1/2) keeps more bass under
    /// resonance than its CEM3320 (Rev 3).
    #[test]
    fn ssm_keeps_more_bass_than_cem() {
        use crate::mono::model::{PROPHET5_REV3, PROPHET5_REV12};
        let t = LadderTables::new(SR);
        let bass = |v: &LadderVoicing| {
            let mut f = Ladder::new();
            let w = std::f64::consts::TAU * 100.0 / f64::from(SR);
            let n = SR as usize;
            let y: Vec<f32> = (0..2 * n)
                .map(|i| {
                    let x = 0.01 * (w * i as f64).sin() as f32;
                    f.voiced(&t, v, x, hz_to_note(1_000.0), 3.5, 1.0)
                })
                .skip(n)
                .collect();
            partial(&y, 100.0)
        };
        let (cem, ssm) = (bass(&PROPHET5_REV3), bass(&PROPHET5_REV12));
        assert!(ssm > 1.3 * cem, "{cem} vs {ssm}");
    }

    #[test]
    fn sat_gain_is_saturate_over_x() {
        assert_eq!(sat_gain(0.0), 1.0);
        for x in [-5.0_f32, -3.0, -1.0, 0.1, 0.5, 2.9, 3.0, 4.0] {
            assert!((sat_gain(x) - saturate(x) / x).abs() < 1.0e-6, "{x}");
        }
    }

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
    fn impulse_peaks(stages: Stages, k: f32, samples: usize) -> (f32, f32) {
        let t = LadderTables::new(SR);
        let mut f = Ladder::default();
        let cutoff = hz_to_note(2_000.0);
        let y: Vec<f32> = (0..SR as usize + samples)
            .map(|i| f.run(&t, stages, if i == 0 { 1.0 } else { 0.0 }, cutoff, k, 1.0))
            .collect();
        let peak = |s: &[f32]| s.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        (peak(&y[..samples]), peak(&y[SR as usize..]))
    }

    #[test]
    fn an_impulse_decays_below_the_threshold_and_rings_above() {
        for stages in STAGES {
            let (start, late) = impulse_peaks(stages, 3.5, 480);
            assert!(
                late < 1.0e-3 * start,
                "{stages:?} k 3.5: {start} then {late}"
            );
            let (start, late) = impulse_peaks(stages, 4.5, 480);
            assert!(
                late > 0.1 && late > 0.5 * start,
                "{stages:?} k 4.5: {start} then {late}"
            );
        }
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
        for (stages, k) in STAGES
            .into_iter()
            .flat_map(|s| [-1.0, 0.0, MAX_K, MAX_K + 2.0, 100.0].map(|k| (s, k)))
        {
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
                let y = f.run(&t, stages, x, cutoff, k, 1.0);
                assert!(
                    y.is_finite() && y.abs() <= 2.0,
                    "{stages:?} k {k}, sample {i}: {y}"
                );
                if i > 4_000 {
                    tail = tail.max(y.abs());
                }
            }
            // And it still passes sound after the bad samples.
            assert!(tail > 0.01, "{stages:?} k {k} went quiet");
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
