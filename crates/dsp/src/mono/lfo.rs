//! The LFO and the sample-and-hold of the Mono voice (spec 004 Req 5).
//!
//! The LFO shares the VCO waveform ids (pulse is its square) but needs no
//! band-limiting at 0.01-50 Hz. The sample-and-hold takes a new noise value
//! at every LFO cycle start and holds it until the next.

use crate::mono::noise::Noise;
use crate::mono::osc::Waveform;
use crate::voice::lookup;

#[derive(Clone, Copy, Default)]
pub struct Lfo {
    phase: f32,
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
        let p = self.phase;
        let y = match wave {
            Waveform::Sine => lookup(sine, p),
            Waveform::Triangle => 1.0 - 4.0 * (wrap(p + 0.25) - 0.5).abs(),
            Waveform::Saw => 2.0 * p - 1.0,
            Waveform::Pulse => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        };
        let held = self.held;
        let next = p + inc.clamp(0.0, 0.5);
        if next >= 1.0 {
            self.held = noise.white();
        }
        self.phase = wrap(next);
        (y, held)
    }
}

fn wrap(p: f32) -> f32 {
    if p >= 1.0 { p - 1.0 } else { p }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::TABLE;

    const SR: f32 = 48_000.0;

    fn render(hz: f32, wave: Waveform, seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let sine: Vec<f32> = (0..=TABLE)
            .map(|i| (i as f32 / TABLE as f32 * std::f32::consts::TAU).sin())
            .collect();
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
                let (y, _) = render(hz, wave, 4.0);
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

    #[test]
    fn sample_and_hold_once_per_period() {
        let (_, held) = render(4.0, Waveform::Sine, 2.0);
        // The first value counts as a change from silence.
        let changes = 1 + held.windows(2).filter(|w| w[0] != w[1]).count();
        assert_eq!(changes, 8);
        assert!(held.iter().all(|v| (-1.0..=1.0).contains(v)));
    }
}
