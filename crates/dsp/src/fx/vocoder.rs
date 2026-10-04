//! A channel vocoder (#161), the sound of electro: the carrier (the strip the
//! insert sits on) is shaped by the spectrum of a modulator (the key, another
//! strip's raw signal, `Param::Key`).
//!
//! Sixteen bands spaced evenly in pitch from 100 Hz to 8 kHz, each two
//! band-passes in series (4th order, so a loud low harmonic stays out of a
//! high band): one pair on the key feeds an envelope follower, which sets the
//! level of the same band of the carrier. A DC blocker follows, since the
//! envelopes ripple with the key and may correlate with the carrier. Noise added to the top bands of the carrier lets consonants
//! through ("unvoiced"). Every filter and follower is allocated with the
//! slot; coefficients are worked out when a knob moves (ADR-0002), and a
//! sample costs 64 band-passes and 16 followers.

use crate::mono::noise::Noise;

/// Bands, and the lowest and highest centre in Hz.
pub const BANDS: usize = 16;
const LOW_HZ: f32 = 100.0;
const HIGH_HZ: f32 = 8_000.0;
/// The bands that take noise for unvoiced sounds: the top four, above 2.5 kHz.
const UNVOICED_FROM: usize = 12;
/// The envelope followers' attack, in seconds.
const ATTACK: f32 = 0.003;
/// Brings the sum of the bands back to about the carrier's level.
const MAKEUP: f32 = 4.0;
/// The DC blocker's pole: about 10 Hz at 48 kHz.
const DC_POLE: f32 = 0.9987;

/// A zero-delay-feedback state-variable band-pass, unity gain at the centre.
#[derive(Clone, Copy, Debug, Default)]
struct Band {
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1: f32,
    ic2: f32,
}

impl Band {
    fn set(&mut self, hz: f32, q: f32, sr: f32) {
        let g = (std::f32::consts::PI * hz.clamp(10.0, 0.45 * sr) / sr).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    fn process(&mut self, x: f32) -> f32 {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        self.k * v1
    }
}

/// Two band-passes in series: a 4th-order band.
type Pair = [Band; 2];

fn through(pair: &mut Pair, x: f32) -> f32 {
    let [a, b] = pair;
    b.process(a.process(x))
}

/// A DC blocker: a one-pole high-pass at about 10 Hz.
#[derive(Clone, Copy, Debug, Default)]
struct DcBlock {
    x1: f32,
    y1: f32,
}

impl DcBlock {
    fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + DC_POLE * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

pub struct Vocoder {
    sr: f32,
    analysis: [Pair; BANDS],
    synthesis: [Pair; BANDS],
    /// The right side's synthesis bands, for a stereo carrier.
    synthesis_r: [Pair; BANDS],
    dc: [DcBlock; 2],
    env: [f32; BANDS],
    attack: f32,
    release: f32,
    unvoiced: f32,
    dry: f32,
    noise: Noise,
}

impl Vocoder {
    pub fn new(sample_rate: f32) -> Vocoder {
        let mut v = Vocoder {
            sr: sample_rate,
            analysis: [[Band::default(); 2]; BANDS],
            synthesis: [[Band::default(); 2]; BANDS],
            synthesis_r: [[Band::default(); 2]; BANDS],
            dc: [DcBlock::default(); 2],
            env: [0.0; BANDS],
            attack: 0.0,
            release: 0.0,
            unvoiced: 0.0,
            dry: 0.0,
            noise: Noise::new(0x5EED_0C0D),
        };
        v.set([0.5, 0.5, 0.0, 0.5, 0.0]);
        v
    }

    /// The knobs, each 0..=1: A band shift (±1 octave, 0.5 none), B release
    /// (20 ms to 500 ms), C unvoiced noise, D band width (narrow to wide),
    /// E dry (0 is all vocoder). At control rate.
    pub fn set(&mut self, knobs: [f32; 5]) {
        let [a, b, c, d, e] = knobs;
        let shift = ((a - 0.5) * 2.0).exp2();
        let q = 12.0 * (0.25_f32).powf(d);
        for (k, ((an, sy), sr)) in self
            .analysis
            .iter_mut()
            .zip(self.synthesis.iter_mut())
            .zip(self.synthesis_r.iter_mut())
            .enumerate()
        {
            let hz = centre(k);
            for b in an.iter_mut() {
                b.set(hz, q, self.sr);
            }
            for b in sy.iter_mut().chain(sr.iter_mut()) {
                b.set(hz * shift, q, self.sr);
            }
        }
        self.attack = follower(ATTACK, self.sr);
        self.release = follower(0.02 * 25.0_f32.powf(b), self.sr);
        self.unvoiced = c;
        self.dry = e;
    }

    /// Start from rest: no ringing filters, closed envelopes.
    pub fn reset(&mut self) {
        for b in self
            .analysis
            .iter_mut()
            .chain(self.synthesis.iter_mut())
            .chain(self.synthesis_r.iter_mut())
            .flatten()
        {
            b.ic1 = 0.0;
            b.ic2 = 0.0;
        }
        self.env = [0.0; BANDS];
        self.dc = [DcBlock::default(); 2];
    }

    /// Follow the key's band `k` and return its envelope.
    fn follow(&mut self, k: usize, key: f32) -> f32 {
        let (Some(band), Some(env)) = (self.analysis.get_mut(k), self.env.get_mut(k)) else {
            return 0.0;
        };
        let level = through(band, key).abs();
        let coeff = if level > *env {
            self.attack
        } else {
            self.release
        };
        *env += coeff * (level - *env);
        *env
    }

    /// Vocode a mono carrier in place by `key`; without a key the vocoder is
    /// silent and only the dry part is left.
    pub fn process_mono(&mut self, x: &mut [f32], key: Option<&[f32]>) {
        for (i, c) in x.iter_mut().enumerate() {
            let m = key.and_then(|k| k.get(i)).copied().unwrap_or(0.0);
            let noise = self.noise.white() * self.unvoiced;
            let mut wet = 0.0;
            for k in 0..BANDS {
                let env = self.follow(k, m);
                let input = if k >= UNVOICED_FROM { *c + noise } else { *c };
                if let Some(band) = self.synthesis.get_mut(k) {
                    wet += through(band, input) * env;
                }
            }
            let [dc, _] = &mut self.dc;
            *c = dc.process(MAKEUP * wet) + self.dry * *c;
        }
    }

    /// Vocode a stereo carrier: both sides follow the same key's envelopes.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32], key: Option<&[f32]>) {
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            let m = key.and_then(|k| k.get(i)).copied().unwrap_or(0.0);
            let noise = self.noise.white() * self.unvoiced;
            let (mut wl, mut wr) = (0.0, 0.0);
            for k in 0..BANDS {
                let env = self.follow(k, m);
                let extra = if k >= UNVOICED_FROM { noise } else { 0.0 };
                if let (Some(bl), Some(br)) =
                    (self.synthesis.get_mut(k), self.synthesis_r.get_mut(k))
                {
                    wl += through(bl, *l + extra) * env;
                    wr += through(br, *r + extra) * env;
                }
            }
            let [dl, dr] = &mut self.dc;
            *l = dl.process(MAKEUP * wl) + self.dry * *l;
            *r = dr.process(MAKEUP * wr) + self.dry * *r;
        }
    }
}

/// Band `k`'s centre: evenly spaced in pitch from `LOW_HZ` to `HIGH_HZ`.
fn centre(k: usize) -> f32 {
    LOW_HZ * (HIGH_HZ / LOW_HZ).powf(k as f32 / (BANDS - 1) as f32)
}

/// A one-pole follower's coefficient for a time constant of `seconds`.
fn follower(seconds: f32, sr: f32) -> f32 {
    1.0 - (-1.0 / (seconds * sr).max(1.0)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 48_000.0;

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (TAU * hz * i as f32 / SR).sin()).collect()
    }

    /// A band-limited saw: every harmonic of 110 Hz up to 10 kHz.
    fn saw(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / SR;
                (1..90)
                    .map(|h| (TAU * 110.0 * h as f32 * t).sin() / h as f32)
                    .sum::<f32>()
                    * 0.55
            })
            .collect()
    }

    fn power(x: &[f32], hz: f32) -> f32 {
        let w = TAU * hz / SR;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0_f32, 0.0_f32);
        for v in x {
            let s0 = v + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        s1 * s1 + s2 * s2 - c * s1 * s2
    }

    fn run(v: &mut Vocoder, carrier: &[f32], key: Option<&[f32]>) -> Vec<f32> {
        let mut out = carrier.to_vec();
        for (i, chunk) in out.chunks_mut(128).enumerate() {
            let k = key.map(|k| &k[i * 128..i * 128 + chunk.len()]);
            v.process_mono(chunk, k);
        }
        out
    }

    #[test]
    fn a_key_in_one_band_lets_only_that_band_of_the_carrier_through() {
        let n = 24_000;
        let carrier = saw(n);
        let band = centre(8);
        let key = sine(band, n);
        let mut v = Vocoder::new(SR);
        let out = run(&mut v, &carrier, Some(&key));
        let tail = &out[n / 2..];
        // The carrier's harmonic nearest the band, against ones far from it.
        let near = (band / 110.0).round() * 110.0;
        for far in [220.0, 440.0, 6_600.0] {
            assert!(
                power(tail, near) > 1000.0 * power(tail, far),
                "{near} Hz over {far} Hz by 30 dB"
            );
        }
    }

    #[test]
    fn a_silent_key_gives_silence_and_unvoiced_adds_only_highs() {
        let n = 12_000;
        let carrier = saw(n);
        let mut v = Vocoder::new(SR);
        let out = run(&mut v, &carrier, Some(&vec![0.0; n]));
        assert!(out.iter().all(|x| *x == 0.0));
        let mut v = Vocoder::new(SR);
        assert!(
            run(&mut v, &carrier, None).iter().all(|x| *x == 0.0),
            "no key, no sound"
        );
        // With the dry knob up, the carrier passes as it is.
        let mut v = Vocoder::new(SR);
        v.set([0.5, 0.5, 0.0, 0.5, 1.0]);
        assert_eq!(run(&mut v, &carrier, None), carrier);
    }

    #[test]
    fn full_scale_in_stays_bounded_without_dc() {
        let n = 48_000;
        let carrier = saw(n);
        let mut key = saw(n);
        // A louder key, clipped square-ish: the worst a modulator can be.
        for k in key.iter_mut() {
            *k = (*k * 4.0).clamp(-1.0, 1.0);
        }
        for knobs in [
            [0.0, 0.0, 1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0, 0.0, 1.0],
            [0.5; 5],
        ] {
            let mut v = Vocoder::new(SR);
            v.set(knobs);
            let out = run(&mut v, &carrier, Some(&key));
            let peak = out.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
            assert!(out.iter().all(|x| x.is_finite()), "{knobs:?}");
            // The dry carrier adds to the vocoder when E is up, as a drive's
            // level can: the strip's fader and the master's limiter follow.
            assert!(peak < 1.5, "{knobs:?}: peak {peak}");
            let mean = out.iter().sum::<f32>() / n as f32;
            assert!(mean.abs() < 1.0e-2, "{knobs:?}: DC {mean}");
        }
    }

    #[test]
    fn a_stereo_carrier_follows_the_same_key_on_both_sides() {
        let n = 12_000;
        let carrier = saw(n);
        let key = sine(centre(6), n);
        let mut mono = Vocoder::new(SR);
        let want = run(&mut mono, &carrier, Some(&key));
        let mut v = Vocoder::new(SR);
        let (mut l, mut r) = (carrier.clone(), carrier.clone());
        for (i, (cl, cr)) in l.chunks_mut(128).zip(r.chunks_mut(128)).enumerate() {
            v.process_stereo(cl, cr, Some(&key[i * 128..i * 128 + cl.len()]));
        }
        assert_eq!(l, want);
        assert_eq!(r, want);
    }
}
