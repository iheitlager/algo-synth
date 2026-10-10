//! Partial tracking after McAulay and Quatieri (1986), spec 009 Req 2: each
//! frame's peaks continue the nearest live tracks, a peak left over starts
//! one, and a track without a peak for more than a few frames ends.

use super::peaks::Peak;

/// One partial: its frequency, amplitude and phase per frame from `start`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub start: usize,
    pub freq: Vec<f32>,
    pub amp: Vec<f32>,
    pub phase: Vec<f32>,
}

impl Track {
    pub fn len(&self) -> usize {
        self.freq.len()
    }

    pub fn is_empty(&self) -> bool {
        self.freq.is_empty()
    }

    fn push(&mut self, p: Peak) {
        self.freq.push(p.freq);
        self.amp.push(p.amp);
        self.phase.push(p.phase);
    }

    fn last(&self) -> Peak {
        Peak {
            freq: self.freq.last().copied().unwrap_or(0.0),
            amp: self.amp.last().copied().unwrap_or(0.0),
            phase: self.phase.last().copied().unwrap_or(0.0),
        }
    }
}

/// A track still taking peaks, and how many frames it has gone without.
struct Live {
    track: usize,
    freq: f32,
    missing: usize,
}

pub struct Tracker {
    tracks: Vec<Track>,
    live: Vec<Live>,
    /// The widest step a track takes between frames, as a frequency ratio.
    ratio: f32,
    gap: usize,
    pairs: Vec<(f32, usize, usize)>,
    live_taken: Vec<bool>,
    peak_taken: Vec<bool>,
}

impl Tracker {
    /// A tracker that joins peaks within `cents` of a track and lets a track
    /// miss up to `gap` frames.
    pub fn new(cents: f32, gap: usize) -> Tracker {
        Tracker {
            tracks: Vec::new(),
            live: Vec::new(),
            ratio: (cents.max(0.0) / 1200.0).exp2(),
            gap,
            pairs: Vec::new(),
            live_taken: Vec::new(),
            peak_taken: Vec::new(),
        }
    }

    /// Frame `frame`'s peaks, sorted by frequency.
    pub fn push(&mut self, frame: usize, peaks: &[Peak]) {
        // Every pair of a live track and a peak close enough, nearest first;
        // each track and each peak is taken once.
        self.pairs.clear();
        for (l, live) in self.live.iter().enumerate() {
            let lo = peaks.partition_point(|p| p.freq < live.freq / self.ratio);
            let hi = peaks.partition_point(|p| p.freq <= live.freq * self.ratio);
            for (i, p) in peaks.iter().enumerate().take(hi).skip(lo) {
                self.pairs.push(((p.freq / live.freq).ln().abs(), l, i));
            }
        }
        self.pairs.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        self.live_taken.clear();
        self.live_taken.resize(self.live.len(), false);
        self.peak_taken.clear();
        self.peak_taken.resize(peaks.len(), false);
        for &(_, l, i) in &self.pairs {
            let (Some(lt), Some(pt)) = (self.live_taken.get(l), self.peak_taken.get(i)) else {
                continue;
            };
            if *lt || *pt {
                continue;
            }
            let (Some(live), Some(&p)) = (self.live.get_mut(l), peaks.get(i)) else {
                continue;
            };
            if let Some(track) = self.tracks.get_mut(live.track) {
                // Bridge the frames it missed with a straight line.
                let from = track.last();
                let steps = live.missing + 1;
                for s in 1..steps {
                    let t = s as f32 / steps as f32;
                    track.push(Peak {
                        freq: from.freq + (p.freq - from.freq) * t,
                        amp: from.amp + (p.amp - from.amp) * t,
                        phase: from.phase,
                    });
                }
                track.push(p);
            }
            live.freq = p.freq;
            live.missing = 0;
            if let Some(v) = self.live_taken.get_mut(l) {
                *v = true;
            }
            if let Some(v) = self.peak_taken.get_mut(i) {
                *v = true;
            }
        }
        for (live, taken) in self.live.iter_mut().zip(&self.live_taken) {
            if !taken {
                live.missing += 1;
            }
        }
        let gap = self.gap;
        self.live.retain(|l| l.missing <= gap);
        for (p, taken) in peaks.iter().zip(&self.peak_taken) {
            if !taken {
                let mut track = Track {
                    start: frame,
                    ..Track::default()
                };
                track.push(*p);
                self.live.push(Live {
                    track: self.tracks.len(),
                    freq: p.freq,
                    missing: 0,
                });
                self.tracks.push(track);
            }
        }
    }

    /// Every track, in the order they started.
    pub fn finish(self) -> Vec<Track> {
        self.tracks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(freq: f32) -> Peak {
        Peak {
            freq,
            amp: 0.5,
            phase: 0.0,
        }
    }

    #[test]
    fn nearest_peaks_continue_tracks() {
        let mut t = Tracker::new(50.0, 3);
        t.push(0, &[peak(200.0), peak(400.0)]);
        t.push(1, &[peak(202.0), peak(398.0)]);
        // 300 Hz is no track's: it starts one.
        t.push(2, &[peak(203.0), peak(300.0), peak(397.0)]);
        let tracks = t.finish();
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[0].freq, [200.0, 202.0, 203.0]);
        assert_eq!(tracks[1].freq, [400.0, 398.0, 397.0]);
        assert_eq!((tracks[2].start, tracks[2].len()), (2, 1));
    }

    #[test]
    fn a_short_gap_is_bridged_and_a_long_one_ends_the_track() {
        let mut t = Tracker::new(50.0, 2);
        t.push(0, &[peak(100.0)]);
        t.push(1, &[]);
        t.push(2, &[]);
        t.push(3, &[peak(102.0)]);
        for f in 4..8 {
            t.push(f, &[]);
        }
        t.push(8, &[peak(102.0)]);
        let tracks = t.finish();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].len(), 4);
        assert!((tracks[0].freq[1] - 100.667).abs() < 1e-3 && tracks[0].freq[3] == 102.0);
        assert_eq!(tracks[1].start, 8);
    }

    #[test]
    fn one_peak_continues_one_track() {
        let mut t = Tracker::new(100.0, 0);
        t.push(0, &[peak(440.0), peak(450.0)]);
        t.push(1, &[peak(446.0)]);
        let tracks = t.finish();
        // 446 is nearer 450 (in cents) than 440: only that track goes on.
        assert_eq!(tracks[1].freq, [450.0, 446.0]);
        assert_eq!(tracks[0].len(), 1);
    }
}
