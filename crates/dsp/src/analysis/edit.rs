//! Editing partial tracks before resynthesis (spec 009 Req 5, spec 010 Req 4):
//! reduction to breakpoints, a top-N cut, a pitch shift, and the transforms
//! that make a sound new, each one function with one amount. Time stretch is a
//! longer hop given to `additive::resynthesise`. The transforms follow the
//! formulas of the research behind ADR-0032, not any synth's code.

use super::envelope::Envelope;
use super::track::Track;

/// A track's value at one frame, kept where a straight line misses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breakpoint {
    pub frame: usize,
    pub freq: f32,
    pub amp: f32,
}

/// Levels under this count as silence when comparing in dB (−120 dB).
const QUIET: f32 = 1e-6;

/// A track's breakpoints: its first and last frame, and every frame where the
/// line between the breakpoints either side of it misses it by more than
/// `db` in level or `cents` in pitch.
pub fn reduce(t: &Track, db: f32, cents: f32) -> Vec<Breakpoint> {
    let point = |i: usize| -> Option<Breakpoint> {
        Some(Breakpoint {
            frame: t.start + i,
            freq: *t.freq.get(i)?,
            amp: *t.amp.get(i)?,
        })
    };
    let n = t.len();
    let Some(first) = point(0) else {
        return Vec::new();
    };
    let mut out = vec![first];
    let mut anchor = 0;
    // Grow the segment from the anchor while every frame inside it stays on
    // the line; where it can't, the frame before becomes the next anchor.
    let mut end = 2;
    while end < n {
        let fits = (anchor + 1..end).all(|i| on_line(t, anchor, end, i, db, cents));
        if fits {
            end += 1;
        } else {
            anchor = end - 1;
            out.extend(point(anchor));
            end = anchor + 2;
        }
    }
    if n > 1 {
        out.extend(point(n - 1));
    }
    out
}

fn on_line(t: &Track, a: usize, b: usize, i: usize, db: f32, cents: f32) -> bool {
    let x = (i - a) as f32 / (b - a) as f32;
    let lerp = |v: &[f32]| -> Option<(f32, f32)> {
        let (va, vb, vi) = (*v.get(a)?, *v.get(b)?, *v.get(i)?);
        Some((va + (vb - va) * x, vi))
    };
    let (Some((f, fi)), Some((amp, ai))) = (lerp(&t.freq), lerp(&t.amp)) else {
        return true;
    };
    let level = |v: f32| 20.0 * v.max(QUIET).log10();
    let pitch = 1200.0 * (fi.max(QUIET) / f.max(QUIET)).log2();
    (level(ai) - level(amp)).abs() <= db && pitch.abs() <= cents
}

/// A track from breakpoints: straight lines between them, a frame each.
/// The phase is lost (resynthesis integrates the frequency).
pub fn expand(points: &[Breakpoint]) -> Track {
    let mut t = Track {
        start: points.first().map_or(0, |p| p.frame),
        ..Track::default()
    };
    if let Some(p) = points.first() {
        t.freq.push(p.freq);
        t.amp.push(p.amp);
    }
    for w in points.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else {
            continue;
        };
        let span = b.frame.saturating_sub(a.frame).max(1);
        for s in 1..=span {
            let x = s as f32 / span as f32;
            t.freq.push(a.freq + (b.freq - a.freq) * x);
            t.amp.push(a.amp + (b.amp - a.amp) * x);
        }
    }
    t.phase = vec![0.0; t.freq.len()];
    t
}

/// Keep the `n` tracks with the most energy, in the order they started.
pub fn top_n(tracks: &mut Vec<Track>, n: usize) {
    if tracks.len() <= n {
        return;
    }
    let energy = |t: &Track| t.amp.iter().map(|a| a * a).sum::<f32>();
    let mut order: Vec<(f32, usize)> = tracks.iter().map(energy).zip(0..).collect();
    order.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    let mut keep = vec![false; tracks.len()];
    for &(_, i) in order.iter().take(n) {
        if let Some(k) = keep.get_mut(i) {
            *k = true;
        }
    }
    let mut it = keep.iter();
    tracks.retain(|_| it.next().copied().unwrap_or(false));
}

/// Scale every frequency by `ratio` (a fifth up is 1.5).
pub fn shift(tracks: &mut [Track], ratio: f32) {
    if !(ratio.is_finite() && ratio > 0.0) {
        return;
    }
    for t in tracks {
        t.freq.iter_mut().for_each(|f| *f *= ratio);
    }
}

/// The frames every track spans together: one past the last.
fn span(tracks: &[Track]) -> usize {
    tracks.iter().map(|t| t.start + t.len()).max().unwrap_or(0)
}

/// Harmonic stretch: labelled partial n moves to `((n − 1)·s + 1)` times the
/// fundamental, so 1 stays, the series spreads (s > 1) or squeezes (s < 1).
pub fn stretch(tracks: &mut [Track], s: f32) {
    if !(s.is_finite() && s > 0.0) {
        return;
    }
    for t in tracks.iter_mut().filter(|t| t.label > 0) {
        let n = t.label as f32;
        let ratio = ((n - 1.0) * s + 1.0) / n;
        t.freq.iter_mut().for_each(|f| *f *= ratio);
    }
}

/// Inharmonic stretch: labelled partial n moves by `s^(log2 n / log2 N)`, N the
/// highest label, so the higher a partial, the further it moves: a string
/// becomes a bell.
pub fn inharmonic(tracks: &mut [Track], s: f32) {
    let top = tracks.iter().map(|t| t.label).max().unwrap_or(0);
    if !(s.is_finite() && s > 0.0) || top < 2 {
        return;
    }
    let log_top = (top as f32).log2();
    for t in tracks.iter_mut().filter(|t| t.label > 1) {
        let ratio = s.powf((t.label as f32).log2() / log_top);
        t.freq.iter_mut().for_each(|f| *f *= ratio);
    }
}

/// Frequency shift: every partial moves by `hz`, which breaks the harmonic
/// series; a partial pushed to or below 0 Hz falls silent.
pub fn freq_shift(tracks: &mut [Track], hz: f32) {
    if !hz.is_finite() {
        return;
    }
    for t in tracks.iter_mut() {
        for (f, a) in t.freq.iter_mut().zip(t.amp.iter_mut()) {
            *f += hz;
            if *f <= 0.0 {
                *f = 1.0;
                *a = 0.0;
            }
        }
    }
}

/// Formant scale: each partial takes the envelope's level at its frequency over
/// `r`, so the formants move by `r` and the pitch stays. `env` describes the
/// partials as they are, so a formant-preserving pitch shift by `s` scales
/// the formants by `1/s` first and shifts after.
pub fn formant_scale(tracks: &mut [Track], env: &Envelope, r: f32) {
    if !(r.is_finite() && r > 0.0) {
        return;
    }
    for t in tracks.iter_mut() {
        for (i, (f, a)) in t.freq.iter().zip(t.amp.iter_mut()).enumerate() {
            let frame = t.start + i;
            let here = env.at(frame, *f).max(1e-9);
            // At most 40 dB either way, so an envelope's floor never blows up a partial.
            *a *= (env.at(frame, f / r) / here).clamp(0.01, 100.0);
        }
    }
}

/// Smear: in each frame, every partial's level leaks into the next one up in
/// frequency, `a ← (1 − m)·a + m·a_below`, then the frame is scaled back to its
/// energy, so the sound blurs without getting louder.
pub fn smear(tracks: &mut [Track], m: f32) {
    let m = if m.is_finite() {
        m.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if m == 0.0 {
        return;
    }
    let mut alive: Vec<(f32, usize, usize)> = Vec::new();
    for frame in 0..span(tracks) {
        alive.clear();
        for (k, t) in tracks.iter().enumerate() {
            if let Some(i) = frame.checked_sub(t.start).filter(|i| *i < t.len()) {
                alive.push((t.freq.get(i).copied().unwrap_or(0.0), k, i));
            }
        }
        alive.sort_by(|a, b| a.0.total_cmp(&b.0));
        let level = |k: usize, i: usize| {
            tracks
                .get(k)
                .and_then(|t| t.amp.get(i))
                .copied()
                .unwrap_or(0.0)
        };
        let before: f32 = alive.iter().map(|&(_, k, i)| level(k, i).powi(2)).sum();
        let mut below = 0.0;
        let mut after = 0.0;
        let mut new = Vec::with_capacity(alive.len());
        for &(_, k, i) in &alive {
            let a = (1.0 - m) * level(k, i) + m * below;
            below = a;
            after += a * a;
            new.push(a);
        }
        let gain = if after > 0.0 {
            (before / after).sqrt()
        } else {
            1.0
        };
        for (&(_, k, i), a) in alive.iter().zip(new) {
            if let Some(v) = tracks.get_mut(k).and_then(|t| t.amp.get_mut(i)) {
                *v = a * gain;
            }
        }
    }
}

/// Odd and even: `b` 0.5 leaves both; towards 0 the even harmonics fade (a
/// hollow, clarinet-like sound), towards 1 the odd ones.
pub fn odd_even(tracks: &mut [Track], b: f32) {
    let b = if b.is_finite() {
        b.clamp(0.0, 1.0)
    } else {
        0.5
    };
    let (odd, even) = ((2.0 * (1.0 - b)).min(1.0), (2.0 * b).min(1.0));
    for t in tracks.iter_mut().filter(|t| t.label > 0) {
        let g = if t.label % 2 == 1 { odd } else { even };
        t.amp.iter_mut().for_each(|a| *a *= g);
    }
}

/// A two-pole low-pass (or high-pass) on the partials themselves: each partial
/// scaled by the filter's magnitude at its frequency, `resonance` 0 to 1
/// taking Q from 0.707 to 10.
pub fn spectral_filter(tracks: &mut [Track], cutoff: f32, resonance: f32, high: bool) {
    if !(cutoff.is_finite() && cutoff > 0.0) {
        return;
    }
    let q = 0.707 * (10.0f32 / 0.707).powf(resonance.clamp(0.0, 1.0));
    for t in tracks.iter_mut() {
        for (f, a) in t.freq.iter().zip(t.amp.iter_mut()) {
            let x = f / cutoff;
            let denom = ((1.0 - x * x).powi(2) + (x / q).powi(2)).sqrt().max(1e-6);
            *a *= if high { x * x } else { 1.0 } / denom;
        }
    }
}

/// Freeze: the partials sounding at `frame`, held as they are there for the
/// whole sound; the rest is gone.
pub fn freeze(tracks: &[Track], frame: usize) -> Vec<Track> {
    let len = span(tracks);
    tracks
        .iter()
        .filter_map(|t| {
            let i = frame.checked_sub(t.start).filter(|i| *i < t.len())?;
            let (f, a, p) = (*t.freq.get(i)?, *t.amp.get(i)?, *t.phase.get(i)?);
            Some(Track {
                start: 0,
                freq: vec![f; len],
                amp: vec![a; len],
                phase: vec![p; len],
                noise: vec![t.noise_at(i); len],
                label: t.label,
            })
        })
        .collect()
}

/// Decay by number: each partial falls by `exp(−d·n·t)`, n its harmonic number
/// (1 when unlabelled) and t seconds from the sound's start, so the highs die
/// first (d > 0) or last (d < 0), after Harmor's Pluck.
pub fn decay_by_number(tracks: &mut [Track], d: f32, hop: usize, rate: f32) {
    if !(d.is_finite() && rate > 0.0) || d == 0.0 {
        return;
    }
    for t in tracks.iter_mut() {
        let n = t.label.max(1) as f32;
        for (i, a) in t.amp.iter_mut().enumerate() {
            let secs = ((t.start + i) * hop) as f32 / rate;
            // Never more than 40 dB up, so a negative decay cannot blow up.
            *a *= (-d * n * secs).exp().min(100.0);
        }
    }
}

/// Noise amount: every partial's noise share times `g`, within 0..=1.
pub fn noise_amount(tracks: &mut [Track], g: f32) {
    let g = if g.is_finite() { g.max(0.0) } else { 1.0 };
    for t in tracks.iter_mut() {
        t.noise.iter_mut().for_each(|b| *b = (*b * g).min(1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::additive::resynthesise;
    use crate::analysis::harmonic::harmonics;
    use crate::analysis::{Settings, analyse};

    const RATE: f32 = 48_000.0;

    fn track(freq: Vec<f32>, amp: Vec<f32>) -> Track {
        let phase = vec![0.0; freq.len()];
        Track {
            start: 5,
            freq,
            amp,
            phase,
            ..Track::default()
        }
    }

    #[test]
    fn reduction_keeps_the_shape() {
        // A straight glide is its two ends.
        let line = track((0..100).map(|i| 200.0 + i as f32).collect(), vec![0.5; 100]);
        let points = reduce(&line, 0.5, 5.0);
        assert_eq!(points.len(), 2);
        assert_eq!((points[0].frame, points[1].frame), (5, 104));
        // A rise then a fall keeps its top, and expands back within tolerance.
        let amp: Vec<f32> = (0..100)
            .map(|i| 0.01 + 0.01 * (50 - (i - 50i32).abs()) as f32)
            .collect();
        let tent = track(vec![300.0; 100], amp.clone());
        let points = reduce(&tent, 0.5, 5.0);
        assert!(
            points.iter().any(|p| p.frame.abs_diff(55) <= 1),
            "{points:?}"
        );
        assert!(points.len() < 30, "{}", points.len());
        let back = expand(&points);
        assert_eq!((back.start, back.len()), (5, 100));
        for (x, y) in back.amp.iter().zip(&amp) {
            assert!((20.0 * (x / y).log10()).abs() <= 0.5 + 1e-3, "{x} vs {y}");
        }
    }

    #[test]
    fn top_n_keeps_the_loudest() {
        let mut tracks: Vec<Track> = [0.1, 0.5, 0.2, 0.4]
            .iter()
            .map(|&a| track(vec![100.0; 10], vec![a; 10]))
            .collect();
        top_n(&mut tracks, 2);
        let amps: Vec<f32> = tracks.iter().map(|t| t.amp[0]).collect();
        assert_eq!(amps, [0.5, 0.4]);
    }

    fn tone() -> Vec<f32> {
        (0..48_000)
            .map(|i| {
                let t = std::f32::consts::TAU * 220.0 * i as f32 / RATE;
                0.4 * t.sin() + 0.2 * (2.0 * t).sin()
            })
            .collect()
    }

    fn sounding(y: &[f32]) -> usize {
        y.iter().rposition(|s| s.abs() > 0.01).unwrap_or(0)
    }

    #[test]
    fn shift_moves_pitch_not_time() {
        let mut a = analyse(&tone(), RATE, &Settings::default()).unwrap();
        shift(&mut a.tracks, 1.5);
        let y = resynthesise(&a.tracks, a.frames(), a.hop as f32, RATE, false);
        let h = harmonics(&analyse(&y, RATE, &Settings::default()).unwrap(), 4);
        assert!((h.f0[h.f0.len() / 2] - 330.0).abs() < 1.0);
        // The analysis window (4096) smears the abrupt end by half its length.
        assert!(
            (sounding(&y) as f32 - 48_000.0).abs() < 2_048.0,
            "{}",
            sounding(&y)
        );
    }

    #[test]
    fn stretch_moves_time_not_pitch() {
        let a = analyse(&tone(), RATE, &Settings::default()).unwrap();
        let y = resynthesise(&a.tracks, a.frames(), 2.0 * a.hop as f32, RATE, false);
        let h = harmonics(&analyse(&y, RATE, &Settings::default()).unwrap(), 4);
        assert!((h.f0[h.f0.len() / 2] - 220.0).abs() < 1.0);
        assert!(
            (sounding(&y) as f32 - 96_000.0).abs() < 4_096.0,
            "{}",
            sounding(&y)
        );
    }

    /// A saw at 220 Hz with its labels: harmonic n at n·220, level 1/n.
    fn labelled_saw(frames: usize) -> Vec<Track> {
        (1..=16)
            .map(|n| Track {
                start: 0,
                freq: vec![220.0 * n as f32; frames],
                amp: vec![0.5 / n as f32; frames],
                phase: vec![0.0; frames],
                noise: vec![],
                label: n,
            })
            .collect()
    }

    #[test]
    fn stretch_moves_harmonics_apart() {
        let mut t = labelled_saw(4);
        stretch(&mut t, 1.1);
        assert_eq!(t[0].freq[0], 220.0);
        assert!(
            (t[3].freq[0] - 220.0 * 4.3).abs() < 0.01,
            "{}",
            t[3].freq[0]
        );
    }

    #[test]
    fn inharmonic_moves_high_partials_most() {
        let mut t = labelled_saw(4);
        inharmonic(&mut t, 1.5);
        assert_eq!(t[0].freq[0], 220.0);
        // 1.5^(log2 8 / log2 16) = 1.5^0.75 ≈ 1.355.
        assert!(
            (t[7].freq[0] / (8.0 * 220.0) - 1.5f32.powf(0.75)).abs() < 1e-4,
            "{}",
            t[7].freq[0]
        );
        assert!((t[15].freq[0] / (16.0 * 220.0) - 1.5).abs() < 1e-4);
        let ratios: Vec<f32> = t
            .iter()
            .map(|t| t.freq[0] / (220.0 * t.label as f32))
            .collect();
        assert!(ratios.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn freq_shift_adds_hertz() {
        let mut t = labelled_saw(2);
        freq_shift(&mut t, 100.0);
        assert_eq!(t[0].freq[0], 320.0);
        assert_eq!(t[1].freq[0], 540.0);
        freq_shift(&mut t, -400.0);
        assert_eq!(t[0].amp[0], 0.0, "pushed below 0 Hz: silent");
    }

    #[test]
    fn formants_stay_when_pitch_moves() {
        use crate::analysis::envelope::true_envelope;
        use crate::analysis::harmonic::label;
        use crate::analysis::{Settings, analyse};
        // Harmonics of 110 Hz under a resonance at 1 kHz.
        let x: Vec<f32> = (0..24_000)
            .map(|i| {
                let t = i as f32 / RATE;
                (1..=60)
                    .map(|k| {
                        let f = 110.0 * k as f32;
                        let g = 1.0 / (1.0 + ((f - 1_000.0) / 250.0).powi(2));
                        0.3 * g * (std::f32::consts::TAU * f * t).sin()
                    })
                    .sum::<f32>()
            })
            .collect();
        let mut a = analyse(&x, RATE, &Settings::default()).unwrap();
        label(&mut a);
        let env = true_envelope(&a);
        let mut t = a.tracks.clone();
        // Formants down first, against the envelope of the partials as they
        // are; then everything up a fifth brings the formant back.
        formant_scale(&mut t, &env, 1.0 / 1.5);
        shift(&mut t, 1.5);
        // The loudest partial is still near 1 kHz, though every partial moved a fifth up.
        let mid = a.frames() / 2;
        let loudest = t
            .iter()
            .filter_map(|t| {
                mid.checked_sub(t.start)
                    .and_then(|i| Some((*t.freq.get(i)?, *t.amp.get(i)?)))
            })
            .max_by(|x, y| x.1.total_cmp(&y.1))
            .unwrap();
        assert!(
            (loudest.0 / 1_000.0).log2().abs() < 1.0 / 3.0,
            "{} Hz",
            loudest.0
        );
    }

    #[test]
    fn smear_keeps_energy() {
        let mut t = labelled_saw(3);
        let energy = |t: &[Track]| t.iter().map(|t| t.amp[1].powi(2)).sum::<f32>();
        let before = energy(&t);
        smear(&mut t, 0.7);
        assert!((energy(&t) - before).abs() < 1e-4);
        // The highs got some of the lows: the top's share rose.
        assert!(t[15].amp[1] > 0.5 / 16.0);
    }

    #[test]
    fn odd_even_hollows_the_sound() {
        let mut t = labelled_saw(2);
        odd_even(&mut t, 0.0);
        assert_eq!(t[1].amp[0], 0.0);
        assert_eq!(t[2].amp[0], 0.5 / 3.0);
    }

    #[test]
    fn spectral_filter_shapes_and_resonates() {
        let mut t = labelled_saw(2);
        spectral_filter(&mut t, 880.0, 0.0, false);
        assert!((t[0].amp[0] - 0.5).abs() < 0.01, "the passband stays");
        assert!(t[15].amp[0] < 0.5 / 16.0 * 0.1, "the top falls away");
        let mut r = labelled_saw(2);
        spectral_filter(&mut r, 880.0, 1.0, false);
        assert!(r[3].amp[0] > 0.5 / 4.0 * 5.0, "a resonance at the cutoff");
    }

    #[test]
    fn freeze_holds_a_frame() {
        let mut t = labelled_saw(10);
        t[0].amp[5] = 0.9;
        let f = freeze(&t, 5);
        assert_eq!(f.len(), 16);
        assert!(f[0].amp.iter().all(|a| *a == 0.9));
        assert_eq!(f[0].len(), 10);
    }

    #[test]
    fn decay_by_number_kills_the_highs_first() {
        let mut t = labelled_saw(400);
        decay_by_number(&mut t, 2.0, 256, RATE);
        let last = 399;
        assert!(t[0].amp[last] / t[0].amp[0] > t[7].amp[last] / t[7].amp[0]);
        assert!(t[7].amp[last] < t[7].amp[0] * 0.01);
    }

    #[test]
    fn nothing_passes_nyquist() {
        use crate::analysis::additive::resynthesise;
        let mut t = labelled_saw(40);
        inharmonic(&mut t, 4.0);
        let y = resynthesise(&t, 40, 256.0, RATE, false);
        assert!(y.iter().all(|s| s.is_finite()));
        // Partial 16 was sent to 14 kHz; a partial past Nyquist adds nothing.
        let mut over = labelled_saw(40);
        over.iter_mut()
            .for_each(|t| t.freq.iter_mut().for_each(|f| *f = 30_000.0));
        assert!(
            resynthesise(&over, 40, 256.0, RATE, false)
                .iter()
                .all(|s| *s == 0.0)
        );
    }

    #[test]
    fn noise_amount_scales_the_share() {
        let mut t = labelled_saw(2);
        t[0].noise = vec![0.4, 0.8];
        noise_amount(&mut t, 2.0);
        assert_eq!(t[0].noise, vec![0.8, 1.0]);
    }
}
