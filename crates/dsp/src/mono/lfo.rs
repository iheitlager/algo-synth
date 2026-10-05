//! The LFO and the sample-and-hold of the Mono voice (spec 004 Req 5).
//!
//! The LFO shares the VCO waveform ids (pulse is its square) but needs no
//! band-limiting at 0.01-50 Hz. The sample-and-hold takes a new noise value
//! at every LFO cycle start and holds it until the next.
//!
//! The phase is f64: at 0.01 Hz the increment is a few f32 ulps of the
//! phase, and rounding made the cycle 8% long.

use crate::mono::noise::Noise;
use crate::mono::osc::{Waveform, naive};

#[derive(Clone, Copy, Default)]
pub struct Lfo {
    phase: f64,
    held: f32,
    started: bool,
}

impl Lfo {
    /// Advance one sample at `inc` cycles per sample; returns the LFO and
    /// the sample-and-hold output, both in −1..1.
    pub fn step(
        &mut self,
        inc: f32,
        wave: Waveform,
        sine: &[f32],
        noise: &mut Noise,
    ) -> (f32, f32) {
        if !self.started {
            self.started = true;
            self.held = noise.white();
        }
        // Pulse at half width is the square.
        let y = naive(wave, self.phase as f32, 0.5, sine);
        let held = self.held;
        let next = self.phase + f64::from(inc.clamp(0.0, 0.5));
        if next >= 1.0 {
            self.held = noise.white();
            self.phase = next - 1.0;
        } else {
            self.phase = next;
        }
        (y, held)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mono::MonoParams;
    use crate::params::Param;
    use crate::voice::sine_table;

    const SR: f32 = 48_000.0;

    fn render(hz: f32, wave: Waveform, seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let sine = sine_table();
        let (mut lfo, mut noise) = (Lfo::default(), Noise::new(7));
        (0..(seconds * SR) as usize)
            .map(|_| lfo.step(hz / SR, wave, &sine, &mut noise))
            .unzip()
    }

    #[test]
    fn lfo_frequency() {
        for hz in [0.5, 4.0, 50.0] {
            for wave in [
                Waveform::Sine,
                Waveform::Triangle,
                Waveform::Saw,
                Waveform::Pulse,
            ] {
                let (y, _) = render(hz, wave, 4.5);
                // Rising crossings of the mid level: one per cycle.
                let c: Vec<usize> = y
                    .windows(2)
                    .enumerate()
                    .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
                    .map(|(i, _)| i)
                    .collect();
                let got = (c.len() - 1) as f32 * SR / (c[c.len() - 1] - c[0]) as f32;
                assert!((got / hz - 1.0).abs() < 0.005, "{wave:?} {hz} Hz: {got}");
            }
        }
    }

    /// Sample indices where the held value changes: the cycle starts.
    fn changes(held: &[f32]) -> Vec<usize> {
        held.windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] != w[1])
            .map(|(i, _)| i + 1)
            .collect()
    }

    #[test]
    fn sample_and_hold_once_per_period() {
        let (_, held) = render(4.0, Waveform::Sine, 2.0);
        // At 4 Hz a cycle is 12 000 samples; the first value is taken at 0.
        let at = changes(&held);
        assert_eq!(at.len(), 7, "{at:?}");
        for (k, i) in at.iter().enumerate() {
            let want = (k + 1) * 12_000;
            assert!(i.abs_diff(want) <= 1, "change {k} at {i}, wanted {want}");
        }
        assert!(held.iter().all(|v| (-1.0..=1.0).contains(v)));
    }

    #[test]
    fn sine_starts_at_zero_and_rises() {
        let (y, _) = render(1.0, Waveform::Sine, 1.0);
        assert!(y[0].abs() < 1.0e-3, "starts at {}", y[0]);
        assert!(y[..12_000].windows(2).all(|w| w[1] >= w[0]), "rises");
        assert!((y[12_000] - 1.0).abs() < 1.0e-3, "peaks at a quarter");
    }

    fn span(y: &[f32]) -> (f32, f32) {
        y.iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(*v), hi.max(*v))
            })
    }

    #[test]
    fn saw_runs_from_minus_one_to_one() {
        let (y, _) = render(10.0, Waveform::Saw, 0.5);
        assert!(y[0] < -0.99, "starts at {}", y[0]);
        // It rises through a cycle, then drops back once.
        let drops = y.windows(2).filter(|w| w[1] < w[0]).count();
        assert_eq!(drops, 4);
        let (lo, hi) = span(&y);
        assert!(
            (-1.0..-0.99).contains(&lo) && hi > 0.99 && hi <= 1.0,
            "{lo}..{hi}"
        );
    }

    #[test]
    fn triangle_stays_in_range() {
        let (y, _) = render(10.0, Waveform::Triangle, 0.5);
        let (lo, hi) = span(&y);
        assert!(
            (-1.0..-0.99).contains(&lo) && hi > 0.99 && hi <= 1.0,
            "{lo}..{hi}"
        );
    }

    #[test]
    fn square_has_fifty_percent_duty() {
        let (y, _) = render(10.0, Waveform::Pulse, 1.0);
        assert!(y.iter().all(|v| v.abs() == 1.0));
        let high = y.iter().filter(|v| **v > 0.0).count();
        assert!(high.abs_diff(y.len() / 2) <= 10, "{high} of {}", y.len());
    }

    /// At 0.01 Hz the increment is a few ulps of the phase: the period
    /// must still come out at 100 s.
    #[test]
    fn slowest_rate_keeps_its_period() {
        let (_, held) = render(0.01, Waveform::Sine, 201.0);
        let at = changes(&held);
        assert_eq!(at.len(), 2, "{at:?}");
        for (k, i) in at.iter().enumerate() {
            let want = (k + 1) as f32 * 100.0 * SR;
            assert!(
                (*i as f32 / want - 1.0).abs() < 0.005,
                "cycle {k} ends at {i}"
            );
        }
    }

    /// `LfoRate` in Hz becomes cycles per sample at any rate.
    #[test]
    fn lfo_rate_at_other_sample_rates() {
        let sine = sine_table();
        for sr in [44_100.0, 96_000.0] {
            let mut p = MonoParams::new(sr);
            p.set(Param::LfoRate, 4.0);
            let (mut lfo, mut noise) = (Lfo::default(), Noise::new(7));
            let held: Vec<f32> = (0..(2.1 * sr) as usize)
                .map(|_| lfo.step(p.lfo_inc, Waveform::Sine, &sine, &mut noise).1)
                .collect();
            let at = changes(&held);
            assert_eq!(at.len(), 8, "{sr}: {at:?}");
            let period = (at[7] - at[0]) as f32 / 7.0;
            assert!((period / (sr / 4.0) - 1.0).abs() < 1.0e-4, "{sr}: {period}");
        }
    }
}
