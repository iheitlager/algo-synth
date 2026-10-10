//! Editing partial tracks before resynthesis (spec 009 Req 5): reduction to
//! breakpoints, a top-N cut and a pitch shift. Time stretch is a longer hop
//! given to `additive::resynthesise`.

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
}
