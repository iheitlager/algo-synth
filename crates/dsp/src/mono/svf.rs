//! The 12 dB state-variable filter and the one-pole high-pass (spec 004
//! Req 13): the MS-20 and CS-15 filters, and the SH-101's high-pass.
//!
//! A zero-delay-feedback state-variable filter after Zavalishin, giving a
//! low-pass and a high-pass at once. Both integrator states go through the
//! same rational saturator as the ladder's, which bounds self-oscillation
//! the way an analog filter's clipping does. ADR-0002: the cutoff is
//! smoothed in pitch and `g` read from the ladder's table, only while the
//! smoothed cutoff moves; a sample costs multiplies and one division.

use crate::mono::ladder::{LadderTables, saturate};
use crate::mono::model::SvfVoicing;

/// Damping at no resonance: a Butterworth response, Q = 0.71.
const K0: f32 = 1.4;
/// Initial state, standing in for an analog noise floor, so full resonance
/// rings up with no input.
const FLOOR: f32 = 1.0e-3;

/// A low-pass and a high-pass output of the same input.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outputs {
    pub lp: f32,
    pub hp: f32,
}

#[derive(Clone, Copy, Default)]
pub struct Svf {
    s1: f32,
    s2: f32,
    /// Smoothed cutoff as a MIDI note; `None` until the first sample.
    note: Option<f32>,
    g: f32,
}

impl Svf {
    pub fn new() -> Svf {
        Svf {
            s1: FLOOR,
            ..Svf::default()
        }
    }

    /// Filter one sample. `cutoff` is a MIDI note, `res` the resonance
    /// 0..=1 (the voicing sets where it self-oscillates).
    pub fn process(
        &mut self,
        t: &LadderTables,
        v: &SvfVoicing,
        x: f32,
        cutoff: f32,
        res: f32,
    ) -> Outputs {
        if let Some(note) = t.follow(&mut self.note, cutoff) {
            self.g = t.g_at(note);
        }
        let res = if res.is_nan() {
            0.0
        } else {
            res.clamp(0.0, 1.0)
        };
        let k = (K0 * (1.0 - res / v.osc_at)).max(v.k_min);
        let g = self.g;
        let hp = (x - (k + g) * self.s1 - self.s2) / (1.0 + g * (k + g));
        let bp = g * hp + self.s1;
        let lp = g * bp + self.s2;
        let inv = 1.0 / v.ceiling;
        self.s1 = v.ceiling * saturate((g * hp + bp) * inv);
        self.s2 = v.ceiling * saturate((g * bp + lp) * inv);
        if lp.is_finite() && hp.is_finite() {
            Outputs { lp, hp }
        } else {
            *self = Svf {
                note: self.note,
                g: self.g,
                ..Svf::default()
            };
            Outputs::default()
        }
    }
}

/// A one-pole high-pass, 6 dB per octave.
#[derive(Clone, Copy, Default)]
pub struct OnePole {
    s: f32,
    note: Option<f32>,
    /// g/(1+g) at `note`.
    big_g: f32,
}

impl OnePole {
    /// Filter one sample; `cutoff` is a MIDI note.
    pub fn process(&mut self, t: &LadderTables, x: f32, cutoff: f32) -> f32 {
        if let Some(note) = t.follow(&mut self.note, cutoff) {
            let g = t.g_at(note);
            self.big_g = g / (1.0 + g);
        }
        let v = (x - self.s) * self.big_g;
        let lp = v + self.s;
        self.s = lp + v;
        let hp = x - lp;
        if hp.is_finite() {
            hp
        } else {
            self.s = 0.0;
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mono::ladder::hz_to_note;
    use crate::mono::model::{CS15, MS20};

    const SR: f32 = 48_000.0;

    /// Gain of a low-pass or high-pass output for a small sine at `hz`,
    /// after it settles.
    fn gain(cutoff_hz: f32, hz: f32, high: bool) -> f64 {
        let t = LadderTables::new(SR);
        let mut f = Svf::new();
        let cutoff = hz_to_note(cutoff_hz);
        let (amp, n) = (0.01, SR as usize);
        let w = std::f64::consts::TAU * f64::from(hz) / f64::from(SR);
        let (mut re, mut im) = (0.0, 0.0);
        for i in 0..2 * n {
            let a = w * i as f64;
            let o = f.process(&t, &MS20, amp * a.sin() as f32, cutoff, 0.0);
            let y = if high { o.hp } else { o.lp };
            if i >= n {
                re += f64::from(y) * a.sin();
                im += f64::from(y) * a.cos();
            }
        }
        2.0 * (re * re + im * im).sqrt() / n as f64 / f64::from(amp)
    }

    fn db(x: f64) -> f64 {
        20.0 * x.log10()
    }

    #[test]
    fn falls_12_db_per_octave() {
        let a = db(gain(1_000.0, 4_000.0, false));
        let b = db(gain(1_000.0, 8_000.0, false));
        assert!((a + 24.0).abs() < 2.0, "{a} dB at 4 kHz");
        assert!(((a - b) - 12.0).abs() < 2.0, "{a} dB to {b} dB");
        let pass = db(gain(1_000.0, 100.0, false));
        assert!(pass.abs() < 0.5, "the pass band is flat: {pass} dB");
    }

    #[test]
    fn high_pass_rises_12_db_per_octave() {
        let a = db(gain(1_000.0, 250.0, true));
        let b = db(gain(1_000.0, 125.0, true));
        assert!((a + 24.0).abs() < 2.0, "{a} dB at 250 Hz");
        assert!(((a - b) - 12.0).abs() < 2.0, "{a} dB to {b} dB");
        let pass = db(gain(1_000.0, 10_000.0, true));
        assert!(pass.abs() < 0.5, "the pass band is flat: {pass} dB");
    }

    #[test]
    fn one_pole_rises_6_db_per_octave() {
        let t = LadderTables::new(SR);
        let gain = |hz: f32| {
            let mut f = OnePole::default();
            let (amp, n) = (0.01, SR as usize);
            let w = std::f64::consts::TAU * f64::from(hz) / f64::from(SR);
            let (mut re, mut im) = (0.0, 0.0);
            for i in 0..2 * n {
                let a = w * i as f64;
                let y = f.process(&t, amp * a.sin() as f32, hz_to_note(1_000.0));
                if i >= n {
                    re += f64::from(y) * a.sin();
                    im += f64::from(y) * a.cos();
                }
            }
            db(2.0 * (re * re + im * im).sqrt() / n as f64 / f64::from(amp))
        };
        let (a, b) = (gain(125.0), gain(250.0));
        assert!(((b - a) - 6.0).abs() < 1.0, "{a} dB to {b} dB");
        assert!(gain(10_000.0).abs() < 0.5, "the pass band is flat");
    }

    /// Peak of the low-pass output over a cutoff sweep with no input.
    fn sweep_peak(v: &SvfVoicing, res: f32) -> (f32, f32) {
        let t = LadderTables::new(SR);
        let mut f = Svf::new();
        let (mut peak, mut hp_peak) = (0.0_f32, 0.0_f32);
        let n = 2 * SR as usize;
        for i in 0..n {
            let note = 20.0 + 100.0 * i as f32 / n as f32;
            let o = f.process(&t, v, 0.0, note, res);
            assert!(o.lp.is_finite() && o.hp.is_finite());
            peak = peak.max(o.lp.abs());
            hp_peak = hp_peak.max(o.hp.abs());
        }
        (peak, hp_peak)
    }

    #[test]
    fn self_oscillation_is_bounded() {
        let (lp, hp) = sweep_peak(&MS20, 1.0);
        assert!(lp > 0.1, "the MS-20 filter rings on its own: {lp}");
        assert!(lp <= 2.0 && hp <= 2.0, "and stays bounded: {lp}, {hp}");
        let (lp, _) = sweep_peak(&CS15, 1.0);
        assert!(lp < 0.01, "the CS-15 filter does not oscillate: {lp}");
    }

    #[test]
    fn any_parameters_stay_finite() {
        let t = LadderTables::new(SR);
        for v in [MS20, CS15] {
            let mut f = Svf::new();
            let mut p = OnePole::default();
            let cutoffs = [-1.0e9, 0.0, 33.0, 136.0, 1.0e9, f32::INFINITY, f32::NAN];
            for (i, cutoff) in cutoffs.iter().cycle().take(20_000).enumerate() {
                let res = [0.0, 0.5, 1.0, 7.0, -3.0, f32::NAN][i % 6];
                let x = [0.0, 1.0, -1.0e6, 1.0e6][i % 4];
                let o = f.process(&t, &v, x, *cutoff, res);
                assert!(o.lp.is_finite() && o.hp.is_finite(), "{v:?} {cutoff} {res}");
                assert!(p.process(&t, x, *cutoff).is_finite());
            }
        }
    }
}
