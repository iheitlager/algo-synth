//! The DX7's envelope, pitch envelope and LFO (spec 006 Req 13), in the integer
//! arithmetic of Google's "music-synthesizer-for-android" (Apache-2.0, see NOTICE), which
//! models the hardware: a four-rate, four-level envelope in a logarithmic level
//! (`level` is log2 of the gain in 24-bit fixed point, plus 14 octaves), with
//! exponential decays, near-linear attacks, and a rate that doubles every four steps.
//!
//! Envelopes and the LFO advance once per control step of `N` samples (ADR-0002: the
//! gain and frequency are worked out per step, not per sample).

/// log2 of the control step: 64 samples.
pub const LG_N: u32 = 6;
/// Samples per control step.
pub const N: usize = 1 << LG_N;

/// The level an attack jumps to at once, in log units: a crisp start.
const JUMP_TARGET: i32 = 1716;

/// The DX7's table for output levels 0..=19 (above that, 28 + level).
const LEVEL_LUT: [i32; 20] = [
    0, 5, 9, 13, 17, 20, 23, 25, 27, 29, 31, 33, 35, 37, 39, 41, 42, 43, 45, 46,
];

/// An output or envelope level 0..=99 on the DX7's scale (the units are 0.75 dB).
pub fn scale_out_level(level: i32) -> i32 {
    let l = level.clamp(0, 127);
    if l >= 20 {
        28 + l
    } else {
        LEVEL_LUT.get(l as usize).copied().unwrap_or(0)
    }
}

#[derive(Clone, Copy, Default)]
pub struct Env {
    rates: [i32; 4],
    levels: [i32; 4],
    outlevel: i32,
    rate_scaling: i32,
    level: i32,
    targetlevel: i32,
    rising: bool,
    /// The segment: 0..=2 run, 3 is the release, 4 is done.
    ix: usize,
    inc: i32,
    down: bool,
}

impl Env {
    /// Start an envelope: the four rates and levels (0..=99), the operator's output
    /// level (key scaling and velocity included) and its rate scaling.
    pub fn init(&mut self, rates: [i32; 4], levels: [i32; 4], outlevel: i32, rate_scaling: i32) {
        self.rates = rates;
        self.levels = levels;
        self.outlevel = outlevel;
        self.rate_scaling = rate_scaling;
        self.level = 0;
        self.down = true;
        self.advance(0);
    }

    /// The level in log units, advanced one control step.
    pub fn step(&mut self) -> i32 {
        if self.ix < 3 || (self.ix < 4 && !self.down) {
            if self.rising {
                if self.level < JUMP_TARGET << 16 {
                    self.level = JUMP_TARGET << 16;
                }
                self.level += (((17 << 24) - self.level) >> 24) * self.inc;
                if self.level >= self.targetlevel {
                    self.level = self.targetlevel;
                    self.advance(self.ix + 1);
                }
            } else {
                self.level -= self.inc;
                if self.level <= self.targetlevel {
                    self.level = self.targetlevel;
                    self.advance(self.ix + 1);
                }
            }
        }
        self.level
    }

    /// The key goes down or up.
    pub fn key(&mut self, down: bool) {
        if self.down != down {
            self.down = down;
            self.advance(if down { 0 } else { 3 });
        }
    }

    fn advance(&mut self, ix: usize) {
        self.ix = ix;
        if ix < 4 {
            let newlevel = self.levels.get(ix).copied().unwrap_or(0);
            let actual = scale_out_level(newlevel) >> 1;
            let actual = ((actual << 6) + self.outlevel - 4256).max(16);
            self.targetlevel = actual << 16;
            self.rising = self.targetlevel > self.level;
            let rate = self.rates.get(ix).copied().unwrap_or(0);
            let qrate = ((rate * 41) >> 6) + self.rate_scaling;
            let qrate = qrate.min(63);
            self.inc = (4 + (qrate & 3)) << (2 + LG_N as i32 + (qrate >> 2));
        }
    }
}

/// Pitch envelope level 0..=99 to log-frequency units (±4 octaves, 1 << 24 per octave after << 19).
const PITCH_TAB: [i8; 100] = [
    -128, -116, -104, -95, -85, -76, -68, -61, -56, -52, -49, -46, -43, -41, -39, -37, -35, -33,
    -32, -31, -30, -29, -28, -27, -26, -25, -24, -23, -22, -21, -20, -19, -18, -17, -16, -15, -14,
    -13, -12, -11, -10, -9, -8, -7, -6, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
    38, 40, 43, 46, 49, 53, 58, 65, 73, 82, 92, 103, 115, 127,
];

/// Pitch envelope rate 0..=99 to a speed.
const RATE_TAB: [u8; 100] = [
    1, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 16,
    16, 17, 18, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 30, 31, 33, 34, 36, 37, 38, 39, 41, 42,
    44, 46, 47, 49, 51, 53, 54, 56, 58, 60, 62, 64, 66, 68, 70, 72, 74, 76, 79, 82, 85, 88, 91, 94,
    98, 102, 106, 110, 115, 120, 125, 130, 135, 141, 147, 153, 159, 165, 171, 178, 185, 193, 202,
    211, 232, 243, 254, 255,
];

#[derive(Clone, Copy, Default)]
pub struct PitchEnv {
    rates: [i32; 4],
    levels: [i32; 4],
    level: i32,
    targetlevel: i32,
    rising: bool,
    ix: usize,
    inc: i32,
    down: bool,
    unit: i32,
}

fn pitch_at(level: i32) -> i32 {
    i32::from(
        PITCH_TAB
            .get(level.clamp(0, 99) as usize)
            .copied()
            .unwrap_or(0),
    ) << 19
}

impl PitchEnv {
    pub fn new(sample_rate: f32) -> PitchEnv {
        PitchEnv {
            unit: ((N as f64) * f64::from(1_u32 << 24) / (21.3 * f64::from(sample_rate)) + 0.5)
                as i32,
            ..PitchEnv::default()
        }
    }

    pub fn set(&mut self, rates: [i32; 4], levels: [i32; 4]) {
        self.rates = rates;
        self.levels = levels;
        self.level = pitch_at(levels[3]);
        self.down = true;
        self.advance(0);
    }

    /// The offset in log-frequency units (1 << 24 is an octave), one control step on.
    pub fn step(&mut self) -> i32 {
        if self.ix < 3 || (self.ix < 4 && !self.down) {
            if self.rising {
                self.level += self.inc;
                if self.level >= self.targetlevel {
                    self.level = self.targetlevel;
                    self.advance(self.ix + 1);
                }
            } else {
                self.level -= self.inc;
                if self.level <= self.targetlevel {
                    self.level = self.targetlevel;
                    self.advance(self.ix + 1);
                }
            }
        }
        self.level
    }

    pub fn key(&mut self, down: bool) {
        if self.down != down {
            self.down = down;
            self.advance(if down { 0 } else { 3 });
        }
    }

    fn advance(&mut self, ix: usize) {
        self.ix = ix;
        if ix < 4 {
            self.targetlevel = pitch_at(self.levels.get(ix).copied().unwrap_or(50));
            self.rising = self.targetlevel > self.level;
            let rate = self.rates.get(ix).copied().unwrap_or(0).clamp(0, 99) as usize;
            self.inc = i32::from(RATE_TAB.get(rate).copied().unwrap_or(1)) * self.unit;
        }
    }
}

/// The DX7's LFO: a speed, a delay before it fades in, and six waveforms.
#[derive(Clone, Copy, Default)]
pub struct Lfo {
    phase: u32,
    delta: u32,
    unit: u32,
    waveform: u8,
    sync: bool,
    randstate: u32,
    delaystate: u32,
    delayinc: u32,
    delayinc2: u32,
}

impl Lfo {
    pub fn new(sample_rate: f32) -> Lfo {
        Lfo {
            unit: ((N as f64) * 25_190_424.0 / f64::from(sample_rate) + 0.5) as u32,
            ..Lfo::default()
        }
    }

    /// Set from the voice's LFO settings: speed, delay, waveform, key sync (each 0..=99).
    pub fn reset(&mut self, speed: u8, delay: u8, waveform: u8, sync: bool) {
        let rate = i32::from(speed);
        let mut sr = if rate == 0 { 1 } else { (165 * rate) >> 6 };
        sr *= if sr < 160 { 11 } else { 11 + ((sr - 160) >> 4) };
        self.delta = self.unit.wrapping_mul(sr as u32);
        let a = 99 - i32::from(delay.min(99));
        if a == 99 {
            self.delayinc = !0;
            self.delayinc2 = !0;
        } else {
            let a = (16 + (a & 15)) << (1 + (a >> 4));
            self.delayinc = self.unit.wrapping_mul(a as u32);
            let a = (a & 0xff80).max(0x80);
            self.delayinc2 = self.unit.wrapping_mul(a as u32);
        }
        self.waveform = waveform;
        self.sync = sync;
    }

    /// The waveform, 0 to 2^24, one control step on.
    pub fn sample(&mut self, sine: &[f32]) -> i32 {
        self.phase = self.phase.wrapping_add(self.delta);
        let p = self.phase;
        match self.waveform {
            0 => {
                let mut x = p >> 7;
                x ^= 0u32.wrapping_sub(p >> 31);
                (x & ((1 << 24) - 1)) as i32
            }
            1 => ((!p ^ (1 << 31)) >> 8) as i32,
            2 => ((p ^ (1 << 31)) >> 8) as i32,
            3 => (((!p) >> 7) & (1 << 24)) as i32,
            4 => {
                let idx = (p >> 21) as usize;
                let s = sine.get(idx).copied().unwrap_or(0.0);
                (1 << 23) + (((s * 16_777_216.0) as i32) >> 1)
            }
            _ => {
                if p < self.delta {
                    self.randstate = (self.randstate.wrapping_mul(179).wrapping_add(17)) & 0xff;
                }
                let x = (self.randstate ^ 0x80) as i32;
                (x + 1) << 16
            }
        }
    }

    /// How far the delay has faded the LFO in, 0 to 2^24.
    pub fn delay(&mut self) -> i32 {
        let delta = if self.delaystate < (1 << 31) {
            self.delayinc
        } else {
            self.delayinc2
        };
        let d = self.delaystate.wrapping_add(delta);
        if d < self.delayinc {
            return 1 << 24;
        }
        self.delaystate = d;
        if d < (1 << 31) {
            0
        } else {
            ((d >> 7) & ((1 << 24) - 1)) as i32
        }
    }

    /// A key goes down: restart the delay, and the waveform when it is key-synced.
    pub fn key_down(&mut self) {
        if self.sync {
            self.phase = (1 << 31) - 1;
        }
        self.delaystate = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::sine_table;

    fn env(rates: [i32; 4], levels: [i32; 4], outlevel: i32) -> Env {
        let mut e = Env::default();
        e.init(rates, levels, outlevel, 0);
        e
    }

    /// The output-level scale: the table for 0..=19 and 28 + level above.
    #[test]
    fn output_levels_are_on_the_dx7_scale() {
        assert_eq!(scale_out_level(0), 0);
        assert_eq!(scale_out_level(19), 46);
        assert_eq!(scale_out_level(20), 48);
        assert_eq!(scale_out_level(99), 127);
    }

    /// At the slowest rate the level falls one doubling (1 << 24 in log units) in
    /// 2^20 samples, and each four steps of rate double the speed.
    #[test]
    fn decay_rates_follow_the_hardware() {
        // Hold at L1 = 99 (rate 99), then decay to 0 at rate 0: watch the fall.
        let fall = |rate: i32| {
            let mut e = env([99, rate, 99, 99], [99, 0, 0, 0], 127 << 5);
            let mut prev = 0;
            let mut steps = 0;
            // Run up to L1, then measure one decay step.
            for _ in 0..40 {
                prev = e.step();
                steps += 1;
                if e.ix == 1 {
                    break;
                }
            }
            assert!(steps < 40, "reaches the decay");
            let after = e.step();
            prev - after
        };
        let slowest = fall(0);
        // 1024 log units per 64-sample step: 16 per sample, 2^24 / 16 = 2^20 samples per doubling.
        assert_eq!(slowest, 1024);
        assert_eq!(fall(7), 2 * slowest, "qrate 4 is twice qrate 0");
        assert_eq!(fall(14), 4 * slowest, "and qrate 8 four times");
    }

    /// An attack jumps up to a crisp level at once and then rises to its target.
    #[test]
    fn attack_jumps_then_rises_to_the_level() {
        let mut e = env([60, 99, 99, 99], [99, 99, 99, 0], 127 << 5);
        let first = e.step();
        assert!(first >= JUMP_TARGET << 16, "{first}");
        for _ in 0..4_000 {
            e.step();
        }
        // Full level: 15 octaves above the floor in log units.
        assert_eq!(e.step(), 15 << 24);
    }

    /// Releasing the key runs the release to the last level.
    #[test]
    fn release_falls_to_the_last_level() {
        let mut e = env([99, 99, 99, 80], [99, 99, 99, 0], 127 << 5);
        for _ in 0..200 {
            e.step();
        }
        let held = e.step();
        e.key(false);
        for _ in 0..4_000 {
            e.step();
        }
        let after = e.step();
        assert!(after < held / 2, "{after} < {held}");
        assert_eq!(after, 16 << 16, "the floor");
    }

    #[test]
    fn the_pitch_envelope_spans_four_octaves() {
        let mut p = PitchEnv::new(48_000.0);
        p.set([99; 4], [99, 99, 99, 50]);
        for _ in 0..800 {
            p.step();
        }
        let top = p.step();
        assert!(
            (f64::from(top) / f64::from(1 << 24) - 3.97).abs() < 0.05,
            "{top}"
        );
        p.set([99; 4], [0, 0, 0, 50]);
        for _ in 0..800 {
            p.step();
        }
        assert!((f64::from(p.step()) / f64::from(1 << 24) + 4.0).abs() < 0.01);
        p.set([99; 4], [50, 50, 50, 50]);
        assert_eq!(p.step(), 0);
    }

    #[test]
    fn the_lfo_runs_at_its_speed_and_waits_out_its_delay() {
        let sine = sine_table();
        let mut l = Lfo::new(48_000.0);
        l.reset(60, 0, 0, true);
        l.key_down();
        let mut crossings = 0;
        let mut last = 0;
        for _ in 0..(48_000 * 4 / N) {
            let x = l.sample(&sine);
            assert!((0..=1 << 24).contains(&x));
            if last < (1 << 23) && x >= (1 << 23) {
                crossings += 1;
            }
            last = x;
        }
        assert!(
            (8..=60).contains(&crossings),
            "speed 60 over 4 s: {crossings} cycles"
        );
        // A long delay keeps the LFO out for a while and then fades it in.
        l.reset(60, 80, 0, true);
        l.key_down();
        assert_eq!(l.delay(), 0);
        let mut faded = 0;
        for _ in 0..(48_000 * 20 / N) {
            faded = l.delay();
        }
        assert_eq!(faded, 1 << 24, "fully in after the delay");
    }

    #[test]
    fn every_lfo_waveform_stays_in_range() {
        let sine = sine_table();
        for wave in 0..=5 {
            let mut l = Lfo::new(48_000.0);
            l.reset(70, 0, wave, true);
            for _ in 0..3_000 {
                let x = l.sample(&sine);
                assert!((0..=(1 << 24)).contains(&x), "wave {wave}: {x}");
            }
        }
    }
}
