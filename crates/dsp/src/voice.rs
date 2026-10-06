//! What every voice shares: who owns it, pitch, and the sine table.
//!
//! Mono's voices live in `mono::voice`; this holds the small helpers its
//! modules and the engine use.

/// Sine table size; one extra guard sample for interpolation.
pub const TABLE: usize = 2048;

/// Who started a voice, so a note-off releases only its own notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// The on-screen or computer keyboard, playing this synth.
    Live(u8),
    /// The MIDI player, on this channel (0..=15).
    Channel(u8),
    /// The song, on this track (spec 002 Req 6).
    Track(u8),
}

/// Equal temperament, A4 = 440 Hz. Called per note, not per sample.
pub fn midi_to_hz(note: u8) -> f32 {
    440.0 * ((f32::from(note) - 69.0) / 12.0).exp2()
}

/// One cycle of a sine in `TABLE + 1` samples (a guard sample for the
/// interpolation). Allocates: build it once, in `Engine::new`.
pub fn sine_table() -> Vec<f32> {
    (0..=TABLE)
        .map(|i| (i as f32 / TABLE as f32 * std::f32::consts::TAU).sin())
        .collect()
}

/// Wrap a phase that is at most one cycle past 1 back into `0..1`.
pub fn wrap(p: f32) -> f32 {
    if p >= 1.0 { p - 1.0 } else { p }
}

/// Linear-interpolated table lookup for a phase in `0..1`.
pub fn lookup(table: &[f32], phase: f32) -> f32 {
    let pos = phase * TABLE as f32;
    let i = pos as usize;
    let frac = pos - i as f32;
    let a = table.get(i).copied().unwrap_or(0.0);
    let b = table.get(i + 1).copied().unwrap_or(0.0);
    a + (b - a) * frac
}

/// Gains that move in a straight line from one block's values to the next
/// (#271): a fader, pan, send or oscillator level written once per block by a
/// knob, lane or `mod` line would otherwise step every block and zipper.
/// `aim` works out the step once per block; a sample costs an add (ADR-0002).
#[derive(Clone, Copy, Debug)]
pub struct Gains<const N: usize> {
    now: [f32; N],
    step: [f32; N],
    to: [f32; N],
}

impl<const N: usize> Default for Gains<N> {
    fn default() -> Gains<N> {
        Gains::new([0.0; N])
    }
}

impl<const N: usize> Gains<N> {
    pub const fn new(at: [f32; N]) -> Gains<N> {
        Gains {
            now: at,
            step: [0.0; N],
            to: at,
        }
    }

    /// Head for `to` over the next `n` samples; false when already there.
    pub fn aim(&mut self, to: [f32; N], n: usize) -> bool {
        self.to = to;
        if self.now == to || n == 0 {
            self.now = to;
            return false;
        }
        let n = n as f32;
        for ((s, a), b) in self.step.iter_mut().zip(self.now).zip(to) {
            *s = (b - a) / n;
        }
        true
    }

    /// Be at `to` at once: a new note starts at its levels, it does not fade in.
    pub fn jump(&mut self, to: [f32; N]) {
        self.now = to;
        self.to = to;
    }

    /// The gains for the next sample.
    pub fn tick(&mut self) -> [f32; N] {
        for (v, s) in self.now.iter_mut().zip(self.step) {
            *v += s;
        }
        self.now
    }

    /// End the block exactly on the target, whatever the rounding on the way.
    pub fn settle(&mut self) {
        self.now = self.to;
    }

    pub fn now(&self) -> [f32; N] {
        self.now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_is_440() {
        assert!((midi_to_hz(69) - 440.0).abs() < 1.0e-3);
        assert!((midi_to_hz(81) - 880.0).abs() < 1.0e-2);
    }

    /// #271: a ramp moves in equal steps and ends exactly on its target.
    #[test]
    fn gains_ramp_in_a_line_to_their_target() {
        let mut g = Gains::new([0.0, 1.0]);
        assert!(!g.aim([0.0, 1.0], 128), "no move, no ramp");
        assert!(g.aim([1.0, 0.0], 4));
        let path: Vec<_> = (0..4).map(|_| g.tick()).collect();
        assert_eq!(
            path,
            vec![[0.25, 0.75], [0.5, 0.5], [0.75, 0.25], [1.0, 0.0]]
        );
        g.settle();
        assert_eq!(g.now(), [1.0, 0.0]);
        g.jump([0.3, 0.3]);
        assert_eq!(g.now(), [0.3, 0.3]);
    }
}
