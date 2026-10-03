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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_is_440() {
        assert!((midi_to_hz(69) - 440.0).abs() < 1.0e-3);
        assert!((midi_to_hz(81) - 880.0).abs() < 1.0e-2);
    }
}
