//! A formant filter (#745), Strudel's `vowel`: three band-passes at a
//! voice's formants, morphing a → e → i → o → u as knob A turns.
//!
//! The formants are a bass voice's (Csound's table): the first three, their
//! levels relative to the first. Each band-pass is a state-variable filter
//! (Simper's), its output normalised to unit gain at its centre, so a
//! signal at a formant comes through at that formant's level. The
//! coefficients are worked out when a knob moves, never per sample.

use std::f32::consts::PI;

/// The formants of a e i o u: frequency in Hz and level in dB.
const FORMANTS: [[(f32, f32); 3]; 5] = [
    [(600.0, 0.0), (1040.0, -7.0), (2250.0, -9.0)],
    [(400.0, 0.0), (1620.0, -12.0), (2400.0, -9.0)],
    [(250.0, 0.0), (1750.0, -30.0), (2600.0, -16.0)],
    [(400.0, 0.0), (750.0, -11.0), (2400.0, -21.0)],
    [(350.0, 0.0), (600.0, -20.0), (2400.0, -32.0)],
];

/// The vowel at knob position `a` (0 a, 0.25 e … 1 u), each formant blended
/// between its two neighbours: frequency in Hz and linear level.
pub fn formants(a: f32) -> [(f32, f32); 3] {
    let x = a.clamp(0.0, 1.0) * 4.0;
    let i = (x.floor() as usize).min(3);
    let t = x - i as f32;
    let mut out = [(0.0, 0.0); 3];
    let (Some(from), Some(to)) = (FORMANTS.get(i), FORMANTS.get(i + 1)) else {
        return out;
    };
    for ((o, lo), hi) in out.iter_mut().zip(from).zip(to) {
        let hz = lo.0 * (hi.0 / lo.0).powf(t);
        let db = lo.1 + (hi.1 - lo.1) * t;
        *o = (hz, 10.0_f32.powf(db / 20.0));
    }
    out
}

/// One band-pass's coefficients.
#[derive(Clone, Copy, Default)]
struct Band {
    a1: f32,
    a2: f32,
    a3: f32,
    /// 1/Q: the band output times this has unit gain at the centre.
    k: f32,
    level: f32,
}

pub struct Vowel {
    sample_rate: f32,
    bands: [Band; 3],
    /// The filters' two integrator states, per band and channel.
    state: [[[f32; 2]; 3]; 2],
    gain: f32,
    dry: f32,
}

impl Vowel {
    pub fn new(sample_rate: f32) -> Vowel {
        let mut v = Vowel {
            sample_rate,
            bands: [Band::default(); 3],
            state: [[[0.0; 2]; 3]; 2],
            gain: 1.0,
            dry: 0.0,
        };
        v.set([0.5, 0.5, 0.5, 0.5, 0.0]);
        v
    }

    /// The knobs: A the vowel (a e i o u, 0 to 1), B the resonance (Q 2 to
    /// 20), C level (2·C, unity at 0.5), D the formants moved from half to
    /// twice their frequency (as is at 0.5), E dry.
    pub fn set(&mut self, [a, b, c, d, e]: [f32; 5]) {
        let q = 2.0 * 10.0_f32.powf(b.clamp(0.0, 1.0));
        let shift = 2.0_f32.powf(2.0 * d.clamp(0.0, 1.0) - 1.0);
        let top = 0.45 * self.sample_rate;
        for (band, (hz, level)) in self.bands.iter_mut().zip(formants(a)) {
            let g = (PI * (hz * shift).min(top) / self.sample_rate).tan();
            let k = 1.0 / q;
            let a1 = 1.0 / (1.0 + g * (g + k));
            *band = Band {
                a1,
                a2: g * a1,
                a3: g * g * a1,
                k,
                level,
            };
        }
        self.gain = 2.0 * c.clamp(0.0, 1.0);
        self.dry = e.clamp(0.0, 1.0);
    }

    pub fn reset(&mut self) {
        self.state = [[[0.0; 2]; 3]; 2];
    }

    fn sample(&mut self, ch: usize, x: f32) -> f32 {
        let mut wet = 0.0;
        if let Some(state) = self.state.get_mut(ch) {
            for (b, s) in self.bands.iter().zip(state.iter_mut()) {
                let v3 = x - s[1];
                let v1 = b.a1 * s[0] + b.a2 * v3;
                let v2 = s[1] + b.a2 * s[0] + b.a3 * v3;
                s[0] = 2.0 * v1 - s[0];
                s[1] = 2.0 * v2 - s[1];
                wet += b.level * b.k * v1;
            }
        }
        (wet * (1.0 - self.dry) + x * self.dry) * self.gain
    }

    pub fn process_mono(&mut self, x: &mut [f32]) {
        for s in x {
            *s = self.sample(0, *s);
        }
    }

    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l = self.sample(0, *l);
            *r = self.sample(1, *r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 48_000.0;

    fn level_at(knobs: [f32; 5], hz: f32) -> f32 {
        let mut v = Vowel::new(SR);
        v.set(knobs);
        let mut y: Vec<f32> = (0..9_600)
            .map(|i| 0.5 * (TAU * hz * i as f32 / SR).sin())
            .collect();
        for c in y.chunks_mut(128) {
            v.process_mono(c);
        }
        y[4_800..].iter().fold(0.0_f32, |m, s| m.max(s.abs())) / 0.5
    }

    #[test]
    fn a_vowel_passes_its_formants_and_not_between() {
        // "a" at A = 0: its first formant at 600 Hz comes through, 4 kHz not.
        let a = [0.0, 0.5, 0.5, 0.5, 0.0];
        assert!(level_at(a, 600.0) > 0.8, "{}", level_at(a, 600.0));
        assert!(level_at(a, 4_000.0) < 0.1, "{}", level_at(a, 4_000.0));
        // "i" has its second formant far up at 1750 Hz; "u" has none there.
        let (i, u) = ([0.5, 0.5, 0.5, 0.5, 0.0], [1.0, 0.5, 0.5, 0.5, 0.0]);
        assert!(level_at(i, 250.0) > 0.8);
        assert!(level_at(u, 1_750.0) < level_at(a, 1_040.0));
    }

    #[test]
    fn the_formants_morph_between_vowels() {
        let [(f1, _), _, _] = formants(0.125);
        assert!(f1 < 600.0 && f1 > 400.0, "half way from a to e: {f1}");
        assert_eq!(formants(0.0)[0].0, 600.0);
        assert_eq!(formants(1.0)[1].0, 600.0);
    }

    #[test]
    fn silence_stays_silent_and_every_setting_is_bounded() {
        let mut v = Vowel::new(SR);
        let mut z = [0.0_f32; 512];
        v.process_mono(&mut z);
        assert!(z.iter().all(|s| *s == 0.0));
        for a in [0.0, 0.4, 1.0] {
            for b in [0.0, 1.0] {
                for d in [0.0, 1.0] {
                    let mut v = Vowel::new(SR);
                    v.set([a, b, 1.0, d, 0.0]);
                    let mut x: Vec<f32> = (0..4_800)
                        .map(|i| if i % 97 < 48 { 0.9 } else { -0.9 })
                        .collect();
                    v.process_mono(&mut x);
                    assert!(x.iter().all(|s| s.is_finite() && s.abs() < 8.0));
                }
            }
        }
    }
}
