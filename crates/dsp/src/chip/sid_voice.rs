//! The SID instrument's voice (#112): one note plays a whole chip, three
//! oscillators with their envelopes, ring modulation and sync wired as on
//! the chip (1←3, 2←1, 3←2), into one filter and the 4-bit volume.
//!
//! A pool of these is a stack of chips, one per note: three notes with one
//! oscillator each is the chip's own three-voice polyphony, one note with
//! all three a fat lead. Pitch, pulse width, cutoff and the chord
//! arpeggio change once per video frame (50 or 60 Hz), as a player
//! routine writes the registers; everything else runs per sample on the
//! chip's grid (`chip::sid`).

use crate::chip::Ring;
use crate::chip::sid::{
    self, Chip, Cycles, Env, Filter, NTSC, NTSC_FRAME, Osc, PAL, PAL_FRAME, env_dac, wave_dac,
    wave_zero,
};
use crate::mono::voice::MonoCtx;

/// Fields per oscillator in `SidPatch::set`, and where the chip-wide ones
/// start.
pub const OSC_FIELDS: usize = 12;
pub const GLOBAL_BASE: usize = 3 * OSC_FIELDS;
/// All of the patch's fields.
pub const FIELDS: usize = GLOBAL_BASE + 16;

/// Which oscillator syncs and ring-modulates each: 1 from 3, 2 from 1, 3
/// from 2.
const SOURCE: [usize; 3] = [2, 0, 1];
/// The chord arpeggios, semitones above the note, stepped one per
/// `arp_speed` frames: off, major, minor, octave, sus4, major 7th, minor
/// 7th, diminished, power.
pub const ARPS: [&[f32]; 9] = [
    &[0.0],
    &[0.0, 4.0, 7.0],
    &[0.0, 3.0, 7.0],
    &[0.0, 12.0],
    &[0.0, 5.0, 7.0],
    &[0.0, 4.0, 7.0, 11.0],
    &[0.0, 3.0, 7.0, 10.0],
    &[0.0, 3.0, 6.0],
    &[0.0, 7.0, 12.0],
];
/// Output level: one oscillator at full scale swings about ±0.5.
const OUT_GAIN: f32 = 0.6;
/// The C64's output coupling: a one-pole high-pass at about 16 Hz.
const DC_HZ: f32 = 16.0;

/// One oscillator's registers and what its player does with them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OscPatch {
    /// Waveform bits: 1 triangle, 2 saw, 4 pulse, 8 noise; 0 is off.
    pub wave: u8,
    /// Pulse width, 12 bits.
    pub pw: u16,
    /// Tuning from the note, in semitones and cents.
    pub coarse: f32,
    pub fine: f32,
    pub ring: bool,
    pub sync: bool,
    pub attack: u8,
    pub decay: u8,
    pub sustain: u8,
    pub release: u8,
    /// Routed through the filter.
    pub filter: bool,
    /// Pulse width added each frame, wrapping in 12 bits.
    pub pw_sweep: i16,
}

impl Default for OscPatch {
    fn default() -> OscPatch {
        OscPatch {
            wave: 0,
            pw: 0x800,
            coarse: 0.0,
            fine: 0.0,
            ring: false,
            sync: false,
            attack: 0,
            decay: 9,
            sustain: 10,
            release: 9,
            filter: false,
            pw_sweep: 0,
        }
    }
}

/// The SID's registers and the player's settings: plain values, set from
/// `Param::Sid*` (see `SidPatch::set` for the field order).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SidPatch {
    pub osc: [OscPatch; 3],
    pub chip: Chip,
    pub ntsc: bool,
    /// Cutoff, 11 bits; resonance, 4 bits; mode bits (1 LP, 2 BP, 4 HP).
    pub cutoff: u16,
    pub res: u8,
    pub mode: u8,
    /// Master volume, 4 bits.
    pub volume: u8,
    /// Mutes oscillator 3 when it is not filtered.
    pub voice3_off: bool,
    /// Band-limit the saw and pulse edges and sync resets (ADR-0007), or
    /// leave the raw steps and their aliasing.
    pub band_limit: bool,
    /// Cutoff added each frame from the note's start, clamped in 11 bits.
    pub cutoff_sweep: i16,
    /// The chord arpeggio (`ARPS`) and frames per step.
    pub arp: u8,
    pub arp_speed: u8,
    /// Clear the envelopes and oscillators on each note.
    pub hard_restart: bool,
    /// The 6581 filter's offset from the typical chip, in octaves.
    pub bias: f32,
    /// The player's vibrato: depth in semitones, rate in Hz, and frames
    /// before it starts.
    pub vibrato: f32,
    pub vibrato_rate: f32,
    pub vibrato_delay: u32,
}

impl Default for SidPatch {
    fn default() -> SidPatch {
        let mut osc = [OscPatch::default(); 3];
        osc[0].wave = sid::PULSE;
        SidPatch {
            osc,
            chip: Chip::Mos6581,
            ntsc: false,
            cutoff: 1024,
            res: 0,
            mode: sid::LP,
            volume: 15,
            voice3_off: false,
            band_limit: true,
            cutoff_sweep: 0,
            arp: 0,
            arp_speed: 1,
            hard_restart: true,
            bias: 0.0,
            vibrato: 0.0,
            vibrato_rate: 5.0,
            vibrato_delay: 0,
        }
    }
}

impl SidPatch {
    /// Set field `i` (`Param::sid`): per oscillator wave, pulse width,
    /// coarse, fine, ring, sync, attack, decay, sustain, release, filter and
    /// pulse sweep; then chip, clock, cutoff, resonance, filter mode,
    /// volume, voice 3 off, band-limit, cutoff sweep, arpeggio, its speed,
    /// hard restart, filter bias, and the vibrato's depth, rate and delay.
    pub fn set(&mut self, i: usize, v: f32) {
        let int = |hi: f32| v.round().clamp(0.0, hi);
        let on = v >= 0.5;
        if i < GLOBAL_BASE {
            let Some(o) = self.osc.get_mut(i / OSC_FIELDS) else {
                return;
            };
            match i % OSC_FIELDS {
                0 => o.wave = int(15.0) as u8,
                1 => o.pw = int(4095.0) as u16,
                2 => o.coarse = v,
                3 => o.fine = v,
                4 => o.ring = on,
                5 => o.sync = on,
                6 => o.attack = int(15.0) as u8,
                7 => o.decay = int(15.0) as u8,
                8 => o.sustain = int(15.0) as u8,
                9 => o.release = int(15.0) as u8,
                10 => o.filter = on,
                _ => o.pw_sweep = v.round().clamp(-4095.0, 4095.0) as i16,
            }
            return;
        }
        match i - GLOBAL_BASE {
            0 => self.chip = Chip::from_value(v),
            1 => self.ntsc = on,
            2 => self.cutoff = int(2047.0) as u16,
            3 => self.res = int(15.0) as u8,
            4 => self.mode = int(7.0) as u8,
            5 => self.volume = int(15.0) as u8,
            6 => self.voice3_off = on,
            7 => self.band_limit = on,
            8 => self.cutoff_sweep = v.round().clamp(-2047.0, 2047.0) as i16,
            9 => self.arp = int((ARPS.len() - 1) as f32) as u8,
            10 => self.arp_speed = int(16.0).max(1.0) as u8,
            11 => self.hard_restart = on,
            12 => self.bias = v.clamp(-1.0, 1.0),
            13 => self.vibrato = v.clamp(0.0, 2.0),
            14 => self.vibrato_rate = v.clamp(0.0, 12.0),
            _ => self.vibrato_delay = int(100.0) as u32,
        }
    }

    fn clock(&self) -> (f64, u32) {
        if self.ntsc {
            (NTSC, NTSC_FRAME)
        } else {
            (PAL, PAL_FRAME)
        }
    }

    /// Video frames per second.
    fn frame_rate(&self) -> f32 {
        let (clock, frame) = self.clock();
        (clock / f64::from(frame)) as f32
    }
}

#[derive(Clone, Copy, Default)]
pub struct SidVoice {
    note: f32,
    velocity: f32,
    gate: bool,
    pressed: bool,
    /// Pitch trim in semitones from the pool (unison, analog variance).
    pub trim: f32,
    pub cutoff_trim: f32,
    cycles: Cycles,
    /// Cycles into the current frame, and frames since the note began.
    frame_at: u32,
    frames: u32,
    osc: [Osc; 3],
    env: [Env; 3],
    ring: [Ring; 3],
    /// The pulse widths and cutoff the player has swept to.
    pw: [u16; 3],
    cutoff: i32,
    filter: Filter,
    /// The output high-pass's state.
    dc_x: f32,
    dc_y: f32,
}

impl SidVoice {
    pub fn new() -> SidVoice {
        SidVoice::default()
    }

    pub fn active(&self) -> bool {
        self.pressed || self.gate || self.env.iter().any(|e| !e.idle())
    }

    pub fn gated(&self) -> bool {
        self.gate
    }

    pub fn press(&mut self, note: u8, velocity: f32) {
        self.note = f32::from(note);
        self.velocity = velocity.clamp(0.0, 1.0);
        self.gate = true;
        self.pressed = true;
    }

    pub fn release_all(&mut self) {
        self.gate = false;
    }

    /// Start a note: the player's first frame.
    fn start(&mut self, ctx: &MonoCtx) {
        let p = &ctx.params.sid;
        let fresh = !self.env.iter().any(|e| !e.idle());
        let (clock, _) = p.clock();
        self.cycles = Cycles::new(clock, ctx.params.sample_rate());
        if p.hard_restart || fresh {
            for (o, e) in self.osc.iter_mut().zip(self.env.iter_mut()) {
                o.reset();
                e.hard_restart();
            }
        }
        if fresh {
            self.filter.reset();
            self.ring = [Ring::default(); 3];
            self.dc_x = 0.0;
            self.dc_y = 0.0;
        }
        // A new note is a gate edge, 0 then 1, as a player writes it: a key
        // let go in the same block as the next pressed still retriggers.
        for e in self.env.iter_mut() {
            e.gate(false);
            e.gate(true);
        }
        self.pw = p.osc.map(|o| o.pw);
        self.cutoff = i32::from(p.cutoff);
        self.frame_at = 0;
        self.frames = 0;
        self.frame(ctx, 0);
    }

    /// A player frame: sweep, arpeggiate, and write the frequencies and the
    /// filter. `i` is the sample in the block, for the shared LFO.
    fn frame(&mut self, ctx: &MonoCtx, i: usize) {
        let p = ctx.params;
        let s = &p.sid;
        let sweep = |v: u16, by: i16| ((i32::from(v) + i32::from(by)) & 0xFFF) as u16;
        if self.frames > 0 {
            for (pw, patch) in self.pw.iter_mut().zip(s.osc) {
                *pw = if patch.pw_sweep == 0 {
                    patch.pw
                } else {
                    sweep(*pw, patch.pw_sweep)
                };
            }
            self.cutoff = if s.cutoff_sweep == 0 {
                i32::from(s.cutoff)
            } else {
                (self.cutoff + i32::from(s.cutoff_sweep)).clamp(0, 2047)
            };
        }
        let chord = ARPS.get(usize::from(s.arp)).copied().unwrap_or(&[0.0]);
        let step = (self.frames / u32::from(s.arp_speed.max(1))) as usize;
        let arp = chord.get(step % chord.len().max(1)).copied().unwrap_or(0.0);
        let lfo = ctx
            .shared
            .map_or(0.0, |m| m.lfo.get(i).copied().unwrap_or(0.0));
        // The player's triangle vibrato, a step per frame after its delay,
        // and the mod wheel's on the shared LFO.
        let vib = match self.frames.checked_sub(s.vibrato_delay) {
            Some(n) if s.vibrato > 0.0 => {
                let phase = (n as f32 * s.vibrato_rate / s.frame_rate()).fract();
                s.vibrato * (1.0 - 4.0 * ((phase + 0.25).fract() - 0.5).abs())
            }
            _ => 0.0,
        };
        let vibrato = vib + lfo * p.mod_wheel * p.normals.vibrato;
        let (clock, _) = s.clock();
        let fs = p.sample_rate();
        let per = self.cycles.per_sample();
        for ((o, patch), pw) in self.osc.iter_mut().zip(s.osc).zip(self.pw) {
            let note =
                self.note + self.trim + p.bend + patch.coarse + patch.fine / 100.0 + arp + vibrato;
            let hz = ctx.pitch.at(note) * fs;
            o.set_freq(sid::freq_register(hz, clock), per);
            o.wave = patch.wave;
            o.pw = pw;
            o.ring = patch.ring;
        }
        for (e, patch) in self.env.iter_mut().zip(s.osc) {
            e.attack = patch.attack;
            e.decay = patch.decay;
            e.sustain = patch.sustain;
            e.release = patch.release;
        }
        // The pool's cutoff drift, in semitones, as a nudge of the register.
        let fc = (self.cutoff + (self.cutoff_trim * 8.0) as i32).clamp(0, 2047) as u16;
        self.filter.set(s.chip, fc, s.res, s.bias, fs);
        self.frames = self.frames.wrapping_add(1);
    }

    /// Add this voice into `out`, advancing its state.
    pub fn render(&mut self, ctx: &MonoCtx, out: &mut [f32]) {
        if self.pressed {
            self.pressed = false;
            self.start(ctx);
        }
        let s = ctx.params.sid;
        let fs = ctx.params.sample_rate();
        let (_, frame_len) = s.clock();
        let chip = s.chip;
        let zero = wave_dac(chip, wave_zero(chip));
        let gain = OUT_GAIN * self.velocity * f32::from(s.volume) / 15.0;
        let dc_r = 1.0 - std::f32::consts::TAU * DC_HZ / fs;
        for e in self.env.iter_mut() {
            e.gate(self.gate);
        }
        for (i, sample) in out.iter_mut().enumerate() {
            if !self.active() {
                return;
            }
            let cycles = self.cycles.take();
            self.frame_at += cycles;
            if self.frame_at >= frame_len {
                self.frame_at -= frame_len;
                self.frame(ctx, i);
            }
            let rise = self.osc.map(|o| o.msb_rise());
            let acc = self.osc.map(|o| o.acc());
            let (mut direct, mut filtered) = (0.0_f32, 0.0_f32);
            for k in 0..3 {
                let (Some(patch), Some(&src)) = (s.osc.get(k), SOURCE.get(k)) else {
                    continue;
                };
                let src_acc = acc.get(src).copied().unwrap_or(0);
                let sync = if patch.sync {
                    rise.get(src).copied().flatten()
                } else {
                    None
                };
                let (Some(o), Some(e), Some(r)) = (
                    self.osc.get_mut(k),
                    self.env.get_mut(k),
                    self.ring.get_mut(k),
                ) else {
                    continue;
                };
                e.clock(cycles);
                if patch.wave == 0 {
                    o.advance(sync);
                    continue;
                }
                let y = sid::render_osc(o, r, ctx.blep, chip, src_acc, sync, s.band_limit) - zero;
                let v = y * env_dac(chip, e.level());
                if patch.filter {
                    filtered += v;
                } else if !(k == 2 && s.voice3_off) {
                    direct += v;
                }
            }
            let mixed = direct + self.filter.process(chip, filtered, s.mode);
            // The output coupling takes off the DC the 6581's offsets leave.
            let y = mixed - self.dc_x + dc_r * self.dc_y;
            self.dc_x = mixed;
            self.dc_y = if y.is_finite() { y } else { 0.0 };
            *sample += self.dc_y * gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_is_idle_until_pressed() {
        let v = SidVoice::new();
        assert!(!v.active() && !v.gated());
    }

    #[test]
    fn the_patch_takes_its_fields() {
        let mut p = SidPatch::default();
        p.set(OSC_FIELDS, 6.0);
        p.set(OSC_FIELDS + 1, 5000.0);
        p.set(GLOBAL_BASE + 2, 3000.0);
        p.set(GLOBAL_BASE + 4, 5.0);
        p.set(GLOBAL_BASE, 1.0);
        assert_eq!(p.osc[1].wave, 6);
        assert_eq!(p.osc[1].pw, 4095);
        assert_eq!(p.cutoff, 2047);
        assert_eq!(p.mode, 5);
        assert_eq!(p.chip, Chip::Mos8580);
    }
}
