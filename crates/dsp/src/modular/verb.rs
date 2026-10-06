//! SuperCollider's `FreeVerb2` inside a Modular voice (ADR-0024): Jezar's
//! Freeverb, eight lowpass-feedback combs and four allpasses a side, its
//! tuning scaled from 44.1 kHz. The buffers are allocated with the voice's
//! state when the program is set; `process` never allocates.

/// Jezar's comb and allpass lengths at 44.1 kHz, and the right side's spread.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;
/// The input gain into the combs, and the wet output's scale.
const GAIN: f32 = 0.015;
const WET: f32 = 3.0;

#[derive(Clone, Default)]
struct Comb {
    buf: Vec<f32>,
    at: usize,
    store: f32,
}

impl Comb {
    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let Some(y) = self.buf.get(self.at).copied() else {
            return 0.0;
        };
        self.store = y * (1.0 - damp) + self.store * damp;
        if let Some(slot) = self.buf.get_mut(self.at) {
            *slot = x + self.store * feedback;
        }
        self.at += 1;
        if self.at >= self.buf.len() {
            self.at = 0;
        }
        y
    }
}

#[derive(Clone, Default)]
struct Allpass {
    buf: Vec<f32>,
    at: usize,
}

impl Allpass {
    fn process(&mut self, x: f32) -> f32 {
        let Some(b) = self.buf.get(self.at).copied() else {
            return x;
        };
        if let Some(slot) = self.buf.get_mut(self.at) {
            *slot = x + b * 0.5;
        }
        self.at += 1;
        if self.at >= self.buf.len() {
            self.at = 0;
        }
        b - x
    }
}

/// One stereo Freeverb.
#[derive(Clone, Default)]
pub struct FreeVerb {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
}

impl FreeVerb {
    /// A reverb with its buffers sized for `sample_rate`.
    pub fn new(sample_rate: f32) -> FreeVerb {
        let scale = |n: usize| ((n as f32 * sample_rate / 44_100.0) as usize).max(1);
        let side = |spread: usize| {
            (
                COMBS
                    .iter()
                    .map(|n| Comb {
                        buf: vec![0.0; scale(n + spread)],
                        at: 0,
                        store: 0.0,
                    })
                    .collect::<Vec<_>>(),
                ALLPASSES
                    .iter()
                    .map(|n| Allpass {
                        buf: vec![0.0; scale(n + spread)],
                        at: 0,
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let (cl, al) = side(0);
        let (cr, ar) = side(SPREAD);
        FreeVerb {
            combs: [cl, cr],
            allpasses: [al, ar],
        }
    }

    /// Silence the tails, keeping the buffers.
    pub fn clear(&mut self) {
        for c in self.combs.iter_mut().flatten() {
            c.buf.iter_mut().for_each(|x| *x = 0.0);
            c.store = 0.0;
        }
        for a in self.allpasses.iter_mut().flatten() {
            a.buf.iter_mut().for_each(|x| *x = 0.0);
        }
    }

    /// One stereo sample: `mix` 0 dry to 1 wet, `room` 0..1, `damp` 0..1,
    /// as SuperCollider's `FreeVerb2` takes them.
    pub fn process(&mut self, l: f32, r: f32, mix: f32, room: f32, damp: f32) -> (f32, f32) {
        let mix = mix.clamp(0.0, 1.0);
        let feedback = room.clamp(0.0, 1.0) * 0.28 + 0.7;
        let damp = damp.clamp(0.0, 1.0) * 0.4;
        let x = (l + r) * GAIN;
        let mut out = [0.0_f32; 2];
        for (side, o) in out.iter_mut().enumerate() {
            let (Some(combs), Some(allpasses)) =
                (self.combs.get_mut(side), self.allpasses.get_mut(side))
            else {
                continue;
            };
            let mut y: f32 = combs.iter_mut().map(|c| c.process(x, feedback, damp)).sum();
            for a in allpasses.iter_mut() {
                y = a.process(y);
            }
            *o = y;
        }
        let [vl, vr] = out;
        (
            l * (1.0 - mix) + vl * mix * WET,
            r * (1.0 - mix) + vr * mix * WET,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_impulse_rings_on_both_sides_and_dies_away() {
        let mut v = FreeVerb::new(48_000.0);
        let mut tail = Vec::new();
        for n in 0..96_000 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            tail.push(v.process(x, x, 1.0, 0.5, 0.5));
        }
        let energy =
            |a: usize, b: usize| tail[a..b].iter().map(|(l, r)| l * l + r * r).sum::<f32>();
        assert!(energy(2_000, 10_000) > 1e-6, "it rings");
        assert!(
            energy(80_000, 96_000) < 0.01 * energy(2_000, 10_000),
            "and dies away"
        );
        assert!(
            tail.iter().any(|(l, r)| (l - r).abs() > 1e-6),
            "the sides differ"
        );
        assert!(tail.iter().all(|(l, r)| l.is_finite() && r.is_finite()));
        let mut dry = FreeVerb::new(48_000.0);
        assert_eq!(
            dry.process(0.5, -0.25, 0.0, 0.5, 0.5),
            (0.5, -0.25),
            "mix 0 is dry"
        );
    }
}
