//! One monophonic Mono voice per owner (spec 004 Req 6).
//!
//! A `MonoVoice` keeps the keys its owner holds and sounds one of them by
//! priority (last, low or high). With legato on, a new key while another is
//! held moves the pitch without retriggering the envelope; releasing a key
//! falls back to the next held one the same way. Glide moves the pitch in a
//! straight line of semitones, arriving in exactly the glide time.
//!
//! Key events arrive between blocks; the envelopes act on them at the next
//! block, so a note shorter than a block still sounds, held for that block.
//! Every sample the voice reads its modulation sources (VCOs, noise, ADSR,
//! AR, LFO, S&H, mod wheel, velocity, key) and sums the normalled
//! connections and the patch (`mono::patch`). Pitch comes from `PitchTable`
//! and cutoff from the ladder's table, so both move every sample without
//! `exp2` in `render` (ADR-0002); the LFO rate follows its modulation once
//! per block.

use crate::mono::env::{Env, Stage};
use crate::mono::ladder::MAX_K;
use crate::mono::ladder::{Ladder, LadderTables};
use crate::mono::lfo::Lfo;
use crate::mono::model::{Filter, Hp};
use crate::mono::noise::Noise;
use crate::mono::osc::{Blep, Osc};
use crate::mono::patch::{ModDest, ModSource, Mods, SOURCES, Sources, is_taken, modulate};
use crate::mono::svf::{OnePole, Svf};
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
    /// The 12 dB filters (low-pass and high-pass stage) and the one-pole
    /// high-pass, for the models that use them.
    svf_lp: Svf,
    svf_hp: Svf,
    pole_hp: OnePole,
    adsr: Env,
    ar: Env,
    fadsr: Env,
    lfo: Lfo,
    noise: Noise,
    /// Last sample's VCO outputs, as modulation sources.
    last: [f32; VCOS],
    /// Last sample's modulation, per destination.
    mods: Mods,
    /// A patch drives the VCA, so the AR (not only the ADSR) can keep the
    /// voice sounding.
    vca_patched: bool,
}

impl MonoVoice {
    pub fn new(seed: u32) -> MonoVoice {
        MonoVoice {
            noise: Noise::new(seed),
            ladder: Ladder::new(),
            svf_lp: Svf::new(),
            svf_hp: Svf::new(),
            ..MonoVoice::default()
        }
    }

    /// Sounding: gated, releasing, or about to start.
    pub fn active(&self) -> bool {
        self.retrigger
            || self.adsr.stage != Stage::Idle
            || (self.vca_patched && self.ar.stage != Stage::Idle)
    }

    /// Last sample's modulation, per destination.
    pub fn mods(&self) -> Mods {
        self.mods
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
            self.svf_lp = Svf::new();
            self.svf_hp = Svf::new();
            self.pole_hp = OnePole::default();
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
        for (env, times) in [
            (&mut self.adsr, &p.adsr),
            (&mut self.ar, &p.ar),
            (&mut self.fadsr, &p.fadsr),
        ] {
            if self.retrigger || (self.gate && !env.gated()) {
                env.gate_on(times);
            } else if !self.gate && env.gated() {
                env.gate_off(times);
            }
        }
        self.retrigger = false;
        self.vca_patched = is_taken(&p.taken, ModDest::Vca);
        self.adsr.set_sustain(p.adsr.sustain);
        self.fadsr.set_sustain(p.fadsr.sustain);
        for (osc, wave) in self.osc.iter_mut().zip(p.wave) {
            osc.wave = wave;
        }
        // Control rate: the LFO's speed follows its modulation per block.
        let lfo_inc = p.lfo_inc * self.mods.lfo_rate.clamp(-8.0, 8.0).exp2();
        let [t1, t2, t3] = p.tune;
        let [_, sync2, sync3] = p.sync;
        let [l1, l2, l3] = p.level;
        let (noise_level, colour) = (p.noise_level, p.noise_colour);
        let hp = p.model.hp();
        for sample in out.iter_mut() {
            let adsr = self.adsr.step();
            let ar = self.ar.step();
            let fenv = self.fadsr.step();
            if !self.active() {
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
            let (lfo, held) = self
                .lfo
                .step(lfo_inc, p.lfo_wave, ctx.sine, &mut self.noise);
            let noise = self.noise.sample(colour);
            let key = self.pitch - 60.0;
            let mut src: Sources = [0.0; SOURCES];
            for (s, v) in [
                (ModSource::Vco1, self.last[0]),
                (ModSource::Vco2, self.last[1]),
                (ModSource::Vco3, self.last[2]),
                (ModSource::Noise, noise),
                (ModSource::Adsr, adsr),
                (ModSource::Ar, ar),
                (ModSource::Fenv, fenv),
                (ModSource::Lfo, lfo),
                (ModSource::SampleHold, held),
                (ModSource::ModWheel, p.mod_wheel),
                (ModSource::Velocity, self.velocity),
                (ModSource::Key, key / 60.0),
            ] {
                if let Some(slot) = src.get_mut(s as usize) {
                    *slot = v;
                }
            }
            let m = modulate(&p.patch, &p.taken, &p.normals, &src, key);
            let [m1, m2, m3] = m.pitch;
            let pw = (p.pulse_width + m.pulse_width).clamp(0.05, 0.95);
            let [o1, o2, o3] = &mut self.osc;
            o1.set_increment(ctx.pitch.at(self.pitch + t1 + m1));
            o2.set_increment(ctx.pitch.at(self.pitch + t2 + m2));
            o3.set_increment(ctx.pitch.at(self.pitch + t3 + m3));
            let (y1, wrap) = o1.step(ctx.blep, ctx.sine, pw, None);
            let (y2, _) = o2.step(ctx.blep, ctx.sine, pw, wrap.filter(|_| sync2));
            let (y3, _) = o3.step(ctx.blep, ctx.sine, pw, wrap.filter(|_| sync3));
            let mix = y1 * l1 + y2 * l2 + y3 * l3 + noise * noise_level;
            // Half the mix keeps two VCOs at full level below the knee.
            let mut x = 0.5 * mix;
            let hp_cutoff = p.hp_cutoff + m.hp_cutoff;
            let filter = p.model.filter();
            if let (Hp::Svf, Filter::Svf(v)) = (hp, filter) {
                x = self
                    .svf_hp
                    .process(ctx.ladder, &v, x, hp_cutoff, p.hp_res)
                    .hp;
            }
            let mut y = match filter {
                Filter::Ladder(v) => {
                    let k = (p.k + m.resonance * MAX_K).clamp(0.0, MAX_K) * v.k_scale;
                    let x = x * (1.0 + v.comp * k);
                    self.ladder
                        .process(ctx.ladder, x, p.cutoff + m.cutoff, k, p.drive * v.drive)
                }
                Filter::Svf(v) => {
                    let res = p.k / MAX_K + m.resonance;
                    self.svf_lp
                        .process(ctx.ladder, &v, x, p.cutoff + m.cutoff, res)
                        .lp
                }
            };
            if hp == Hp::OnePole {
                y = self.pole_hp.process(ctx.ladder, y, hp_cutoff);
            }
            *sample += y * MONO_GAIN * m.vca * self.velocity;
            self.last = [y1, y2, y3];
            self.mods = m;
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

    /// Spec 004 Req 7: S&H → cutoff at 0.5 with the LFO at 4 Hz changes the
    /// cutoff exactly once per LFO period, and the ADSR no longer moves it.
    #[test]
    fn sample_and_hold_takes_over_the_cutoff() {
        let mut r = Rig::new(&[
            (Param::LfoRate, 4.0),
            (Param::EnvCutoff, 1.0),
            (Param::AdsrAttack, 0.3),
            (Param::Patch1Source, 8.0),
            (Param::Patch1Dest, 5.0),
            (Param::Patch1Amount, 0.5),
        ]);
        r.press(60);
        let mut cutoffs = Vec::new();
        for _ in 0..(SR as usize + 100) {
            r.render(1);
            cutoffs.push(r.voice.mods().cutoff);
        }
        let changes = cutoffs.windows(2).filter(|w| w[0] != w[1]).count();
        assert_eq!(changes, 4, "one change per LFO period in 1 s");
        // During the 300 ms attack the cutoff still only steps with the S&H.
        assert!(cutoffs[..11_000].windows(2).all(|w| w[0] == w[1]));
    }

    /// The normals: ADSR → cutoff, key tracking, vibrato through the wheel.
    /// Spec 004 Req 12: the ladder is voiced per model, and every voicing
    /// stays bounded at full resonance and drive.
    #[test]
    fn ladder_voicings_differ_and_stay_bounded() {
        let rms_of = |model: f32| {
            let mut r = Rig::new(&[
                (Param::Model, model),
                (Param::Cutoff, 1_200.0),
                (Param::Resonance, 1.0),
                (Param::Drive, 1.0),
                (Param::AdsrSustain, 1.0),
            ]);
            r.press(45);
            let out = r.render(24_000);
            assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 2.0));
            (out.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / out.len() as f64).sqrt()
        };
        // Moog (ARP 2600 and Minimoog), Pro-One, SH-101.
        let (moog, pro, sh) = (rms_of(0.0), rms_of(2.0), rms_of(5.0));
        assert_eq!(moog, rms_of(1.0), "the Minimoog ladder is the Moog voicing");
        assert!(moog > 0.0 && pro > 0.0 && sh > 0.0);
        assert!((moog - pro).abs() > 1.0e-3, "{moog} vs {pro}");
        assert!((moog - sh).abs() > 1.0e-3, "{moog} vs {sh}");
        assert!((pro - sh).abs() > 1.0e-3, "{pro} vs {sh}");
    }

    /// Spec 004 Req 11: on a model that uses the filter ADSR, the loudness
    /// and the cutoff have envelopes of their own.
    #[test]
    fn filter_envelope_is_independent() {
        let mut r = Rig::new(&[
            (Param::Model, 1.0),
            (Param::EnvCutoff, 1.0),
            (Param::AdsrAttack, 0.001),
            (Param::AdsrSustain, 1.0),
            (Param::FenvAttack, 1.0),
            (Param::FenvSustain, 1.0),
        ]);
        r.press(60);
        let mut reached = None;
        for i in 0..52_000 {
            r.render(1);
            let m = r.voice.mods();
            if i == 240 {
                assert!(m.vca > 0.99, "loudness is full after 5 ms: {}", m.vca);
                assert!(m.cutoff < 2.5, "the cutoff has barely moved: {}", m.cutoff);
            }
            if reached.is_none() && m.cutoff >= 47.99 {
                reached = Some(i);
            }
        }
        let at = reached.expect("the filter envelope peaks");
        assert!(
            (at as i64 - 48_000).abs() <= 48,
            "filter attack takes 1 s ± 1 ms, took {at} samples"
        );
    }

    /// The ARP 2600 has one envelope: its cutoff follows the ADSR, whatever
    /// the filter ADSR does.
    #[test]
    fn arp_cutoff_still_follows_the_adsr() {
        let mut r = Rig::new(&[
            (Param::EnvCutoff, 1.0),
            (Param::AdsrAttack, 0.001),
            (Param::AdsrSustain, 1.0),
            (Param::FenvAttack, 1.0),
        ]);
        r.press(60);
        r.render(2_400);
        assert!((r.voice.mods().cutoff - 48.0).abs() < 1.0e-3);
    }

    #[test]
    fn normalled_connections_move_their_destinations() {
        let mut r = Rig::new(&[(Param::EnvCutoff, 0.5), (Param::AdsrSustain, 1.0)]);
        r.press(60);
        r.render(4_800);
        assert!(
            (r.voice.mods().cutoff - 24.0).abs() < 1.0e-3,
            "ADSR → cutoff"
        );

        let cutoff_at = |note: u8| {
            let mut r = Rig::new(&[(Param::KeyTrack, 1.0)]);
            r.press(note);
            r.render(128);
            r.voice.mods().cutoff
        };
        assert!(
            (cutoff_at(72) - cutoff_at(60) - 12.0).abs() < 1.0e-4,
            "key tracking"
        );

        let pitch_range = |wheel: f32| {
            let mut r = Rig::new(&[
                (Param::Vibrato, 1.0),
                (Param::ModWheel, wheel),
                (Param::LfoRate, 5.0),
            ]);
            r.press(60);
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            for _ in 0..96 {
                r.render(100);
                let p = r.voice.mods().pitch[0];
                lo = lo.min(p);
                hi = hi.max(p);
            }
            hi - lo
        };
        assert_eq!(pitch_range(0.0), 0.0, "no wheel, no vibrato");
        assert!(
            (pitch_range(1.0) - 4.0).abs() < 0.05,
            "±2 semitones at full wheel"
        );
    }

    /// AR patched to the VCA keeps the voice sounding after the ADSR ends.
    #[test]
    fn ar_on_the_vca_shapes_the_note() {
        let mut r = Rig::new(&[
            (Param::AdsrRelease, 0.01),
            (Param::ArRelease, 0.5),
            (Param::Patch1Source, 6.0),
            (Param::Patch1Dest, 7.0),
            (Param::Patch1Amount, 1.0),
        ]);
        r.press(60);
        r.render(4_800);
        r.release(60);
        let tail = r.render(4_800);
        assert!(
            tail[4_000..].iter().any(|s| s.abs() > 1.0e-3),
            "still sounding at 100 ms"
        );
        r.render(48_000);
        assert!(!r.voice.active());
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
