//! Noise for the Mono voice (spec 004 Req 2), shared with the drums: white
//! from a seeded xorshift32, pink from Paul Kellet's refined filter. Same
//! seed, same samples.

/// A noise colour; the ids are mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum NoiseColour {
    #[default]
    White = 0,
    Pink = 1,
}

impl NoiseColour {
    /// Every colour with the name the TypeScript mirror uses.
    pub const ALL: [(NoiseColour, &'static str); 2] =
        [(NoiseColour::White, "White"), (NoiseColour::Pink, "Pink")];

    /// The colour for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<NoiseColour> {
        Self::ALL
            .iter()
            .find(|(c, _)| *c as u32 == id)
            .map(|(c, _)| *c)
    }
}

/// Brings pink to about the loudness of white.
const PINK_GAIN: f32 = 0.11;
/// DC blocker pole: about 19 Hz at 48 kHz. Kellet's filter passes DC with
/// a gain near 8, so without it the sub-audio wander reads as an offset.
const DC_POLE: f32 = 0.9975;

#[derive(Clone, Copy, Default)]
pub struct Noise {
    state: u32,
    /// Kellet's filter state.
    b: [f32; 7],
    /// DC blocker: last input and output.
    dc_in: f32,
    dc_out: f32,
}

impl Noise {
    /// A generator; a zero seed is replaced, since xorshift would stick at 0.
    pub fn new(seed: u32) -> Noise {
        Noise {
            state: if seed == 0 { 0x9E37_79B9 } else { seed },
            b: [0.0; 7],
            dc_in: 0.0,
            dc_out: 0.0,
        }
    }

    pub fn sample(&mut self, colour: NoiseColour) -> f32 {
        match colour {
            NoiseColour::White => self.white(),
            NoiseColour::Pink => self.pink(),
        }
    }

    /// xorshift32 white noise in −1..1.
    pub fn white(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// −3 dB per octave: white through a bank of one-pole low-passes, then
    /// a DC blocker.
    pub fn pink(&mut self) -> f32 {
        let w = self.white();
        let [b0, b1, b2, b3, b4, b5, b6] = &mut self.b;
        *b0 = 0.99886 * *b0 + w * 0.055_517_9;
        *b1 = 0.99332 * *b1 + w * 0.075_075_9;
        *b2 = 0.969 * *b2 + w * 0.153_852;
        *b3 = 0.8665 * *b3 + w * 0.310_485_6;
        *b4 = 0.55 * *b4 + w * 0.532_952_2;
        *b5 = -0.7616 * *b5 - w * 0.016_898;
        let pink = *b0 + *b1 + *b2 + *b3 + *b4 + *b5 + *b6 + w * 0.5362;
        *b6 = w * 0.115_926;
        let x = pink * PINK_GAIN;
        self.dc_out = x - self.dc_in + DC_POLE * self.dc_out;
        self.dc_in = x;
        self.dc_out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;
    const SEG: usize = 4096;

    fn render(colour: NoiseColour, n: usize) -> Vec<f32> {
        let mut noise = Noise::new(12_345);
        (0..n).map(|_| noise.sample(colour)).collect()
    }

    /// In-place radix-2 FFT of (re, im).
    fn fft(re: &mut [f64], im: &mut [f64]) {
        let n = re.len();
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j |= bit;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let a = -std::f64::consts::TAU / len as f64;
            for start in (0..n).step_by(len) {
                for k in 0..len / 2 {
                    let (s, c) = (a * k as f64).sin_cos();
                    let (p, q) = (start + k, start + k + len / 2);
                    let (tr, ti) = (re[q] * c - im[q] * s, re[q] * s + im[q] * c);
                    re[q] = re[p] - tr;
                    im[q] = im[p] - ti;
                    re[p] += tr;
                    im[p] += ti;
                }
            }
            len <<= 1;
        }
    }

    /// Mean power density per octave band from 100 Hz, in dB, by Welch's
    /// method (Hann windows, half overlap).
    fn octave_bands(x: &[f32], bands: usize) -> Vec<f64> {
        let mut psd = vec![0.0; SEG / 2];
        let hann: Vec<f64> = (0..SEG)
            .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / SEG as f64).cos())
            .collect();
        for seg in x.windows(SEG).step_by(SEG / 2) {
            let mut re: Vec<f64> = seg
                .iter()
                .zip(&hann)
                .map(|(s, w)| f64::from(*s) * w)
                .collect();
            let mut im = vec![0.0; SEG];
            fft(&mut re, &mut im);
            for (k, p) in psd.iter_mut().enumerate() {
                *p += re[k] * re[k] + im[k] * im[k];
            }
        }
        let bin = SR / SEG as f64;
        (0..bands)
            .map(|b| {
                let lo = 100.0 * f64::from(1 << b);
                let in_band: Vec<f64> = psd
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| (lo..2.0 * lo).contains(&(*k as f64 * bin)))
                    .map(|(_, p)| *p)
                    .collect();
                10.0 * (in_band.iter().sum::<f64>() / in_band.len() as f64).log10()
            })
            .collect()
    }

    /// Octaves 100 Hz..12.8 kHz, so 100 Hz to 10 kHz is covered.
    fn slope_check(colour: NoiseColour, db_per_octave: f64) {
        let bands = octave_bands(&render(colour, 10 * SR as usize), 7);
        for (b, db) in bands.iter().enumerate() {
            let expected = bands[0] + db_per_octave * b as f64;
            assert!(
                (db - expected).abs() <= 1.0,
                "{colour:?} octave {b}: {:.2} dB, expected {expected:.2}",
                db
            );
        }
    }

    #[test]
    fn white_is_flat() {
        slope_check(NoiseColour::White, 0.0);
    }

    #[test]
    fn pink_falls_3_db_per_octave() {
        slope_check(NoiseColour::Pink, -3.0);
    }

    #[test]
    fn same_seed_same_noise() {
        for colour in [NoiseColour::White, NoiseColour::Pink] {
            assert_eq!(render(colour, 1_000), render(colour, 1_000));
            let mut other = Noise::new(54_321);
            let differs = render(colour, 1_000)
                .iter()
                .any(|s| *s != other.sample(colour));
            assert!(differs, "{colour:?}: another seed gives other noise");
        }
    }

    #[test]
    fn no_dc_and_bounded() {
        for colour in [NoiseColour::White, NoiseColour::Pink] {
            let x = render(colour, 10 * SR as usize);
            let mean = x.iter().map(|s| f64::from(*s)).sum::<f64>() / x.len() as f64;
            assert!(mean.abs() < 0.01, "{colour:?}: DC {mean}");
            assert!(
                x.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
                "{colour:?}"
            );
        }
    }

    #[test]
    fn zero_seed_still_makes_noise() {
        let mut n = Noise::new(0);
        assert!((0..16).any(|_| n.white() != 0.0));
    }

    #[test]
    fn typescript_mirror_matches() {
        let ts = include_str!("../../../../web/src/audio/params.ts");
        for (c, name) in NoiseColour::ALL {
            let line = format!("{name}: {},", c as u32);
            assert!(ts.contains(&line), "web/src/audio/params.ts lacks `{line}`");
        }
        assert_eq!(NoiseColour::from_id(7), None);
    }
}
