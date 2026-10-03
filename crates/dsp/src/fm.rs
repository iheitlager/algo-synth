//! The FM voice of the Yamaha DX7 (spec 006 Req 13): six sine operators, 32
//! algorithms, the DX7's envelopes, feedback, LFO and pitch envelope.
//!
//! Written in Rust from the public DX7 voice format, with Google's
//! "music-synthesizer-for-android" (Apache-2.0, see NOTICE) as the reference for
//! the algorithm routing, the envelope and the scaling curves.

pub mod algorithms;
pub mod envelope;
pub mod patch;
pub mod sysex;

use crate::fm::algorithms::{FB_IN, FB_OUT, OUT_BUS_ADD, algorithm, carriers};
use crate::fm::envelope::{Env, Lfo, N, PitchEnv, scale_out_level};
use crate::fm::patch::{FmPatch, OpPatch};
use crate::mono::MonoParams;
use crate::mono::voice::MonoCtx;

/// Operators below this gain make no sound.
const SILENT: f32 = 1.0e-4;
/// The output level of the voice: a full-level carrier peaks near 0.3.
const OUT_GAIN: f32 = 0.15;

/// How velocity (0..=127) changes an operator's output level, in 1/16 of a step.
const VELOCITY_DATA: [u8; 64] = [
    0, 70, 86, 97, 106, 114, 121, 126, 132, 138, 142, 148, 152, 156, 160, 163, 166, 170, 173, 174,
    178, 181, 184, 186, 189, 190, 194, 196, 198, 200, 202, 205, 206, 209, 211, 214, 216, 218, 220,
    222, 224, 225, 227, 229, 230, 232, 233, 235, 237, 238, 240, 241, 242, 243, 244, 246, 246, 248,
    249, 250, 251, 252, 253, 254,
];

fn scale_velocity(velocity: i32, sensitivity: i32) -> i32 {
    let v = velocity.clamp(0, 127);
    let delta = i32::from(VELOCITY_DATA.get((v >> 1) as usize).copied().unwrap_or(0)) - 239;
    ((sensitivity * delta + 7) >> 3) << 4
}

fn scale_rate(note: i32, sensitivity: i32) -> i32 {
    let x = (note / 3 - 7).clamp(0, 31);
    (sensitivity * x) >> 3
}

/// The exponential key-scaling curve's levels.
const EXP_SCALE: [u8; 33] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 14, 16, 19, 23, 27, 33, 39, 47, 56, 66, 80, 94, 110, 126,
    142, 158, 174, 190, 206, 222, 238, 250,
];

fn scale_curve(group: i32, depth: i32, curve: u8) -> i32 {
    let scale = if curve == 0 || curve == 3 {
        (group * depth * 329) >> 12
    } else {
        let raw = i32::from(
            EXP_SCALE
                .get((group as usize).min(EXP_SCALE.len() - 1))
                .copied()
                .unwrap_or(0),
        );
        (raw * depth * 329) >> 15
    };
    if curve < 2 { -scale } else { scale }
}

fn scale_level(note: i32, op: &OpPatch) -> i32 {
    let offset = note - i32::from(op.break_point) - 17;
    if offset >= 0 {
        scale_curve(offset / 3, i32::from(op.right_depth), op.right_curve)
    } else {
        scale_curve((-offset) / 3, i32::from(op.left_depth), op.left_curve)
    }
}

/// An operator's base frequency as log2 of Hz, from the note and its frequency settings.
fn osc_octaves(note: i32, op: &OpPatch) -> f64 {
    if op.mode == 0 {
        let ratio = if op.coarse == 0 {
            0.5
        } else {
            f64::from(op.coarse)
        };
        440.0_f64.log2()
            + f64::from(note - 69) / 12.0
            + ratio.log2()
            + (1.0 + 0.01 * f64::from(op.fine)).log2()
            + 12_606.0 * f64::from(i32::from(op.detune) - 7) / f64::from(1_u32 << 24)
    } else {
        // A fixed frequency: ten to the power of the coarse step and the fine.
        let steps = f64::from(i32::from(op.coarse & 3) * 100 + i32::from(op.fine));
        steps * 0.01 * 10.0_f64.log2()
            + if op.detune > 7 {
                13_457.0 * f64::from(i32::from(op.detune) - 7) / f64::from(1_u32 << 24)
            } else {
                0.0
            }
    }
}

/// One cycle of sine at `x` cycles (any real), interpolated from the 2049-point table.
fn sin_cycles(sine: &[f32], x: f32) -> f32 {
    let p = (x - x.floor()) * 2_048.0;
    let i = (p as usize).min(2_047);
    let frac = p - i as f32;
    let a = sine.get(i).copied().unwrap_or(0.0);
    let b = sine.get(i + 1).copied().unwrap_or(a);
    a + (b - a) * frac
}

#[derive(Clone, Copy)]
pub struct FmVoice {
    note: u8,
    gate: bool,
    retrigger: bool,
    idle: bool,
    /// Pitch trim in semitones from the pool (unison, analog variance).
    pub trim: f32,
    pub cutoff_trim: f32,
    env: [Env; 6],
    pitch_env: PitchEnv,
    lfo: Lfo,
    /// Each operator's base frequency as log2 of Hz, its phase in cycles, and its gain and
    /// step per sample at the start and end of the control step.
    base: [f64; 6],
    phase: [f32; 6],
    gain: [[f32; 2]; 6],
    inc: [f32; 6],
    /// The feedback history: the last two outputs of the operator that feeds back.
    fb: [f32; 2],
    /// Where in the control step the voice is.
    step: usize,
    pmd: i64,
    pms: i64,
    sample_rate: f32,
}

impl FmVoice {
    pub fn new(sample_rate: f32) -> FmVoice {
        FmVoice {
            note: 0,
            gate: false,
            retrigger: false,
            idle: true,
            trim: 0.0,
            cutoff_trim: 0.0,
            env: [Env::default(); 6],
            pitch_env: PitchEnv::new(sample_rate),
            lfo: Lfo::new(sample_rate),
            base: [0.0; 6],
            phase: [0.0; 6],
            gain: [[0.0; 2]; 6],
            inc: [0.0; 6],
            fb: [0.0; 2],
            step: 0,
            pmd: 0,
            pms: 0,
            sample_rate,
        }
    }

    pub fn active(&self) -> bool {
        self.retrigger || !self.idle
    }

    pub fn gated(&self) -> bool {
        self.gate
    }

    /// Start a note: set up each operator's envelope, frequency and scaling from the
    /// patch, and the pitch envelope and LFO.
    pub fn press(&mut self, note: u8, velocity: f32, p: &MonoParams) {
        let patch: &FmPatch = &p.fm;
        let midi = (i32::from(note) + i32::from(patch.transpose) - 24).clamp(0, 127);
        self.note = note;
        self.gate = true;
        self.retrigger = true;
        self.idle = false;
        self.step = 0;
        self.fb = [0.0; 2];
        let vel = (velocity.clamp(0.0, 1.0) * 127.0).round() as i32;
        for (i, op) in patch.ops.iter().enumerate() {
            let rates = op.rates.map(i32::from);
            let levels = op.levels.map(i32::from);
            let out = (scale_out_level(i32::from(op.level)) + scale_level(midi, op)).min(127);
            let outlevel = ((out << 5) + scale_velocity(vel, i32::from(op.vel_sens))).max(0);
            if let Some(e) = self.env.get_mut(i) {
                e.init(
                    rates,
                    levels,
                    outlevel,
                    scale_rate(midi, i32::from(op.rate_scale)),
                );
            }
            if let Some(b) = self.base.get_mut(i) {
                *b = osc_octaves(midi, op);
            }
            if let (Some(ph), Some(g)) = (self.phase.get_mut(i), self.gain.get_mut(i)) {
                *ph = 0.0;
                *g = [0.0; 2];
            }
        }
        self.pitch_env.set(
            patch.pitch_rates.map(i32::from),
            patch.pitch_levels.map(i32::from),
        );
        self.lfo.reset(
            patch.lfo_speed,
            patch.lfo_delay,
            patch.lfo_shape,
            patch.lfo_sync,
        );
        self.lfo.key_down();
        self.pmd = (i64::from(patch.lfo_pitch_depth) * 165) >> 6;
        const PITCH_MOD_SENS: [i64; 8] = [0, 10, 20, 33, 55, 92, 153, 255];
        self.pms = PITCH_MOD_SENS
            .get(usize::from(patch.pitch_sens & 7))
            .copied()
            .unwrap_or(0);
    }

    pub fn release_all(&mut self) {
        if self.gate {
            self.gate = false;
            for e in self.env.iter_mut() {
                e.key(false);
            }
            self.pitch_env.key(false);
        }
    }

    /// Work out every operator's gain and step for the next control step.
    fn control(&mut self, sine: &[f32], algorithm: usize) {
        let lfo = self.lfo.sample(sine);
        let delay = self.lfo.delay();
        let mut pitchmod = i64::from(self.pitch_env.step());
        let pmd = (self.pmd * i64::from(delay)) as u32 as i64;
        let senslfo = self.pms * i64::from(lfo - (1 << 23));
        pitchmod += (pmd * senslfo) >> 39;
        let octaves = pitchmod as f64 / f64::from(1_u32 << 24) + f64::from(self.trim) / 12.0;
        let carrier = carriers(algorithm);
        let mut loud = false;
        for (i, ((env, base), (gain, inc))) in self
            .env
            .iter_mut()
            .zip(self.base.iter())
            .zip(self.gain.iter_mut().zip(self.inc.iter_mut()))
            .enumerate()
        {
            let level = env.step();
            gain[0] = gain[1];
            gain[1] = (f64::from(level) / f64::from(1_u32 << 24) - 14.0).exp2() as f32;
            *inc = ((base + octaves).exp2() / f64::from(self.sample_rate)) as f32;
            if carrier.get(i).copied().unwrap_or(false) && (gain[0] > SILENT || gain[1] > SILENT) {
                loud = true;
            }
        }
        // A released voice ends when no carrier is heard any more.
        if !self.gate && !loud {
            self.idle = true;
        }
    }

    /// Add this voice into `out`, advancing its state.
    pub fn render(&mut self, ctx: &MonoCtx, out: &mut [f32]) {
        let patch = &ctx.params.fm;
        let alg_index = usize::from(patch.algorithm);
        let alg = algorithm(alg_index);
        let fb_scale = if patch.feedback == 0 {
            0.0
        } else {
            0.5 / f32::from(1_u16 << (8 - patch.feedback.min(7)))
        };
        for sample in out.iter_mut() {
            if self.idle && !self.retrigger {
                return;
            }
            if self.step == 0 {
                self.retrigger = false;
                self.control(ctx.sine, alg_index);
                if self.idle {
                    return;
                }
            }
            let t = self.step as f32 / N as f32;
            let mut bus = [0.0_f32; 3];
            let mut has = [true, false, false];
            let mut mix = 0.0;
            for (op, flags) in alg.iter().enumerate() {
                let (Some(gain), Some(inc), Some(phase)) =
                    (self.gain.get(op), self.inc.get(op), self.phase.get_mut(op))
                else {
                    continue;
                };
                let g = gain[0] + (gain[1] - gain[0]) * t;
                let outbus = usize::from(flags & 3);
                let inbus = usize::from((flags >> 4) & 3);
                let add = flags & OUT_BUS_ADD != 0;
                let reads = inbus != 0 && has.get(inbus).copied().unwrap_or(false);
                let feeds = flags & (FB_IN | FB_OUT) == (FB_IN | FB_OUT) && !reads;
                let y = if gain[0] < SILENT && gain[1] < SILENT {
                    0.0
                } else {
                    let input = if reads {
                        bus.get(inbus).copied().unwrap_or(0.0)
                    } else if feeds {
                        (self.fb[0] + self.fb[1]) * fb_scale
                    } else {
                        0.0
                    };
                    sin_cycles(ctx.sine, *phase + input) * g
                };
                if feeds {
                    self.fb = [self.fb[1], y];
                }
                if outbus == 0 {
                    mix += y;
                } else if let (Some(b), Some(h)) = (bus.get_mut(outbus), has.get_mut(outbus)) {
                    if add && *h {
                        *b += y;
                    } else {
                        *b = y;
                    }
                    *h = true;
                }
                *phase = (*phase + *inc).fract();
            }
            *sample += mix * OUT_GAIN;
            self.step += 1;
            if self.step == N {
                self.step = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mono::ladder::LadderTables;
    use crate::mono::osc::Blep;
    use crate::mono::voice::PitchTable;
    use crate::params::Param;
    use crate::table::Tables;
    use crate::voice::sine_table;

    const SR: f32 = 48_000.0;

    struct Rig {
        params: MonoParams,
        sine: Vec<f32>,
        blep: Blep,
        ladder: LadderTables,
        pitch: PitchTable,
        tables: &'static Tables,
        voice: FmVoice,
    }

    impl Rig {
        fn new(settings: &[(Param, f32)]) -> Rig {
            let mut params = MonoParams::new(SR);
            params.set(Param::Model, 13.0);
            for (p, v) in settings {
                params.set(*p, p.clamp(*v));
            }
            Rig {
                params,
                sine: sine_table(),
                blep: Blep::new(),
                ladder: LadderTables::new(SR),
                pitch: PitchTable::new(SR),
                tables: Tables::shared(SR),
                voice: FmVoice::new(SR),
            }
        }

        fn press(&mut self, note: u8, velocity: f32) {
            self.voice.press(note, velocity, &self.params);
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

    /// Algorithm 32 (index 31): six carriers, so one operator alone is a sine.
    const ALL_CARRIERS: (Param, f32) = (Param::Algorithm, 31.0);

    fn tone(out: &[f32], hz: f64) -> f64 {
        let w = std::f64::consts::TAU * hz / f64::from(SR);
        let (mut re, mut im) = (0.0, 0.0);
        for (i, y) in out.iter().enumerate() {
            re += f64::from(*y) * (w * i as f64).cos();
            im += f64::from(*y) * (w * i as f64).sin();
        }
        2.0 * (re * re + im * im).sqrt() / out.len() as f64
    }

    /// The frequency from the interpolated times of the upward zero crossings.
    fn measured_hz(out: &[f32]) -> f64 {
        let mut times = Vec::new();
        for (i, w) in out.windows(2).enumerate() {
            if w[0] <= 0.0 && w[1] > 0.0 {
                times.push(i as f64 + f64::from(-w[0] / (w[1] - w[0])));
            }
        }
        (times.len() - 1) as f64 / ((times[times.len() - 1] - times[0]) / f64::from(SR))
    }

    /// Spec 006 Req 13: a carrier sounds at the played pitch, within a cent, from note 24 to 108.
    #[test]
    fn a_carrier_sounds_at_the_played_pitch() {
        for note in [24_u8, 36, 48, 60, 69, 84, 96, 108] {
            let mut r = Rig::new(&[ALL_CARRIERS]);
            r.press(note, 1.0);
            r.render(4_800);
            let out = r.render(96_000);
            let want = 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0);
            let cents = 1200.0 * (measured_hz(&out) / want).log2();
            assert!(cents.abs() < 1.0, "note {note}: {cents} cents");
        }
    }

    /// An operator that nothing modulates is a sine; modulated, it grows partials, but only
    /// through the algorithm's routes.
    #[test]
    fn modulators_add_partials_only_through_their_routes() {
        let h2 = |settings: &[(Param, f32)]| {
            let mut r = Rig::new(settings);
            r.press(57, 1.0);
            r.render(9_600);
            let out = r.render(48_000);
            (tone(&out, 220.0), tone(&out, 440.0))
        };
        // Algorithm 32: operator 2 is a carrier like the others, so it adds nothing to operator 1.
        let (f, second) = h2(&[ALL_CARRIERS, (Param::Op2Level, 90.0)]);
        assert!(f > 0.01 && second < 0.02 * f, "a pure sine: {f} {second}");
        // Algorithm 1: operator 2 modulates operator 1, one octave up in its ratio.
        let (f, second) = h2(&[(Param::Algorithm, 0.0), (Param::Op2Level, 80.0)]);
        assert!(f > 0.01 && second > 0.2 * f, "FM partials: {f} {second}");
    }

    #[test]
    fn feedback_adds_harmonics_to_operator_6() {
        let h3 = |feedback: f32| {
            let mut r = Rig::new(&[
                ALL_CARRIERS,
                (Param::Op1Level, 0.0),
                (Param::Op6Level, 99.0),
                (Param::Feedback, feedback),
            ]);
            r.press(57, 1.0);
            r.render(9_600);
            let out = r.render(48_000);
            (tone(&out, 220.0), tone(&out, 660.0))
        };
        let (f0, h0) = h3(0.0);
        let (f7, h7) = h3(7.0);
        assert!(h0 < 0.01 * f0, "no feedback, a sine: {f0} {h0}");
        assert!(h7 > 0.1 * f7, "feedback 7 is rich: {f7} {h7}");
    }

    /// Every algorithm with every operator at full level and full feedback stays finite and bounded.
    #[test]
    fn every_algorithm_stays_bounded_at_full_modulation() {
        for algorithm in 0..32 {
            for note in [36_u8, 60, 84, 108] {
                let mut settings =
                    vec![(Param::Algorithm, algorithm as f32), (Param::Feedback, 7.0)];
                for (op, coarse) in [1.0, 2.0, 3.0, 1.0, 5.0, 14.0].into_iter().enumerate() {
                    let n = op + 1;
                    settings.push((op_param(n, "Level"), 99.0));
                    settings.push((op_param(n, "Coarse"), coarse));
                }
                let mut r = Rig::new(&settings);
                r.press(note, 1.0);
                let out = r.render(24_000);
                assert!(
                    out.iter().all(|s| s.is_finite() && s.abs() <= 3.0),
                    "algorithm {} note {note}",
                    algorithm + 1
                );
                assert!(
                    out.iter().any(|s| s.abs() > 0.01),
                    "algorithm {} sounds",
                    algorithm + 1
                );
            }
        }
    }

    fn op_param(op: usize, field: &str) -> Param {
        let name = format!("Op{op}{field}");
        Param::ALL
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(p, _)| *p)
            .expect("a DX7 parameter")
    }

    #[test]
    fn velocity_sensitivity_shapes_the_level() {
        let rms = |velocity: f32, sens: f32| {
            let mut r = Rig::new(&[ALL_CARRIERS, (Param::Op1VelSens, sens)]);
            r.press(57, velocity);
            r.render(9_600);
            let out = r.render(24_000);
            (out.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / out.len() as f64).sqrt()
        };
        assert!(
            rms(1.0, 7.0) > 1.5 * rms(0.2, 7.0),
            "sensitive: loud against soft"
        );
        let (a, b) = (rms(1.0, 0.0), rms(0.2, 0.0));
        assert!((a / b - 1.0).abs() < 0.05, "insensitive: {a} {b}");
    }

    #[test]
    fn a_released_voice_falls_silent_and_ends() {
        let mut r = Rig::new(&[ALL_CARRIERS, (Param::Op1R4, 70.0)]);
        r.press(57, 1.0);
        r.render(9_600);
        assert!(r.voice.active() && r.voice.gated());
        r.voice.release_all();
        let tail = r.render(96_000);
        assert!(tail[tail.len() - 2_000..].iter().all(|s| s.abs() < 1.0e-4));
        assert!(!r.voice.active(), "a silent, released voice is done");
    }

    /// A pitch envelope that starts high and glides down: the early pitch is higher.
    #[test]
    fn the_pitch_envelope_bends_the_note() {
        let mut r = Rig::new(&[
            ALL_CARRIERS,
            (Param::PitchL4, 70.0),
            (Param::PitchL1, 50.0),
            (Param::PitchL2, 50.0),
            (Param::PitchL3, 50.0),
            (Param::PitchR1, 55.0),
        ]);
        r.press(57, 1.0);
        let early = measured_hz(&r.render(4_800)[480..]);
        r.render(96_000);
        let late = measured_hz(&r.render(48_000));
        assert!(early > late * 1.1, "{early} falls to {late}");
        assert!(
            (late - 220.0).abs() < 1.0,
            "and settles on the note: {late}"
        );
    }

    #[test]
    fn key_scaling_follows_the_curves() {
        let op = OpPatch {
            break_point: 39,
            right_depth: 99,
            right_curve: 3,
            left_depth: 99,
            left_curve: 0,
            ..OpPatch::default()
        };
        assert!(
            scale_level(100, &op) > scale_level(60, &op),
            "+lin to the right raises high notes"
        );
        assert!(
            scale_level(20, &op) < 0,
            "-lin to the left lowers low notes"
        );
        assert_eq!(
            scale_level(
                56,
                &OpPatch {
                    right_depth: 0,
                    ..op
                }
            ),
            0
        );
    }

    #[test]
    fn transposition_moves_the_pitch() {
        let mut r = Rig::new(&[ALL_CARRIERS, (Param::Transpose, 36.0)]);
        r.press(45, 1.0);
        r.render(4_800);
        let hz = measured_hz(&r.render(96_000));
        assert!((hz - 220.0).abs() < 0.5, "an octave up from A2: {hz}");
    }
}
