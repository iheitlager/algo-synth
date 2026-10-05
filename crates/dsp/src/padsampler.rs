//! The drum/pad sampler (plan.md samplers epic, #124): sixteen pads that play
//! samples of the store, after an Akai MPC.
//!
//! Pad *i* answers MIDI note `FIRST_NOTE + i` (General MIDI's kick through
//! D♯2), so a MIDI file's drum channel and the clock's drum lanes play it like
//! any synth. Each pad has a sample, tune, level, pan, a decay, a choke group,
//! how much velocity moves its level and its start point, and whether it is a
//! one-shot (it ignores the key coming up) or fades when the key does.
//! A pad is one voice, retriggered as on the TR-808 kit (#114); a hit chokes
//! the other pads of its group.
//!
//! The voice writes both sides, so its synth has a stereo bus and the strip
//! balances rather than pans (ADR-0010). Real-time rules (ADR-0002): a hit
//! costs one `exp2`, one `exp` and a sine/cosine pair; a sample a few multiplies.

use crate::sample::SampleStore;
use crate::sampler::read;

/// Pads in a kit, and the note of the first.
pub const PADS: usize = 16;
pub const FIRST_NOTE: u8 = 36;

/// The pad a MIDI note plays, if any.
pub fn pad_of(note: u8) -> Option<usize> {
    note.checked_sub(FIRST_NOTE)
        .map(usize::from)
        .filter(|p| *p < PADS)
}

/// A pad parameter id for `PadKit::set`; mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PadField {
    /// Sample slot, or −1 for none.
    Sample = 0,
    /// Semitones, −24..=24.
    Tune = 1,
    Level = 2,
    /// −1 (left) to 1 (right).
    Pan = 3,
    /// Seconds to fall 60 dB; 0 plays the sample out.
    Decay = 4,
    /// Group 1–8: a hit silences the other pads of its group; 0 is none.
    Choke = 5,
    /// How much velocity moves the level: 0 fixed, 1 fully.
    VelLevel = 6,
    /// How much a soft hit starts later in the sample, 0..=1.
    VelStart = 7,
    /// Plays through regardless of the key coming up.
    OneShot = 8,
    /// Where the pad goes: 0 the sampler's own strip, 1–8 straight into that group (#220).
    Out = 9,
}

impl PadField {
    /// Every field with the name the TypeScript mirror uses.
    pub const ALL: [(PadField, &'static str); 10] = [
        (PadField::Sample, "Sample"),
        (PadField::Tune, "Tune"),
        (PadField::Level, "Level"),
        (PadField::Pan, "Pan"),
        (PadField::Decay, "Decay"),
        (PadField::Choke, "Choke"),
        (PadField::VelLevel, "VelLevel"),
        (PadField::VelStart, "VelStart"),
        (PadField::OneShot, "OneShot"),
        (PadField::Out, "Out"),
    ];

    pub fn from_id(id: u32) -> Option<PadField> {
        Self::ALL
            .iter()
            .find(|(f, _)| *f as u32 == id)
            .map(|(f, _)| *f)
    }
}

/// One pad's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadCfg {
    pub sample: Option<usize>,
    pub tune: f32,
    pub level: f32,
    pub pan: f32,
    pub decay: f32,
    pub choke: u8,
    pub vel_level: f32,
    pub vel_start: f32,
    pub one_shot: bool,
    /// 0 is the sampler's strip, 1–8 a group (#220).
    pub out: u8,
}

impl Default for PadCfg {
    fn default() -> PadCfg {
        PadCfg {
            sample: None,
            tune: 0.0,
            level: 0.8,
            pan: 0.0,
            decay: 0.0,
            choke: 0,
            vel_level: 1.0,
            vel_start: 0.0,
            one_shot: true,
            out: 0,
        }
    }
}

impl PadCfg {
    /// Set a field from a view value, clamped into its range.
    pub fn set(&mut self, field: PadField, v: f32) {
        let v = if v.is_finite() { v } else { 0.0 };
        match field {
            PadField::Sample => {
                let slot = v.round();
                self.sample = (slot >= 0.0 && (slot as usize) < crate::sample::SLOTS)
                    .then_some(slot as usize);
            }
            PadField::Tune => self.tune = v.clamp(-24.0, 24.0),
            PadField::Level => self.level = v.clamp(0.0, 2.0),
            PadField::Pan => self.pan = v.clamp(-1.0, 1.0),
            PadField::Decay => self.decay = v.clamp(0.0, 10.0),
            PadField::Choke => self.choke = v.round().clamp(0.0, 8.0) as u8,
            PadField::VelLevel => self.vel_level = v.clamp(0.0, 1.0),
            PadField::VelStart => self.vel_start = v.clamp(0.0, 1.0),
            PadField::OneShot => self.one_shot = v >= 0.5,
            PadField::Out => self.out = v.round().clamp(0.0, 8.0) as u8,
        }
    }

    /// A field as the view reads it: `set`'s inverse.
    pub fn get(&self, field: PadField) -> f32 {
        match field {
            PadField::Sample => self.sample.map_or(-1.0, |s| s as f32),
            PadField::Tune => self.tune,
            PadField::Level => self.level,
            PadField::Pan => self.pan,
            PadField::Decay => self.decay,
            PadField::Choke => f32::from(self.choke),
            PadField::VelLevel => self.vel_level,
            PadField::VelStart => self.vel_start,
            PadField::OneShot => f32::from(u8::from(self.one_shot)),
            PadField::Out => f32::from(self.out),
        }
    }
}

/// A synth's sixteen pads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadKit {
    pads: [PadCfg; PADS],
}

impl Default for PadKit {
    fn default() -> PadKit {
        PadKit {
            pads: [PadCfg::default(); PADS],
        }
    }
}

impl PadKit {
    pub fn set(&mut self, pad: usize, field: PadField, v: f32) {
        if let Some(p) = self.pads.get_mut(pad) {
            p.set(field, v);
        }
    }

    pub fn pad(&self, pad: usize) -> Option<&PadCfg> {
        self.pads.get(pad)
    }

    /// Put every pad back to its defaults.
    pub fn clear(&mut self) {
        *self = PadKit::default();
    }
}

/// Seconds a choke takes to silence a pad, and a released pad to fade.
const CHOKE_SECS: f32 = 0.005;
const RELEASE_SECS: f32 = 0.02;

/// One pad sounding.
#[derive(Clone, Copy, Default)]
pub struct PadVoice {
    pad: usize,
    note: u8,
    cfg: PadCfg,
    /// Velocity, level and the kit's master level, before the pan.
    gain: f32,
    left: f32,
    right: f32,
    rate: f32,
    /// Frames of sample per output frame.
    inc: f64,
    pos: f64,
    /// A hit that has not yet found its start point (it needs the sample's length).
    fresh: bool,
    start: f32,
    on: bool,
    /// The decay envelope and its per-sample factor.
    env: f32,
    env_k: f32,
    /// The fade of a choke or a release: 1 down to 0 by `fade_step` a sample.
    fade: f32,
    fade_step: f32,
}

impl PadVoice {
    pub fn active(&self) -> bool {
        self.on || self.fresh
    }

    pub fn pad(&self) -> usize {
        self.pad
    }

    pub fn choke_group(&self) -> u8 {
        self.cfg.choke
    }

    /// Where it goes: 0 the sampler's strip, 1–8 a group (#220).
    pub fn out(&self) -> usize {
        usize::from(self.cfg.out)
    }

    /// Hit the pad `note` plays at `velocity` (0..=1); `master` is the kit's own level.
    pub fn press(&mut self, note: u8, velocity: f32, kit: &PadKit, master: f32, sample_rate: f32) {
        let Some(pad) = pad_of(note) else {
            return;
        };
        let cfg = kit.pad(pad).copied().unwrap_or_default();
        let v = velocity.clamp(0.0, 1.0);
        // Pan as equal power, worked out once per hit.
        let angle = (cfg.pan + 1.0) * std::f32::consts::FRAC_PI_4;
        *self = PadVoice {
            pad,
            note,
            cfg,
            gain: (1.0 - cfg.vel_level + cfg.vel_level * v) * cfg.level * master,
            rate: sample_rate,
            left: angle.cos(),
            right: angle.sin(),
            inc: f64::from((cfg.tune / 12.0).exp2()),
            pos: 0.0,
            fresh: cfg.sample.is_some(),
            // A soft hit starts up to half way into the sample.
            start: (1.0 - v) * cfg.vel_start * 0.5,
            on: false,
            env: 1.0,
            env_k: if cfg.decay > 0.0 {
                (-6.907_755 / (cfg.decay * sample_rate)).exp()
            } else {
                1.0
            },
            fade: 1.0,
            fade_step: 0.0,
        };
    }

    fn fade_over(&mut self, secs: f32) {
        if self.active() && self.rate > 0.0 {
            self.fade_step = 1.0 / (secs * self.rate);
        }
    }

    /// A hit of the same group: silence this one quickly.
    pub fn choke(&mut self) {
        self.fade_over(CHOKE_SECS);
    }

    /// The key came up: a pad that is not a one-shot fades.
    pub fn release(&mut self, note: u8) {
        if note == self.note && !self.cfg.one_shot {
            self.fade_over(RELEASE_SECS);
        }
    }

    /// Everything off (a panic): fade at once, one-shot or not.
    pub fn release_all(&mut self) {
        self.fade_over(RELEASE_SECS);
    }

    /// Add this pad into `left` and `right`.
    pub fn render(&mut self, store: &SampleStore, left: &mut [f32], right: &mut [f32]) {
        let Some(sample) = self.cfg.sample.and_then(|s| store.get(s)) else {
            self.on = false;
            self.fresh = false;
            return;
        };
        if self.fresh {
            self.pos = f64::from(self.start) * sample.frames() as f64;
            self.on = true;
            self.fresh = false;
        }
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if !self.on {
                return;
            }
            if self.pos >= sample.frames() as f64 {
                self.on = false;
                return;
            }
            let y = read(sample, self.pos) * self.env * self.fade * self.gain;
            *l += y * self.left;
            *r += y * self.right;
            self.pos += self.inc;
            self.env *= self.env_k;
            if self.fade_step > 0.0 {
                self.fade -= self.fade_step;
                if self.fade <= 0.0 {
                    self.on = false;
                }
            }
            // 80 dB down: nothing left to hear.
            if self.env < 1.0e-4 {
                self.on = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{BLOCK, Engine};
    use crate::mono::preset::Preset;
    use crate::params::Param;
    use crate::sample::test_wav;

    const SR: f32 = 48_000.0;

    #[test]
    fn notes_map_to_pads() {
        assert_eq!(pad_of(36), Some(0));
        assert_eq!(pad_of(51), Some(15));
        assert_eq!(pad_of(35), None);
        assert_eq!(pad_of(52), None);
    }

    #[test]
    fn fields_clamp_and_read_back() {
        let mut c = PadCfg::default();
        assert_eq!(c.get(PadField::Sample), -1.0);
        for (field, name) in PadField::ALL {
            let v = match field {
                PadField::Sample => 7.0,
                PadField::Tune => -5.0,
                PadField::Level => 1.5,
                PadField::Pan => -0.5,
                PadField::Decay => 0.25,
                PadField::Choke => 3.0,
                PadField::VelLevel => 0.5,
                PadField::VelStart => 0.75,
                PadField::OneShot => 0.0,
                PadField::Out => 5.0,
            };
            c.set(field, v);
            assert_eq!(c.get(field), v, "{name}");
        }
        c.set(PadField::Pan, 9.0);
        c.set(PadField::Tune, f32::NAN);
        c.set(PadField::Sample, 9999.0);
        assert_eq!((c.pan, c.tune, c.sample), (1.0, 0.0, None));
    }

    // --- through the engine ------------------------------------------------

    /// A pad sampler on synth 0 with `waves` (values, loop-free) in slots 0.. .
    fn rig(waves: &[Vec<f32>]) -> Engine {
        let mut e = Engine::new(SR);
        e.preset(0, Preset::PadsLoud);
        for (slot, data) in waves.iter().enumerate() {
            let file = test_wav(48_000, data, None);
            e.sample_buffer(file.len())
                .expect("fits")
                .copy_from_slice(&file);
            e.load_sample(slot).expect("loads");
        }
        e
    }

    fn sine(frames: usize, period: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| (i as f32 / period * std::f32::consts::TAU).sin() * 0.9)
            .collect()
    }

    /// Both channels of the next `blocks` blocks.
    fn stereo(e: &mut Engine, blocks: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (Vec::new(), Vec::new());
        for _ in 0..blocks {
            e.render(BLOCK);
            let out = e.output();
            l.extend_from_slice(out.get(..BLOCK).unwrap_or(&[]));
            r.extend_from_slice(out.get(BLOCK..).unwrap_or(&[]));
        }
        (l, r)
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0, |m, v| m.max(v.abs()))
    }

    fn crossings(x: &[f32]) -> usize {
        x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    fn set(e: &mut Engine, pad: usize, field: PadField, v: f32) {
        e.set_pad(0, pad, field, v);
    }

    #[test]
    fn a_hit_plays_its_sample_and_tune_moves_the_pitch() {
        let mut e = rig(&[sine(48_000, 100.0)]);
        set(&mut e, 0, PadField::Sample, 0.0);
        e.note_on(0, 36, 1.0);
        let (l, _) = stereo(&mut e, 100);
        assert!(
            l.iter().all(|v| v.is_finite()) && peak(&l) > 0.05,
            "it sounds"
        );
        let n = crossings(&l);
        assert!((126..=130).contains(&n), "recorded pitch: {n}");
        e.all_off();
        stereo(&mut e, 20);
        set(&mut e, 0, PadField::Tune, 12.0);
        e.note_on(0, 36, 1.0);
        let n = crossings(&stereo(&mut e, 100).0);
        assert!((252..=260).contains(&n), "an octave up: {n}");
    }

    #[test]
    fn a_pad_without_a_sample_or_a_note_off_the_pads_is_silent() {
        let mut e = rig(&[sine(4800, 100.0)]);
        set(&mut e, 1, PadField::Sample, 0.0);
        e.note_on(0, 36, 1.0);
        e.note_on(0, 60, 1.0);
        assert!(peak(&stereo(&mut e, 10).0) < 1.0e-4);
        set(&mut e, 2, PadField::Sample, 9.0);
        e.note_on(0, 38, 1.0);
        assert!(
            peak(&stereo(&mut e, 10).0) < 1.0e-4,
            "an empty slot is silence"
        );
    }

    #[test]
    fn a_one_shot_ignores_the_key_coming_up_and_another_pad_fades() {
        let mut e = rig(&[sine(48_000, 100.0)]);
        for pad in [0, 1] {
            set(&mut e, pad, PadField::Sample, 0.0);
        }
        set(&mut e, 1, PadField::OneShot, 0.0);
        e.note_on(0, 36, 1.0);
        e.note_on(0, 37, 1.0);
        stereo(&mut e, 4);
        e.note_off(0, 36);
        e.note_off(0, 37);
        let (l, _) = stereo(&mut e, 10);
        let tail = l.get(l.len() - BLOCK..).unwrap_or(&[]);
        assert!(peak(tail) > 0.05, "the one-shot rings on");
        assert_eq!(e.active_voices(), 1, "the other pad faded and ended");
    }

    #[test]
    fn decay_shortens_a_pad() {
        let mut e = rig(&[sine(48_000, 100.0)]);
        set(&mut e, 0, PadField::Sample, 0.0);
        set(&mut e, 0, PadField::Decay, 0.05);
        e.note_on(0, 36, 1.0);
        let (l, _) = stereo(&mut e, 40);
        let late = l.get(l.len() - 4 * BLOCK..).unwrap_or(&[]);
        assert!(peak(late) < 0.01, "gone after 0.1 s: {}", peak(late));
        assert_eq!(e.active_voices(), 0);
    }

    #[test]
    fn a_choke_group_silences_the_others_and_a_pad_retriggers() {
        let mut e = rig(&[sine(48_000, 100.0)]);
        for pad in [6, 10, 3] {
            set(&mut e, pad, PadField::Sample, 0.0);
        }
        set(&mut e, 6, PadField::Choke, 1.0);
        set(&mut e, 10, PadField::Choke, 1.0);
        e.note_on(0, 36 + 10, 1.0);
        e.note_on(0, 36 + 3, 1.0);
        stereo(&mut e, 4);
        assert_eq!(e.active_voices(), 2);
        e.note_on(0, 36 + 6, 1.0);
        stereo(&mut e, 4);
        assert_eq!(
            e.active_voices(),
            2,
            "pad 10 was choked; pad 3 is in no group"
        );
        // The same pad again takes the voice it has.
        e.note_on(0, 36 + 6, 1.0);
        e.note_on(0, 36 + 6, 1.0);
        stereo(&mut e, 2);
        assert_eq!(e.active_voices(), 2);
    }

    #[test]
    fn velocity_moves_level_and_start() {
        let mut e = rig(&[sine(48_000, 100.0)]);
        set(&mut e, 0, PadField::Sample, 0.0);
        e.note_on(0, 36, 1.0);
        let hard = peak(&stereo(&mut e, 8).0);
        e.all_off();
        stereo(&mut e, 20);
        e.note_on(0, 36, 0.25);
        let soft = peak(&stereo(&mut e, 8).0);
        assert!(soft < hard * 0.4, "soft {soft} hard {hard}");
        e.all_off();
        stereo(&mut e, 20);
        set(&mut e, 0, PadField::VelLevel, 0.0);
        e.note_on(0, 36, 0.25);
        let fixed = peak(&stereo(&mut e, 8).0);
        assert!(
            (fixed - hard).abs() < hard * 0.1,
            "fixed {fixed} hard {hard}"
        );

        // Start point: a quiet first sixth, then a tone.
        let mut half = vec![0.0; 8_000];
        half.extend(sine(40_000, 100.0));
        let mut e = rig(&[half]);
        set(&mut e, 0, PadField::Sample, 0.0);
        set(&mut e, 0, PadField::VelStart, 1.0);
        e.note_on(0, 36, 1.0);
        assert!(
            peak(&stereo(&mut e, 4).0) < 0.01,
            "a hard hit starts at the top"
        );
        e.all_off();
        stereo(&mut e, 20);
        e.note_on(0, 36, 0.2);
        assert!(
            peak(&stereo(&mut e, 4).0) > 0.001,
            "a soft hit starts later, in the tone"
        );
    }

    #[test]
    fn pan_places_the_pad_between_the_sides() {
        let mut e = rig(&[sine(48_000, 100.0)]);
        set(&mut e, 0, PadField::Sample, 0.0);
        set(&mut e, 0, PadField::Pan, -1.0);
        e.note_on(0, 36, 1.0);
        let (l, r) = stereo(&mut e, 20);
        assert!(
            peak(&l) > 0.05 && peak(&r) < 1.0e-3,
            "hard left: {} {}",
            peak(&l),
            peak(&r)
        );
        e.all_off();
        stereo(&mut e, 20);
        set(&mut e, 0, PadField::Pan, 1.0);
        e.note_on(0, 36, 1.0);
        let (l, r) = stereo(&mut e, 20);
        assert!(peak(&r) > 0.05 && peak(&l) < 1.0e-3, "hard right");
        e.all_off();
        stereo(&mut e, 20);
        set(&mut e, 0, PadField::Pan, 0.0);
        e.note_on(0, 36, 1.0);
        let (l, r) = stereo(&mut e, 20);
        assert!((peak(&l) - peak(&r)).abs() < 1.0e-3, "centre");
    }

    #[test]
    fn sixteen_pads_at_once_stay_bounded() {
        let mut e = rig(&[sine(48_000, 100.0)]);
        for pad in 0..PADS {
            set(&mut e, pad, PadField::Sample, 0.0);
            set(&mut e, pad, PadField::Level, 2.0);
            e.note_on(0, 36 + pad as u8, 1.0);
        }
        let (l, r) = stereo(&mut e, 30);
        assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() <= 1.0));
        assert!(e.active_voices() <= PADS);
    }

    // --- a pad's own out (#220) ----------------------------------------------

    const GROUP_3: usize = crate::engine::SYNTHS + 2;

    /// One hit on pad 0 of a pad sampler at synth 0, after `setup`: the meters, then both channels.
    fn pad_hit(setup: &dyn Fn(&mut Engine)) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let mut e = rig(&[sine(24_000, 100.0)]);
        set(&mut e, 0, PadField::Sample, 0.0);
        setup(&mut e);
        e.clear_meters();
        e.note_on(0, 36, 0.8);
        let (l, r) = stereo(&mut e, 20);
        (e.meters().to_vec(), l, r)
    }

    /// The same, with the two channels as one.
    fn pad_meters(setup: &dyn Fn(&mut Engine)) -> (Vec<f32>, Vec<f32>) {
        let (meters, l, r) = pad_hit(setup);
        (meters, [l, r].concat())
    }

    #[test]
    fn a_pad_on_a_group_goes_only_through_that_group() {
        let (main_meters, main_out) = pad_meters(&|_| {});
        assert!(main_meters[0] > 0.0 && main_meters[GROUP_3] == 0.0);
        let (meters, out) = pad_meters(&|e| set(e, 0, PadField::Out, 3.0));
        assert_eq!(meters[0], 0.0, "not on the sampler's strip");
        assert!(meters[GROUP_3] > 0.0, "on group 3");
        assert!(
            out.iter().any(|x| *x != 0.0) && out.iter().all(|x| x.is_finite() && x.abs() <= 1.0)
        );
        // The group's fader applies; the sampler's own mute does not.
        let (_, down) = pad_meters(&|e| {
            set(e, 0, PadField::Out, 3.0);
            e.set_param(GROUP_3, Param::Level, 0.0);
        });
        assert!(down.iter().all(|x| *x == 0.0));
        let (_, strip_muted) = pad_meters(&|e| {
            set(e, 0, PadField::Out, 3.0);
            e.set_param(0, Param::Mute, 1.0);
        });
        assert_eq!(
            strip_muted, out,
            "an individual out bypasses the sampler's strip"
        );
        // Back on the strip, bit for bit as a fresh pad.
        let (_, back) = pad_meters(&|e| {
            set(e, 0, PadField::Out, 3.0);
            set(e, 0, PadField::Out, 0.0);
        });
        assert_eq!(back, main_out);
    }

    #[test]
    fn a_pad_keeps_its_pan_into_its_group() {
        let (_, left, right) = pad_hit(&|e| {
            set(e, 0, PadField::Out, 3.0);
            set(e, 0, PadField::Pan, -1.0);
        });
        assert!(left.iter().any(|x| *x != 0.0));
        assert!(
            right.iter().all(|x| x.abs() < 1.0e-6),
            "nothing on the right"
        );
    }

    #[test]
    fn solos_follow_a_pad_samplers_outs() {
        let heard = |setup: &dyn Fn(&mut Engine)| pad_meters(setup).1.iter().any(|x| *x != 0.0);
        assert!(
            heard(&|e| {
                set(e, 0, PadField::Out, 3.0);
                e.set_param(0, Param::Solo, 1.0);
            }),
            "soloing the sampler keeps the group its pad goes to"
        );
        assert!(
            !heard(&|e| {
                set(e, 0, PadField::Out, 3.0);
                e.set_param(1, Param::Solo, 1.0);
            }),
            "soloing another synth silences it"
        );
    }
}
