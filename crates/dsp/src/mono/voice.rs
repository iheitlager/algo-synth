//! One monophonic Mono voice per owner (spec 004 Req 6).
//!
//! A `MonoVoice` keeps the keys its owner holds and sounds one of them by
//! priority (last, low or high). With legato on, a new key while another is
//! held moves the pitch without retriggering the envelope; releasing a key
//! falls back to the next held one the same way. Glide moves the pitch in a
//! straight line of semitones, arriving in exactly the glide time.
//!
//! Key events arrive between blocks; the envelope acts on them at the next
//! block, so a note shorter than a block still sounds, held for that block.
//! Pitch comes from `PitchTable` every sample: no `exp2` in `render`
//! (ADR-0002), so glide (and later modulation) can move it per sample.

use crate::mono::env::{Env, Stage};
use crate::mono::ladder::{Ladder, LadderTables};
use crate::mono::noise::Noise;
use crate::mono::osc::{Blep, Osc};
use crate::mono::{MonoParams, VCOS};
use crate::voice::midi_to_hz;

/// Keys a voice remembers; pressing one more forgets the oldest.
pub const KEYS: usize = 16;
/// Mono's level after the ladder: one VCO at full level comes out near the
/// previous preview voice's 0.35.
const MONO_GAIN: f32 = 0.7;

/// Which held key sounds; the ids are mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum NotePriority {
    #[default]
    Last = 0,
    Low = 1,
    High = 2,
}

impl NotePriority {
    /// Every priority with the name the TypeScript mirror uses.
    pub const ALL: [(NotePriority, &'static str); 3] = [
        (NotePriority::Last, "Last"),
        (NotePriority::Low, "Low"),
        (NotePriority::High, "High"),
    ];

    /// The priority for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<NotePriority> {
        Self::ALL
            .iter()
            .find(|(p, _)| *p as u32 == id)
            .map(|(p, _)| *p)
    }
}

/// Lowest and highest table note: room for coarse tune, glide and
/// modulation around the MIDI range.
const LO_NOTE: f32 = -48.0;
const HI_NOTE: f32 = 180.0;
/// Table entries per semitone, linearly interpolated: about 0.003 cents.
const STEPS: f32 = 16.0;

/// A fractional MIDI note to cycles per sample. Built once in `Engine::new`.
pub struct PitchTable {
    inc: Vec<f32>,
}

impl PitchTable {
    pub fn new(sample_rate: f32) -> PitchTable {
        let n = ((HI_NOTE - LO_NOTE) * STEPS) as usize + 1;
        let a4 = f64::from(midi_to_hz(69));
        let inc = (0..n)
            .map(|i| {
                let note = f64::from(LO_NOTE) + i as f64 / f64::from(STEPS);
                (a4 * ((note - 69.0) / 12.0).exp2() / f64::from(sample_rate)) as f32
            })
            .collect();
        PitchTable { inc }
    }

    /// Cycles per sample at `note`, clamped to the table's range.
    pub fn at(&self, note: f32) -> f32 {
        let pos = ((note - LO_NOTE) * STEPS).clamp(0.0, (HI_NOTE - LO_NOTE) * STEPS);
        let i = pos as usize;
        let frac = pos - i as f32;
        let last = self.inc.last().copied().unwrap_or(0.0);
        let a = self.inc.get(i).copied().unwrap_or(last);
        let b = self.inc.get(i + 1).copied().unwrap_or(last);
        a + (b - a) * frac
    }
}

/// What a Mono voice reads from the engine while it renders.
pub struct MonoCtx<'a> {
    pub params: &'a MonoParams,
    pub sine: &'a [f32],
    pub blep: &'a Blep,
    pub ladder: &'a LadderTables,
    pub pitch: &'a PitchTable,
}

#[derive(Clone, Copy, Default)]
pub struct MonoVoice {
    /// Held keys and their velocities, oldest first.
    keys: [(u8, f32); KEYS],
    held: usize,
    /// The key that sounds, and its velocity.
    note: u8,
    velocity: f32,
    /// Whether a key is held, and whether the next block opens the gate
    /// afresh (a new note, not a legato change).
    gate: bool,
    retrigger: bool,
    /// Pitch in MIDI notes: where it is and where it glides.
    pitch: f32,
    target: f32,
    glide_step: f32,
    glide_left: u32,
    osc: [Osc; VCOS],
    ladder: Ladder,
    adsr: Env,
    noise: Noise,
}

impl MonoVoice {
    pub fn new(seed: u32) -> MonoVoice {
        MonoVoice {
            noise: Noise::new(seed),
            ladder: Ladder::new(),
            ..MonoVoice::default()
        }
    }

    /// Sounding: gated, releasing, or about to start.
    pub fn active(&self) -> bool {
        self.retrigger || self.adsr.stage != Stage::Idle
    }

    /// A key is held.
    pub fn gated(&self) -> bool {
        self.gate
    }

    /// The key that sounds (or sounded last).
    pub fn note(&self) -> u8 {
        self.note
    }

    /// The current pitch in MIDI notes, gliding or not.
    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    /// Press `note`. It sounds if the priority picks it; then the envelope
    /// retriggers unless legato is on and another key was already held.
    pub fn press(&mut self, note: u8, velocity: f32, p: &MonoParams) {
        let was_held = self.held > 0;
        self.forget(note);
        if self.held == KEYS {
            self.keys.copy_within(1.., 0);
            self.held -= 1;
        }
        if let Some(slot) = self.keys.get_mut(self.held) {
            *slot = (note, velocity.clamp(0.0, 1.0));
            self.held += 1;
        }
        if !self.active() {
            // From silence: a fresh filter, which also seeds self-oscillation.
            self.ladder = Ladder::new();
        }
        let before = self.note;
        self.sound_chosen(p, was_held);
        // A new note from no keys retriggers; so does a change of note
        // without legato. Legato changes only the pitch.
        if !was_held || (!p.legato && self.note != before) {
            self.retrigger = true;
        }
        self.gate = true;
    }

    /// Release `note`; fall back to the next held key by priority, or close
    /// the gate when none is left.
    pub fn release(&mut self, note: u8, p: &MonoParams) {
        if !self.forget(note) {
            return;
        }
        if self.held == 0 {
            self.gate = false;
        } else {
            self.sound_chosen(p, true);
        }
    }

    /// Release every key.
    pub fn release_all(&mut self) {
        self.held = 0;
        self.gate = false;
    }

    /// Drop `note` from the held keys; false if it wasn't held.
    fn forget(&mut self, note: u8) -> bool {
        let held = self.keys.get(..self.held).unwrap_or(&[]);
        let Some(i) = held.iter().position(|(n, _)| *n == note) else {
            return false;
        };
        if let Some(tail) = self.keys.get_mut(i..self.held) {
            tail.rotate_left(1);
        }
        self.held -= 1;
        true
    }

    /// Sound the key the priority picks; glide to it if a key was held.
    fn sound_chosen(&mut self, p: &MonoParams, glide: bool) {
        let held = self.keys.get(..self.held).unwrap_or(&[]);
        let pick = match p.priority {
            NotePriority::Last => held.last(),
            NotePriority::Low => held.iter().min_by_key(|(n, _)| *n),
            NotePriority::High => held.iter().max_by_key(|(n, _)| *n),
        };
        let Some(&(note, velocity)) = pick else {
            return;
        };
        self.note = note;
        self.velocity = velocity;
        self.target = f32::from(note);
        let samples = p.glide.max(0.0) as u32;
        if glide && samples > 0 {
            self.glide_left = samples;
            self.glide_step = (self.target - self.pitch) / samples as f32;
        } else {
            self.glide_left = 0;
            self.pitch = self.target;
        }
    }

    /// Add this voice into `out`, advancing its state.
    pub fn render(&mut self, ctx: &MonoCtx, out: &mut [f32]) {
        let p = ctx.params;
        if self.retrigger {
            self.retrigger = false;
            self.adsr.gate_on(&p.adsr);
        } else if self.gate && !self.adsr.gated() {
            self.adsr.gate_on(&p.adsr);
        } else if !self.gate && self.adsr.gated() {
            self.adsr.gate_off(&p.adsr);
        }
        self.adsr.set_sustain(p.adsr.sustain);
        for (osc, wave) in self.osc.iter_mut().zip(p.wave) {
            osc.wave = wave;
        }
        let [t1, t2, t3] = p.tune;
        let [_, sync2, sync3] = p.sync;
        let [l1, l2, l3] = p.level;
        let (noise_level, colour) = (p.noise_level, p.noise_colour);
        for sample in out.iter_mut() {
            let env = self.adsr.step();
            if self.adsr.stage == Stage::Idle {
                return;
            }
            if self.glide_left > 0 {
                self.glide_left -= 1;
                self.pitch = if self.glide_left == 0 {
                    self.target
                } else {
                    self.pitch + self.glide_step
                };
            }
            let [o1, o2, o3] = &mut self.osc;
            o1.set_increment(ctx.pitch.at(self.pitch + t1));
            o2.set_increment(ctx.pitch.at(self.pitch + t2));
            o3.set_increment(ctx.pitch.at(self.pitch + t3));
            let (y1, wrap) = o1.step(ctx.blep, ctx.sine, p.pulse_width, None);
            let (y2, _) = o2.step(ctx.blep, ctx.sine, p.pulse_width, wrap.filter(|_| sync2));
            let (y3, _) = o3.step(ctx.blep, ctx.sine, p.pulse_width, wrap.filter(|_| sync3));
            let noise = if noise_level > 0.0 {
                self.noise.sample(colour) * noise_level
            } else {
                0.0
            };
            let mix = y1 * l1 + y2 * l2 + y3 * l3 + noise;
            // Half the mix keeps two VCOs at full level below the knee.
            let y = self
                .ladder
                .process(ctx.ladder, 0.5 * mix, p.cutoff, p.k, p.drive);
            *sample += y * MONO_GAIN * env * self.velocity;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Param;
    use crate::voice::sine_table;

    const SR: f32 = 48_000.0;

    struct Rig {
        params: MonoParams,
        sine: Vec<f32>,
        blep: Blep,
        ladder: LadderTables,
        pitch: PitchTable,
        voice: MonoVoice,
    }

    impl Rig {
        fn new(settings: &[(Param, f32)]) -> Rig {
            let mut params = MonoParams::new(SR);
            for (p, v) in settings {
                params.set(*p, p.clamp(*v));
            }
            Rig {
                params,
                sine: sine_table(),
                blep: Blep::new(),
                ladder: LadderTables::new(SR),
                pitch: PitchTable::new(SR),
                voice: MonoVoice::new(1),
            }
        }

        fn press(&mut self, note: u8) {
            self.voice.press(note, 1.0, &self.params);
        }

        fn release(&mut self, note: u8) {
            self.voice.release(note, &self.params);
        }

        fn render(&mut self, frames: usize) -> Vec<f32> {
            let ctx = MonoCtx {
                params: &self.params,
                sine: &self.sine,
                blep: &self.blep,
                ladder: &self.ladder,
                pitch: &self.pitch,
            };
            let mut out = vec![0.0; frames];
            for chunk in out.chunks_mut(128) {
                self.voice.render(&ctx, chunk);
            }
            out
        }
    }

    #[test]
    fn priority_falls_back_on_release() {
        let mut r = Rig::new(&[]);
        for n in [60, 64, 67] {
            r.press(n);
        }
        assert_eq!(r.voice.note(), 67);
        r.release(67);
        assert_eq!(r.voice.note(), 64);
        r.release(60);
        assert_eq!(r.voice.note(), 64);
        r.release(64);
        assert!(!r.voice.gated());

        // Low and high pick from what is held, whatever the order: release
        // the sounding key, then the key at the other end.
        for (priority, first, other, want) in
            [(1.0, 60, 67, [60, 64, 64]), (2.0, 67, 60, [67, 64, 64])]
        {
            let mut r = Rig::new(&[(Param::Priority, priority)]);
            for n in [64, 60, 67] {
                r.press(n);
            }
            let mut got = vec![r.voice.note()];
            r.release(first);
            got.push(r.voice.note());
            r.release(other);
            got.push(r.voice.note());
            assert_eq!(got, want, "priority {priority}");
        }
    }

    #[test]
    fn legato_keeps_the_envelope() {
        for (legato, retriggers) in [(1.0, false), (0.0, true)] {
            let mut r = Rig::new(&[(Param::Legato, legato), (Param::AdsrDecay, 0.01)]);
            r.press(60);
            r.render(4_800);
            assert_eq!(r.voice.adsr.stage, Stage::Sustain);
            r.press(62);
            r.render(1);
            assert_eq!(r.voice.note(), 62);
            assert_eq!(
                r.voice.adsr.stage == Stage::Attack,
                retriggers,
                "legato {legato}"
            );
        }
    }

    #[test]
    fn glide_time() {
        let mut r = Rig::new(&[(Param::Glide, 0.1)]);
        r.press(48);
        r.render(480);
        assert_eq!(r.voice.pitch(), 48.0, "the first note doesn't glide");
        r.press(60);
        r.render(2_400);
        let half = r.voice.pitch();
        assert!((half - 54.0).abs() < 0.01, "linear in semitones: {half}");
        r.render(2_399);
        assert!(r.voice.pitch() < 60.0, "not there 1 sample early");
        r.render(1);
        assert_eq!(r.voice.pitch(), 60.0, "there after exactly 100 ms");
    }

    #[test]
    fn a_tap_shorter_than_a_block_still_sounds() {
        let mut r = Rig::new(&[(Param::AdsrAttack, 0.001), (Param::AdsrRelease, 0.01)]);
        r.press(69);
        r.release(69);
        let out = r.render(4_800);
        assert!(out.iter().any(|s| s.abs() > 0.01));
        assert!(!r.voice.active());
    }

    #[test]
    fn a_full_key_stack_forgets_the_oldest() {
        let mut r = Rig::new(&[]);
        for n in 40..40 + KEYS as u8 + 4 {
            r.press(n);
        }
        // 40..=43 were forgotten: releasing the rest closes the gate.
        for n in 44..40 + KEYS as u8 + 4 {
            r.release(n);
        }
        assert!(!r.voice.gated());
    }

    /// The table gives spec 001 Req 7's pitch within 0.01 cent everywhere.
    #[test]
    fn pitch_table_is_equal_tempered() {
        let t = PitchTable::new(SR);
        let mut note = -24.0;
        while note <= 150.0 {
            let exact = 440.0 * ((f64::from(note) - 69.0) / 12.0).exp2() / f64::from(SR);
            let cents = 1200.0 * (f64::from(t.at(note)) / exact).log2();
            assert!(cents.abs() < 0.01, "note {note}: {cents} cents");
            note += 0.37;
        }
    }

    /// Coarse and fine tune reach the oscillator: A4 + 7 semitones + 50
    /// cents, measured by zero crossings.
    #[test]
    fn tune_is_heard_within_a_cent() {
        let mut r = Rig::new(&[
            (Param::Vco1Wave, 3.0),
            (Param::Vco1Coarse, 7.0),
            (Param::Vco1Fine, 50.0),
            (Param::Cutoff, 20_000.0),
            (Param::AdsrSustain, 1.0),
        ]);
        r.press(69);
        let y = r.render(SR as usize);
        let c: Vec<f64> = y[4_800..]
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
            .map(|(i, w)| i as f64 + f64::from(w[0] / (w[0] - w[1])))
            .collect();
        let hz = (c.len() - 1) as f64 * f64::from(SR) / (c[c.len() - 1] - c[0]);
        let want = 440.0 * (7.5_f64 / 12.0).exp2();
        let cents = 1200.0 * (hz / want).log2();
        assert!(cents.abs() < 1.0, "{hz} Hz, {cents} cents");
    }
}
