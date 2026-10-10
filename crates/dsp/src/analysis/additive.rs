//! Additive resynthesis (spec 009 Req 4): a sine bank driven by the partial
//! tracks. Frequency and amplitude go in straight lines between frames, and
//! each track fades in over the hop before its first frame and out over the
//! hop after its last.
//!
//! The phase is the integral of the frequency. Locked, it is also pulled to
//! the measured phase at every frame by a constant frequency correction
//! across the hop (after McAulay and Quatieri 1986), so the partials keep
//! their phase relations and the waveform its shape: needed to subtract the
//! sines from the original. A shift, a stretch or a reduction to breakpoints
//! makes the measured phases meaningless, so those resynthesise unlocked.

use super::track::Track;

/// The tracks as mono PCM at `rate`, frame `i` at sample `i * hop`,
/// `frames * hop` samples long. `hop` may differ from the analysis hop: that
/// stretches time and leaves pitch alone. `lock` locks the phase to the
/// tracks' phases. A partial at or above Nyquist is silent.
pub fn resynthesise(tracks: &[Track], frames: usize, hop: f32, rate: f32, lock: bool) -> Vec<f32> {
    if frames == 0 || !(hop.is_finite() && hop > 0.0 && rate.is_finite() && rate > 0.0) {
        return Vec::new();
    }
    let len = (frames as f32 * hop).ceil() as usize;
    let mut out = vec![0.0f32; len];
    let nyquist = 0.5 * rate;
    let step = std::f64::consts::TAU / f64::from(rate);
    for t in tracks {
        let (Some(&f0), Some(&p0)) = (t.freq.first(), t.phase.first()) else {
            continue;
        };
        // Breakpoints (frequency, amplitude, measured phase): silent one hop
        // either side, so a track never clicks.
        let points = std::iter::once((f0, 0.0, None))
            .chain(
                t.freq
                    .iter()
                    .zip(&t.amp)
                    .zip(&t.phase)
                    .map(|((&f, &a), &p)| (f, a, Some(p))),
            )
            .chain(t.freq.last().map(|&f| (f, 0.0, None)));
        let first = t.start as f32 - 1.0;
        let start = (first * hop).ceil().max(0.0) as usize;
        // The phase at `start`, so the track meets its measured phase at its
        // first frame (the frequency held over the fade-in).
        let lead = (t.start as f32 * hop - start as f32).max(0.0);
        let mut phase = f64::from(p0) - step * f64::from(f0) * f64::from(lead);
        let mut prev: Option<(f32, f32, Option<f32>)> = None;
        for (i, (f, a, measured)) in points.enumerate() {
            let Some((pf, pa, from_measured)) = prev.replace((f, a, measured)) else {
                continue;
            };
            // From breakpoint i−1 to i: samples in [from, to).
            let at = first + i as f32;
            let from = (((at - 1.0) * hop).ceil().max(0.0) as usize).max(start);
            let to = ((at * hop).ceil().max(0.0) as usize).min(len);
            let ramp = |s: usize| (s as f32 / hop - (at - 1.0)).clamp(0.0, 1.0);
            // Between two measured frames, the correction that lands the
            // integrated phase on the measured one (the nearest turn of it).
            let mut correction = 0.0;
            if let (true, Some(_), Some(target)) = (lock, from_measured, measured)
                && to > from
            {
                let advance: f64 = (from..to)
                    .map(|s| step * f64::from(pf + (f - pf) * ramp(s)))
                    .sum();
                let end = phase + advance;
                let turns = ((end - f64::from(target)) / std::f64::consts::TAU).round();
                let goal = f64::from(target) + turns * std::f64::consts::TAU;
                correction = (goal - end) / (to - from) as f64;
            }
            for (s, y) in out.iter_mut().enumerate().take(to).skip(from) {
                let x = ramp(s);
                let freq = pf + (f - pf) * x;
                let amp = if freq < nyquist {
                    pa + (a - pa) * x
                } else {
                    0.0
                };
                *y += amp * phase.cos() as f32;
                phase += step * f64::from(freq) + correction;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::harmonic::harmonics;
    use crate::analysis::{Settings, analyse};
    use crate::mono::osc::{Blep, Osc, Waveform};
    use crate::voice::sine_table;

    const RATE: f32 = 48_000.0;

    fn saw(hz: f32, n: usize) -> Vec<f32> {
        let (blep, table) = (Blep::new(), sine_table());
        let mut osc = Osc::default();
        osc.wave = Waveform::Saw;
        osc.set_increment(hz / RATE);
        (0..n)
            .map(|_| 0.5 * osc.step(&blep, &table, 0.5, None).0)
            .collect()
    }

    fn steady(start: usize, frames: usize, hz: f32, amp: f32) -> Track {
        Track {
            start,
            freq: vec![hz; frames],
            amp: vec![amp; frames],
            phase: vec![0.0; frames],
        }
    }

    #[test]
    fn round_trip_keeps_the_partials() {
        let s = Settings::default();
        let a = analyse(&saw(220.0, 48_000), RATE, &s).unwrap();
        let y = resynthesise(&a.tracks, a.frames(), a.hop as f32, RATE, true);
        let b = analyse(&y, RATE, &s).unwrap();
        let (ha, hb) = (harmonics(&a, 16), harmonics(&b, 16));
        let mid = ha.f0.len() / 2;
        assert!((hb.f0[mid] - 220.0).abs() < 1.0, "{}", hb.f0[mid]);
        for (k, (x, y)) in ha.frame(mid).iter().zip(hb.frame(mid)).enumerate() {
            let db = 20.0 * (y / x).log10();
            assert!(db.abs() < 1.0, "harmonic {}: {db} dB", k + 1);
        }
    }

    #[test]
    fn output_is_finite_and_bounded() {
        let x = saw(110.0, 48_000);
        let a = analyse(&x, RATE, &Settings::default()).unwrap();
        let y = resynthesise(&a.tracks, a.frames(), a.hop as f32, RATE, true);
        assert!(y.len() >= x.len());
        let peak = |v: &[f32]| v.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(y.iter().all(|s| s.is_finite()));
        assert!(peak(&y) < 1.5 * peak(&x), "{} vs {}", peak(&y), peak(&x));
    }

    #[test]
    fn locked_resynthesis_follows_the_waveform() {
        let x = saw(110.0, 48_000);
        let a = analyse(&x, RATE, &Settings::default()).unwrap();
        let y = resynthesise(&a.tracks, a.frames(), a.hop as f32, RATE, true);
        // Away from the ends, where the window hangs over the signal.
        let inner = 4_096..44_000;
        let rms = |v: &mut dyn Iterator<Item = f32>| {
            let (sum, n) = v.fold((0.0, 0), |(s, n), e| (s + e * e, n + 1));
            (sum / n as f32).sqrt()
        };
        let err = rms(&mut x[inner.clone()]
            .iter()
            .zip(&y[inner.clone()])
            .map(|(a, b)| a - b));
        let level = rms(&mut x[inner].iter().copied());
        assert!(err < 0.01 * level, "error {err} against {level}");
    }

    #[test]
    fn tracks_fade_in_and_out() {
        let hop = 256.0;
        let y = resynthesise(&[steady(10, 10, 1_000.0, 0.5)], 40, hop, RATE, false);
        // Silent before frame 9 and after frame 20, and no step anywhere.
        assert!(y[..9 * 256].iter().all(|s| *s == 0.0));
        assert!(y[20 * 256 + 1..].iter().all(|s| *s == 0.0));
        let biggest_step = y.windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        // A 1 kHz sine at 0.5 moves at most 2π·1000/48000·0.5 ≈ 0.065 a sample.
        assert!(biggest_step < 0.07, "{biggest_step}");
        let peak = y[12 * 256..18 * 256]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
    }

    #[test]
    fn above_nyquist_is_silent_and_nothing_is_nothing() {
        let y = resynthesise(&[steady(0, 5, 30_000.0, 0.5)], 5, 256.0, RATE, false);
        assert!(y.iter().all(|s| *s == 0.0));
        assert!(resynthesise(&[], 0, 256.0, RATE, false).is_empty());
        assert!(resynthesise(&[steady(0, 5, 100.0, 0.5)], 5, 0.0, RATE, false).is_empty());
    }
}
