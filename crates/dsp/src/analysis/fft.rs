//! An in-place radix-2 FFT, std-only (ADR-0001, spec 009 Req 1). The twiddles
//! and the bit reversal are built once per size, so a transform is only
//! butterflies.

/// Smallest and largest size, both powers of two.
pub const MIN: usize = 64;
pub const MAX: usize = 16_384;

pub struct Fft {
    cos: Vec<f32>,
    sin: Vec<f32>,
    rev: Vec<u32>,
}

impl Fft {
    /// A transform of `n` points, or `None` unless `n` is a power of two in
    /// `MIN..=MAX`.
    pub fn new(n: usize) -> Option<Fft> {
        if !n.is_power_of_two() || !(MIN..=MAX).contains(&n) {
            return None;
        }
        let (cos, sin) = (0..n / 2)
            .map(|k| {
                let (s, c) = (-std::f64::consts::TAU * k as f64 / n as f64).sin_cos();
                (c as f32, s as f32)
            })
            .unzip();
        let bits = n.trailing_zeros();
        let rev = (0..n as u32)
            .map(|i| i.reverse_bits() >> (32 - bits))
            .collect();
        Some(Fft { cos, sin, rev })
    }

    pub fn len(&self) -> usize {
        self.rev.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rev.is_empty()
    }

    /// The forward transform of (re, im), in place. Both slices must be
    /// `len()` long; anything else is left as it is.
    pub fn forward(&self, re: &mut [f32], im: &mut [f32]) {
        let n = self.len();
        if re.len() != n || im.len() != n {
            return;
        }
        for (i, &j) in self.rev.iter().enumerate() {
            let j = j as usize;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut half = 1;
        while half < n {
            let step = n / (2 * half);
            for (re, im) in re
                .chunks_exact_mut(2 * half)
                .zip(im.chunks_exact_mut(2 * half))
            {
                let (rp, rq) = re.split_at_mut(half);
                let (ip, iq) = im.split_at_mut(half);
                let twiddles = self
                    .cos
                    .iter()
                    .step_by(step)
                    .zip(self.sin.iter().step_by(step));
                for ((((rp, rq), ip), iq), (c, s)) in
                    rp.iter_mut().zip(rq).zip(ip).zip(iq).zip(twiddles)
                {
                    let (tr, ti) = (*rq * c - *iq * s, *rq * s + *iq * c);
                    *rq = *rp - tr;
                    *iq = *ip - ti;
                    *rp += tr;
                    *ip += ti;
                }
            }
            half *= 2;
        }
    }

    /// The inverse transform, scaled by 1/n so it undoes `forward`.
    pub fn inverse(&self, re: &mut [f32], im: &mut [f32]) {
        im.iter_mut().for_each(|v| *v = -*v);
        self.forward(re, im);
        let scale = 1.0 / self.len() as f32;
        re.iter_mut().for_each(|v| *v *= scale);
        im.iter_mut().for_each(|v| *v *= -scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_powers_of_two_in_range() {
        assert!(Fft::new(1024).is_some());
        for n in [0, 32, 1000, 32_768] {
            assert!(Fft::new(n).is_none(), "{n}");
        }
    }

    #[test]
    fn forward_then_inverse_is_identity() {
        let fft = Fft::new(512).unwrap();
        let x: Vec<f32> = (0..512)
            .map(|i| ((i * 7919) % 101) as f32 / 50.0 - 1.0)
            .collect();
        let (mut re, mut im) = (x.clone(), vec![0.0; 512]);
        fft.forward(&mut re, &mut im);
        fft.inverse(&mut re, &mut im);
        for (a, b) in re.iter().zip(&x) {
            assert!((a - b).abs() < 1e-5, "{a} vs {b}");
        }
        assert!(im.iter().all(|v| v.abs() < 1e-5));
    }

    #[test]
    fn a_sine_peaks_in_its_bin() {
        let n = 1024;
        let fft = Fft::new(n).unwrap();
        let mut re: Vec<f32> = (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 37.0 * i as f32 / n as f32).sin())
            .collect();
        let mut im = vec![0.0; n];
        fft.forward(&mut re, &mut im);
        let mag: Vec<f32> = (0..n / 2).map(|k| re[k].hypot(im[k])).collect();
        let top = (0..n / 2)
            .max_by(|&a, &b| mag[a].total_cmp(&mag[b]))
            .unwrap();
        assert_eq!(top, 37);
        // A sine of amplitude A gives A·n/2 in its bin.
        assert!((mag[37] - 0.5 * n as f32 / 2.0).abs() < 1e-2);
    }
}
