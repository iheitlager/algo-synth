//! The Casio VL-1 (VL-Tone, 1981) as a voice (#723, ADR-0040): one digital
//! tone generator stepping a 16-step, 4-bit waveform, the 8-digit "ADSR"
//! code (waveform, attack, decay, sustain level, sustain time, release,
//! vibrato, tremolo, each 0–9), an octave switch and the RC filter and
//! speaker the square waves went through.
//!
//! With `kit` on, a key plays one of the rhythm unit's three sounds,
//! as Casio named them: Po (30 ms), Pi (20 ms) and Sha (160 ms).
//!
//! Public sources give the tone names, the code's digits and the drums'
//! names and lengths; the waveforms ("square waves with varied pulse
//! widths"), the digits' times and the drums' pitches are reconstructions.

use crate::chip::Ring;
use crate::mono::voice::MonoCtx;

/// All of the patch's fields (`VlPatch::set`).
pub const FIELDS: usize = 10;

/// The ten waveforms, 16 steps of 4 bits a period, built as the VL-1's tones
/// are said to be, from pulses of several widths summed: Piano, Fantasy,
/// Violin, Flute, Guitar I, Guitar II, English Horn, Electro I–III.
pub const WAVES: [[u8; 16]; 10] = [
    // A quarter pulse over an octave square at a third of its level.
    [15, 15, 15, 15, 0, 0, 0, 0, 5, 5, 5, 5, 0, 0, 0, 0],
    // An eighth pulse on a half square: nasal.
    [15, 15, 7, 7, 7, 7, 7, 7, 0, 0, 0, 0, 0, 0, 0, 0],
    // Four pulses halving in width: a staircase saw.
    [15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0],
    // A square, the filter takes the edge off.
    [15, 15, 15, 15, 15, 15, 15, 15, 0, 0, 0, 0, 0, 0, 0, 0],
    // A 37.5 % pulse.
    [15, 15, 15, 15, 15, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    // A quarter pulse with an eighth on top.
    [15, 15, 10, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    // An eighth pulse: reedy.
    [15, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    // A square and its octave.
    [15, 15, 15, 15, 8, 8, 8, 8, 7, 7, 7, 7, 0, 0, 0, 0],
    // A sixteenth pulse: thin and buzzy.
    [15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    // Four steps down a period.
    [15, 15, 15, 15, 10, 10, 10, 10, 5, 5, 5, 5, 0, 0, 0, 0],
];

/// Seconds for each digit of attack, decay, sustain time and release.
const TIMES: [f32; 10] = [0.002, 0.02, 0.05, 0.1, 0.2, 0.35, 0.6, 1.0, 1.6, 2.5];
/// Output level: a full square swings ±0.5 before this.
const OUT_GAIN: f32 = 0.7;
/// The RC filter after the tone generator, and the output coupling.
const LP_HZ: f32 = 4_500.0;
const DC_HZ: f32 = 20.0;
/// The vibrato's and tremolo's rates, and their depth per digit: semitones,
/// and a fraction of the level.
const VIBRATO_HZ: f32 = 5.5;
const VIBRATO_STEP: f32 = 0.04;
const TREMOLO_HZ: f32 = 6.0;
const TREMOLO_STEP: f32 = 0.08;
/// The vibrato waits this long after the key before it starts.
const VIBRATO_DELAY: f32 = 0.25;
/// The noise generator's clock, as a fraction of the sample rate's 48 kHz.
const NOISE_HZ: f32 = 16_000.0;

/// The 8-digit code, the octave switch and the drum kit: plain values,
/// set from `Param::Vl*` (see `VlPatch::set` for the field order).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VlPatch {
    pub wave: u8,
    pub attack: u8,
    pub decay: u8,
    pub sustain: u8,
    /// How long the sustain holds before the release starts; 9 holds while
    /// the key is down.
    pub sustain_time: u8,
    pub release: u8,
    pub vibrato: u8,
    pub tremolo: u8,
    /// −1, 0 or 1 octave.
    pub octave: i8,
    pub kit: bool,
}

impl Default for VlPatch {
    /// The Piano: no attack, a decay to nothing, a short release.
    fn default() -> VlPatch {
        VlPatch {
            wave: 0,
            attack: 0,
            decay: 7,
            sustain: 0,
            sustain_time: 0,
            release: 3,
            vibrato: 0,
            tremolo: 0,
            octave: 0,
            kit: false,
        }
    }
}

impl VlPatch {
    /// Set field `i` (`Param::vl`): waveform, attack, decay, sustain level,
    /// sustain time, release, vibrato, tremolo, octave and kit.
    pub fn set(&mut self, i: usize, v: f32) {
        let digit = v.round().clamp(0.0, 9.0) as u8;
        match i {
            0 => self.wave = digit,
            1 => self.attack = digit,
            2 => self.decay = digit,
            3 => self.sustain = digit,
            4 => self.sustain_time = digit,
            5 => self.release = digit,
            6 => self.vibrato = digit,
            7 => self.tremolo = digit,
            8 => self.octave = v.round().clamp(-1.0, 1.0) as i8,
            _ => self.kit = v >= 0.5,
        }
    }
}

/// The rhythm unit's three sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drum {
    /// A low blip, 30 ms: the kick and the toms.
    Po,
    /// A high blip, 20 ms: rim, claves, cowbell.
    Pi,
    /// A burst of noise, 160 ms: snare, hats and cymbals.
    Sha,
}

impl Drum {
    /// The sound a General MIDI drum key plays: Po on the kicks and toms,
    /// Pi on the rim shot, claves, cowbell and woodblocks, Sha on the rest.
    pub fn from_gm(note: u8) -> Drum {
        match note {
            35 | 36 | 41 | 43 | 45 | 47 | 48 | 50 | 60..=64 => Drum::Po,
            37 | 56 | 75 | 76 | 77 => Drum::Pi,
            _ => Drum::Sha,
        }
    }

    fn seconds(self) -> f32 {
        match self {
            Drum::Po => 0.03,
            Drum::Pi => 0.02,
            Drum::Sha => 0.16,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    #[default]
    Idle,
}

#[derive(Clone, Copy, Default)]
pub struct VlVoice {
    note: f32,
    velocity: f32,
    gate: bool,
    pressed: bool,
    /// Pitch trim in semitones from the pool (unison, analog variance).
    pub trim: f32,
    /// Read by the pool; the VL-1 has no filter to drift.
    pub cutoff_trim: f32,
    /// Phase through the waveform's 16 steps, 0..16.
    phase: f32,
    stage: Stage,
    level: f32,
    /// Samples the stage has run, for the attack's ramp and sustain time.
    held: u32,
    /// Samples since the key, for the vibrato's delay and the LFOs.
    age: u32,
    drum: Option<Drum>,
    /// Samples the drum has left, and its length.
    left: u32,
    length: u32,
    lfsr: u16,
    noise_at: f32,
    ring: Ring,
    lp: f32,
    dc_x: f32,
    dc_y: f32,
}

impl VlVoice {
    pub fn new() -> VlVoice {
        VlVoice::default()
    }

    pub fn active(&self) -> bool {
        self.pressed || self.gate || self.stage != Stage::Idle || self.left > 0
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

    /// Start a note: from the level it is at, so a monophonic VL-1 playing
    /// legato does not click.
    fn start(&mut self, ctx: &MonoCtx) {
        let p = &ctx.params.vl;
        if self.stage == Stage::Idle && self.left == 0 {
            self.ring = Ring::default();
            self.phase = 0.0;
            self.lp = 0.0;
            self.dc_x = 0.0;
            self.dc_y = 0.0;
            self.level = 0.0;
        }
        self.age = 0;
        self.held = 0;
        self.drum = None;
        self.left = 0;
        if p.kit {
            let d = Drum::from_gm(self.note as u8);
            self.length = (d.seconds() * ctx.params.sample_rate()) as u32;
            self.left = self.length;
            self.drum = Some(d);
            self.stage = Stage::Idle;
            self.lfsr = 0x4A31;
            return;
        }
        self.stage = Stage::Attack;
    }

    /// Add this voice into `out`, advancing its state.
    pub fn render(&mut self, ctx: &MonoCtx, out: &mut [f32]) {
        if self.pressed {
            self.pressed = false;
            self.start(ctx);
        }
        let p = ctx.params;
        let s = p.vl;
        let fs = p.sample_rate();
        // Coefficients once a block: −60 dB in each stage's time.
        let fall = |digit: u8| {
            let t = TIMES.get(usize::from(digit)).copied().unwrap_or(1.0);
            (-6.9 / (t * fs)).exp()
        };
        let attack = (TIMES.get(usize::from(s.attack)).copied().unwrap_or(0.0) * fs).max(1.0);
        let (decay, release) = (fall(s.decay), fall(s.release));
        let sustain = f32::from(s.sustain) / 9.0;
        let hold = match s.sustain_time {
            9 => u32::MAX,
            d => (TIMES.get(usize::from(d)).copied().unwrap_or(0.0) * fs) as u32,
        };
        let lp_a = 1.0 - (-std::f32::consts::TAU * LP_HZ / fs).exp();
        let dc_r = 1.0 - std::f32::consts::TAU * DC_HZ / fs;
        let wave = WAVES.get(usize::from(s.wave)).copied().unwrap_or(WAVES[0]);
        let gain = OUT_GAIN * self.velocity;
        let lfo = ctx
            .shared
            .map_or(0.0, |m| m.lfo.first().copied().unwrap_or(0.0));
        let base = self.note + self.trim + p.bend + 12.0 * f32::from(s.octave);
        let vib_depth = VIBRATO_STEP * f32::from(s.vibrato);
        let trem_depth = TREMOLO_STEP * f32::from(s.tremolo);
        let delay = (VIBRATO_DELAY * fs) as u32;
        let noise_step = NOISE_HZ / fs;
        if !self.gate && matches!(self.stage, Stage::Attack | Stage::Decay | Stage::Sustain) {
            self.stage = Stage::Release;
        }
        for sample in out.iter_mut() {
            if !self.active() {
                return;
            }
            let t = self.age as f32 / fs;
            let (y, amp) = match self.drum {
                Some(d) => self.drum_sample(d, ctx, noise_step),
                None => {
                    let vib = if self.age > delay && vib_depth > 0.0 {
                        let ph = ((self.age - delay) as f32 / fs * VIBRATO_HZ).fract();
                        vib_depth * (1.0 - 4.0 * ((ph + 0.25).fract() - 0.5).abs())
                    } else {
                        0.0
                    };
                    let note = base + vib + lfo * p.mod_wheel * p.normals.vibrato;
                    let inc = 16.0 * ctx.pitch.at(note);
                    let y = self.tone(&wave, inc, ctx);
                    let trem = 1.0
                        - trem_depth * 0.5 * (1.0 - (t * TREMOLO_HZ * std::f32::consts::TAU).cos());
                    self.envelope(attack, decay, sustain, hold, release);
                    (y, self.level * trem)
                }
            };
            self.age = self.age.saturating_add(1);
            self.lp += lp_a * (y * amp - self.lp);
            let v = self.lp - self.dc_x + dc_r * self.dc_y;
            self.dc_x = self.lp;
            self.dc_y = if v.is_finite() { v } else { 0.0 };
            *sample += self.dc_y * gain;
        }
    }

    /// The next sample of the waveform, its steps band-limited (ADR-0007),
    /// moving on by `inc` steps.
    fn tone(&mut self, wave: &[u8; 16], inc: f32, ctx: &MonoCtx) -> f32 {
        let dac = |k: usize| f32::from(wave.get(k & 15).copied().unwrap_or(0)) / 15.0 - 0.5;
        let k = self.phase as usize;
        self.ring.add_now(dac(k));
        let inc = inc.clamp(0.0, 8.0);
        let end = self.phase + inc;
        let mut step = k;
        // Each step boundary crossed before the next sample.
        while (step + 1) as f32 <= end && inc > 0.0 {
            let edge = (step + 1) as f32;
            let after = (end - edge) / inc;
            self.ring
                .correct(ctx.blep, dac(step + 1) - dac(step), after);
            step += 1;
        }
        self.phase = end % 16.0;
        self.ring.take()
    }

    fn envelope(&mut self, attack: f32, decay: f32, sustain: f32, hold: u32, release: f32) {
        self.held = self.held.saturating_add(1);
        match self.stage {
            Stage::Attack => {
                self.level += 1.0 / attack;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                    self.held = 0;
                }
            }
            Stage::Decay => {
                self.level = sustain + (self.level - sustain) * decay;
                if self.level - sustain < 0.001 {
                    self.level = sustain;
                    self.stage = Stage::Sustain;
                    self.held = 0;
                }
            }
            Stage::Sustain => {
                if self.level < 0.001 {
                    self.stage = Stage::Idle;
                    self.level = 0.0;
                } else if self.held >= hold {
                    self.stage = Stage::Release;
                }
            }
            Stage::Release => {
                self.level *= release;
                if self.level < 0.0005 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
            Stage::Idle => {}
        }
    }

    /// The next sample of a drum and its level: Po and Pi a square that
    /// falls a little in pitch and fades over its length, Sha the noise
    /// generator dying away.
    fn drum_sample(&mut self, d: Drum, ctx: &MonoCtx, noise_step: f32) -> (f32, f32) {
        self.left = self.left.saturating_sub(1);
        let x = 1.0 - self.left as f32 / self.length.max(1) as f32;
        match d {
            Drum::Po | Drum::Pi => {
                let (hz, fall) = if d == Drum::Po {
                    (180.0, 0.5)
                } else {
                    (1_400.0, 0.2)
                };
                let inc = 16.0 * hz * (1.0 - fall * x) / ctx.params.sample_rate();
                let y = self.tone(&WAVES[3], inc, ctx);
                (y, 1.0 - x)
            }
            Drum::Sha => {
                self.noise_at += noise_step;
                while self.noise_at >= 1.0 {
                    self.noise_at -= 1.0;
                    let bit = (self.lfsr ^ (self.lfsr >> 1)) & 1;
                    self.lfsr = (self.lfsr >> 1) | (bit << 14);
                }
                let y = if self.lfsr & 1 == 1 { 0.5 } else { -0.5 };
                (y, (1.0 - x) * (1.0 - x) * 0.8)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_is_idle_until_pressed() {
        let v = VlVoice::new();
        assert!(!v.active() && !v.gated());
    }

    #[test]
    fn the_patch_takes_its_digits() {
        let mut p = VlPatch::default();
        p.set(0, 7.0);
        p.set(3, 12.0);
        p.set(8, -3.0);
        p.set(9, 1.0);
        assert_eq!(p.wave, 7);
        assert_eq!(p.sustain, 9);
        assert_eq!(p.octave, -1);
        assert!(p.kit);
    }

    #[test]
    fn drums_come_from_general_midi() {
        assert_eq!(Drum::from_gm(36), Drum::Po);
        assert_eq!(Drum::from_gm(37), Drum::Pi);
        assert_eq!(Drum::from_gm(38), Drum::Sha);
        assert_eq!(Drum::from_gm(42), Drum::Sha);
        assert_eq!(Drum::from_gm(45), Drum::Po);
    }

    #[test]
    fn every_wave_spans_its_four_bits() {
        for w in WAVES {
            assert_eq!(w.iter().max(), Some(&15));
            assert_eq!(w.iter().min(), Some(&0));
        }
    }
}
