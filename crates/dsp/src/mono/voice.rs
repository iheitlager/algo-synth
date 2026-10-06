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

use crate::engine::BLOCK;
use crate::mono::env::{Env, EnvTimes, Stage};
use crate::mono::ladder::MAX_K;
use crate::mono::ladder::{Ladder, LadderTables};
use crate::mono::lfo::Lfo;
use crate::mono::model::{Filter, Hp};
use crate::mono::noise::Noise;
use crate::mono::osc::{Blep, Osc, Waveform};
use crate::mono::patch::{ModDest, ModSource, Mods, SOURCES, Sources, is_taken, modulate};
use crate::mono::svf::{OnePole, Svf};
use crate::mono::{MonoParams, VCOS};
use crate::sample::SampleStore;
use crate::sampler::ZoneMap;
use crate::table::{TableOsc, Tables};
use crate::voice::Gains;
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

/// The LFO and sample-and-hold a polyphonic synth's pool computes once for all
/// its voices (spec 006 Req 4), one value per sample of the block.
pub struct SharedMod {
    pub lfo: [f32; BLOCK],
    pub held: [f32; BLOCK],
}

impl SharedMod {
    pub fn new() -> SharedMod {
        SharedMod {
            lfo: [0.0; BLOCK],
            held: [0.0; BLOCK],
        }
    }

    /// Run the pool's LFO for `n` samples.
    pub fn fill(
        &mut self,
        lfo: &mut Lfo,
        noise: &mut Noise,
        p: &MonoParams,
        sine: &[f32],
        n: usize,
    ) {
        for (l, h) in self.lfo.iter_mut().zip(self.held.iter_mut()).take(n) {
            (*l, *h) = lfo.step(p.lfo_inc, p.lfo_wave, sine, noise);
        }
    }
}

impl Default for SharedMod {
    fn default() -> SharedMod {
        SharedMod::new()
    }
}

/// The lookup tables the engine builds once and every voice reads.
#[derive(Clone, Copy)]
pub struct Tools<'a> {
    pub sine: &'a [f32],
    pub blep: &'a Blep,
    pub ladder: &'a LadderTables,
    pub pitch: &'a PitchTable,
    pub tables: &'a Tables,
    /// The sample store and this synth's zones, for the sampler's voices.
    pub samples: &'a SampleStore,
    pub zones: &'a ZoneMap,
}

/// What a Mono voice reads from the engine while it renders.
pub struct MonoCtx<'a> {
    pub params: &'a MonoParams,
    pub sine: &'a [f32],
    pub blep: &'a Blep,
    pub ladder: &'a LadderTables,
    pub pitch: &'a PitchTable,
    /// A poly synth's shared LFO; `None` gives the voice its own.
    pub shared: Option<&'a SharedMod>,
    /// The generated wavetables and samples.
    pub tables: &'a Tables,
}

#[derive(Clone, Copy, Default)]
pub struct MonoVoice {
    /// Semitones added to the oscillators' pitch and to the cutoff: a polyphonic
    /// pool's unison spread and analog variance, set once per block (spec 006 Req 3).
    pub trim: f32,
    pub cutoff_trim: f32,
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
    /// The wavetable oscillators of the models that use them, in place of VCO 1 and 2.
    tabs: [TableOsc; 2],
    /// The sub-oscillator: a square at a half or quarter of VCO 1's pitch.
    sub: Osc,
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
    /// The second LFO and the ramp, for the models with a modulation matrix.
    lfo2: Lfo,
    ramp: f32,
    noise: Noise,
    /// Last sample's VCO outputs, as modulation sources.
    last: [f32; VCOS],
    /// Last sample's modulation, per destination.
    mods: Mods,
    /// A patch drives the VCA, so the AR (not only the ADSR) can keep the
    /// voice sounding.
    vca_patched: bool,
    /// The VCO, noise, ring and sub levels, ramping from one block's to the
    /// next so a modulated level does not zipper (#271).
    levels: Gains<6>,
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
        // A voice starting from silence takes its levels at once; a sounding
        // one ramps to them (#271).
        let sounding =
            self.adsr.stage != Stage::Idle || (self.vca_patched && self.ar.stage != Stage::Idle);
        // Where a model's contours have no release knob, decay is the release.
        let times = |t: &EnvTimes| {
            if p.model.decay_is_release() {
                EnvTimes {
                    release: t.decay,
                    ..*t
                }
            } else {
                *t
            }
        };
        let (adsr_times, fadsr_times) = (times(&p.adsr), times(&p.fadsr));
        for (env, times) in [
            (&mut self.adsr, &adsr_times),
            (&mut self.ar, &p.ar),
            (&mut self.fadsr, &fadsr_times),
        ] {
            if self.retrigger || (self.gate && !env.gated()) {
                env.gate_on(times);
            } else if !self.gate && env.gated() {
                env.gate_off(times);
            }
        }
        if self.retrigger {
            self.ramp = 0.0;
            for t in self.tabs.iter_mut() {
                t.reset();
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
        let lfo2_inc = p.lfo2_inc * self.mods.lfo2_rate.clamp(-8.0, 8.0).exp2();
        let matrix = p.model.has_matrix();
        let tables = p.model.uses_tables();
        let [t1, t2, t3] = p.tune;
        let [_, sync2, sync3] = p.sync;
        let [v1, v2, v3] = p.level;
        let levels = [v1, v2, v3, p.noise_level, p.ring_level, p.sub_level];
        let colour = p.noise_colour;
        if !sounding {
            self.levels.jump(levels);
        }
        let moving = self.levels.aim(levels, out.len());
        let hp = p.model.hp();
        let filter_env_is_adsr = p.model.filter_env_is_adsr();
        for (i, sample) in out.iter_mut().enumerate() {
            let [l1, l2, l3, noise_level, ring_level, sub_level] =
                if moving { self.levels.tick() } else { levels };
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
            let (lfo, held) = match ctx.shared {
                Some(sm) => (
                    sm.lfo.get(i).copied().unwrap_or(0.0),
                    sm.held.get(i).copied().unwrap_or(0.0),
                ),
                None => self
                    .lfo
                    .step(lfo_inc, p.lfo_wave, ctx.sine, &mut self.noise),
            };
            let noise = self.noise.sample(colour);
            let lfo2 = if matrix {
                self.ramp = (self.ramp + p.ramp_inc).min(1.0);
                self.lfo2
                    .step(lfo2_inc, p.lfo2_wave, ctx.sine, &mut self.noise)
                    .0
            } else {
                0.0
            };
            let key = self.pitch - 60.0;
            let mut src: Sources = [0.0; SOURCES];
            for (s, v) in [
                (ModSource::Vco1, self.last[0]),
                (ModSource::Vco2, self.last[1]),
                (ModSource::Vco3, self.last[2]),
                (ModSource::Noise, noise),
                (ModSource::Adsr, adsr),
                (ModSource::Ar, ar),
                (ModSource::Lfo2, lfo2),
                (ModSource::Ramp, self.ramp),
                (
                    ModSource::Fenv,
                    if filter_env_is_adsr { adsr } else { fenv },
                ),
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
            let base = self.pitch + self.trim;
            let inc1 = ctx.pitch.at(base + t1 + m1);
            o1.set_increment(inc1);
            // The SH-101's pulse is VCO 1's own phase: same pitch, reset with it.
            let locked = p.model.pulse_locked();
            o2.set_increment(if locked {
                inc1
            } else {
                ctx.pitch.at(base + t2 + m2)
            });
            // Off the key and five octaves down, VCO 3 is a modulator.
            let key3 = if p.vco3_follow { base } else { 60.0 };
            let low3 = if p.vco3_low { 60.0 } else { 0.0 };
            o3.set_increment(ctx.pitch.at(key3 + t3 + m3 - low3));
            let (y1, wrap, y2) = if tables {
                // The envelope and the LFO move both wave positions.
                let [t1, t2] = &mut self.tabs;
                t1.set_increment(inc1);
                t2.set_increment(ctx.pitch.at(base + p.tune[1] + m2));
                let sweep = fenv * p.env_wt + lfo * p.lfo_wt;
                let pos =
                    |i: usize| (p.wt_pos.get(i).copied().unwrap_or(0.0) + sweep).clamp(0.0, 1.0);
                let tab = |i: usize| p.wt_table.get(i).copied().unwrap_or(0);
                (
                    t1.step(ctx.tables, tab(0), pos(0), p.wt_steps),
                    None,
                    t2.step(ctx.tables, tab(1), pos(1), p.wt_steps),
                )
            } else {
                let (y1, wrap) = o1.step(ctx.blep, ctx.sine, pw, None);
                let (y2, _) = o2.step(ctx.blep, ctx.sine, pw, wrap.filter(|_| sync2 || locked));
                (y1, wrap, y2)
            };
            // The rising saw starts low where a pulse is high, so a pulse in
            // the saw's phase would cancel it: the SH-101's is inverted.
            let y2 = if locked { -y2 } else { y2 };
            let (y3, _) = o3.step(ctx.blep, ctx.sine, pw, wrap.filter(|_| sync3));
            // The sub is a pulse at an exact fraction of VCO 1's increment, so
            // it stays an octave (or two) down through glide and vibrato.
            let sub = if sub_level > 0.0 {
                self.sub.wave = Waveform::Pulse;
                self.sub.set_increment(inc1 * p.sub_ratio);
                self.sub.step(ctx.blep, ctx.sine, 0.5, None).0
            } else {
                0.0
            };
            let mix = y1 * l1
                + y2 * l2
                + y3 * l3
                + noise * noise_level
                + y1 * y2 * ring_level
                + sub * sub_level;
            // Half the mix keeps two VCOs at full level below the knee.
            let mut x = 0.5 * mix;
            let hp_cutoff = p.hp_cutoff + m.hp_cutoff;
            let filter = p.filter();
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
                    self.ladder.process(
                        ctx.ladder,
                        x,
                        p.cutoff + m.cutoff + self.cutoff_trim,
                        k,
                        p.drive * v.drive,
                    )
                }
                Filter::Svf(v) => {
                    let res = p.k / MAX_K + m.resonance;
                    self.svf_lp
                        .process(
                            ctx.ladder,
                            &v,
                            x,
                            p.cutoff + m.cutoff + self.cutoff_trim,
                            res,
                        )
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
        self.levels.settle();
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
        tables: &'static Tables,
        voice: MonoVoice,
    }

    impl Rig {
        fn new(settings: &[(Param, f32)]) -> Rig {
            Rig::at(SR, settings)
        }

        fn at(sr: f32, settings: &[(Param, f32)]) -> Rig {
            let mut params = MonoParams::new(sr);
            for (p, v) in settings {
                params.set(*p, p.clamp(*v));
            }
            Rig {
                params,
                sine: sine_table(),
                blep: Blep::new(),
                ladder: LadderTables::new(sr),
                pitch: PitchTable::new(sr),
                tables: Tables::shared(sr),
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
                shared: None,
                tables: self.tables,
            };
            let mut out = vec![0.0; frames];
            for chunk in out.chunks_mut(128) {
                self.voice.render(&ctx, chunk);
            }
            out
        }
    }

    /// #271: an oscillator level written every block, as a `mod` line or a
    /// lane writes it, ramps inside the voice. On a sine through an open
    /// filter, a smooth signal, a moving level jumps no more than a steady one.
    #[test]
    fn a_level_moved_every_block_does_not_step() {
        let largest = |moving: bool| {
            let mut rig = Rig::new(&[
                (Param::Vco1Wave, 3.0),
                (Param::Vco2Level, 0.0),
                (Param::Vco3Level, 0.0),
                (Param::NoiseLevel, 0.0),
                (Param::Cutoff, 20_000.0),
                (Param::AdsrSustain, 1.0),
            ]);
            rig.press(45);
            rig.render(SR as usize / 2);
            let mut out = Vec::new();
            for block in 0..40 {
                let level = if moving && block % 2 == 1 { 0.1 } else { 1.0 };
                rig.params.set(Param::Vco1Level, level);
                out.extend(rig.render(128));
            }
            out.windows(2)
                .fold(0.0_f32, |m, w| m.max((w[1] - w[0]).abs()))
        };
        let (steady, moving) = (largest(false), largest(true));
        assert!(steady > 0.0);
        assert!(moving <= 1.2 * steady, "moving {moving} vs steady {steady}");
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
    /// Amplitude of `hz` in `out` (a Goertzel bin, scaled to a sine's peak).
    fn tone(out: &[f32], hz: f64) -> f64 {
        let w = std::f64::consts::TAU * hz / f64::from(SR);
        let (mut re, mut im) = (0.0, 0.0);
        for (i, y) in out.iter().enumerate() {
            re += f64::from(*y) * (w * i as f64).cos();
            im += f64::from(*y) * (w * i as f64).sin();
        }
        2.0 * (re * re + im * im).sqrt() / out.len() as f64
    }

    /// Brightness: how much of a signal's energy is in its steps.
    fn brightness(out: &[f32]) -> f64 {
        let steps: f64 = out.windows(2).map(|w| f64::from(w[1] - w[0]).powi(2)).sum();
        let level: f64 = out.iter().map(|s| f64::from(*s).powi(2)).sum();
        (steps / level).sqrt()
    }

    /// Rising zero crossings: whole cycles of a tone.
    fn cycles(out: &[f32]) -> usize {
        out.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    /// Spec 004 Req 14: VCO 1 × VCO 2 has the sum and difference
    /// frequencies and neither of its inputs.
    #[test]
    fn ring_modulation_has_sum_and_difference() {
        let mut r = Rig::new(&[
            (Param::Vco1Wave, 3.0),
            (Param::Vco1Level, 0.0),
            (Param::Vco2Wave, 3.0),
            (Param::Vco2Coarse, 7.0),
            (Param::RingLevel, 1.0),
            (Param::Cutoff, 20_000.0),
            (Param::AdsrSustain, 1.0),
        ]);
        r.press(57);
        r.render(4_800);
        let out = r.render(48_000);
        let a = 440.0 / 2.0;
        let b = a * 2.0_f64.powf(7.0 / 12.0);
        let (diff, sum) = (tone(&out, b - a), tone(&out, a + b));
        assert!(diff > 0.1 && sum > 0.1, "difference {diff}, sum {sum}");
        for input in [a, b] {
            assert!(tone(&out, input) < 0.01 * diff, "{input} Hz leaks through");
        }
    }

    /// The sub is exactly twice VCO 1's period, through vibrato and glide.
    #[test]
    fn sub_is_exactly_an_octave_down() {
        let counts = |octave: f32| {
            let run = |vco1: f32, sub: f32| {
                let mut r = Rig::new(&[
                    (Param::Vco1Wave, 3.0),
                    (Param::Vco1Level, vco1),
                    (Param::SubLevel, sub),
                    (Param::SubOctave, octave),
                    (Param::Cutoff, 20_000.0),
                    (Param::AdsrSustain, 1.0),
                    (Param::Vibrato, 1.0),
                    (Param::ModWheel, 1.0),
                    (Param::LfoRate, 5.0),
                    (Param::Glide, 0.4),
                ]);
                r.press(45);
                r.press(57);
                r.render(2_400);
                cycles(&r.render(96_000))
            };
            (run(1.0, 0.0), run(0.0, 1.0))
        };
        let (vco, sub) = counts(0.0);
        assert!((2 * sub).abs_diff(vco) <= 1, "{vco} cycles, sub {sub}");
        let (vco, sub) = counts(1.0);
        assert!((4 * sub).abs_diff(vco) <= 2, "{vco} cycles, sub {sub}");
    }

    /// VCO 3 off the key and in the low range is a fixed modulator, and a
    /// modulation source whatever its level in the mixer.
    #[test]
    fn osc3_low_and_unfollowed_is_a_fixed_modulator() {
        let hz = |note: u8| {
            let mut r = Rig::new(&[
                (Param::Vco1Level, 0.0),
                (Param::Vco3Wave, 3.0),
                (Param::Vco3Level, 1.0),
                (Param::Vco3Low, 1.0),
                (Param::Vco3KeyFollow, 0.0),
                (Param::Cutoff, 20_000.0),
                (Param::AdsrSustain, 1.0),
            ]);
            r.press(note);
            r.render(4_800);
            cycles(&r.render(96_000)) as f64 / 2.0
        };
        let (low, high) = (hz(36), hz(96));
        assert_eq!(low, high, "the key does not move it");
        assert!(
            (low - 8.18).abs() < 1.0,
            "five octaves below middle C: {low} Hz"
        );

        let mut r = Rig::new(&[
            (Param::Vco1Level, 0.0),
            (Param::Vco3Level, 0.0),
            (Param::Vco3Low, 1.0),
            (Param::Vco3Wave, 3.0),
            (Param::Patch1Source, 3.0),
            (Param::Patch1Dest, 5.0),
            (Param::Patch1Amount, 0.5),
        ]);
        r.press(60);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for _ in 0..600 {
            r.render(160);
            let c = r.voice.mods().cutoff;
            lo = lo.min(c);
            hi = hi.max(c);
        }
        assert!(
            hi - lo > 40.0,
            "VCO 3 at level 0 still modulates: {lo}..{hi}"
        );
    }

    /// Spec 006 Req 11: the filter envelope moves the wave position, so the
    /// spectrum of a held note changes over the envelope; with no amount it does not.
    #[test]
    fn ppg_envelope_sweeps_the_wave_position() {
        let brightness_over_time = |amount: f32| {
            let mut r = Rig::new(&[
                (Param::Model, 11.0),
                (Param::Wt1Table, 0.0),
                (Param::Wt1Pos, 0.0),
                (Param::Vco1Level, 1.0),
                (Param::Vco2Level, 0.0),
                (Param::EnvWt, amount),
                (Param::Cutoff, 20_000.0),
                (Param::AdsrSustain, 1.0),
                (Param::FenvAttack, 0.5),
                (Param::FenvSustain, 1.0),
            ]);
            r.press(45);
            r.render(480);
            let early = brightness(&r.render(2_400));
            r.render(24_000);
            let late = brightness(&r.render(2_400));
            (early, late)
        };
        let (early, late) = brightness_over_time(1.0);
        assert!(
            late > 2.0 * early,
            "the Sweep table brightens: {early} to {late}"
        );
        let (a, b) = brightness_over_time(0.0);
        assert!(
            (a / b - 1.0).abs() < 0.05,
            "and stands still without it: {a} {b}"
        );
    }

    /// The table oscillators stand in for VCO 1 and 2 on this model only: a model
    /// without them ignores the table parameters, and the PPG does not.
    #[test]
    fn only_the_ppg_reads_wavetables() {
        let sound = |model: f32, pos: f32| {
            let mut r = Rig::new(&[
                (Param::Model, model),
                (Param::Vco1Level, 1.0),
                (Param::Cutoff, 20_000.0),
                (Param::AdsrSustain, 1.0),
                (Param::Wt1Pos, pos),
            ]);
            r.press(57);
            r.render(2_400);
            r.render(9_600)
        };
        assert_eq!(
            sound(9.0, 0.0),
            sound(9.0, 1.0),
            "a Jupiter-8 ignores the wave position"
        );
        assert_ne!(
            sound(11.0, 0.0),
            sound(11.0, 1.0),
            "and the PPG moves with it"
        );
    }

    /// Spec 006 Req 9: the ramp runs from 0 to 1 over its time and starts again with each note.
    #[test]
    fn the_ramp_runs_over_its_time_and_restarts() {
        let mut r = Rig::new(&[
            (Param::Model, 10.0),
            (Param::RampTime, 0.1),
            (Param::Patch1Source, 14.0),
            (Param::Patch1Dest, 5.0),
            (Param::Patch1Amount, 1.0),
            (Param::EnvCutoff, 0.0),
            (Param::KeyTrack, 0.0),
            (Param::AdsrSustain, 1.0),
        ]);
        r.press(60);
        r.render(2_400);
        let half = r.voice.mods().cutoff;
        assert!((half - 24.0).abs() < 0.5, "half way up after 50 ms: {half}");
        r.render(4_800);
        assert!(
            (r.voice.mods().cutoff - 48.0).abs() < 1.0e-3,
            "and full after 100 ms"
        );
        r.release(60);
        r.render(128);
        r.press(62);
        r.render(1_200);
        let again = r.voice.mods().cutoff;
        assert!(again < 15.0, "a new note starts it over: {again}");
    }

    /// The second LFO is a modulation source of its own, at its own rate.
    #[test]
    fn the_second_lfo_moves_what_it_is_patched_to() {
        let range = |rate: f32| {
            let mut r = Rig::new(&[
                (Param::Model, 10.0),
                (Param::Lfo2Rate, rate),
                (Param::Lfo2Wave, 3.0),
                (Param::Patch1Source, 13.0),
                (Param::Patch1Dest, 5.0),
                (Param::Patch1Amount, 0.5),
                (Param::EnvCutoff, 0.0),
                (Param::KeyTrack, 0.0),
            ]);
            r.press(60);
            let (mut lo, mut hi, mut crossings, mut last) = (f32::MAX, f32::MIN, 0, 0.0_f32);
            for _ in 0..750 {
                r.render(128);
                let c = r.voice.mods().cutoff;
                lo = lo.min(c);
                hi = hi.max(c);
                if last <= 0.0 && c > 0.0 {
                    crossings += 1;
                }
                last = c;
            }
            (hi - lo, crossings)
        };
        let (swing, cycles) = range(5.0);
        assert!(swing > 40.0, "±24 semitones of cutoff: {swing}");
        assert!(
            (9..=11).contains(&cycles),
            "2 s at 5 Hz is ten cycles: {cycles}"
        );
    }

    /// Spec 006 Req 8: the Jupiter-8's switch takes its low-pass between 12 and
    /// 24 dB per octave. Measured on noise, an octave and two above the cutoff.
    #[test]
    fn jupiter_slope_switch_is_12_or_24_db_per_octave() {
        let falls = |slope: f32| {
            let mut r = Rig::new(&[
                (Param::Model, 9.0),
                (Param::Vco1Level, 0.0),
                (Param::NoiseLevel, 1.0),
                (Param::Cutoff, 1_000.0),
                (Param::Resonance, 0.0),
                (Param::Slope, slope),
                (Param::AdsrSustain, 1.0),
            ]);
            r.press(60);
            r.render(9_600);
            let out = r.render(192_000);
            let band = |f: f64| {
                (0..8)
                    .map(|k| tone(&out, f * (0.94 + 0.02 * k as f64)).powi(2))
                    .sum::<f64>()
                    / 8.0
            };
            let db = |a: f64, b: f64| 10.0 * (a / b).log10();
            (
                db(band(3_000.0), band(6_000.0)),
                db(band(6_000.0), band(12_000.0)),
            )
        };
        let (a12, b12) = falls(0.0);
        let (a24, b24) = falls(1.0);
        assert!(
            (a12 - 12.0).abs() < 3.5 && (b12 - 12.0).abs() < 3.5,
            "12 dB: {a12} {b12}"
        );
        // Two octaves up the 24 dB filter is near the noise floor of the measure.
        assert!((a24 - 24.0).abs() < 4.5 && b24 > 8.0, "24 dB: {a24} {b24}");
        assert!(a24 > a12 + 8.0, "and it falls much faster than 12 dB");
    }

    /// Cross-modulation: VCO 2 moves VCO 1's pitch, at any mixer level.
    #[test]
    fn cross_mod_moves_vco1_from_vco2() {
        let range = |xmod: f32| {
            let mut r = Rig::new(&[
                (Param::Model, 9.0),
                (Param::Vco2Level, 0.0),
                (Param::Vco2Wave, 3.0),
                (Param::Vco2Coarse, -24.0),
                (Param::XMod, xmod),
            ]);
            r.press(60);
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for _ in 0..400 {
                r.render(128);
                let p = r.voice.mods().pitch[0];
                lo = lo.min(p);
                hi = hi.max(p);
            }
            hi - lo
        };
        assert_eq!(range(0.0), 0.0);
        assert!(range(1.0) > 30.0, "{}", range(1.0));
    }

    /// Spec 006 Req 7: the Juno's high-pass steps thin the bass.
    #[test]
    fn juno_high_pass_steps_thin_the_bass() {
        let low = |hp: f32| {
            let mut r = Rig::new(&[
                (Param::Model, 8.0),
                (Param::Vco1Level, 1.0),
                (Param::HpCutoff, hp),
                (Param::Cutoff, 20_000.0),
                (Param::AdsrSustain, 1.0),
            ]);
            r.press(45);
            r.render(9_600);
            tone(&r.render(48_000), 110.0)
        };
        let (off, step) = (low(20.0), low(1_600.0));
        assert!(
            20.0 * (step / off).log10() < -12.0,
            "the top step takes the fundamental down: {off} {step}"
        );
        assert!(low(240.0) > step, "and the first step less");
    }

    /// Spec 005 Req 7: the SH-101's cutoff and loudness are moved by the
    /// same envelope, whatever the filter ADSR is set to.
    #[test]
    fn sh101_one_envelope_moves_cutoff_and_loudness() {
        let mut r = Rig::new(&[
            (Param::Model, 5.0),
            (Param::EnvCutoff, 1.0),
            (Param::AdsrAttack, 0.5),
            (Param::AdsrSustain, 1.0),
            (Param::FenvAttack, 0.001),
            (Param::EnvPw, 1.0),
        ]);
        r.press(60);
        for i in 0..40_000 {
            r.render(1);
            let m = r.voice.mods();
            if i % 2_000 == 1_999 {
                assert!((m.vca - m.cutoff / 48.0).abs() < 1.0e-3, "{i}: {m:?}");
                assert!((m.vca - m.pulse_width / 0.45).abs() < 1.0e-3, "{i}: {m:?}");
            }
        }
        assert!(r.voice.mods().vca > 0.5, "and the attack is under way");
        r.release(60);
        r.render(2_400);
        let m = r.voice.mods();
        assert!(
            (m.vca - m.cutoff / 48.0).abs() < 1.0e-3,
            "release too: {m:?}"
        );
    }

    /// Saw and locked pulse add up: the pulse reinforces the saw instead of
    /// cancelling it.
    #[test]
    fn sh101_saw_and_pulse_add_up() {
        // The SH-101 and the Juno-106 both mix a saw with a pulse locked to it.
        for model in [5.0, 8.0] {
            saw_and_pulse(model);
        }
    }

    fn saw_and_pulse(model: f32) {
        let rms = |pulse: f32| {
            let mut r = Rig::new(&[
                (Param::Model, model),
                (Param::Vco1Level, 0.5),
                (Param::Vco2Wave, 1.0),
                (Param::Vco2Level, pulse),
                (Param::Cutoff, 20_000.0),
                (Param::AdsrSustain, 1.0),
            ]);
            r.press(45);
            r.render(4_800);
            let out = r.render(48_000);
            (out.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / out.len() as f64).sqrt()
        };
        let (saw, both) = (rms(0.0), rms(0.5));
        assert!(both > 1.2 * saw, "saw {saw}, saw and pulse {both}");
    }

    /// The pulse is VCO 1's own phase: saw plus pulse make one cycle, not a
    /// beat between two oscillators.
    #[test]
    fn sh101_pulse_is_locked_to_the_saw() {
        let mut r = Rig::new(&[
            (Param::Model, 5.0),
            (Param::Vco1Level, 1.0),
            (Param::Vco2Wave, 1.0),
            (Param::Vco2Level, 1.0),
            (Param::Vco2Coarse, 7.0),
            (Param::Cutoff, 20_000.0),
            (Param::AdsrSustain, 1.0),
            (Param::Glide, 0.2),
        ]);
        r.press(40);
        r.press(52);
        r.render(4_800);
        let out = r.render(96_000);
        // Locked, the sound repeats every VCO 1 cycle: the spectrum is the
        // harmonics of the glide's target, nothing in between.
        let f0 = 440.0 * 2.0_f64.powf((52.0 - 69.0) / 12.0);
        let off = tone(&out, 1.5 * f0);
        assert!(
            off < 0.02 * tone(&out, f0),
            "a beating partner would show here: {off}"
        );
    }

    /// Spec 005 Req 6: the AR moves the high-pass and the filter ADSR the
    /// low-pass, each on its own part of the spectrum.
    #[test]
    fn cs15_filters_have_their_own_envelopes() {
        let note = |env_cutoff: f32, env_hp: f32| {
            let mut r = Rig::new(&[
                (Param::Model, 4.0),
                (Param::Vco1Level, 0.5),
                (Param::HpCutoff, 100.0),
                (Param::Cutoff, 400.0),
                (Param::AdsrSustain, 1.0),
                (Param::FenvAttack, 0.001),
                (Param::FenvSustain, 1.0),
                (Param::ArAttack, 0.001),
                (Param::EnvCutoff, env_cutoff),
                (Param::EnvHpCutoff, env_hp),
            ]);
            r.press(45);
            r.render(9_600);
            let out = r.render(48_000);
            // The fundamental, and the 20th harmonic.
            (tone(&out, 110.0), tone(&out, 2_200.0))
        };
        let db = |a: f64, b: f64| 20.0 * (a / b).log10();
        let (low0, high0) = note(0.0, 0.0);
        let (low_hp, high_hp) = note(0.0, 0.8);
        assert!(
            db(low_hp, low0) < -10.0,
            "the AR lifts the high-pass over the fundamental"
        );
        assert!(db(high_hp, high0).abs() < 3.0, "and leaves the top alone");
        let (low_lp, high_lp) = note(0.8, 0.0);
        assert!(
            db(high_lp, high0) > 10.0,
            "the filter ADSR opens the low-pass"
        );
        assert!(
            db(low_lp, low0).abs() < 2.0,
            "and leaves the fundamental alone"
        );
    }

    /// Spec 005 Req 5: noise through the MS-20's high-pass at 2 kHz and
    /// low-pass at 8 kHz is down below the one and above the other.
    #[test]
    fn ms20_band_limits_noise() {
        let mut r = Rig::new(&[
            (Param::Model, 3.0),
            (Param::Vco1Level, 0.0),
            (Param::NoiseLevel, 1.0),
            (Param::HpCutoff, 2_000.0),
            (Param::Cutoff, 8_000.0),
            (Param::AdsrSustain, 1.0),
        ]);
        r.press(60);
        r.render(4_800);
        let out = r.render(96_000);
        let band =
            |hz: &[f64]| hz.iter().map(|h| tone(&out, *h).powi(2)).sum::<f64>() / hz.len() as f64;
        let db = |x: f64| 10.0 * x.log10();
        let (low, mid, high) = (
            band(&[250.0, 300.0, 350.0, 400.0, 450.0, 500.0]),
            band(&[3_000.0, 3_500.0, 4_000.0, 4_500.0, 5_000.0]),
            band(&[18_000.0, 19_000.0, 20_000.0, 21_000.0, 22_000.0]),
        );
        assert!(db(mid / low) > 20.0, "high-pass: {} dB", db(mid / low));
        assert!(db(mid / high) > 10.0, "low-pass: {} dB", db(mid / high));
    }

    /// Spec 005 Req 4: with oscillator A synced to a silent B, the filter
    /// envelope into A's pitch sweeps the sound; without it nothing moves.
    #[test]
    fn pro_one_poly_mod_sweeps_the_synced_oscillator() {
        let spectrum = |amount: f32| {
            let mut r = Rig::new(&[
                (Param::Model, 2.0),
                (Param::Vco1Level, 0.0),
                (Param::Vco2Level, 1.0),
                (Param::Vco2Sync, 1.0),
                (Param::Cutoff, 20_000.0),
                (Param::AdsrSustain, 1.0),
                (Param::FenvAttack, 0.001),
                (Param::FenvDecay, 0.3),
                (Param::FenvSustain, 0.0),
                (Param::EnvFreq2, amount),
            ]);
            r.press(45);
            r.render(480);
            let early = brightness(&r.render(2_400));
            r.render(48_000);
            let late = brightness(&r.render(2_400));
            (early, late)
        };
        let (early, late) = spectrum(1.0);
        assert!(early > 1.15 * late, "swept: {early} falls to {late}");
        let (early, late) = spectrum(0.0);
        assert!(
            (early / late - 1.0).abs() < 0.1,
            "still: {early} and {late}"
        );
    }

    /// Spec 005 Req 3: on the Minimoog the decay time is the release too.
    #[test]
    fn minimoog_decay_is_release() {
        let fall = |model: f32| {
            let mut r = Rig::new(&[
                (Param::Model, model),
                (Param::AdsrDecay, 0.1),
                (Param::AdsrRelease, 4.0),
                (Param::AdsrSustain, 0.5),
                (Param::Cutoff, 20_000.0),
            ]);
            r.press(60);
            r.render(48_000);
            r.release(60);
            r.render(14_400);
            r.voice.active()
        };
        assert!(!fall(1.0), "the Minimoog has fallen silent within 0.3 s");
        assert!(fall(0.0), "the ARP 2600 is still releasing");
    }

    /// Spec 005 Req 3: the loudness is full at once while a slow filter
    /// contour opens the spectrum.
    #[test]
    fn minimoog_filter_contour_brightens_a_held_note() {
        let mut r = Rig::new(&[
            (Param::Model, 1.0),
            (Param::Cutoff, 150.0),
            (Param::EnvCutoff, 1.0),
            (Param::AdsrAttack, 0.001),
            (Param::AdsrSustain, 1.0),
            (Param::FenvAttack, 0.5),
            (Param::FenvSustain, 1.0),
        ]);
        r.press(45);
        r.render(480);
        assert!(r.voice.mods().vca > 0.99, "the loudness contour is full");
        let early = brightness(&r.render(2_400));
        r.render(24_000);
        let late = brightness(&r.render(2_400));
        assert!(late > 2.0 * early, "{early} brightens to {late}");
    }

    /// Spec 004 Req 13: the ladder is voiced per model, and every voicing
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
        // Moog (ARP 2600 and Minimoog), Pro-One, SH-101, Odyssey.
        let (moog, pro, sh, ody) = (rms_of(0.0), rms_of(2.0), rms_of(5.0), rms_of(6.0));
        assert_eq!(moog, rms_of(1.0), "the Minimoog ladder is the Moog voicing");
        assert!(moog > 0.0 && pro > 0.0 && sh > 0.0 && ody > 0.0);
        assert!((moog - pro).abs() > 1.0e-3, "{moog} vs {pro}");
        assert!((moog - sh).abs() > 1.0e-3, "{moog} vs {sh}");
        assert!((pro - sh).abs() > 1.0e-3, "{pro} vs {sh}");
        for other in [moog, pro, sh] {
            assert!((ody - other).abs() > 1.0e-3, "Odyssey {ody} vs {other}");
        }
    }

    /// Spec 005 Req 10: the Odyssey's high-pass is a 6 dB stage after the
    /// low-pass, and takes the lows out of a low note; the ARP 2600 has none.
    #[test]
    fn odyssey_high_pass_takes_out_the_lows() {
        let rms = |model: f32, hp: f32| {
            let mut r = Rig::new(&[
                (Param::Model, model),
                (Param::HpCutoff, hp),
                (Param::Cutoff, 20_000.0),
                (Param::AdsrSustain, 1.0),
            ]);
            r.press(33);
            r.render(4_800);
            let out = r.render(24_000);
            (out.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / out.len() as f64).sqrt()
        };
        assert_eq!(crate::mono::model::Model::Odyssey.hp(), Hp::OnePole);
        let (open, high) = (rms(6.0, 20.0), rms(6.0, 2_000.0));
        assert!(high < 0.6 * open, "open {open}, high-passed {high}");
        assert_eq!(
            rms(0.0, 20.0),
            rms(0.0, 2_000.0),
            "the 2600 has no high-pass"
        );
    }

    /// Spec 005 Req 10: one ADSR moves both the Odyssey's cutoff and its
    /// loudness; the filter ADSR plays no part.
    #[test]
    fn odyssey_one_envelope_moves_cutoff_and_loudness() {
        let mut r = Rig::new(&[
            (Param::Model, 6.0),
            (Param::EnvCutoff, 1.0),
            (Param::AdsrAttack, 0.5),
            (Param::AdsrSustain, 1.0),
            (Param::FenvAttack, 0.001),
        ]);
        r.press(60);
        for i in 0..20_000 {
            r.render(1);
            if i % 2_000 == 1_999 {
                let m = r.voice.mods();
                assert!((m.vca - m.cutoff / 48.0).abs() < 1.0e-3, "{i}: {m:?}");
            }
        }
    }

    /// Spec 004 Req 12: on a model that uses the filter ADSR, the loudness
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

    /// Pitch from rising zero crossings, interpolated, at `sr`.
    fn heard_hz(y: &[f32], sr: f32) -> f64 {
        let c: Vec<f64> = y
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
            .map(|(i, w)| i as f64 + f64::from(w[0] / (w[0] - w[1])))
            .collect();
        (c.len() - 1) as f64 * f64::from(sr) / (c[c.len() - 1] - c[0])
    }

    fn cents(hz: f64, want: f64) -> f64 {
        1200.0 * (hz / want).log2()
    }

    /// The band-limited pulse and triangle keep their pitch at the ends of
    /// the keyboard, where the BLEP corrections are largest or longest.
    #[test]
    fn pulse_and_triangle_pitch_at_notes_24_and_108() {
        for wave in [Waveform::Pulse, Waveform::Triangle] {
            for note in [24_u8, 108] {
                let mut r = Rig::new(&[
                    (Param::Vco1Wave, wave as u32 as f32),
                    (Param::Cutoff, 20_000.0),
                    (Param::AdsrSustain, 1.0),
                ]);
                r.press(note);
                let y = r.render(SR as usize);
                let want = 440.0 * ((f64::from(note) - 69.0) / 12.0).exp2();
                let off = cents(heard_hz(&y[4_800..], SR), want);
                assert!(off.abs() < 1.0, "{wave:?} note {note}: {off} cents");
            }
        }
    }

    /// Turning knobs on a sounding voice every block keeps it bounded.
    #[test]
    fn parameters_changed_every_block_stay_bounded() {
        let knobs = [
            Param::Vco1Wave,
            Param::Vco2Wave,
            Param::Vco1Coarse,
            Param::Vco2Fine,
            Param::Vco2Level,
            Param::PulseWidth,
            Param::NoiseLevel,
            Param::Cutoff,
            Param::Resonance,
            Param::Drive,
            Param::AdsrSustain,
            Param::FenvSustain,
            Param::LfoRate,
            Param::LfoWave,
        ];
        let mut r = Rig::new(&[(Param::AdsrSustain, 1.0)]);
        r.press(48);
        let mut seed = 1_u32;
        for block in 0..2_000 {
            for p in knobs {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let (lo, hi) = (p.clamp(f32::MIN), p.clamp(f32::MAX));
                let v = lo + (hi - lo) * (seed >> 8) as f32 / (1 << 24) as f32;
                r.params.set(p, v);
            }
            for s in r.render(128) {
                assert!(s.is_finite() && s.abs() <= 2.0, "block {block}: {s}");
            }
        }
    }

    /// A coarse change on a held note is heard from the next block.
    #[test]
    fn coarse_change_applies_on_the_next_block() {
        let mut r = Rig::new(&[
            (Param::Vco1Wave, 3.0),
            (Param::Cutoff, 20_000.0),
            (Param::AdsrSustain, 1.0),
        ]);
        r.press(69);
        r.render(4_800);
        r.params.set(Param::Vco1Coarse, 12.0);
        let y = r.render(128);
        let off = cents(heard_hz(&y, SR), 880.0);
        assert!(off.abs() < 5.0, "{off} cents off 880 Hz");
    }

    /// The pitch table, the BLEP and the envelope's seconds-to-samples all
    /// depend on the rate: spot checks at 44.1 and 96 kHz.
    #[test]
    fn pitch_and_envelope_at_other_sample_rates() {
        for sr in [44_100.0, 96_000.0] {
            let mut r = Rig::at(
                sr,
                &[
                    (Param::Vco1Wave, 0.0),
                    (Param::Cutoff, 20_000.0),
                    (Param::AdsrAttack, 0.1),
                    (Param::AdsrSustain, 1.0),
                ],
            );
            r.press(69);
            let y = r.render(sr as usize);
            let off = cents(heard_hz(&y[(sr * 0.2) as usize..], sr), 440.0);
            assert!(off.abs() < 1.0, "A4 at {sr}: {off} cents");
            // The attack is about 60% up half way and arrives 100 ms in
            // (its concave curve, checked in `env`).
            let peak = |from: f32, to: f32| {
                y[(from * sr) as usize..(to * sr) as usize]
                    .iter()
                    .fold(0.0_f32, |m, s| m.max(s.abs()))
            };
            let full = peak(0.15, 0.2);
            let (half, end) = (peak(0.04, 0.05) / full, peak(0.09, 0.1) / full);
            assert!(half > 0.5 && half < 0.75, "{sr}: {half} half way");
            assert!(end > 0.95, "{sr}: {end} at the end");
        }
    }
}
