//! Morphing two sounds (spec 010 Req 5-7), after Loris (Fitz, Haken et al.,
//! CMJ 27(3), 2003; Tellman, Haken & Holloway, JAES 1995), written from the
//! papers, not from Loris's code: each sound is distilled to one partial per
//! harmonic number, both are aligned on their onset, peak and release, and
//! every pair of partials is interpolated with separate positions for pitch,
//! amplitude, noise and formant.

use std::collections::BTreeMap;

use super::Analysis;
use super::envelope::Envelope;
use super::harmonic::f0;
use super::track::Track;

/// The amplitude shape: interpolation in the log domain, nearly linear near
/// silence so a partial fades in without a jump (Loris's default).
const SHAPE: f32 = 1e-5;
/// A frame counts as sounding above this share of the loudest frame's energy (−30 dB).
const SOUNDING: f32 = 1e-3;
/// Frames under this level have their frequency drawn to the harmonic's
/// place before morphing, so near-silent noise never glides audibly (−90 dB).
const QUIET: f32 = 3.2e-5;

/// A partial in one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub freq: f32,
    pub amp: f32,
    pub noise: f32,
}

/// A sound distilled for morphing: per harmonic number one partial over the
/// frames (absent where it is silent), the partials with no number, and the
/// fundamental per frame (0 unvoiced).
#[derive(Clone, Debug, Default)]
pub struct Distilled {
    pub frames: usize,
    pub hop: usize,
    pub rate: f32,
    pub f0: Vec<f32>,
    pub labelled: BTreeMap<usize, Vec<Option<Point>>>,
    pub unlabelled: Vec<Track>,
}

/// Where between A (0) and B (1) each dimension of a morph lies.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Positions {
    pub pitch: f32,
    pub amp: f32,
    pub noise: f32,
    pub formant: f32,
}

impl Positions {
    /// Every dimension at `x`.
    pub fn all(x: f32) -> Positions {
        Positions {
            pitch: x,
            amp: x,
            noise: x,
            formant: x,
        }
    }
}

/// `tracks` (labelled, as `harmonic::label` leaves them) of an analysis of
/// `frames` frames, distilled: where partials share a number, the loudest is
/// the sinusoid and the others' energy joins it as noise:
/// `a' = sqrt(a² + o²)`, `β' = (a²·β + o²) / (a² + o²)` (Req 5).
pub fn distill(tracks: &[Track], frames: usize, hop: usize, rate: f32, f0: Vec<f32>) -> Distilled {
    let mut labelled: BTreeMap<usize, Vec<Option<Point>>> = BTreeMap::new();
    let mut unlabelled = Vec::new();
    for t in tracks {
        if t.label == 0 {
            unlabelled.push(t.clone());
            continue;
        }
        let row = labelled
            .entry(t.label)
            .or_insert_with(|| vec![None; frames]);
        for (i, (f, a)) in t.freq.iter().zip(&t.amp).enumerate() {
            let Some(slot) = row.get_mut(t.start + i) else {
                continue;
            };
            let p = Point {
                freq: *f,
                amp: *a,
                noise: t.noise_at(i),
            };
            *slot = Some(match *slot {
                None => p,
                Some(have) => {
                    // The louder is the sinusoid; the other is noise around it.
                    let (s, o) = if have.amp >= p.amp {
                        (have, p)
                    } else {
                        (p, have)
                    };
                    let (e, extra) = (s.amp * s.amp, o.amp * o.amp);
                    let total = e + extra;
                    Point {
                        freq: s.freq,
                        amp: total.sqrt(),
                        noise: if total > 0.0 {
                            (e * s.noise + extra) / total
                        } else {
                            0.0
                        },
                    }
                }
            });
        }
    }
    Distilled {
        frames,
        hop,
        rate,
        f0,
        labelled,
        unlabelled,
    }
}

/// An analysis and its (perhaps transformed) tracks, distilled; the
/// fundamental per frame from the analysis's peaks.
pub fn from_analysis(a: &Analysis, tracks: &[Track]) -> Distilled {
    let f0s = a.peaks.iter().map(|p| f0(p).unwrap_or(0.0)).collect();
    distill(tracks, a.frames(), a.hop, a.rate, f0s)
}

/// The frames of a sound's onset, peak and release: where its energy first
/// rises over −30 dB of its loudest frame, the loudest frame, and where it
/// last falls under that (Req 6).
pub fn features(d: &Distilled) -> [f32; 3] {
    let mut energy = vec![0.0f32; d.frames];
    for row in d.labelled.values() {
        for (e, p) in energy.iter_mut().zip(row) {
            if let Some(p) = p {
                *e += p.amp * p.amp;
            }
        }
    }
    for t in &d.unlabelled {
        for (i, a) in t.amp.iter().enumerate() {
            if let Some(e) = energy.get_mut(t.start + i) {
                *e += a * a;
            }
        }
    }
    let (peak, top) =
        energy.iter().enumerate().fold(
            (0, 0.0f32),
            |best, (i, e)| if *e > best.1 { (i, *e) } else { best },
        );
    let floor = top * SOUNDING;
    let onset = energy
        .iter()
        .position(|e| *e > floor)
        .unwrap_or(0)
        .min(peak);
    let release = energy
        .iter()
        .rposition(|e| *e > floor)
        .unwrap_or(peak)
        .max(peak);
    [onset as f32, peak as f32, release as f32]
}

/// A sound's frame for shared time `t`, its features `own` laid onto the
/// `shared` ones piecewise-linearly: scaled before the first, shifted after the last.
fn source_frame(t: f32, own: [f32; 3], shared: [f32; 3]) -> f32 {
    let [s0, s1, s2] = shared;
    let [o0, o1, o2] = own;
    let between = |t: f32, a: f32, b: f32, x: f32, y: f32| {
        if b - a > 1e-6 {
            x + (t - a) * (y - x) / (b - a)
        } else {
            x
        }
    };
    if t <= s0 {
        if s0 > 1e-6 { t * o0 / s0 } else { t - s0 + o0 }
    } else if t <= s1 {
        between(t, s0, s1, o0, o1)
    } else if t <= s2 {
        between(t, s1, s2, o1, o2)
    } else {
        t - s2 + o2
    }
}

/// Partial `label` of `d` at a fractional frame, interpolated between the frames around it.
fn sample(d: &Distilled, label: usize, frame: f32) -> Option<Point> {
    let row = d.labelled.get(&label)?;
    if frame < 0.0 {
        return None;
    }
    let i = frame as usize;
    let x = frame - i as f32;
    match (
        row.get(i).copied().flatten(),
        row.get(i + 1).copied().flatten(),
    ) {
        (Some(a), Some(b)) => Some(Point {
            freq: a.freq + (b.freq - a.freq) * x,
            amp: a.amp + (b.amp - a.amp) * x,
            noise: a.noise + (b.noise - a.noise) * x,
        }),
        (Some(a), None) if x < 0.5 => Some(a),
        (None, Some(b)) if x >= 0.5 => Some(b),
        _ => None,
    }
}

/// The fundamental of `d` at a fractional frame, 0 when unvoiced.
fn f0_at(d: &Distilled, frame: f32) -> f32 {
    d.f0.get(frame.max(0.0).round() as usize)
        .copied()
        .unwrap_or(0.0)
}

/// Interpolation in the log domain, `(x+s)·((y+s)/(x+s))^α − s`, never under 0.
pub fn log_lerp(x: f32, y: f32, alpha: f32) -> f32 {
    ((x + SHAPE) * ((y + SHAPE) / (x + SHAPE)).powf(alpha) - SHAPE).max(0.0)
}

/// Morph `a` and `b` (at the same hop and rate) at `pos`: their tracks, and
/// how many frames they span. `envs` are their spectral envelopes, for the
/// formant position; without them it has no effect (Req 7).
pub fn morph(
    a: &Distilled,
    b: &Distilled,
    pos: Positions,
    envs: Option<(&Envelope, &Envelope)>,
) -> (Vec<Track>, usize) {
    let p = Positions {
        pitch: pos.pitch.clamp(0.0, 1.0),
        amp: pos.amp.clamp(0.0, 1.0),
        noise: pos.noise.clamp(0.0, 1.0),
        formant: pos.formant.clamp(0.0, 1.0),
    };
    let (fa, fb) = (features(a), features(b));
    let shared = [0, 1, 2].map(|i| 0.5 * (fa.get(i).unwrap_or(&0.0) + fb.get(i).unwrap_or(&0.0)));
    // How long each sound lasts on the shared time: past its release it runs on unwarped.
    let span = |d: &Distilled, own: [f32; 3]| (shared[2] + (d.frames as f32 - own[2])).max(0.0);
    let frames = span(a, fa).max(span(b, fb)).ceil() as usize;
    let labels: Vec<usize> = a
        .labelled
        .keys()
        .chain(b.labelled.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut out = Vec::new();
    for label in labels {
        let mut run: Option<Track> = None;
        for t in 0..frames {
            let (ta, tb) = (
                source_frame(t as f32, fa, shared),
                source_frame(t as f32, fb, shared),
            );
            let n = label as f32;
            // A partial one sound lacks is silent there, at its number's place
            // when that sound has a fundamental, else in place (Req 7).
            let place = |d: &Distilled, at: f32, other: Option<Point>| -> Option<Point> {
                let f = f0_at(d, at);
                match (f > 0.0, other) {
                    (true, _) => Some(Point {
                        freq: n * f,
                        amp: 0.0,
                        noise: 0.0,
                    }),
                    (false, Some(o)) => Some(Point {
                        freq: o.freq,
                        amp: 0.0,
                        noise: 0.0,
                    }),
                    (false, None) => None,
                }
            };
            let quiet = |p: Point, d: &Distilled, at: f32| {
                let f = f0_at(d, at);
                if p.amp < QUIET && f > 0.0 {
                    Point { freq: n * f, ..p }
                } else {
                    p
                }
            };
            let (pa, pb) = (sample(a, label, ta), sample(b, label, tb));
            let point = match (pa, pb) {
                (None, None) => None,
                (Some(x), Some(y)) => Some((quiet(x, a, ta), quiet(y, b, tb))),
                (Some(x), None) => place(b, tb, Some(x)).map(|y| (x, y)),
                (None, Some(y)) => place(a, ta, Some(y)).map(|x| (x, y)),
            };
            let Some((x, y)) = point else {
                if let Some(done) = run.take() {
                    out.push(done);
                }
                continue;
            };
            let freq = x.freq + (y.freq - x.freq) * p.pitch;
            let mut amp = log_lerp(x.amp, y.amp, p.amp);
            let noise = log_lerp(x.noise, y.noise, p.noise).min(1.0);
            if let Some((ea, eb)) = envs {
                let (la, lb) = (
                    ea.at(ta.round() as usize, freq).max(1e-9).ln(),
                    eb.at(tb.round() as usize, freq).max(1e-9).ln(),
                );
                // The formant's envelope over the one the amplitude morph gave.
                let gain = ((lb - la) * (p.formant - p.amp)).exp().clamp(0.01, 100.0);
                amp *= gain;
            }
            let track = run.get_or_insert_with(|| Track {
                start: t,
                label,
                ..Track::default()
            });
            track.freq.push(freq);
            track.amp.push(amp);
            track.phase.push(0.0);
            track.noise.push(noise);
        }
        if let Some(done) = run {
            out.push(done);
        }
    }
    // Partials with no number crossfade, each on its own sound's time.
    for (d, own, gain) in [(a, fa, 1.0 - p.amp), (b, fb, p.amp)] {
        if gain <= 0.0 {
            continue;
        }
        for t in &d.unlabelled {
            let mut moved = t.clone();
            // Its start on the shared time.
            let start = shared_frame(t.start as f32, own, shared).round().max(0.0) as usize;
            moved.start = start;
            moved.amp.iter_mut().for_each(|x| *x *= gain);
            out.push(moved);
        }
    }
    (out, frames)
}

/// The shared time of a sound's own frame `f`: `source_frame` the other way round.
fn shared_frame(f: f32, own: [f32; 3], shared: [f32; 3]) -> f32 {
    source_frame(f, shared, own)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A steady harmonic set: `n` harmonics of `f0` at `amp(k)`, `frames`
    /// long, its first and last frames quieter so it has an onset and a release.
    fn steady(
        f0: f32,
        harmonics: &[usize],
        amp: impl Fn(usize) -> f32,
        frames: usize,
    ) -> Distilled {
        let env = |i: usize| if i == 0 || i + 1 == frames { 0.01 } else { 1.0 };
        let tracks: Vec<Track> = harmonics
            .iter()
            .map(|&k| Track {
                start: 0,
                freq: vec![f0 * k as f32; frames],
                amp: (0..frames).map(|i| amp(k) * env(i)).collect(),
                phase: vec![0.0; frames],
                noise: vec![],
                label: k,
            })
            .collect();
        distill(&tracks, frames, 256, 48_000.0, vec![f0; frames])
    }

    fn at(tracks: &[Track], label: usize, frame: usize) -> Option<(f32, f32)> {
        tracks.iter().filter(|t| t.label == label).find_map(|t| {
            frame
                .checked_sub(t.start)
                .and_then(|i| Some((*t.freq.get(i)?, *t.amp.get(i)?)))
        })
    }

    #[test]
    fn ends_are_the_sources() {
        let a = steady(220.0, &[1, 2, 3, 4, 5], |k| 0.5 / k as f32, 20);
        let b = steady(330.0, &[1, 3, 5, 7], |k| 0.4 / k as f32, 20);
        let (at0, _) = morph(&a, &b, Positions::all(0.0), None);
        let (at1, _) = morph(&a, &b, Positions::all(1.0), None);
        for k in [1, 2, 3, 4, 5] {
            let (f, amp) = at(&at0, k, 10).unwrap();
            assert!(
                (f - 220.0 * k as f32).abs() < 0.01 && (amp - 0.5 / k as f32).abs() < 1e-4,
                "A {k}"
            );
        }
        for k in [1, 3, 5, 7] {
            let (f, amp) = at(&at1, k, 10).unwrap();
            assert!(
                (f - 330.0 * k as f32).abs() < 0.01 && (amp - 0.4 / k as f32).abs() < 1e-4,
                "B {k}"
            );
        }
        // A's even harmonics are gone at B.
        assert!(at(&at1, 2, 10).is_none_or(|(_, a)| a < 1e-4));
    }

    #[test]
    fn pitch_of_one_timbre_of_the_other() {
        let saw = steady(220.0, &(1..=12).collect::<Vec<_>>(), |k| 0.5 / k as f32, 20);
        let square = steady(330.0, &[1, 3, 5, 7, 9, 11], |k| 0.5 / k as f32, 20);
        let pos = Positions {
            pitch: 0.0,
            amp: 1.0,
            noise: 1.0,
            formant: 1.0,
        };
        let (t, _) = morph(&saw, &square, pos, None);
        assert!(
            (at(&t, 1, 10).unwrap().0 - 220.0).abs() < 0.01,
            "the saw's pitch"
        );
        assert!(
            at(&t, 2, 10).is_none_or(|(_, a)| a < 1e-4),
            "the square's lack of evens"
        );
        assert!((at(&t, 3, 10).unwrap().0 - 660.0).abs() < 0.01);
    }

    #[test]
    fn orphans_glide_to_their_place() {
        let a = steady(200.0, &(1..=10).collect::<Vec<_>>(), |_| 0.1, 20);
        let b = steady(300.0, &(1..=5).collect::<Vec<_>>(), |_| 0.1, 20);
        let (t, _) = morph(&a, &b, Positions::all(0.5), None);
        let (f, amp) = at(&t, 8, 10).expect("A's 8th, which B lacks");
        assert!((f - 8.0 * 250.0).abs() < 0.01, "halfway to 8 · 300: {f}");
        assert!((amp - log_lerp(0.1, 0.0, 0.5)).abs() < 1e-6);
        assert!(amp < 0.01, "faded in the log sense: {amp}");
    }

    #[test]
    fn amplitude_morph_is_logarithmic() {
        assert!((log_lerp(0.5, 0.005, 0.5) - (0.5f32 * 0.005).sqrt()).abs() < 1e-3);
        assert_eq!(log_lerp(0.3, 0.7, 0.0), 0.3);
        assert!((log_lerp(0.3, 0.7, 1.0) - 0.7).abs() < 1e-6);
        assert_eq!(log_lerp(0.0, 0.0, 0.5), 0.0);
    }

    #[test]
    fn distilling_joins_a_broken_harmonic_and_folds_overlap_into_noise() {
        let part = |start: usize, len: usize, amp: f32| Track {
            start,
            freq: vec![1_100.0; len],
            amp: vec![amp; len],
            phase: vec![0.0; len],
            noise: vec![],
            label: 5,
        };
        let d = distill(
            &[part(0, 4, 0.2), part(6, 4, 0.2), part(2, 1, 0.1)],
            10,
            256,
            48_000.0,
            vec![220.0; 10],
        );
        let row = &d.labelled[&5];
        assert_eq!(d.labelled.len(), 1, "one channel for harmonic 5");
        assert!(row[0].is_some() && row[7].is_some() && row[4].is_none());
        let p = row[2].unwrap();
        assert!((p.amp - (0.04f32 + 0.01).sqrt()).abs() < 1e-6);
        assert!((p.noise - 0.01 / 0.05).abs() < 1e-6, "{}", p.noise);
    }

    #[test]
    fn alignment_meets_the_peaks() {
        // A peaks at frame 2, B at frame 40; shared peak at 21.
        let shaped = |peak: usize| {
            let frames = 80;
            let amp: Vec<f32> = (0..frames)
                .map(|i| {
                    if i <= peak {
                        0.01 + 0.99 * i as f32 / peak as f32
                    } else {
                        (-(i as f32 - peak as f32) / 10.0).exp()
                    }
                })
                .collect();
            let t = Track {
                start: 0,
                freq: vec![440.0; frames],
                amp,
                phase: vec![0.0; frames],
                noise: vec![],
                label: 1,
            };
            distill(&[t], frames, 256, 48_000.0, vec![440.0; frames])
        };
        let (a, b) = (shaped(2), shaped(40));
        let (fa, fb) = (features(&a), features(&b));
        assert_eq!(fa[1], 2.0);
        assert_eq!(fb[1], 40.0);
        let shared = [0, 1, 2].map(|i| 0.5 * (fa[i] + fb[i]));
        assert!((source_frame(shared[1], fa, shared) - 2.0).abs() < 1e-4);
        assert!((source_frame(shared[1], fb, shared) - 40.0).abs() < 1e-4);
        // The morph's loudest frame is at the shared peak.
        let (t, _) = morph(&a, &b, Positions::all(0.5), None);
        let loudest = t[0]
            .amp
            .iter()
            .enumerate()
            .max_by(|x, y| x.1.total_cmp(y.1))
            .unwrap()
            .0
            + t[0].start;
        assert!(
            loudest.abs_diff(shared[1].round() as usize) <= 1,
            "{loudest} vs {}",
            shared[1]
        );
    }

    #[test]
    fn unlabelled_partials_crossfade() {
        let mut a = steady(220.0, &[1], |_| 0.5, 10);
        a.unlabelled.push(Track {
            start: 0,
            freq: vec![5_000.0; 10],
            amp: vec![0.2; 10],
            phase: vec![0.0; 10],
            noise: vec![],
            label: 0,
        });
        let b = steady(220.0, &[1], |_| 0.5, 10);
        let (t, _) = morph(&a, &b, Positions::all(0.25), None);
        let un = t.iter().find(|t| t.label == 0).unwrap();
        assert!((un.amp[5] - 0.15).abs() < 1e-6);
    }
}
