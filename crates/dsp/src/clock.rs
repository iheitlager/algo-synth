//! The clock: tempo, swing and a transport over sixteenth-note steps, run
//! inside `render` (spec 002 Req 5, ADR-0005).
//!
//! A step's sample is computed from its index against an anchor, never by
//! adding step lengths, so a long run does not drift. A tempo change
//! re-anchors at the next step, whose place on the grid stays put; later steps
//! follow the new tempo. Swing only moves the off-beats, from the next one on. Tempo and swing belong to the song (ADR-0012), not to the
//! parameter registry.

/// Steps per quarter note: the clock counts sixteenths.
pub const STEPS_PER_BEAT: u64 = 4;
/// Tempo range, in BPM.
pub const TEMPO: (f32, f32) = (20.0, 300.0);
/// Swing range, in percent: 50 is straight, about 67 a triplet feel.
pub const SWING: (f32, f32) = (50.0, 75.0);

#[derive(Clone, Debug)]
pub struct Clock {
    sample_rate: f64,
    tempo: f32,
    swing: f32,
    /// Samples per step at the current tempo.
    step_len: f64,
    /// Step `anchor_step` falls on `anchor_sample` before swing; later steps
    /// count from there.
    anchor_step: u64,
    anchor_sample: f64,
    /// The next step to fire.
    next: u64,
    pos: u64,
    playing: bool,
}

impl Clock {
    /// A stopped clock at 120 BPM, straight, at the top.
    pub fn new(sample_rate: f32) -> Clock {
        let mut c = Clock {
            sample_rate: f64::from(sample_rate),
            tempo: 120.0,
            swing: SWING.0,
            step_len: 0.0,
            anchor_step: 0,
            anchor_sample: 0.0,
            next: 0,
            pos: 0,
            playing: false,
        };
        c.step_len = c.len_at(c.tempo);
        c
    }

    pub fn tempo(&self) -> f32 {
        self.tempo
    }

    pub fn swing(&self) -> f32 {
        self.swing
    }

    /// Set the tempo in BPM, clamped; NaN is ignored. Takes effect after the
    /// next step.
    pub fn set_tempo(&mut self, bpm: f32) {
        if bpm.is_nan() {
            return;
        }
        let bpm = bpm.clamp(TEMPO.0, TEMPO.1);
        self.reanchor(self.len_at(bpm));
        self.tempo = bpm;
    }

    /// Set the swing in percent, clamped; NaN is ignored. Moves the off-beats
    /// still to come.
    pub fn set_swing(&mut self, pct: f32) {
        if !pct.is_nan() {
            self.swing = pct.clamp(SWING.0, SWING.1);
        }
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Jump to `sample`, measured at the current tempo from the top: there is
    /// no tempo map yet. The next step is the first at or after it.
    pub fn seek(&mut self, sample: u64) {
        self.pos = sample;
        self.anchor_step = 0;
        self.anchor_sample = 0.0;
        let mut k = ((sample as f64 / self.step_len) as u64).saturating_sub(1);
        while self.step_sample(k) < sample {
            k += 1;
        }
        self.next = k;
    }

    /// Position in samples.
    pub fn position(&self) -> u64 {
        self.pos
    }

    /// The last step fired, or `None` before the first.
    pub fn step(&self) -> Option<u64> {
        self.next.checked_sub(1)
    }

    /// The sample step `k` fires on.
    pub fn step_sample(&self, k: u64) -> u64 {
        let off = if k % 2 == 1 {
            (2.0 * f64::from(self.swing) / 100.0 - 1.0) * self.step_len
        } else {
            0.0
        };
        (self.grid(k) + off).round() as u64
    }

    /// The next step due at the current position, advancing past it.
    pub(crate) fn due(&mut self) -> Option<u64> {
        if !self.playing || self.step_sample(self.next) > self.pos {
            return None;
        }
        self.next += 1;
        Some(self.next - 1)
    }

    /// Frames to render before the next step, at least 1, at most `remaining`.
    pub(crate) fn frames_until_next(&self, remaining: usize) -> usize {
        if !self.playing {
            return remaining;
        }
        let gap = self.step_sample(self.next).saturating_sub(self.pos);
        usize::try_from(gap)
            .unwrap_or(remaining)
            .clamp(1, remaining.max(1))
    }

    pub(crate) fn advance(&mut self, frames: usize) {
        if self.playing {
            self.pos = self.pos.saturating_add(frames as u64);
        }
    }

    fn len_at(&self, bpm: f32) -> f64 {
        self.sample_rate * 60.0 / f64::from(bpm) / STEPS_PER_BEAT as f64
    }

    /// Step `k` on the grid, before swing and rounding.
    fn grid(&self, k: u64) -> f64 {
        self.anchor_sample + k.saturating_sub(self.anchor_step) as f64 * self.step_len
    }

    /// Anchor at the next step's grid point, then space later steps by `len`.
    fn reanchor(&mut self, len: f64) {
        self.anchor_sample = self.grid(self.next);
        self.anchor_step = self.next;
        self.step_len = len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `samples` frames the way `Engine::render` does, in blocks of 128
    /// split at steps, and return each step's sample.
    fn run(c: &mut Clock, samples: u64) -> Vec<u64> {
        let mut fired = Vec::new();
        let end = c.position() + samples;
        while c.position() < end {
            let n = (end - c.position()).min(128) as usize;
            let mut t = 0;
            while t < n {
                while c.due().is_some() {
                    fired.push(c.position());
                }
                let chunk = c.frames_until_next(n - t);
                c.advance(chunk);
                t += chunk;
            }
        }
        fired
    }

    fn playing(rate: f32) -> Clock {
        let mut c = Clock::new(rate);
        c.play();
        c
    }

    #[test]
    fn sixteenths_at_120_bpm() {
        let mut c = playing(48_000.0);
        let fired = run(&mut c, 4 * 96_000);
        assert_eq!(fired.len(), 64);
        for (k, s) in fired.iter().enumerate() {
            assert_eq!(*s, k as u64 * 6000);
        }
    }

    #[test]
    fn swing_delays_the_off_beats() {
        let mut c = playing(48_000.0);
        c.set_swing(75.0);
        let fired = run(&mut c, 48_000);
        assert_eq!(&fired[..4], &[0, 9000, 12_000, 21_000]);
    }

    #[test]
    fn a_tempo_change_keeps_the_next_step() {
        let mut c = playing(48_000.0);
        assert_eq!(run(&mut c, 7000), vec![0, 6000]);
        c.set_tempo(60.0);
        assert_eq!(run(&mut c, 30_000), vec![12_000, 24_000, 36_000]);
        assert_eq!(c.tempo(), 60.0);
    }

    #[test]
    fn a_swing_change_moves_the_next_off_beat_not_the_grid() {
        let mut c = playing(48_000.0);
        run(&mut c, 1);
        c.set_swing(75.0); // the next step, 1, is an off-beat
        assert_eq!(run(&mut c, 30_000), vec![9000, 12_000, 21_000, 24_000]);
    }

    #[test]
    fn no_drift_over_a_thousand_bars() {
        let mut c = Clock::new(44_100.0);
        c.set_tempo(133.0);
        let len = 44_100.0 * 60.0 / 133.0 / 4.0;
        let k = 16_000;
        assert_eq!(c.step_sample(k), (k as f64 * len).round() as u64);
    }

    #[test]
    fn stopped_it_neither_moves_nor_fires() {
        let mut c = Clock::new(48_000.0);
        assert!(run(&mut c, 0).is_empty());
        c.advance(10_000);
        assert_eq!(c.position(), 0);
        assert_eq!(c.due(), None);
        assert_eq!(c.frames_until_next(128), 128);
    }

    #[test]
    fn seek_lands_on_the_next_step() {
        let mut c = playing(48_000.0);
        c.seek(12_000);
        assert_eq!(c.due(), Some(2));
        c.seek(12_001);
        assert_eq!(c.due(), None);
        assert_eq!(run(&mut c, 6000), vec![18_000]);
        assert_eq!(c.step(), Some(3));
    }

    #[test]
    fn settings_are_clamped_and_nan_ignored() {
        let mut c = Clock::new(48_000.0);
        c.set_tempo(f32::NAN);
        c.set_swing(f32::NAN);
        assert_eq!((c.tempo(), c.swing()), (120.0, 50.0));
        c.set_tempo(1000.0);
        c.set_swing(0.0);
        assert_eq!((c.tempo(), c.swing()), (300.0, 50.0));
    }
}
