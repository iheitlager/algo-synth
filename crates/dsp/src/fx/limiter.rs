//! The master limiter (ADR-0002 rule 5): the last stage before the speakers.
//! The gain drops at once to keep a peak inside ±1 and recovers over the
//! release time, so the output never passes full scale. NaN and infinity
//! become silence instead of reaching the output.

/// Seconds the gain takes to recover.
const RELEASE: f32 = 0.05;

pub struct Limiter {
    gain: f32,
    release: f32,
}

impl Limiter {
    pub fn new(sample_rate: f32) -> Limiter {
        Limiter {
            gain: 1.0,
            release: 1.0 - (-1.0 / (RELEASE * sample_rate)).exp(),
        }
    }

    /// Limit `left` and `right` in place.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if !l.is_finite() {
                *l = 0.0;
            }
            if !r.is_finite() {
                *r = 0.0;
            }
            let peak = l.abs().max(r.abs());
            let need = if peak > 1.0 { 1.0 / peak } else { 1.0 };
            if need < self.gain {
                self.gain = need;
            } else {
                self.gain += (need - self.gain) * self.release;
                if 1.0 - self.gain < 1.0e-6 {
                    self.gain = 1.0;
                }
            }
            *l = (*l * self.gain).clamp(-1.0, 1.0);
            *r = (*r * self.gain).clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_signals_are_untouched() {
        let mut l = Limiter::new(48_000.0);
        let x: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin() * 0.99).collect();
        let (mut a, mut b) = (x.clone(), x.clone());
        l.process(&mut a, &mut b);
        assert!(a == x && b == x);
    }

    #[test]
    fn nothing_passes_full_scale_and_it_recovers() {
        let mut l = Limiter::new(48_000.0);
        let mut a: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.05).sin() * 50.0).collect();
        let mut b = a.clone();
        l.process(&mut a, &mut b);
        assert!(a.iter().chain(&b).all(|x| x.abs() <= 1.0));
        assert!(
            a.iter().any(|x| x.abs() > 0.99),
            "it limits, it doesn't mute"
        );
        let (mut q, mut q2) = (vec![0.5; 48_000], vec![0.5; 48_000]);
        l.process(&mut q, &mut q2);
        assert!((q[47_999] - 0.5).abs() < 1.0e-3, "back to unity");
    }

    #[test]
    fn non_finite_input_becomes_silence() {
        let mut l = Limiter::new(48_000.0);
        let (mut a, mut b) = (
            vec![f32::NAN, f32::INFINITY, 0.3],
            vec![f32::NEG_INFINITY, 0.0, 0.3],
        );
        l.process(&mut a, &mut b);
        assert_eq!((a[0], a[1], b[0]), (0.0, 0.0, 0.0));
        assert!((a[2] - 0.3).abs() < 1.0e-6);
    }
}
