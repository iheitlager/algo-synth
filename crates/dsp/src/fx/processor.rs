//! The effect processors P1–P4 (spec 002 Req 2): four slots fed by the
//! strips' sends, each one of a few effect types, returned into the master.
//!
//! A slot has a type, a return level and five knobs A–E, all 0..=1. What a
//! knob does depends on the type:
//!
//! | type    | A    | B        | C         | D         | E    |
//! |---------|------|----------|-----------|-----------|------|
//! | Echo    | time | feedback | tone      | ping-pong | –    |
//! | Reverb  | size | damping  | pre-delay | –         | –    |
//! | Chorus  | rate | depth    | delay     | spread    | tone |
//! | Flanger | rate | depth    | manual    | feedback  | tone |
//!
//! Every slot holds every type, allocated up front. Switching type hands the
//! input to the new one and lets the old one ring out, so nothing clicks and
//! nothing allocates. The effects make a wet signal at full level; the slot
//! scales it by the return.

use super::chorus::Chorus;
use super::echo::{Echo, MAX_FEEDBACK};
use super::flanger::{self, Flanger};
use super::reverb::Reverb;
use crate::engine::BLOCK;
use crate::params::ProcField;

/// What a slot does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ProcType {
    Off = 0,
    Echo = 1,
    Reverb = 2,
    Chorus = 3,
    Flanger = 4,
}

impl ProcType {
    pub const ALL: [(ProcType, &'static str); 5] = [
        (ProcType::Off, "Off"),
        (ProcType::Echo, "Echo"),
        (ProcType::Reverb, "Reverb"),
        (ProcType::Chorus, "Chorus"),
        (ProcType::Flanger, "Flanger"),
    ];

    pub fn from_id(id: u32) -> Option<ProcType> {
        Self::ALL
            .iter()
            .find(|(t, _)| *t as u32 == id)
            .map(|(t, _)| *t)
    }
}

/// How long a deselected effect keeps ringing out, in seconds.
const TAIL_SECONDS: f32 = 10.0;

pub struct Processor {
    kind: ProcType,
    echo: Echo,
    reverb: Reverb,
    chorus: Chorus,
    flanger: Flanger,
    knob: [f32; 5],
    ret: f32,
    /// Samples a deselected effect still has to ring out, by type id.
    tails: [usize; 5],
    tail: usize,
    silence: [f32; BLOCK],
    /// What the slot makes this block at full level, kept apart to scale and measure it.
    wet: [[f32; BLOCK]; 2],
    /// The highest sample the slot has added since `take_peak`.
    peak: f32,
}

impl Processor {
    pub fn new(sample_rate: f32) -> Processor {
        Processor {
            kind: ProcType::Off,
            echo: Echo::new(sample_rate),
            reverb: Reverb::new(sample_rate),
            chorus: Chorus::new(sample_rate),
            flanger: Flanger::new(sample_rate),
            knob: [0.0; 5],
            ret: 0.0,
            tails: [0; 5],
            tail: (TAIL_SECONDS * sample_rate) as usize,
            silence: [0.0; BLOCK],
            wet: [[0.0; BLOCK]; 2],
            peak: 0.0,
        }
    }

    /// Apply an already clamped parameter of this slot.
    pub fn set(&mut self, field: ProcField, v: f32) {
        match field {
            ProcField::Type => {
                if let Some(kind) = ProcType::from_id(v.round() as u32) {
                    self.set_type(kind);
                }
            }
            ProcField::Return => {
                // A return of 0 silences the slot and forgets what it held.
                if v == 0.0 && self.ret != 0.0 {
                    self.clear();
                }
                self.ret = v;
            }
            ProcField::Knob(i) => {
                if let Some(k) = self.knob.get_mut(i) {
                    *k = v;
                }
                self.apply_knobs();
            }
        }
    }

    fn clear(&mut self) {
        self.echo.clear();
        self.reverb.clear();
        self.chorus.clear();
        self.flanger.clear();
        self.tails = [0; 5];
    }

    fn set_type(&mut self, kind: ProcType) {
        if kind == self.kind {
            return;
        }
        // The effect being left rings out; the one entered starts fresh input.
        if let Some(t) = self.tails.get_mut(self.kind as usize) {
            *t = self.tail;
        }
        self.kind = kind;
        self.apply_knobs();
    }

    /// Map the knobs onto the active effect.
    fn apply_knobs(&mut self) {
        let [a, b, c, d, e] = self.knob;
        match self.kind {
            ProcType::Echo => {
                self.echo.set_time(2_000.0_f32.powf(a));
                self.echo.set_feedback(b * MAX_FEEDBACK);
                self.echo.set_tone(c);
                self.echo.set_ping_pong(d >= 0.5);
            }
            ProcType::Reverb => {
                self.reverb.set_size(0.1 * 100.0_f32.powf(a));
                self.reverb.set_damping(b);
                self.reverb.set_pre_delay(c * 100.0);
            }
            ProcType::Chorus => {
                self.chorus.set_rate(0.1 * 80.0_f32.powf(a));
                self.chorus.set_depth(b);
                self.chorus.set_delay(5.0 + 25.0 * c);
                self.chorus.set_spread(d);
                self.chorus.set_tone(e);
            }
            ProcType::Flanger => {
                self.flanger.set_rate(0.05 * 100.0_f32.powf(a));
                self.flanger.set_depth(b);
                self.flanger.set_manual(0.5 * 20.0_f32.powf(c));
                self.flanger
                    .set_feedback((d - 0.5) * 2.0 * flanger::MAX_FEEDBACK);
                self.flanger.set_tone(e);
            }
            ProcType::Off => {}
        }
    }

    /// Feed `send` in and add the effect, at the return level, into `left` and `right`.
    pub fn process(&mut self, send: &[f32], left: &mut [f32], right: &mut [f32]) {
        if self.ret == 0.0 {
            return;
        }
        let n = send.len().min(left.len()).min(right.len());
        let silence = self.silence.get(..n).unwrap_or(&[]);
        let [wl, wr] = &mut self.wet;
        let (Some(wl), Some(wr)) = (wl.get_mut(..n), wr.get_mut(..n)) else {
            return;
        };
        wl.fill(0.0);
        wr.fill(0.0);
        // The active type hears the input; a deselected one with a tail hears silence.
        let input =
            |kind: ProcType, current: ProcType| if kind == current { send } else { silence };
        let live = |kind: ProcType, kinds: &ProcType, tails: &[usize; 5]| {
            *kinds == kind || tails.get(kind as usize).copied().unwrap_or(0) > 0
        };
        if live(ProcType::Echo, &self.kind, &self.tails) {
            self.echo.process(input(ProcType::Echo, self.kind), wl, wr);
        }
        if live(ProcType::Reverb, &self.kind, &self.tails) {
            self.reverb
                .process(input(ProcType::Reverb, self.kind), wl, wr);
        }
        if live(ProcType::Chorus, &self.kind, &self.tails) {
            let x = input(ProcType::Chorus, self.kind);
            self.chorus.process(x, x, wl, wr);
        }
        if live(ProcType::Flanger, &self.kind, &self.tails) {
            let x = input(ProcType::Flanger, self.kind);
            self.flanger.process(x, x, wl, wr);
        }
        for t in self.tails.iter_mut() {
            *t = t.saturating_sub(n);
        }
        let ret = self.ret;
        for ((w, l), (v, r)) in wl
            .iter()
            .zip(left.iter_mut())
            .zip(wr.iter().zip(right.iter_mut()))
        {
            *l += w * ret;
            *r += v * ret;
            self.peak = self.peak.max((w * ret).abs()).max((v * ret).abs());
        }
    }

    /// The highest sample added since the last call, then start over.
    pub fn take_peak(&mut self) -> f32 {
        std::mem::take(&mut self.peak)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn run(p: &mut Processor, input: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (vec![0.0; input.len()], vec![0.0; input.len()]);
        for ((x, l), r) in input
            .chunks(BLOCK)
            .zip(l.chunks_mut(BLOCK))
            .zip(r.chunks_mut(BLOCK))
        {
            p.process(x, l, r);
        }
        (l, r)
    }

    fn impulse(n: usize) -> Vec<f32> {
        let mut v = vec![0.0; n];
        v[0] = 1.0;
        v
    }

    fn slot(kind: ProcType) -> Processor {
        let mut p = Processor::new(SR);
        p.set(ProcField::Type, kind as u32 as f32);
        p.set(ProcField::Return, 1.0);
        p
    }

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|x| x * x).sum()
    }

    #[test]
    fn off_is_silent() {
        let mut p = slot(ProcType::Off);
        let (l, r) = run(&mut p, &impulse(12_000));
        assert!(l.iter().chain(&r).all(|x| *x == 0.0));
    }

    #[test]
    fn the_echo_knobs_set_time_and_feedback() {
        let mut p = slot(ProcType::Echo);
        // 2000^0.5 = 44.7 ms; feedback 0.5/0.95 of max.
        p.set(ProcField::Knob(0), 0.5);
        p.set(ProcField::Knob(1), 0.5);
        // The time glides to its new value; let it settle first.
        run(&mut p, &vec![0.0; 24_000]);
        let (l, _) = run(&mut p, &impulse(12_000));
        let t = (44.72 * 48.0) as usize;
        let first: f32 = l[t - 100..t + 100].iter().sum();
        assert!(first > 0.5, "echo at the mapped time: {first}");
        let second: f32 = l[2 * t - 100..2 * t + 100].iter().sum();
        assert!(
            second > 0.1 && second < first,
            "and a decaying repeat: {second}"
        );
    }

    #[test]
    fn the_reverb_knob_sets_the_size() {
        let tail = |a: f32| {
            let mut p = slot(ProcType::Reverb);
            p.set(ProcField::Knob(0), a);
            let (l, _) = run(&mut p, &impulse(96_000));
            energy(&l[48_000..])
        };
        assert!(tail(1.0) > 10.0 * tail(0.2));
    }

    #[test]
    fn switching_type_lets_the_old_tail_ring_out() {
        let mut p = slot(ProcType::Reverb);
        let (before, _) = run(&mut p, &impulse(4_800));
        assert!(energy(&before) > 0.0);
        p.set(ProcField::Type, ProcType::Echo as u32 as f32);
        // No input now; the reverb keeps decaying into the output.
        let (after, _) = run(&mut p, &vec![0.0; 4_800]);
        assert!(energy(&after) > 0.0, "the reverb tail continues");
        // And no jump at the seam: the tail is continuous.
        let step = (after[0] - before[before.len() - 1]).abs();
        assert!(step < 0.05, "seam {step}");
    }

    #[test]
    fn a_return_of_zero_silences_the_slot() {
        let mut p = slot(ProcType::Echo);
        p.set(ProcField::Return, 0.0);
        let (l, r) = run(&mut p, &impulse(24_000));
        assert!(l.iter().chain(&r).all(|x| *x == 0.0));
    }

    fn tone(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.4 * (std::f32::consts::TAU * 800.0 * i as f32 / SR).sin())
            .collect()
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0_f32, |m, v| m.max(v.abs()))
    }

    #[test]
    fn every_type_makes_sound_bounded_and_scales_with_the_return() {
        for (kind, name) in ProcType::ALL.iter().skip(1) {
            let heard = |ret: f32| {
                let mut p = slot(*kind);
                p.set(ProcField::Return, ret);
                let (l, r) = run(&mut p, &tone(48_000));
                assert!(
                    l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 20.0),
                    "{name}"
                );
                peak(&l).max(peak(&r))
            };
            let (full, half) = (heard(1.0), heard(0.5));
            assert!(full > 0.05, "{name} sounds: {full}");
            assert!(
                (half / full - 0.5).abs() < 0.02,
                "{name} return: {half} vs {full}"
            );
        }
    }

    #[test]
    fn a_return_of_zero_silences_chorus_and_flanger_too() {
        for kind in [ProcType::Chorus, ProcType::Flanger] {
            let mut p = slot(kind);
            p.set(ProcField::Return, 0.0);
            let (l, r) = run(&mut p, &tone(24_000));
            assert!(l.iter().chain(&r).all(|x| *x == 0.0), "{kind:?}");
        }
    }

    #[test]
    fn switching_to_chorus_lets_the_flanger_ring_out() {
        let mut p = slot(ProcType::Flanger);
        p.set(ProcField::Knob(3), 0.95);
        run(&mut p, &tone(9_600));
        p.set(ProcField::Type, ProcType::Chorus as u32 as f32);
        let (after, _) = run(&mut p, &vec![0.0; 4_800]);
        assert!(energy(&after) > 0.0, "the flanger's loop keeps ringing");
    }
}
