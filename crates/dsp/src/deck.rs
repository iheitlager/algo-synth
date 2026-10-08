//! The deck mixer (ADR-0029): deck A is this engine's own output, decks B–D
//! arrive as input blocks rendered ahead by engines in Web Workers. Each deck
//! has a level and a side of the crossfader; the sum goes through one limiter.
//!
//! A deck whose block was not fed this block is silent for it: a late deck
//! drops its block and stays on its timeline. With nothing fed and deck A at
//! unity, the output passes untouched.
//!
//! Real-time rules (ADR-0002): the crossfade's sine and cosine are taken when
//! the fader moves; per sample, gains ramp linearly across the block.

use crate::engine::BLOCK;
use crate::fx::limiter::Limiter;

/// Decks in all: A, this engine, and B–D, the inputs.
pub const DECKS: usize = 4;
/// Decks that arrive as input blocks: B, C and D.
pub const INPUTS: usize = DECKS - 1;

/// A deck parameter id for `deck_set`; mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DeckField {
    /// The deck's fader, 0..=1.
    Level = 0,
    /// Its side of the crossfader (`Side`): 0 not on it, 1 left, 2 right.
    Side = 1,
}

impl DeckField {
    /// Every field with the name the TypeScript mirror uses.
    pub const ALL: [(DeckField, &'static str); 2] =
        [(DeckField::Level, "Level"), (DeckField::Side, "Side")];

    pub fn from_id(id: u32) -> Option<DeckField> {
        Self::ALL
            .iter()
            .find(|(f, _)| *f as u32 == id)
            .map(|(f, _)| *f)
    }
}

/// Where a deck sits on the crossfader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Thru,
    Left,
    Right,
}

pub struct DeckMixer {
    /// Planar stereo blocks of decks B–D: left, then right.
    inputs: [[f32; 2 * BLOCK]; INPUTS],
    fed: [bool; INPUTS],
    level: [f32; DECKS],
    side: [Side; DECKS],
    /// Equal-power gains of the left and right side, from the fader.
    xfade: (f32, f32),
    /// The gain each deck had at the end of the last block, and the one it ramps to.
    gain: [f32; DECKS],
    target: [f32; DECKS],
    peaks: [f32; DECKS],
    limiter: Limiter,
}

impl DeckMixer {
    pub fn new(sample_rate: f32) -> DeckMixer {
        let mut m = DeckMixer {
            inputs: [[0.0; 2 * BLOCK]; INPUTS],
            fed: [false; INPUTS],
            level: [1.0; DECKS],
            side: [Side::Thru; DECKS],
            xfade: (1.0, 0.0),
            gain: [1.0; DECKS],
            target: [1.0; DECKS],
            peaks: [0.0; DECKS],
            limiter: Limiter::new(sample_rate),
        };
        m.set_crossfade(0.0);
        m
    }

    /// Where deck `deck` (1–3) writes its next block: `2 * BLOCK` values,
    /// left then right.
    pub fn input_mut(&mut self, deck: usize) -> Option<&mut [f32; 2 * BLOCK]> {
        self.inputs.get_mut(deck.checked_sub(1)?)
    }

    /// Deck `deck` (1–3) wrote this block's input; it plays in the next `process`.
    pub fn fed(&mut self, deck: usize) {
        if let Some(f) = deck.checked_sub(1).and_then(|i| self.fed.get_mut(i)) {
            *f = true;
        }
    }

    pub fn set(&mut self, deck: usize, field: DeckField, value: f32) {
        if deck >= DECKS || !value.is_finite() {
            return;
        }
        match field {
            DeckField::Level => {
                if let Some(l) = self.level.get_mut(deck) {
                    *l = value.clamp(0.0, 1.0);
                }
            }
            DeckField::Side => {
                let side = match value.round() as i32 {
                    1 => Side::Left,
                    2 => Side::Right,
                    _ => Side::Thru,
                };
                if let Some(s) = self.side.get_mut(deck) {
                    *s = side;
                }
            }
        }
        self.retarget();
    }

    /// The crossfader, 0 (all left) to 1 (all right), with equal power: the
    /// middle has each side at −3 dB.
    pub fn set_crossfade(&mut self, x: f32) {
        if !x.is_finite() {
            return;
        }
        let a = x.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
        self.xfade = (a.cos(), a.sin());
        self.retarget();
    }

    fn retarget(&mut self) {
        for d in 0..DECKS {
            let side = match self.side.get(d) {
                Some(Side::Left) => self.xfade.0,
                Some(Side::Right) => self.xfade.1,
                _ => 1.0,
            };
            if let (Some(t), Some(l)) = (self.target.get_mut(d), self.level.get(d)) {
                *t = l * side;
            }
        }
    }

    /// The highest level of deck `deck` after its gain since the last call,
    /// and start over.
    pub fn take_peak(&mut self, deck: usize) -> f32 {
        self.peaks.get_mut(deck).map_or(0.0, std::mem::take)
    }

    /// Mix the fed decks into `left` and `right`, which hold deck A.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len()).min(BLOCK);
        let (Some(l), Some(r)) = (left.get_mut(..n), right.get_mut(..n)) else {
            return;
        };
        let peak_of = |x: &[f32], y: &[f32]| x.iter().chain(y).fold(0.0_f32, |m, v| m.max(v.abs()));
        let unity = self.gain.first() == Some(&1.0) && self.target.first() == Some(&1.0);
        if unity && !self.fed.iter().any(|f| *f) {
            let peak = peak_of(l, r);
            if let Some(p) = self.peaks.first_mut() {
                *p = p.max(peak);
            }
            return;
        }
        let per = 1.0 / n.max(1) as f32;
        // Deck A, in place.
        let (g0, g1) = (
            self.gain.first().copied().unwrap_or(1.0),
            self.target.first().copied().unwrap_or(1.0),
        );
        ramp(l, r, g0, (g1 - g0) * per);
        if let Some(p) = self.peaks.first_mut() {
            *p = p.max(peak_of(l, r));
        }
        // Decks B–D, added.
        let decks = self.inputs.iter().zip(self.fed.iter());
        let gains = self.gain.iter().zip(self.target.iter()).skip(1);
        for (((input, fed), (g0, g1)), peak) in decks.zip(gains).zip(self.peaks.iter_mut().skip(1))
        {
            if !*fed {
                continue;
            }
            let (il, ir) = input.split_at(BLOCK);
            let dg = (g1 - g0) * per;
            let mut g = *g0;
            for (((x, y), a), b) in l.iter_mut().zip(r.iter_mut()).zip(il).zip(ir) {
                g += dg;
                let (a, b) = (a * g, b * g);
                *x += a;
                *y += b;
                *peak = peak.max(a.abs()).max(b.abs());
            }
        }
        // Every gain reaches its target by the end of the block, fed or not.
        self.gain = self.target;
        self.fed = [false; INPUTS];
        self.limiter.process(l, r);
    }
}

/// Scale `l` and `r` by a gain starting at `g` and moving `dg` a sample.
fn ramp(l: &mut [f32], r: &mut [f32], mut g: f32, dg: f32) {
    for (x, y) in l.iter_mut().zip(r.iter_mut()) {
        g += dg;
        *x *= g;
        *y *= g;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn block(v: f32) -> ([f32; BLOCK], [f32; BLOCK]) {
        ([v; BLOCK], [-v; BLOCK])
    }

    fn feed(m: &mut DeckMixer, deck: usize, v: f32) {
        let input = m.input_mut(deck).unwrap();
        input[..BLOCK].fill(v);
        input[BLOCK..].fill(v);
        m.fed(deck);
    }

    #[test]
    fn deck_a_alone_passes_bit_for_bit() {
        let mut m = DeckMixer::new(SR);
        let (mut l, mut r) = block(0.3);
        l[5] = 0.123_456_7;
        let (l0, r0) = (l, r);
        m.process(&mut l, &mut r);
        assert_eq!(l, l0);
        assert_eq!(r, r0);
    }

    #[test]
    fn an_unfed_deck_is_silent_and_a_fed_one_sums() {
        let mut m = DeckMixer::new(SR);
        let (mut l, mut r) = block(0.0);
        m.process(&mut l, &mut r);
        assert!(l.iter().all(|x| *x == 0.0));
        feed(&mut m, 2, 0.25);
        m.process(&mut l, &mut r);
        assert!(l.iter().chain(r.iter()).all(|x| (*x - 0.25).abs() < 1e-6));
        // Not fed again: silent again, it doesn't repeat its last block.
        let (mut l, mut r) = block(0.0);
        m.process(&mut l, &mut r);
        assert!(l.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn the_crossfade_has_equal_power() {
        let mut m = DeckMixer::new(SR);
        m.set(0, DeckField::Side, 1.0);
        m.set(1, DeckField::Side, 2.0);
        for x in [0.0, 0.25, 0.5, 0.75, 1.0] {
            m.set_crossfade(x);
            let (a, b) = (m.target[0], m.target[1]);
            assert!((a * a + b * b - 1.0).abs() < 1e-5, "{x}: {a} {b}");
        }
        m.set_crossfade(0.0);
        assert_eq!((m.target[0], m.target[1]), (1.0, 0.0));
        m.set_crossfade(1.0);
        assert!(m.target[0].abs() < 1e-6 && (m.target[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn gains_ramp_without_a_step() {
        let mut m = DeckMixer::new(SR);
        m.set(0, DeckField::Level, 0.0);
        let (mut l, mut r) = block(0.5);
        m.process(&mut l, &mut r);
        // From 1 down to 0 across the block, never jumping more than 1/BLOCK of the way.
        for w in l.windows(2) {
            assert!((w[0] - w[1]).abs() <= 0.5 / BLOCK as f32 + 1e-6);
        }
        assert!(l[BLOCK - 1].abs() < 1e-6);
    }

    #[test]
    fn the_sum_stays_within_full_scale() {
        let mut m = DeckMixer::new(SR);
        for _ in 0..4 {
            for d in 1..DECKS {
                feed(&mut m, d, 0.9);
            }
            let (mut l, mut r) = block(0.9);
            m.process(&mut l, &mut r);
            assert!(
                l.iter()
                    .chain(r.iter())
                    .all(|x| x.is_finite() && x.abs() <= 1.0)
            );
        }
    }

    #[test]
    fn out_of_range_and_unknown_are_ignored() {
        let mut m = DeckMixer::new(SR);
        m.set(9, DeckField::Level, 0.5);
        m.set(1, DeckField::Level, f32::NAN);
        m.set_crossfade(f32::INFINITY);
        m.fed(0);
        m.fed(7);
        assert!(m.input_mut(0).is_none() && m.input_mut(4).is_none());
        assert_eq!(m.target, [1.0; DECKS]);
        assert_eq!(DeckField::from_id(1), Some(DeckField::Side));
        assert_eq!(DeckField::from_id(2), None);
    }
}
