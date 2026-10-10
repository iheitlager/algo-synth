//! An insert slot (spec 002 Req 2, ADR-0010): one of a few effects in series
//! on a strip, before its fader. A slot has a type and five knobs A–E in
//! 0..=1 whose meaning depends on the type:
//!
//! | type                       | A         | B         | C         | D         | E      |
//! |----------------------------|-----------|-----------|-----------|-----------|--------|
//! | Overdrive, Distortion, Fuzz| amount    | tone      | level     | –         | –      |
//! | EQ (three bands)           | low gain  | mid freq  | mid gain  | high gain | mid Q  |
//! | Compressor                 | threshold | ratio     | attack    | release   | make-up|
//! | Vocoder (#161)             | shift     | release   | unvoiced  | width     | dry    |
//! | Bitcrush (#470)            | bits      | rate      | level     | –         | dry    |
//!
//! A vocoder hears its key: another synth's raw signal, picked by the strip's
//! `Key`; without one it is silent.
//!
//! Every slot holds every type, allocated up front. A new type starts from
//! rest, the old ones are reset so nothing old comes back, and nothing
//! allocates. At 0.5 (0 for E) every knob is neutral: an EQ is flat and a
//! compressor adds no make-up.

use super::compressor::Compressor;
use super::crush::Crush;
use super::drive::{Drive, DriveMode};
use super::eq::{EqBand, Equalizer};
use super::vocoder::Vocoder;

/// What a slot does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum InsertType {
    Off = 0,
    Overdrive = 1,
    Distortion = 2,
    Fuzz = 3,
    Eq = 4,
    Comp = 5,
    Vocoder = 6,
    Bitcrush = 7,
}

impl InsertType {
    pub const ALL: [(InsertType, &'static str); 8] = [
        (InsertType::Off, "Off"),
        (InsertType::Overdrive, "Overdrive"),
        (InsertType::Distortion, "Distortion"),
        (InsertType::Fuzz, "Fuzz"),
        (InsertType::Eq, "Eq"),
        (InsertType::Comp, "Comp"),
        (InsertType::Vocoder, "Vocoder"),
        (InsertType::Bitcrush, "Bitcrush"),
    ];

    pub fn from_id(id: u32) -> Option<InsertType> {
        Self::ALL
            .iter()
            .find(|(t, _)| *t as u32 == id)
            .map(|(t, _)| *t)
    }

    fn drive_mode(self) -> Option<DriveMode> {
        match self {
            InsertType::Overdrive => Some(DriveMode::Overdrive),
            InsertType::Distortion => Some(DriveMode::Distortion),
            InsertType::Fuzz => Some(DriveMode::Fuzz),
            _ => None,
        }
    }
}

/// The corners of the compact equalizer's shelves, in Hz.
const LOW_SHELF: f32 = 200.0;
const HIGH_SHELF: f32 = 5_000.0;
/// ±15 dB, as the master equalizer.
const RANGE_DB: f32 = 30.0;

pub struct Insert {
    kind: InsertType,
    knob: [f32; 5],
    drive: Drive,
    /// The right channel's shaper, for a stereo bus.
    drive_r: Drive,
    eq: Equalizer,
    comp: Compressor,
    vocoder: Vocoder,
    crush: Crush,
}

impl Insert {
    pub fn new(sample_rate: f32) -> Insert {
        Insert {
            kind: InsertType::Off,
            knob: [0.5, 0.5, 0.5, 0.5, 0.0],
            drive: Drive::new(sample_rate),
            drive_r: Drive::new(sample_rate),
            eq: Equalizer::new(sample_rate),
            comp: Compressor::new(sample_rate),
            vocoder: Vocoder::new(sample_rate),
            crush: Crush::new(sample_rate),
        }
    }

    pub fn set_type(&mut self, kind: InsertType) {
        if kind == self.kind {
            return;
        }
        self.kind = kind;
        // Whatever was running starts from rest the next time it is picked.
        self.drive.set_mode(DriveMode::Off);
        self.drive_r.set_mode(DriveMode::Off);
        self.eq.reset();
        self.comp.reset();
        self.vocoder.reset();
        self.crush.reset();
        self.apply();
    }

    /// Set knob `i` (0 is A) to `v`.
    pub fn set_knob(&mut self, i: usize, v: f32) {
        if let Some(k) = self.knob.get_mut(i) {
            *k = v;
        }
        self.apply();
    }

    /// Map the knobs onto the active effect, at control rate.
    fn apply(&mut self) {
        let [a, b, c, d, e] = self.knob;
        if let Some(mode) = self.kind.drive_mode() {
            for d in [&mut self.drive, &mut self.drive_r] {
                d.set_mode(mode);
                d.set_amount(a);
                d.set_tone(b);
                d.set_level(c);
            }
            return;
        }
        match self.kind {
            InsertType::Eq => {
                self.eq.set_freq(EqBand::Low, LOW_SHELF);
                self.eq.set_gain(EqBand::Low, (a - 0.5) * RANGE_DB);
                self.eq.set_freq(EqBand::Mid1, 200.0 * 40.0_f32.powf(b));
                self.eq.set_gain(EqBand::Mid1, (c - 0.5) * RANGE_DB);
                self.eq.set_q(EqBand::Mid1, 8.0_f32.powf(e));
                self.eq.set_freq(EqBand::High, HIGH_SHELF);
                self.eq.set_gain(EqBand::High, (d - 0.5) * RANGE_DB);
            }
            InsertType::Comp => {
                self.comp.set_threshold(-60.0 * (1.0 - a));
                self.comp.set_ratio(20.0_f32.powf(b));
                self.comp.set_attack(0.1 * 1_000.0_f32.powf(c));
                self.comp.set_release(10.0 * 100.0_f32.powf(d));
                self.comp.set_makeup(24.0 * e);
            }
            InsertType::Vocoder => self.vocoder.set([a, b, c, d, e]),
            InsertType::Bitcrush => self.crush.set([a, b, c, d, e]),
            _ => {}
        }
    }

    /// Process one channel in place; Off leaves it untouched. `key` is the
    /// side-chain a vocoder follows.
    pub fn process_mono(&mut self, x: &mut [f32], key: Option<&[f32]>) {
        match self.kind {
            InsertType::Off => {}
            InsertType::Eq => self.eq.process_mono(x),
            InsertType::Comp => self.comp.process_mono(x),
            InsertType::Vocoder => self.vocoder.process_mono(x, key),
            InsertType::Bitcrush => self.crush.process_mono(x),
            _ => self.drive.process(x),
        }
    }

    /// Process a stereo bus in place: the compressor sees both channels and
    /// gives them one gain; the others treat each side alike.
    pub fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32], key: Option<&[f32]>) {
        match self.kind {
            InsertType::Off => {}
            InsertType::Eq => self.eq.process(left, right),
            InsertType::Comp => self.comp.process(left, right),
            InsertType::Vocoder => self.vocoder.process_stereo(left, right, key),
            InsertType::Bitcrush => self.crush.process_stereo(left, right),
            _ => {
                self.drive.process(left);
                self.drive_r.process(right);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 48_000.0;

    fn sine(hz: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (TAU * hz * i as f32 / SR).sin())
            .collect()
    }

    fn slot(kind: InsertType, knobs: [f32; 5]) -> Insert {
        let mut s = Insert::new(SR);
        s.set_type(kind);
        for (i, k) in knobs.iter().enumerate() {
            s.set_knob(i, *k);
        }
        s
    }

    fn run(s: &mut Insert, x: &[f32]) -> Vec<f32> {
        let mut y = x.to_vec();
        for chunk in y.chunks_mut(128) {
            s.process_mono(chunk, None);
        }
        y
    }

    fn peak(x: &[f32]) -> f32 {
        x[x.len() / 2..].iter().fold(0.0_f32, |m, v| m.max(v.abs()))
    }

    #[test]
    fn off_is_bit_exact_and_so_are_neutral_knobs() {
        let x = sine(440.0, 0.7, 4_800);
        assert!(run(&mut Insert::new(SR), &x) == x);
        assert!(
            run(&mut slot(InsertType::Eq, [0.5, 0.5, 0.5, 0.5, 0.0]), &x) == x,
            "a flat EQ"
        );
        assert!(
            run(&mut slot(InsertType::Comp, [1.0, 0.0, 0.5, 0.5, 0.0]), &x) == x,
            "ratio 1, no make-up"
        );
    }

    #[test]
    fn drive_types_shape_and_stay_bounded() {
        let x = sine(220.0, 0.3, 9_600);
        let (od, ds, fz) = (
            run(
                &mut slot(InsertType::Overdrive, [0.9, 0.8, 0.8, 0.0, 0.0]),
                &x,
            ),
            run(
                &mut slot(InsertType::Distortion, [0.9, 0.8, 0.8, 0.0, 0.0]),
                &x,
            ),
            run(&mut slot(InsertType::Fuzz, [0.9, 0.8, 0.8, 0.0, 0.0]), &x),
        );
        assert!(od != x && ds != x && fz != x && od != ds && ds != fz);
        for y in [&od, &ds, &fz] {
            assert!(y.iter().all(|s| s.is_finite() && s.abs() < 3.0));
        }
    }

    #[test]
    fn the_eq_shelves_and_mid_reach_their_gain() {
        let at = |knobs: [f32; 5], hz: f32| {
            let x = sine(hz, 0.3, 24_000);
            20.0 * (peak(&run(&mut slot(InsertType::Eq, knobs), &x)) / 0.3).log10()
        };
        assert!(
            (at([1.0, 0.5, 0.5, 0.5, 0.0], 40.0) - 15.0).abs() < 1.0,
            "low shelf +15"
        );
        assert!(
            (at([0.5, 0.5, 0.5, 0.0, 0.0], 16_000.0) + 15.0).abs() < 1.0,
            "high shelf -15"
        );
        // The mid at B = 0.5 sits at 200 · 40^0.5 = 1265 Hz.
        assert!(
            (at([0.5, 0.5, 1.0, 0.5, 0.0], 1_265.0) - 15.0).abs() < 0.7,
            "mid +15"
        );
    }

    #[test]
    fn the_compressor_slot_turns_a_loud_signal_down() {
        let x = sine(440.0, 0.8, 48_000);
        // Threshold −24 dB, ratio about 4:1, fast attack.
        let y = run(&mut slot(InsertType::Comp, [0.6, 0.46, 0.1, 0.3, 0.0]), &x);
        assert!(peak(&y) < 0.6 * peak(&x), "{} vs {}", peak(&y), peak(&x));
    }

    #[test]
    fn switching_type_starts_the_new_one_from_rest() {
        let x = sine(440.0, 0.8, 24_000);
        let mut s = slot(InsertType::Comp, [0.8, 0.9, 0.0, 0.9, 0.0]);
        let loud = run(&mut s, &x);
        s.set_type(InsertType::Off);
        assert!(run(&mut s, &x) == x, "off passes at once");
        s.set_type(InsertType::Comp);
        let again = run(&mut s, &x);
        assert!(
            again[..2_000] == loud[..2_000],
            "no gain reduction carried over"
        );
    }

    #[test]
    fn stereo_off_and_neutral_are_bit_exact_and_sides_match_mono() {
        let x = sine(440.0, 0.7, 4_800);
        let both = |s: &mut Insert| {
            let (mut l, mut r) = (x.clone(), x.clone());
            for (l, r) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
                s.process_stereo(l, r, None);
            }
            (l, r)
        };
        let (l, r) = both(&mut Insert::new(SR));
        assert!(l == x && r == x);
        let (l, r) = both(&mut slot(InsertType::Eq, [0.5, 0.5, 0.5, 0.5, 0.0]));
        assert!(l == x && r == x);
        // The shapers and the EQ do on each side what they do on one channel.
        for kind in [InsertType::Fuzz, InsertType::Eq] {
            let knobs = [0.9, 0.6, 0.8, 0.3, 0.2];
            let (l, r) = both(&mut slot(kind, knobs));
            let mono = run(&mut slot(kind, knobs), &x);
            assert!(l == mono && r == mono, "{kind:?}");
        }
    }

    #[test]
    fn a_stereo_compressor_gives_both_sides_one_gain() {
        let loud = sine(440.0, 0.9, 24_000);
        let quiet = sine(440.0, 0.02, 24_000);
        let mut s = slot(InsertType::Comp, [0.6, 0.7, 0.1, 0.5, 0.0]);
        let (mut l, mut r) = (loud.clone(), quiet.clone());
        for (l, r) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
            s.process_stereo(l, r, None);
        }
        let ratio_r = peak(&r) / peak(&quiet);
        let ratio_l = peak(&l) / peak(&loud);
        assert!(
            (ratio_r - ratio_l).abs() < 0.02,
            "linked: {ratio_l} vs {ratio_r}"
        );
        assert!(
            ratio_r < 0.7,
            "the quiet side is turned down with the loud one"
        );
    }

    #[test]
    fn a_bitcrush_slot_steps_the_signal() {
        let x = sine(440.0, 0.7, 4_800);
        // About 3 bits at about 1.5 kHz.
        let y = run(
            &mut slot(InsertType::Bitcrush, [0.15, 0.25, 0.5, 0.0, 0.0]),
            &x,
        );
        assert!(y != x && y.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        let mut levels: Vec<i32> = y.iter().map(|s| (s * 1e4) as i32).collect();
        levels.sort_unstable();
        levels.dedup();
        assert!(levels.len() <= 9, "{levels:?}");
    }

    #[test]
    fn unknown_type_ids_are_none() {
        assert_eq!(InsertType::from_id(5), Some(InsertType::Comp));
        assert_eq!(InsertType::from_id(6), Some(InsertType::Vocoder));
        assert_eq!(InsertType::from_id(7), Some(InsertType::Bitcrush));
        assert_eq!(InsertType::from_id(8), None);
    }
}
