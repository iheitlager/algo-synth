//! The NES instrument's voice (#112): one note plays a whole 2A03, two
//! pulses, the triangle, the noise and the DMC, through the console's
//! non-linear mixer and its output filters.
//!
//! The channels step on the chip's grid (`chip::nes`); their levels are
//! mixed at every step and the mixed output band-limited there (ADR-0007),
//! so the mixer's compression acts on the levels the chip has. Pitch, the
//! chord arpeggio and the vibrato change once per frame-counter sequence
//! (60 Hz on NTSC, 50 on PAL), as a sound driver writes the registers. With
//! `kit` on, a General MIDI drum key plays the drums NES scores make of the
//! channels (a triangle diving for the kick, noise for snare and hats) and
//! any other key the DMC's sample, so the model plays from drum lanes.

use crate::chip::Ring;
use crate::chip::nes::{
    self, Dmc, Events, FrameCounter, Noise, Pulse, Region, Triangle, mix, noise_index, pulse_timer,
    triangle_timer,
};
use crate::chip::sid_voice::ARPS;
use crate::mono::voice::MonoCtx;
use crate::sample::SampleStore;

/// Fields of each block in `NesPatch::set`: two pulses, then the triangle,
/// the noise, the DMC and the chip-wide ones.
pub const PULSE_FIELDS: usize = 9;
pub const TRI_BASE: usize = 2 * PULSE_FIELDS;
pub const NOISE_BASE: usize = TRI_BASE + 4;
pub const DMC_BASE: usize = NOISE_BASE + 7;
pub const GLOBAL_BASE: usize = DMC_BASE + 5;
pub const FIELDS: usize = GLOBAL_BASE + 9;

/// Output level: both pulses and the triangle at full scale swing about ±0.6.
const OUT_GAIN: f32 = 1.4;
/// How long a voice runs on after its channels fall silent, in seconds:
/// the output filters settle.
const TAIL: f32 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PulsePatch {
    pub on: bool,
    pub duty: u8,
    /// The 4-bit volume, or the decay's period when `decay`.
    pub volume: u8,
    pub decay: bool,
    pub looped: bool,
    pub coarse: f32,
    pub fine: f32,
    /// The sweep's shift, signed: negative sweeps the pitch up; 0 is off.
    pub sweep: i8,
    pub sweep_period: u8,
}

impl Default for PulsePatch {
    fn default() -> PulsePatch {
        PulsePatch {
            on: false,
            duty: 2,
            volume: 12,
            decay: false,
            looped: false,
            coarse: 0.0,
            fine: 0.0,
            sweep: 0,
            sweep_period: 0,
        }
    }
}

/// The 2A03's registers and the sound driver's settings (see
/// `NesPatch::set` for the field order).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NesPatch {
    pub pulse: [PulsePatch; 2],
    pub tri_on: bool,
    pub tri_coarse: f32,
    pub tri_fine: f32,
    /// The linear counter's reload in quarter frames; 0 holds the note.
    pub tri_length: u8,
    pub noise_on: bool,
    pub noise_volume: u8,
    pub noise_decay: bool,
    pub noise_loop: bool,
    pub noise_period: u8,
    pub noise_short: bool,
    /// The period follows the note.
    pub noise_track: bool,
    /// The pad (1..16) of the synth's kit whose sample the DMC plays on a
    /// note; 0 none.
    pub dmc_sample: u8,
    pub dmc_rate: u8,
    pub dmc_loop: bool,
    /// The counter's level at the note's start (`$4011`).
    pub dmc_level: u8,
    pub dmc_direct: bool,
    pub region: Region,
    /// 0 a Famicom's output (high-pass 37 Hz), 1 a NES's (high-pass 90 and
    /// 440 Hz, low-pass 14 kHz), 2 clean (a DC blocker).
    pub output: u8,
    pub band_limit: bool,
    pub arp: u8,
    pub arp_speed: u8,
    pub vibrato: f32,
    pub vibrato_rate: f32,
    pub vibrato_delay: u32,
    pub kit: bool,
}

impl Default for NesPatch {
    fn default() -> NesPatch {
        let mut pulse = [PulsePatch::default(); 2];
        pulse[0].on = true;
        NesPatch {
            pulse,
            tri_on: false,
            tri_coarse: 0.0,
            tri_fine: 0.0,
            tri_length: 0,
            noise_on: false,
            noise_volume: 12,
            noise_decay: false,
            noise_loop: false,
            noise_period: 8,
            noise_short: false,
            noise_track: false,
            dmc_sample: 0,
            dmc_rate: 15,
            dmc_loop: false,
            dmc_level: 0,
            dmc_direct: false,
            region: Region::Ntsc,
            output: 0,
            band_limit: true,
            arp: 0,
            arp_speed: 1,
            vibrato: 0.0,
            vibrato_rate: 5.0,
            vibrato_delay: 0,
            kit: false,
        }
    }
}

impl NesPatch {
    /// Set field `i` (`Param::nes`): per pulse on, duty, volume, decay,
    /// loop, coarse, fine, sweep and sweep period; the triangle's on,
    /// coarse, fine and length; the noise's on, volume, decay, loop, period,
    /// short mode and tracking; the DMC's sample, rate, loop, level and
    /// direct mode; then region, output, band-limit, arpeggio, its speed,
    /// the vibrato's depth, rate and delay, and kit.
    pub fn set(&mut self, i: usize, v: f32) {
        let int = |hi: f32| v.round().clamp(0.0, hi);
        let on = v >= 0.5;
        if i < TRI_BASE {
            let Some(p) = self.pulse.get_mut(i / PULSE_FIELDS) else {
                return;
            };
            match i % PULSE_FIELDS {
                0 => p.on = on,
                1 => p.duty = int(3.0) as u8,
                2 => p.volume = int(15.0) as u8,
                3 => p.decay = on,
                4 => p.looped = on,
                5 => p.coarse = v,
                6 => p.fine = v,
                7 => p.sweep = v.round().clamp(-7.0, 7.0) as i8,
                _ => p.sweep_period = int(7.0) as u8,
            }
            return;
        }
        match i {
            _ if i == TRI_BASE => self.tri_on = on,
            _ if i == TRI_BASE + 1 => self.tri_coarse = v,
            _ if i == TRI_BASE + 2 => self.tri_fine = v,
            _ if i == TRI_BASE + 3 => self.tri_length = int(127.0) as u8,
            _ if i == NOISE_BASE => self.noise_on = on,
            _ if i == NOISE_BASE + 1 => self.noise_volume = int(15.0) as u8,
            _ if i == NOISE_BASE + 2 => self.noise_decay = on,
            _ if i == NOISE_BASE + 3 => self.noise_loop = on,
            _ if i == NOISE_BASE + 4 => self.noise_period = int(15.0) as u8,
            _ if i == NOISE_BASE + 5 => self.noise_short = on,
            _ if i == NOISE_BASE + 6 => self.noise_track = on,
            _ if i == DMC_BASE => self.dmc_sample = int(16.0) as u8,
            _ if i == DMC_BASE + 1 => self.dmc_rate = int(15.0) as u8,
            _ if i == DMC_BASE + 2 => self.dmc_loop = on,
            _ if i == DMC_BASE + 3 => self.dmc_level = int(127.0) as u8,
            _ if i == DMC_BASE + 4 => self.dmc_direct = on,
            _ if i == GLOBAL_BASE => self.region = Region::from_value(v),
            _ if i == GLOBAL_BASE + 1 => self.output = int(2.0) as u8,
            _ if i == GLOBAL_BASE + 2 => self.band_limit = on,
            _ if i == GLOBAL_BASE + 3 => self.arp = int((ARPS.len() - 1) as f32) as u8,
            _ if i == GLOBAL_BASE + 4 => self.arp_speed = int(16.0).max(1.0) as u8,
            _ if i == GLOBAL_BASE + 5 => self.vibrato = v.clamp(0.0, 2.0),
            _ if i == GLOBAL_BASE + 6 => self.vibrato_rate = v.clamp(0.0, 12.0),
            _ if i == GLOBAL_BASE + 7 => self.vibrato_delay = int(100.0) as u32,
            _ => self.kit = on,
        }
    }

    /// Driver frames per second.
    fn frame_rate(&self) -> f32 {
        (self.region.clock() / f64::from(self.region.sequence())) as f32
    }
}

/// A drum of the kit, from its General MIDI key.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Drum {
    Kick,
    Snare,
    Rim,
    ClosedHat,
    OpenHat,
    Crash,
    Ride,
    /// A tom at its pitch in Hz.
    Tom(f32),
    /// The DMC's sample, or noise at the key's pitch without one.
    #[default]
    Sample,
}

impl Drum {
    pub fn from_gm(note: u8) -> Drum {
        match note {
            35 | 36 => Drum::Kick,
            38 | 40 => Drum::Snare,
            37 | 39 => Drum::Rim,
            42 | 44 => Drum::ClosedHat,
            46 => Drum::OpenHat,
            49 | 52 | 55 | 57 => Drum::Crash,
            51 | 53 | 59 => Drum::Ride,
            41 => Drum::Tom(98.0),
            43 => Drum::Tom(117.0),
            45 => Drum::Tom(139.0),
            47 => Drum::Tom(165.0),
            48 => Drum::Tom(196.0),
            50 => Drum::Tom(233.0),
            _ => Drum::Sample,
        }
    }
}

/// A one-pole filter's state and coefficient.
#[derive(Clone, Copy, Debug, Default)]
struct Pole {
    x1: f32,
    y1: f32,
}

impl Pole {
    fn hp(&mut self, a: f32, x: f32) -> f32 {
        let y = a * (self.y1 + x - self.x1);
        self.x1 = x;
        self.y1 = if y.is_finite() { y } else { 0.0 };
        self.y1
    }

    fn lp(&mut self, b: f32, x: f32) -> f32 {
        self.y1 += b * (x - self.y1);
        if !self.y1.is_finite() {
            self.y1 = 0.0;
        }
        self.y1
    }
}

#[derive(Clone, Copy, Default)]
pub struct NesVoice {
    note: f32,
    velocity: f32,
    gate: bool,
    pressed: bool,
    /// Pitch trim in semitones from the pool (unison, analog variance).
    pub trim: f32,
    pub cutoff_trim: f32,
    per: u64,
    frame: FrameCounter,
    frames: u32,
    pulse: [Pulse; 2],
    tri: Triangle,
    linear: u8,
    noise: Noise,
    dmc: Dmc,
    dmc_on: bool,
    /// The store slot the DMC reads, where in it, in frames, and how far a
    /// bit moves.
    dmc_slot: usize,
    dmc_pos: f32,
    dmc_step: f32,
    levels: [u8; 5],
    ring: Ring,
    drum: Option<Drum>,
    /// Samples left to run after the channels fall silent.
    tail: u32,
    filters: [Pole; 3],
}

impl NesVoice {
    pub fn new() -> NesVoice {
        NesVoice::default()
    }

    pub fn active(&self) -> bool {
        self.pressed || self.gate || self.sounding() || self.tail > 0
    }

    pub fn gated(&self) -> bool {
        self.gate
    }

    /// A channel still moves. A stopped triangle and an idle DMC hold their
    /// level, a constant the output filter takes away, so they do not count.
    fn sounding(&self) -> bool {
        let [p1, p2, _, noise, _] = self.levels;
        p1 > 0
            || p2 > 0
            || noise > 0
            || self.tri.running
            || self.dmc_on
            || (self.noise.on && self.noise.env.level() > 0)
            || self.pulse.iter().any(|p| p.on && p.env.level() > 0)
    }

    pub fn press(&mut self, note: u8, velocity: f32) {
        self.note = f32::from(note);
        self.velocity = velocity.clamp(0.0, 1.0);
        self.gate = true;
        self.pressed = true;
    }

    /// A key let go: the pulses and noise stop and the triangle holds, as a
    /// driver writes volume 0; a drum rings out on its own.
    pub fn release_all(&mut self) {
        self.gate = false;
    }

    fn start(&mut self, ctx: &MonoCtx, samples: &SampleStore) {
        let p = &ctx.params.nes;
        let fresh = !self.sounding() && self.tail == 0;
        self.per = nes::cycles_per_sample(p.region, ctx.params.sample_rate());
        if fresh {
            *self = NesVoice {
                note: self.note,
                velocity: self.velocity,
                gate: true,
                trim: self.trim,
                cutoff_trim: self.cutoff_trim,
                per: self.per,
                ..NesVoice::default()
            };
        }
        self.frames = 0;
        self.drum = p.kit.then(|| Drum::from_gm(self.note as u8));
        for (k, (pl, patch)) in self.pulse.iter_mut().zip(p.pulse).enumerate() {
            pl.ones = k == 0;
            pl.on = patch.on && !p.kit;
            pl.restart();
        }
        self.tri.running = p.tri_on && !p.kit;
        self.linear = p.tri_length;
        self.noise.on = p.noise_on && !p.kit;
        self.noise.env.restart();
        self.dmc_on = false;
        if !p.kit {
            // The melodic DMC plays the patch's pad, tuned by the note
            // against the sample's root.
            if let Some((slot, _)) =
                pad_sample(ctx, samples, usize::from(p.dmc_sample).checked_sub(1))
            {
                let root = samples.get(slot).map_or(60.0, |s| f32::from(s.root));
                self.start_dmc(ctx, slot, ((self.note - root) / 12.0).exp2());
            }
        }
        if let Some(d) = self.drum {
            // A drum key whose pad holds a sample plays it through the DMC,
            // as NES scores put a sampled kick or snare beside the noise.
            match pad_sample(ctx, samples, crate::padsampler::pad_of(self.note as u8)) {
                Some((slot, tune)) => {
                    self.drum = Some(Drum::Sample);
                    self.start_dmc(ctx, slot, (tune / 12.0).exp2());
                }
                None => self.start_drum(d, ctx, samples),
            }
        }
        self.frame(ctx);
    }

    /// Start the DMC on the sample in `slot`, read `speed` times as fast as
    /// it was recorded, from the patch's `$4011` level.
    fn start_dmc(&mut self, ctx: &MonoCtx, slot: usize, speed: f32) {
        let p = &ctx.params.nes;
        self.dmc.rate = p.dmc_rate;
        self.dmc.direct = p.dmc_direct;
        self.dmc.load(p.dmc_level);
        self.dmc.restart();
        self.dmc_on = true;
        self.dmc_slot = slot;
        self.dmc_pos = 0.0;
        let bit = f64::from(p.region.dmc_period(p.dmc_rate)) / p.region.clock();
        self.dmc_step = (bit * f64::from(ctx.params.sample_rate())) as f32 * speed;
    }

    /// Set up a drum's channels: what its first frame plays.
    fn start_drum(&mut self, d: Drum, ctx: &MonoCtx, samples: &SampleStore) {
        let n = &mut self.noise;
        let mut noise = |period: u8, short: bool, decay: u8| {
            n.on = true;
            n.period = period;
            n.short = short;
            n.env.constant = false;
            n.env.looped = false;
            n.env.volume = decay;
            n.env.restart();
        };
        match d {
            Drum::Kick => noise(12, false, 0),
            Drum::Snare => noise(6, false, 1),
            Drum::Rim => noise(5, true, 0),
            Drum::ClosedHat => noise(1, false, 0),
            Drum::OpenHat => noise(1, false, 3),
            Drum::Crash => noise(2, false, 8),
            Drum::Ride => noise(3, true, 5),
            Drum::Tom(_) => {}
            // A key without a drum or a pad sample: the patch's pad, or noise
            // at its pitch.
            Drum::Sample => {
                let pad = usize::from(ctx.params.nes.dmc_sample).checked_sub(1);
                match pad_sample(ctx, samples, pad) {
                    Some((slot, tune)) => self.start_dmc(ctx, slot, (tune / 12.0).exp2()),
                    None => {
                        let hz = ctx.pitch.at(self.note) * ctx.params.sample_rate();
                        noise(noise_index(hz, ctx.params.nes.region), false, 2);
                    }
                }
            }
        }
        self.tri.running = matches!(d, Drum::Kick | Drum::Snare | Drum::Tom(_));
        self.linear = 0;
    }

    /// A drum's frame `f`: the triangle's pitch dive and when channels stop.
    fn drum_frame(&mut self, d: Drum, f: u32, region: Region) {
        let k = f as f32;
        let (tri_hz, tri_frames, noise_frames) = match d {
            Drum::Kick => (220.0 * 0.62_f32.powf(k), 7, 2),
            Drum::Snare => (260.0 * 0.82_f32.powf(k), 4, u32::MAX),
            Drum::Tom(hz) => (hz * (1.0 + 0.5 * 0.7_f32.powf(k)), 14, 0),
            Drum::Rim | Drum::ClosedHat => (0.0, 0, 3),
            _ => (0.0, 0, u32::MAX),
        };
        if f < tri_frames {
            self.tri.period = triangle_timer(tri_hz, region);
        } else {
            self.tri.running = false;
        }
        if f >= noise_frames {
            self.noise.on = false;
        }
    }

    /// A driver frame: pitch, arpeggio and vibrato into the timers, the
    /// patch's live registers, a drum's step.
    fn frame(&mut self, ctx: &MonoCtx) {
        let p = ctx.params;
        let s = &p.nes;
        if let Some(d) = self.drum {
            self.drum_frame(d, self.frames, s.region);
            self.frames = self.frames.wrapping_add(1);
            return;
        }
        let chord = ARPS.get(usize::from(s.arp)).copied().unwrap_or(&[0.0]);
        let step = (self.frames / u32::from(s.arp_speed.max(1))) as usize;
        let arp = chord.get(step % chord.len().max(1)).copied().unwrap_or(0.0);
        let vib = match self.frames.checked_sub(s.vibrato_delay) {
            Some(n) if s.vibrato > 0.0 => {
                let phase = (n as f32 * s.vibrato_rate / s.frame_rate()).fract();
                s.vibrato * (1.0 - 4.0 * ((phase + 0.25).fract() - 0.5).abs())
            }
            _ => 0.0,
        };
        let base = self.note + self.trim + p.bend + arp + vib;
        let fs = p.sample_rate();
        let hz = |n: f32| ctx.pitch.at(n) * fs;
        for (pl, patch) in self.pulse.iter_mut().zip(s.pulse) {
            pl.duty = patch.duty;
            pl.env.volume = patch.volume;
            pl.env.constant = !patch.decay;
            pl.env.looped = patch.looped;
            pl.shift = patch.sweep.unsigned_abs();
            // Off, the negate flag is set as drivers set it: with shift 0 and
            // negate clear the chip mutes every period from 0x400 (under 110 Hz).
            pl.negate = patch.sweep <= 0;
            pl.sweep_period = patch.sweep_period;
            // A sweep owns the pitch once the note starts.
            if patch.sweep == 0 || self.frames == 0 {
                pl.period = pulse_timer(hz(base + patch.coarse + patch.fine / 100.0), s.region);
            }
        }
        self.tri.period = triangle_timer(hz(base + s.tri_coarse + s.tri_fine / 100.0), s.region);
        self.noise.env.volume = s.noise_volume;
        self.noise.env.constant = !s.noise_decay;
        self.noise.env.looped = s.noise_loop;
        self.noise.short = s.noise_short;
        self.noise.period = if s.noise_track {
            noise_index(hz(base), s.region)
        } else {
            s.noise_period
        };
        self.dmc.rate = s.dmc_rate;
        self.frames = self.frames.wrapping_add(1);
    }

    /// Add this voice into `out`, advancing its state.
    pub fn render(&mut self, ctx: &MonoCtx, samples: &SampleStore, out: &mut [f32]) {
        if self.pressed {
            self.pressed = false;
            self.start(ctx, samples);
        }
        let s = ctx.params.nes;
        let fs = ctx.params.sample_rate();
        let region = s.region;
        if !self.gate && self.drum.is_none() {
            self.pulse.iter_mut().for_each(|p| p.on = false);
            self.noise.on = false;
            self.tri.running = false;
        }
        // The output filters' coefficients, for the block.
        let hp = |hz: f32| 1.0 / (1.0 + std::f32::consts::TAU * hz / fs);
        let lp = 1.0 - (-std::f32::consts::TAU * 14_000.0 / fs).exp();
        let gain = OUT_GAIN * self.velocity;
        let sample = samples.get(self.dmc_slot);
        let mut ev = Events::default();
        for y in out.iter_mut() {
            if !self.active() {
                return;
            }
            let clocks = self.frame.run(self.per, region);
            for _ in 0..clocks.quarters {
                self.pulse.iter_mut().for_each(|p| p.env.quarter());
                self.noise.env.quarter();
                if self.linear > 0 {
                    self.linear -= 1;
                    if self.linear == 0 {
                        self.tri.running = false;
                    }
                }
            }
            for _ in 0..clocks.halves {
                self.pulse.iter_mut().for_each(Pulse::half);
            }
            for _ in 0..clocks.frames {
                self.frame(ctx);
            }
            ev.clear();
            let [p1, p2] = &mut self.pulse;
            p1.run(self.per, 0, &mut ev);
            p2.run(self.per, 1, &mut ev);
            self.tri.run(self.per, 2, &mut ev);
            self.noise.run(self.per, region, 3, &mut ev);
            if self.dmc_on {
                let (mut pos, step) = (self.dmc_pos, self.dmc_step);
                let (mut ended, mut looped) = (false, false);
                let dmc = &mut self.dmc;
                let bits = dmc.bits;
                dmc.run(self.per, region, 4, &mut ev, |k| {
                    let smp = sample?;
                    let ch = usize::from(smp.channels.max(1));
                    // The sample's end, or the longest the chip plays.
                    if pos as usize >= smp.data.len() / ch || bits + k >= nes::DMC_MAX_BITS {
                        if !s.dmc_loop {
                            ended = true;
                            return None;
                        }
                        pos = 0.0;
                        looped = true;
                    }
                    let i = pos as usize * ch;
                    pos += step;
                    smp.data.get(i).copied()
                });
                if ended {
                    self.dmc_on = false;
                }
                if looped {
                    self.dmc.restart();
                }
                self.dmc_pos = pos;
            }
            let start = mix(self.levels);
            ev.sort();
            let x = if s.band_limit {
                self.ring.add_now(start);
                for &(after, ch, level) in ev.iter() {
                    let before = mix(self.levels);
                    if let Some(l) = self.levels.get_mut(ch) {
                        *l = level;
                    }
                    self.ring
                        .correct(ctx.blep, mix(self.levels) - before, after);
                }
                self.ring.take()
            } else {
                for &(_, ch, level) in ev.iter() {
                    if let Some(l) = self.levels.get_mut(ch) {
                        *l = level;
                    }
                }
                mix(self.levels)
            };
            let [a, b, c] = &mut self.filters;
            let v = match s.output {
                0 => a.hp(hp(37.0), x),
                1 => c.lp(lp, b.hp(hp(440.0), a.hp(hp(90.0), x))),
                _ => a.hp(hp(10.0), x),
            };
            *y += v * gain;
            if self.sounding() || self.gate {
                self.tail = (TAIL * fs) as u32;
            } else {
                self.tail = self.tail.saturating_sub(1);
            }
        }
    }
}

/// The store slot and tune of pad `pad` of the synth's kit (the pad
/// sampler's pads, #124, #527), if it holds a loaded sample.
fn pad_sample(ctx: &MonoCtx, samples: &SampleStore, pad: Option<usize>) -> Option<(usize, f32)> {
    let cfg = ctx.params.pad_kit.pad(pad?)?;
    let slot = cfg.sample?;
    samples.get(slot).map(|_| (slot, cfg.tune))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_is_idle_until_pressed() {
        let v = NesVoice::new();
        assert!(!v.active() && !v.gated());
    }

    #[test]
    fn the_patch_takes_its_fields() {
        let mut p = NesPatch::default();
        p.set(PULSE_FIELDS + 1, 3.0);
        p.set(7, -9.0);
        p.set(TRI_BASE, 1.0);
        p.set(NOISE_BASE + 4, 20.0);
        p.set(DMC_BASE + 3, 200.0);
        p.set(GLOBAL_BASE + 8, 1.0);
        assert_eq!(p.pulse[1].duty, 3);
        assert_eq!(p.pulse[0].sweep, -7);
        assert!(p.tri_on);
        assert_eq!(p.noise_period, 15);
        assert_eq!(p.dmc_level, 127);
        assert!(p.kit);
    }

    #[test]
    fn drums_come_from_general_midi() {
        assert_eq!(Drum::from_gm(36), Drum::Kick);
        assert_eq!(Drum::from_gm(42), Drum::ClosedHat);
        assert_eq!(Drum::from_gm(45), Drum::Tom(139.0));
        assert_eq!(Drum::from_gm(72), Drum::Sample);
    }
}
