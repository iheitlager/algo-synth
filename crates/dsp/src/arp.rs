//! The arpeggiator: a note pattern as a plain function of the held notes, the
//! mode, the octaves, the step and a seed, and the live arp built on it
//! (spec 002 Req 7). It keeps no state of its own beyond the held set, so the
//! notation's `arp` and the keyboard's play the same order.
//!
//! The held set is a fixed array: nothing here allocates (ADR-0002).

use crate::algo::mix;
use crate::clock::TICKS_PER_STEP;
use crate::params::Param;

/// Most notes the arp holds; further keys are ignored.
pub const MAX_HELD: usize = 8;

/// The order a held chord is played in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArpMode {
    Up = 0,
    Down = 1,
    UpDown = 2,
    /// Press order.
    AsPlayed = 3,
    Random = 4,
}

impl ArpMode {
    pub const ALL: [(ArpMode, &'static str); 5] = [
        (ArpMode::Up, "Up"),
        (ArpMode::Down, "Down"),
        (ArpMode::UpDown, "UpDown"),
        (ArpMode::AsPlayed, "AsPlayed"),
        (ArpMode::Random, "Random"),
    ];

    fn from_value(v: f32) -> ArpMode {
        match v.round() as i32 {
            1 => ArpMode::Down,
            2 => ArpMode::UpDown,
            3 => ArpMode::AsPlayed,
            4 => ArpMode::Random,
            _ => ArpMode::Up,
        }
    }
}

/// Step length in ticks for each rate id: 1/8, 1/16, 1/8T, 1/16T. A sixteenth is
/// `TICKS_PER_STEP` ticks (ADR-0016).
pub const RATE_TICKS: [u64; 4] = [2 * TICKS_PER_STEP, TICKS_PER_STEP, 4, 2];

/// The notes held, in the order they were pressed.
#[derive(Clone, Copy, Debug, Default)]
pub struct Held {
    notes: [u8; MAX_HELD],
    len: usize,
}

impl Held {
    pub fn as_slice(&self) -> &[u8] {
        self.notes.get(..self.len).unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Add `note` after the others; a note already held keeps its place.
    pub fn add(&mut self, note: u8) {
        if self.as_slice().contains(&note) {
            return;
        }
        if let Some(slot) = self.notes.get_mut(self.len) {
            *slot = note;
            self.len += 1;
        }
    }

    pub fn remove(&mut self, note: u8) {
        if let Some(i) = self.as_slice().iter().position(|n| *n == note) {
            self.notes.copy_within(i + 1..self.len, i);
            self.len -= 1;
        }
    }

    /// Keep only the notes `keep` says yes to.
    pub fn retain(&mut self, keep: impl Fn(u8) -> bool) {
        let mut out = 0;
        for i in 0..self.len {
            let n = self.notes.get(i).copied().unwrap_or(0);
            if keep(n) {
                if let Some(slot) = self.notes.get_mut(out) {
                    *slot = n;
                }
                out += 1;
            }
        }
        self.len = out;
    }
}

/// The note at `step` of the pattern over `held` (in press order), or `None`
/// with nothing held. Octaves 1–4 repeat the chord a twelve higher each time;
/// up-down turns at both ends without repeating them; random picks from the
/// whole span by `seed` and `step`, the same every time.
pub fn arp_note(held: &[u8], mode: ArpMode, octaves: u8, step: u64, seed: u32) -> Option<u8> {
    let mut buf = [0u8; MAX_HELD];
    let n = held.len().min(MAX_HELD);
    let chord = buf.get_mut(..n)?;
    chord.copy_from_slice(held.get(..n)?);
    if mode != ArpMode::AsPlayed {
        chord.sort_unstable();
    }
    let span = n * usize::from(octaves.clamp(1, 4));
    if span == 0 {
        return None;
    }
    let at = (step % (span as u64 * 2)) as usize; // wide enough for every period
    let idx = match mode {
        ArpMode::Up | ArpMode::AsPlayed => at % span,
        ArpMode::Down => span - 1 - at % span,
        ArpMode::UpDown if span > 1 => {
            let period = 2 * span - 2;
            let i = (step % period as u64) as usize;
            if i < span { i } else { period - i }
        }
        ArpMode::UpDown => 0,
        ArpMode::Random => mix(seed, step as u32) as usize % span,
    };
    let note = u16::from(*chord.get(idx % n)?) + 12 * (idx / n) as u16;
    Some(note.min(127) as u8)
}

/// One synth's live arp: its settings, the held set and the note it sounds.
#[derive(Clone, Copy, Debug)]
pub struct Arp {
    pub on: bool,
    mode: ArpMode,
    octaves: u8,
    /// Ticks per step.
    period: u64,
    gate: f32,
    latch: bool,
    /// Plays on its own grid when the song's clock is stopped.
    pub free: bool,
    seed: u32,
    held: Held,
    /// The keys physically down, one bit each.
    down: u128,
    /// The note sounding and the tick it ends on.
    sounding: Option<(u8, u64)>,
    /// The tick the pattern started on: a chord begins at its first note,
    /// whatever the grid is doing.
    origin: Option<u64>,
}

/// What a fresh synth's arp parameters are set to.
pub const ARP_DEFAULTS: [(Param, f32); 8] = [
    (Param::ArpOn, 0.0),
    (Param::ArpMode, 0.0),
    (Param::ArpOctaves, 1.0),
    (Param::ArpRate, 1.0),
    (Param::ArpGate, 0.5),
    (Param::ArpLatch, 0.0),
    (Param::ArpFree, 0.0),
    (Param::ArpSeed, 1.0),
];

impl Default for Arp {
    fn default() -> Arp {
        Arp {
            on: false,
            mode: ArpMode::Up,
            octaves: 1,
            period: TICKS_PER_STEP,
            gate: 0.5,
            latch: false,
            free: false,
            seed: 1,
            held: Held::default(),
            down: 0,
            sounding: None,
            origin: None,
        }
    }
}

impl Arp {
    /// Apply an arp parameter. Returns true when the arp was just turned on or
    /// off, so the engine can release what the other path left sounding.
    pub fn set(&mut self, param: Param, v: f32) -> bool {
        match param {
            Param::ArpOn => {
                let on = v >= 0.5;
                let changed = on != self.on;
                self.on = on;
                self.held.clear();
                self.down = 0;
                self.origin = None;
                return changed;
            }
            Param::ArpMode => self.mode = ArpMode::from_value(v),
            Param::ArpOctaves => self.octaves = v.round().clamp(1.0, 4.0) as u8,
            Param::ArpRate => {
                let i = (v.round().max(0.0) as usize).min(RATE_TICKS.len() - 1);
                self.period = RATE_TICKS.get(i).copied().unwrap_or(TICKS_PER_STEP);
            }
            Param::ArpGate => self.gate = v.clamp(0.05, 1.0),
            Param::ArpLatch => {
                let latch = v >= 0.5;
                if self.latch && !latch {
                    // Unlatched: only the keys still down stay held.
                    let down = self.down;
                    self.held.retain(|n| down >> n & 1 == 1);
                }
                self.latch = latch;
            }
            Param::ArpFree => self.free = v >= 0.5,
            Param::ArpSeed => self.seed = v.round().max(0.0) as u32,
            _ => {}
        }
        false
    }

    pub fn press(&mut self, note: u8) {
        let note = note.min(127);
        // A new chord after all keys came up replaces the latched one.
        if self.latch && self.down == 0 {
            self.held.clear();
            self.origin = None;
        }
        self.down |= 1 << note;
        self.held.add(note);
    }

    pub fn release(&mut self, note: u8) {
        let note = note.min(127);
        self.down &= !(1u128 << note);
        if !self.latch {
            self.held.remove(note);
        }
    }

    pub fn held(&self) -> &[u8] {
        self.held.as_slice()
    }

    /// The note sounding, if any, forgotten: the caller releases it.
    pub fn take_sounding(&mut self) -> Option<u8> {
        self.sounding.take().map(|(n, _)| n)
    }

    /// Tick `j`: what to release and what to start. The gate's end comes
    /// first, so a full gate hands over on the same tick.
    pub fn tick(&mut self, j: u64) -> (Option<u8>, Option<u8>) {
        let mut off = None;
        if let Some((n, until)) = self.sounding {
            if until <= j {
                off = Some(n);
                self.sounding = None;
            }
        }
        let mut on = None;
        if self.held.is_empty() {
            self.origin = None;
        }
        if j % self.period == 0 && !self.held.is_empty() {
            let origin = *self.origin.get_or_insert(j);
            on = arp_note(
                self.held.as_slice(),
                self.mode,
                self.octaves,
                (j - origin) / self.period,
                self.seed,
            );
            if let Some(n) = on {
                // A note still sounding is ended by the one that replaces it.
                if let Some((old, _)) = self.sounding.take() {
                    off = Some(old);
                }
                let len = ((self.gate * self.period as f32).round() as u64).clamp(1, self.period);
                self.sounding = Some((n, j + len));
            }
        }
        (off, on)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CEG: [u8; 3] = [60, 64, 67];

    fn run(held: &[u8], mode: ArpMode, octaves: u8, steps: u64) -> Vec<u8> {
        (0..steps)
            .filter_map(|s| arp_note(held, mode, octaves, s, 1))
            .collect()
    }

    #[test]
    fn each_mode_orders_a_held_chord() {
        assert_eq!(run(&CEG, ArpMode::Up, 1, 6), [60, 64, 67, 60, 64, 67]);
        assert_eq!(run(&CEG, ArpMode::Down, 1, 3), [67, 64, 60]);
        assert_eq!(
            run(&CEG, ArpMode::UpDown, 1, 8),
            [60, 64, 67, 64, 60, 64, 67, 64]
        );
        // As played: the order the keys were pressed in.
        assert_eq!(run(&[67, 60, 64], ArpMode::AsPlayed, 1, 3), [67, 60, 64]);
        assert_eq!(run(&[67, 60, 64], ArpMode::Up, 1, 3), [60, 64, 67]);
    }

    #[test]
    fn octaves_extend_the_pattern() {
        assert_eq!(run(&CEG, ArpMode::Up, 2, 6), [60, 64, 67, 72, 76, 79]);
        assert_eq!(run(&CEG, ArpMode::Down, 2, 6), [79, 76, 72, 67, 64, 60]);
        assert_eq!(
            run(&CEG, ArpMode::UpDown, 2, 10),
            [60, 64, 67, 72, 76, 79, 76, 72, 67, 64]
        );
    }

    #[test]
    fn random_repeats_with_its_seed() {
        let a: Vec<_> = (0..32)
            .map(|s| arp_note(&CEG, ArpMode::Random, 2, s, 7))
            .collect();
        let b: Vec<_> = (0..32)
            .map(|s| arp_note(&CEG, ArpMode::Random, 2, s, 7))
            .collect();
        let c: Vec<_> = (0..32)
            .map(|s| arp_note(&CEG, ArpMode::Random, 2, s, 8))
            .collect();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn nothing_held_gives_nothing() {
        assert_eq!(arp_note(&[], ArpMode::Up, 1, 0, 1), None);
    }

    #[test]
    fn a_single_note_up_down_holds_it() {
        assert_eq!(run(&[60], ArpMode::UpDown, 1, 3), [60, 60, 60]);
    }

    #[test]
    fn the_held_set_is_idempotent_and_capped() {
        let mut h = Held::default();
        h.add(60);
        h.add(60);
        h.add(64);
        assert_eq!(h.as_slice(), [60, 64]);
        h.remove(60);
        h.remove(99);
        assert_eq!(h.as_slice(), [64]);
        for n in 0..20 {
            h.add(n);
        }
        assert_eq!(h.as_slice().len(), MAX_HELD);
    }

    #[test]
    fn latch_keeps_the_chord_until_a_new_one() {
        let mut a = Arp::default();
        a.set(Param::ArpLatch, 1.0);
        a.press(60);
        a.press(64);
        a.release(60);
        a.release(64);
        assert_eq!(a.held(), [60, 64]);
        a.press(67);
        assert_eq!(a.held(), [67]);
        a.release(67);
        a.set(Param::ArpLatch, 0.0);
        assert!(a.held().is_empty());
    }

    #[test]
    fn gate_sets_the_note_length() {
        let mut a = Arp::default();
        a.set(Param::ArpRate, 0.0); // 1/8: 6 ticks
        a.set(Param::ArpGate, 0.5);
        a.press(60);
        assert_eq!(a.tick(0), (None, Some(60)));
        assert_eq!(a.tick(2), (None, None));
        assert_eq!(a.tick(3), (Some(60), None));
        // A full gate hands over on the next onset.
        a.set(Param::ArpGate, 1.0);
        assert_eq!(a.tick(6), (None, Some(60)));
        assert_eq!(a.tick(12), (Some(60), Some(60)));
    }
}
