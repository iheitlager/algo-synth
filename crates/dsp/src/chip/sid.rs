//! The MOS 6581/8580 SID's parts, at register precision (#112, ADR-0034):
//! the oscillator, the envelope, the DACs and the filter. The instrument
//! (`chip::sid_voice`) and the Modular's SID unit generators share them.
//!
//! Written from the datasheet, Bob Yannes's interviews and published
//! measurements; no emulator's code. The chip runs at about 1 MHz; here
//! each part steps once per output sample by the chip cycles that sample
//! holds, counted in 16.16 fixed point, so pitch, envelope timing and the
//! noise clock land on the chip's own grid:
//!
//! - frequency: a 16-bit register into a 24-bit accumulator,
//!   `f = Fn · clock / 2^24` (0.0587 Hz steps on a PAL machine);
//! - waveforms: 12 bits; pulse width 12 bits; noise a 23-bit LFSR read
//!   as 8 bits;
//! - envelope: an 8-bit counter stepped by a 15-bit rate counter against
//!   the 16-entry period table, with the exponential decay divider and the
//!   rate counter's wrap (the "ADSR bug");
//! - DACs: R-2R ladders, ideal on the 8580, 2R/R = 2.2 and unterminated on
//!   the 6581 (so its codes are not evenly spaced, nor always rising);
//! - filter: an 11-bit cutoff and 4-bit resonance into a two-integrator
//!   state-variable filter, with each chip's cutoff curve.

/// The chip clocks in Hz: a PAL and an NTSC Commodore 64.
pub const PAL: f64 = 985_248.0;
pub const NTSC: f64 = 1_022_727.0;
/// Chip cycles per video frame, the rate a player routine runs at: 312
/// lines of 63 cycles (50.12 Hz) and 263 lines of 65 (60.0 Hz).
pub const PAL_FRAME: u32 = 19_656;
pub const NTSC_FRAME: u32 = 17_045;

/// Fractional bits of the cycle and accumulator counts.
const FRAC: u32 = 16;
/// The accumulator's 24 bits, with `FRAC` bits below them.
const ACC_BITS: u32 = 24;
const ACC_FX: u64 = 1 << (ACC_BITS + FRAC);
const ACC_MASK: u64 = ACC_FX - 1;
const MSB_FX: u64 = 1 << (ACC_BITS - 1 + FRAC);
/// Bit 19 of the accumulator clocks the noise LFSR.
const B19_FX: u64 = 1 << (19 + FRAC);

/// The 6581 or the 8580.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Chip {
    #[default]
    Mos6581,
    Mos8580,
}

impl Chip {
    pub fn from_value(v: f32) -> Chip {
        if v >= 0.5 {
            Chip::Mos8580
        } else {
            Chip::Mos6581
        }
    }
}

/// Chip cycles per output sample, in 16.16 fixed point, and the running
/// remainder: `take` hands out whole cycles so none are lost.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cycles {
    per_sample: u64,
    rest: u64,
}

impl Cycles {
    pub fn new(clock: f64, sample_rate: f32) -> Cycles {
        Cycles {
            per_sample: fixed(clock / f64::from(sample_rate.max(1.0))),
            rest: 0,
        }
    }

    /// Cycles per sample in 16.16.
    pub fn per_sample(&self) -> u64 {
        self.per_sample
    }

    /// The whole cycles of the next sample.
    pub fn take(&mut self) -> u32 {
        self.rest += self.per_sample;
        let whole = self.rest >> FRAC;
        self.rest &= (1 << FRAC) - 1;
        whole as u32
    }
}

fn fixed(v: f64) -> u64 {
    (v * f64::from(1u32 << FRAC)).round().max(0.0) as u64
}

/// The frequency register for `hz` on a chip clocked at `clock`: the 16-bit
/// value the chip plays nearest, clamped to the register (3848 Hz at most
/// on a PAL machine).
pub fn freq_register(hz: f32, clock: f64) -> u16 {
    let fn_ = (f64::from(hz) * f64::from(1u32 << ACC_BITS) / clock).round();
    fn_.clamp(0.0, f64::from(u16::MAX)) as u16
}

// ---- DACs -------------------------------------------------------------------

/// The weight of each bit of an R-2R ladder `N` bits wide, LSB first, for a
/// 2R/R ratio of `two_r`; `terminated` puts the 2R to ground below bit 0.
/// The output node is the MSB's, unloaded. Superposition: each bit alone
/// drives its node through 2R; the rest of the ladder is the load, and the
/// voltage divides up the ladder to the output.
// Evaluated at compile time into the statics below: an index out of range
// fails the build, never `render` (ADR-0002).
#[allow(clippy::indexing_slicing)]
const fn ladder<const N: usize>(two_r: f64, terminated: bool) -> [f64; N] {
    const OPEN: f64 = 1.0e18;
    const fn par(a: f64, b: f64) -> f64 {
        a * b / (a + b)
    }
    // What hangs below each node, its own 2R included.
    let mut down = [0.0; N];
    let mut k = 0;
    while k < N {
        let below = if k == 0 {
            if terminated { two_r } else { OPEN }
        } else {
            1.0 + down[k - 1]
        };
        down[k] = par(two_r, below);
        k += 1;
    }
    // What the ladder above each node loads it with.
    let mut up = [OPEN; N];
    let mut k = N - 1;
    while k > 0 {
        up[k - 1] = 1.0 + par(two_r, up[k]);
        k -= 1;
    }
    let mut w = [0.0; N];
    let mut i = 0;
    while i < N {
        let below = if i == 0 {
            if terminated { two_r } else { OPEN }
        } else {
            1.0 + down[i - 1]
        };
        let load = par(below, up[i]);
        let mut v = load / (two_r + load);
        let mut k = i + 1;
        while k < N {
            let z = par(two_r, up[k]);
            v *= z / (1.0 + z);
            k += 1;
        }
        w[i] = v;
        i += 1;
    }
    w
}

/// The DAC's output for every code, 0..1 at full scale.
// Evaluated at compile time into the statics below: an index out of range
// fails the build, never `render` (ADR-0002).
#[allow(clippy::indexing_slicing)]
const fn dac_table<const N: usize, const CODES: usize>(
    two_r: f64,
    terminated: bool,
) -> [f32; CODES] {
    let w = ladder::<N>(two_r, terminated);
    let mut full = 0.0;
    let mut i = 0;
    while i < N {
        full += w[i];
        i += 1;
    }
    let mut t = [0.0_f32; CODES];
    let mut code = 0;
    while code < CODES {
        let mut sum = 0.0;
        let mut b = 0;
        while b < N {
            if code & (1 << b) != 0 {
                sum += w[b];
            }
            b += 1;
        }
        t[code] = (sum / full) as f32;
        code += 1;
    }
    t
}

/// The 6581's ladders: 2R/R = 2.2, no termination below bit 0.
const R6581: f64 = 2.2;
static WAVE_6581: [f32; 4096] = dac_table::<12, 4096>(R6581, false);
static ENV_6581: [f32; 256] = dac_table::<8, 256>(R6581, false);
static WAVE_8580: [f32; 4096] = dac_table::<12, 4096>(2.0, true);
static ENV_8580: [f32; 256] = dac_table::<8, 256>(2.0, true);

/// A chip's waveform DAC: a 12-bit code to 0..1.
pub fn wave_dac(chip: Chip, code: u16) -> f32 {
    let t = match chip {
        Chip::Mos6581 => &WAVE_6581,
        Chip::Mos8580 => &WAVE_8580,
    };
    t.get(usize::from(code & 0xFFF)).copied().unwrap_or(0.0)
}

/// A chip's envelope DAC: an 8-bit code to 0..1.
pub fn env_dac(chip: Chip, code: u8) -> f32 {
    let t = match chip {
        Chip::Mos6581 => &ENV_6581,
        Chip::Mos8580 => &ENV_8580,
    };
    t.get(usize::from(code)).copied().unwrap_or(0.0)
}

/// The waveform code a voice swings about: the 6581's sits low (0x380), so
/// its voices carry a DC offset the envelope scales; the 8580's is centred.
pub fn wave_zero(chip: Chip) -> u16 {
    match chip {
        Chip::Mos6581 => 0x380,
        Chip::Mos8580 => 0x800,
    }
}

// ---- Oscillator ---------------------------------------------------------------

/// Waveform bits as in the control register's upper nibble.
pub const TRI: u8 = 1;
pub const SAW: u8 = 2;
pub const PULSE: u8 = 4;
pub const NOISE: u8 = 8;

/// The LFSR's state at reset: all ones.
const LFSR_RESET: u32 = 0x7F_FFFF;
/// Which LFSR bit drives each noise output bit, output bit 11 first.
const NOISE_TAPS: [(u32, u32); 8] = [
    (20, 11),
    (18, 10),
    (14, 9),
    (11, 8),
    (9, 7),
    (5, 6),
    (2, 5),
    (0, 4),
];

/// One oscillator: its accumulator in 24.16, the noise LFSR, and the
/// registers it reads.
#[derive(Clone, Copy, Debug)]
pub struct Osc {
    acc: u64,
    lfsr: u32,
    /// The frequency register, and the accumulator step per sample it gives.
    freq: u16,
    step: u64,
    /// Pulse width, 12 bits.
    pub pw: u16,
    /// Waveform bits (`TRI | SAW | PULSE | NOISE`).
    pub wave: u8,
    /// The triangle's MSB is XORed with the sync source's: ring modulation.
    pub ring: bool,
}

impl Default for Osc {
    fn default() -> Osc {
        Osc {
            acc: 0,
            lfsr: LFSR_RESET,
            freq: 0,
            step: 0,
            pw: 0x800,
            wave: TRI,
            ring: false,
        }
    }
}

impl Osc {
    /// Set the frequency register; `per_sample` is `Cycles::per_sample`.
    pub fn set_freq(&mut self, freq: u16, per_sample: u64) {
        self.freq = freq;
        self.step = u64::from(freq) * per_sample;
    }

    pub fn freq(&self) -> u16 {
        self.freq
    }

    /// The 24-bit accumulator.
    pub fn acc(&self) -> u32 {
        (self.acc >> FRAC) as u32
    }

    /// Restart the accumulator and the noise, as the test bit does.
    pub fn reset(&mut self) {
        self.acc = 0;
        self.lfsr = LFSR_RESET;
    }

    /// When this sample's MSB rises, as the time from the rise to the end of
    /// the sample in samples: what a sync or ring partner reads. Unsynced.
    pub fn msb_rise(&self) -> Option<f32> {
        crossing(self.acc, self.step, MSB_FX)
    }

    /// The 12-bit output at the current accumulator; `src` is the ring
    /// source's accumulator. Noise with another waveform writes the AND back
    /// into the LFSR: zeros pile up until the noise locks silent, as on the
    /// chip.
    pub fn output(&mut self, src: u32) -> u16 {
        let out = self.code_at(self.acc(), src);
        if self.wave & NOISE != 0 && self.wave != NOISE {
            for (bit, o) in NOISE_TAPS {
                if out & (1 << o) == 0 {
                    self.lfsr &= !(1 << bit);
                }
            }
        }
        out
    }

    /// The 12-bit output the waveform bits give at accumulator `acc`.
    pub fn code_at(&self, acc: u32, src: u32) -> u16 {
        if self.wave & (TRI | SAW | PULSE | NOISE) == 0 {
            return 0;
        }
        let mut out = 0xFFF_u16;
        if self.wave & TRI != 0 {
            let x = if self.ring { acc ^ src } else { acc };
            let t = if x & 0x80_0000 != 0 { !acc } else { acc };
            out &= ((t >> 11) & 0xFFE) as u16;
        }
        if self.wave & SAW != 0 {
            out &= ((acc >> 12) & 0xFFF) as u16;
        }
        if self.wave & PULSE != 0 {
            out &= if ((acc >> 12) & 0xFFF) as u16 >= self.pw {
                0xFFF
            } else {
                0
            };
        }
        if self.wave & NOISE != 0 {
            out &= self.noise();
        }
        out
    }

    /// The output `frac` of the way through this sample's step, unsynced.
    pub fn code_at_run(&self, frac: f32, src: u32) -> u16 {
        let run = (self.step as f64 * f64::from(frac.clamp(0.0, 1.0))) as u64;
        self.code_at((((self.acc + run) & ACC_MASK) >> FRAC) as u32, src)
    }

    fn noise(&self) -> u16 {
        NOISE_TAPS.iter().fold(0, |n, (bit, o)| {
            n | ((((self.lfsr >> bit) & 1) as u16) << o)
        })
    }

    /// Advance one sample, resetting to 0 `sync` samples before its end
    /// when the sync source's MSB rose.
    pub fn advance(&mut self, sync: Option<f32>) {
        let start = self.acc;
        let (from, run) = match sync {
            Some(after) => {
                // Run to the reset, then from 0 for the rest of the sample.
                let before = (self.step as f64 * f64::from(1.0 - after.clamp(0.0, 1.0))) as u64;
                self.clock_noise(start, before);
                (
                    0,
                    (self.step as f64 * f64::from(after.clamp(0.0, 1.0))) as u64,
                )
            }
            None => (start, self.step),
        };
        self.clock_noise(from, run);
        self.acc = (from + run) & ACC_MASK;
    }

    /// Shift the LFSR once for each rise of accumulator bit 19 in a run of
    /// `run` from `from`.
    fn clock_noise(&mut self, from: u64, run: u64) {
        let rises = edges(from, run, B19_FX, 1 << (20 + FRAC));
        for _ in 0..rises.min(8) {
            let bit = ((self.lfsr >> 22) ^ (self.lfsr >> 17)) & 1;
            self.lfsr = ((self.lfsr << 1) | bit) & LFSR_RESET;
        }
    }

    /// The steps the next `advance` makes in a pure saw or pulse, as
    /// (old code, new code, time from the step to the end of the sample).
    /// At most three (a wrap, a pulse edge, and a sync reset elsewhere).
    pub fn steps(&self, sync: Option<f32>) -> Steps {
        let mut s = Steps::default();
        let step = self.step;
        if step == 0 {
            return s;
        }
        let run_to = |from: u64, run: u64, tail: f32, s: &mut Steps| {
            let pw_fx = u64::from(self.pw) << (12 + FRAC);
            match self.wave {
                SAW => {
                    if let Some(t) = crossing(from, run, 0) {
                        s.push(0xFFF, 0, tail + t_scaled(t, run, step));
                    }
                }
                PULSE if self.pw > 0 => {
                    if let Some(t) = crossing(from, run, pw_fx) {
                        s.push(0, 0xFFF, tail + t_scaled(t, run, step));
                    }
                    if let Some(t) = crossing(from, run, 0) {
                        s.push(0xFFF, 0, tail + t_scaled(t, run, step));
                    }
                }
                _ => {}
            }
        };
        match sync {
            Some(after) => {
                let after = after.clamp(0.0, 1.0);
                let before = (step as f64 * f64::from(1.0 - after)) as u64;
                run_to(self.acc, before, after, &mut s);
                s.sync = Some(after);
                run_to(0, (step as f64 * f64::from(after)) as u64, 0.0, &mut s);
            }
            None => run_to(self.acc, step, 0.0, &mut s),
        }
        s
    }
}

/// A crossing's time as a fraction of a run, rescaled to samples of a full
/// step.
fn t_scaled(t: f32, run: u64, step: u64) -> f32 {
    t * run as f32 / step as f32
}

/// When the accumulator, running `run` from `from` (wrapping at 2^24),
/// crosses `at`: the time from the crossing to the end of the run, as a
/// fraction of the run.
fn crossing(from: u64, run: u64, at: u64) -> Option<f32> {
    if run == 0 {
        return None;
    }
    let to = from + run;
    let target = if at > from { at } else { at + ACC_FX };
    (to >= target && target > from).then(|| (to - target) as f32 / run as f32)
}

/// How many times a run of `run` from `from` passes `offset` modulo `period`.
fn edges(from: u64, run: u64, offset: u64, period: u64) -> u64 {
    let count = |x: u64| (x + period - offset) / period;
    count(from + run) - count(from)
}

/// Up to four steps of a waveform within one sample.
#[derive(Clone, Copy, Debug, Default)]
pub struct Steps {
    pub at: [(u16, u16, f32); 4],
    pub len: usize,
    /// The sync reset, as the time after it.
    pub sync: Option<f32>,
}

impl Steps {
    fn push(&mut self, from: u16, to: u16, after: f32) {
        if let Some(s) = self.at.get_mut(self.len) {
            *s = (from, to, after.clamp(0.0, 1.0));
            self.len += 1;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(u16, u16, f32)> {
        self.at.iter().take(self.len)
    }
}

/// One sample of an oscillator through its DAC, 0..1, and the oscillator
/// advanced: band-limited through `ring` (its saw and pulse edges and any
/// sync reset), or the raw steps. `src` is the ring source's accumulator,
/// `sync` the time after the sync source's MSB rose.
pub fn render_osc(
    o: &mut Osc,
    ring: &mut crate::chip::Ring,
    blep: &crate::mono::osc::Blep,
    chip: Chip,
    src: u32,
    sync: Option<f32>,
    band_limit: bool,
) -> f32 {
    let y = wave_dac(chip, o.output(src));
    if !band_limit {
        o.advance(sync);
        return y;
    }
    ring.add_now(y);
    let steps = o.steps(sync);
    for &(from, to, after) in steps.iter() {
        ring.correct(blep, wave_dac(chip, to) - wave_dac(chip, from), after);
    }
    if let Some(after) = steps.sync {
        // The jump a sync reset makes, whatever the waveform.
        let before = o.code_at_run(1.0 - after, src);
        let jump = wave_dac(chip, o.code_at(0, src)) - wave_dac(chip, before);
        ring.correct(blep, jump, after);
    }
    o.advance(sync);
    ring.take()
}

// ---- Envelope -----------------------------------------------------------------

/// Chip cycles per envelope step at each 4-bit rate: attack 2 ms to 8 s,
/// decay and release three times as long (the datasheet's table).
pub const RATE_PERIODS: [u16; 16] = [
    9, 32, 63, 95, 149, 220, 267, 313, 392, 977, 1954, 3126, 3907, 11720, 19532, 31251,
];

/// The decay divider: rate steps per counter step at and below each level,
/// the chip's piecewise-linear approximation of an exponential.
fn exp_period(level: u8) -> u8 {
    match level {
        0x5E..=0xFF => 1,
        0x37..=0x5D => 2,
        0x1B..=0x36 => 4,
        0x0F..=0x1A => 8,
        0x07..=0x0E => 16,
        0x01..=0x06 => 30,
        0 => 1,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    Attack,
    Decay,
    #[default]
    Release,
}

/// The ADSR: four 4-bit rates and levels into an 8-bit counter.
#[derive(Clone, Copy, Debug, Default)]
pub struct Env {
    pub attack: u8,
    pub decay: u8,
    pub sustain: u8,
    pub release: u8,
    stage: Stage,
    counter: u8,
    /// The 15-bit rate counter and the decay divider's count.
    rate: u16,
    exp: u8,
}

impl Env {
    pub fn gate(&mut self, on: bool) {
        match (on, self.stage) {
            (true, Stage::Release) => self.stage = Stage::Attack,
            (false, Stage::Attack | Stage::Decay) => self.stage = Stage::Release,
            _ => {}
        }
    }

    pub fn gated(&self) -> bool {
        self.stage != Stage::Release
    }

    /// Clear the counter and the rate counter: a tracker's hard restart,
    /// which also keeps the next attack clear of the rate counter's wrap.
    pub fn hard_restart(&mut self) {
        self.counter = 0;
        self.rate = 0;
        self.exp = 0;
    }

    /// The 8-bit level.
    pub fn level(&self) -> u8 {
        self.counter
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// Idle: released and at zero.
    pub fn idle(&self) -> bool {
        self.stage == Stage::Release && self.counter == 0
    }

    fn period(&self) -> u16 {
        let r = match self.stage {
            Stage::Attack => self.attack,
            Stage::Decay => self.decay,
            Stage::Release => self.release,
        };
        RATE_PERIODS.get(usize::from(r & 15)).copied().unwrap_or(9)
    }

    /// Run `cycles` chip cycles. The rate counter counts up to the period
    /// and steps the envelope; one past the period (a rate lowered under
    /// the count) runs on to the 15-bit wrap first, the chip's "ADSR bug".
    pub fn clock(&mut self, cycles: u32) {
        let mut n = cycles;
        loop {
            let period = self.period();
            let need = if self.rate < period {
                u32::from(period - self.rate)
            } else {
                0x8000 - u32::from(self.rate) + u32::from(period)
            };
            if n < need {
                self.rate = ((u32::from(self.rate) + n) & 0x7FFF) as u16;
                return;
            }
            n -= need;
            self.rate = 0;
            self.tick();
        }
    }

    fn tick(&mut self) {
        match self.stage {
            Stage::Attack => {
                self.exp = 0;
                self.counter = self.counter.saturating_add(1);
                if self.counter == 0xFF {
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay | Stage::Release => {
                self.exp += 1;
                if self.exp < exp_period(self.counter) {
                    return;
                }
                self.exp = 0;
                let floor = if self.stage == Stage::Decay {
                    (self.sustain & 15) * 0x11
                } else {
                    0
                };
                // The counter stops at the sustain level and at zero.
                if self.counter != floor && self.counter != 0 {
                    self.counter -= 1;
                }
            }
        }
    }
}

// ---- Filter -------------------------------------------------------------------

/// Filter mode bits as in `$D418`.
pub const LP: u8 = 1;
pub const BP: u8 = 2;
pub const HP: u8 = 4;

/// The cutoff in Hz for an 11-bit register value, `bias` octaves off the
/// chip's typical curve (6581s differ from chip to chip). The 8580's curve
/// is close to linear from 30 Hz to 12 kHz; the 6581's an S from about
/// 220 Hz to 18 kHz, steepest past the middle.
pub fn cutoff_hz(chip: Chip, fc: u16, bias: f32) -> f32 {
    let x = f32::from(fc.min(2047)) / 2047.0;
    let hz = match chip {
        Chip::Mos8580 => 30.0 + x * (12_000.0 - 30.0),
        Chip::Mos6581 => {
            let (a, x0) = (5.0_f32, 0.6_f32);
            let lo = (-a * x0).tanh();
            let hi = (a * (1.0 - x0)).tanh();
            let s = ((a * (x - x0)).tanh() - lo) / (hi - lo);
            220.0 + s * (18_000.0 - 220.0)
        }
    };
    hz * bias.exp2()
}

/// 1/Q for the 4-bit resonance: the 8580's runs 0.707 to about 2.6, the
/// 6581's only from 6 up, to about 1.9.
pub fn damping(chip: Chip, res: u8) -> f32 {
    let r = f32::from(res & 15);
    match chip {
        Chip::Mos8580 => ((4.0 - r) / 8.0).exp2(),
        Chip::Mos6581 if r > 5.0 => 8.0 / r,
        Chip::Mos6581 => std::f32::consts::SQRT_2,
    }
}

/// The two-integrator-loop filter, as a zero-delay-feedback state-variable
/// filter (Zavalishin): low-, band- and high-pass of one input, mixed by
/// the mode bits. The 6581's input is driven into a soft clip.
#[derive(Clone, Copy, Debug, Default)]
pub struct Filter {
    s1: f32,
    s2: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
    /// The register values the coefficients are for.
    at: Option<(Chip, u16, u8, u32)>,
}

impl Filter {
    /// Set the cutoff and resonance; only recomputes when they change.
    pub fn set(&mut self, chip: Chip, fc: u16, res: u8, bias: f32, sample_rate: f32) {
        let key = (chip, fc, res, bias.to_bits());
        if self.at == Some(key) {
            return;
        }
        self.at = Some(key);
        let hz = cutoff_hz(chip, fc, bias).min(0.45 * sample_rate);
        let g = (std::f32::consts::PI * hz / sample_rate).tan();
        let k = damping(chip, res);
        self.k = k;
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    /// Filter one sample and mix the outputs `mode` selects.
    pub fn process(&mut self, chip: Chip, x: f32, mode: u8) -> f32 {
        let v0 = match chip {
            Chip::Mos6581 => soft(x * 0.8) / 0.8,
            Chip::Mos8580 => x,
        };
        let v3 = v0 - self.s2;
        let v1 = self.a1 * self.s1 + self.a2 * v3;
        let v2 = self.s2 + self.a2 * self.s1 + self.a3 * v3;
        self.s1 = 2.0 * v1 - self.s1;
        self.s2 = 2.0 * v2 - self.s2;
        if !(self.s1.is_finite() && self.s2.is_finite()) {
            self.reset();
        }
        let mut y = 0.0;
        if mode & LP != 0 {
            y += v2;
        }
        if mode & BP != 0 {
            y += v1;
        }
        if mode & HP != 0 {
            y += v0 - self.k * v1 - v2;
        }
        y
    }
}

/// A rational tanh, bounded to ±1.
fn soft(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn the_8580_dac_is_linear() {
        for code in [0_u16, 1, 0x380, 0x800, 0xFFF] {
            let want = f32::from(code) / 4095.0;
            assert!(
                (wave_dac(Chip::Mos8580, code) - want).abs() < 1e-6,
                "{code}"
            );
        }
        assert!((env_dac(Chip::Mos8580, 0x80) - 128.0 / 255.0).abs() < 1e-6);
    }

    /// The 6581's ladder is off: its bits do not double, and a code is not
    /// always above the one below it.
    #[test]
    fn the_6581_dac_is_not() {
        let w = ladder::<8>(R6581, false);
        // Without the termination bit 0 weighs more than half of bit 1, and
        // with 2R/R above 2 each bit less than twice the one below.
        assert!(w[0] > 0.6 * w[1]);
        assert!(w.windows(2).all(|p| p[1] < 2.0 * p[0]));
        assert!((env_dac(Chip::Mos6581, 0xFF) - 1.0).abs() < 1e-6);
        let steps: Vec<f32> = (1..4096)
            .map(|c| wave_dac(Chip::Mos6581, c) - wave_dac(Chip::Mos6581, c - 1))
            .collect();
        assert!(steps.iter().any(|s| *s < 0.0), "non-monotonic");
        assert!(steps.iter().any(|s| (*s - 1.0 / 4095.0).abs() > 1e-4));
    }

    /// Fn = f · 2^24 / clock, so A4 is 7493 on a PAL machine, and the
    /// register ends at 3848 Hz.
    #[test]
    fn the_frequency_register_is_16_bits() {
        assert_eq!(freq_register(440.0, PAL), 7493);
        assert_eq!(freq_register(440.0, NTSC), 7218);
        assert_eq!(freq_register(5000.0, PAL), u16::MAX);
        let top = f64::from(u16::MAX) * PAL / f64::from(1u32 << 24);
        assert!((top - 3848.6).abs() < 0.1);
    }

    /// The accumulator advances by Fn × cycles: after one second of PAL
    /// cycles at Fn it has gone round Fn · clock / 2^24 times.
    #[test]
    fn the_accumulator_keeps_the_chip_pitch() {
        let mut c = Cycles::new(PAL, SR);
        let mut o = Osc::default();
        o.set_freq(7493, c.per_sample());
        o.wave = SAW;
        let mut wraps = 0;
        let mut last = 0;
        for _ in 0..SR as usize {
            c.take();
            o.advance(None);
            if o.acc() < last {
                wraps += 1;
            }
            last = o.acc();
        }
        let want = 7493.0 * PAL / f64::from(1u32 << 24);
        assert!((f64::from(wraps) - want).abs() <= 1.0, "{wraps} vs {want}");
    }

    #[test]
    fn waveforms_are_12_bits() {
        let mut o = Osc {
            acc: 0x40_0000 << FRAC,
            ..Osc::default()
        };
        o.wave = SAW;
        assert_eq!(o.output(0), 0x400);
        o.wave = TRI;
        assert_eq!(o.output(0), 0x800);
        o.wave = PULSE;
        o.pw = 0x400;
        assert_eq!(o.output(0), 0xFFF);
        o.pw = 0x401;
        assert_eq!(o.output(0), 0);
        o.wave = SAW | TRI;
        assert_eq!(o.output(0), 0x400 & 0x800);
    }

    /// Ring modulation flips the triangle where the source's MSB is set.
    #[test]
    fn ring_modulation_flips_the_triangle() {
        let mut o = Osc {
            acc: 0x20_0000 << FRAC,
            ..Osc::default()
        };
        let plain = o.output(0);
        o.ring = true;
        assert_eq!(o.output(0), plain);
        assert_eq!(o.output(0x80_0000), 0xFFE - plain);
    }

    /// Noise is 8 bits: the low four output bits are always clear, and it
    /// changes only as bit 19 rises.
    #[test]
    fn noise_is_an_8_bit_lfsr() {
        let mut c = Cycles::new(PAL, SR);
        let mut o = Osc {
            wave: NOISE,
            ..Osc::default()
        };
        o.set_freq(2000, c.per_sample());
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..20_000 {
            c.take();
            let v = o.output(0);
            assert_eq!(v & 0xF, 0);
            seen.insert(v);
            o.advance(None);
        }
        assert!(seen.len() > 200, "{}", seen.len());
        // 2000 · 985248 / 2^24 / 2^20 · 2^24 ≈ 1879 shifts a second.
        let mut o = Osc {
            wave: NOISE,
            ..Osc::default()
        };
        o.set_freq(2000, c.per_sample());
        let mut changes = 0;
        let mut last = o.output(0);
        for _ in 0..SR as usize {
            o.advance(None);
            let v = o.output(0);
            changes += usize::from(v != last);
            last = v;
        }
        let shifts = 2000.0 * PAL / f64::from(1u32 << 20);
        assert!((changes as f64) <= shifts + 1.0 && changes as f64 > 0.9 * shifts);
    }

    /// Noise combined with a waveform locks up: zeros fill the LFSR.
    #[test]
    fn combined_noise_locks_up() {
        let c = Cycles::new(PAL, SR);
        let mut o = Osc {
            wave: NOISE | PULSE,
            pw: 0xFFF,
            ..Osc::default()
        };
        o.set_freq(30_000, c.per_sample());
        for _ in 0..48_000 {
            o.output(0);
            o.advance(None);
        }
        o.wave = NOISE;
        assert_eq!(o.output(0), 0);
        o.reset();
        o.advance(None);
        assert_ne!(o.output(0), 0);
    }

    /// Sync resets the accumulator part-way through the sample.
    #[test]
    fn sync_resets_the_accumulator() {
        let c = Cycles::new(PAL, SR);
        let mut o = Osc {
            acc: 0x70_0000 << FRAC,
            ..Osc::default()
        };
        o.set_freq(1000, c.per_sample());
        o.advance(Some(0.25));
        let want = (o.step as f64 * 0.25) as u64 >> FRAC;
        assert_eq!(u64::from(o.acc()), want);
    }

    fn run_env(env: &mut Env, seconds: f32, c: &mut Cycles) -> Vec<u8> {
        (0..(seconds * SR) as usize)
            .map(|_| {
                env.clock(c.take());
                env.level()
            })
            .collect()
    }

    /// Attack 0 rises 255 steps of 9 cycles: 2.3 ms; attack 8 takes 255 ×
    /// 392 cycles, 101 ms.
    #[test]
    fn the_attack_follows_the_rate_table() {
        for (rate, ms) in [(0_u8, 2.33_f32), (8, 101.5), (12, 1011.0)] {
            let mut c = Cycles::new(PAL, SR);
            let mut e = Env {
                attack: rate,
                sustain: 15,
                ..Env::default()
            };
            e.gate(true);
            let levels = run_env(&mut e, 1.2, &mut c);
            let top = levels.iter().position(|l| *l == 0xFF).unwrap_or(usize::MAX);
            let got = top as f32 / SR * 1000.0;
            assert!((got - ms).abs() < ms * 0.02 + 0.05, "rate {rate}: {got} ms");
        }
    }

    /// Decay steps down to the sustain level, S × 0x11, and stays.
    #[test]
    fn decay_stops_at_the_sustain_level() {
        let mut c = Cycles::new(PAL, SR);
        let mut e = Env {
            decay: 2,
            sustain: 8,
            ..Env::default()
        };
        e.gate(true);
        let levels = run_env(&mut e, 0.5, &mut c);
        assert_eq!(levels.last().copied(), Some(0x88));
        assert_eq!(e.stage(), Stage::Decay);
    }

    /// Release is exponential: the steps slow down as the level falls, and
    /// it ends at zero.
    #[test]
    fn release_is_exponential_and_ends_at_zero() {
        let mut c = Cycles::new(PAL, SR);
        let mut e = Env {
            sustain: 15,
            release: 4,
            ..Env::default()
        };
        e.gate(true);
        run_env(&mut e, 0.05, &mut c);
        e.gate(false);
        let levels = run_env(&mut e, 2.0, &mut c);
        let at = |l: u8| levels.iter().position(|x| *x <= l).unwrap_or(usize::MAX);
        // From 0xFF to 0x5D at divider 1, then 0x5D to 0x36 at divider 2.
        let fast = (at(0x5D) as f32) / f32::from(0xFF_u8 - 0x5D);
        let slow = (at(0x36) - at(0x5D)) as f32 / f32::from(0x5D_u8 - 0x36);
        assert!((slow / fast - 2.0).abs() < 0.1, "{fast} {slow}");
        assert_eq!(levels.last().copied(), Some(0));
        assert!(e.idle());
    }

    /// A rate lowered under the rate counter's count waits for its 15-bit
    /// wrap: 32768 cycles, 33 ms on a PAL machine, before the envelope moves.
    #[test]
    fn the_adsr_bug_stalls_the_envelope() {
        let mut c = Cycles::new(PAL, SR);
        let mut e = Env {
            release: 15,
            ..Env::default()
        };
        e.gate(false);
        // Count up under the slow release period, then attack fast.
        e.counter = 0;
        e.clock(10_000);
        e.attack = 0;
        e.gate(true);
        let levels = run_env(&mut e, 0.05, &mut c);
        let first = levels.iter().position(|l| *l > 0).unwrap_or(usize::MAX);
        let ms = first as f32 / SR * 1000.0;
        let want = (32_768.0 - 10_000.0) / PAL as f32 * 1000.0;
        assert!((ms - want).abs() < 0.2, "{ms} vs {want}");
        // A hard restart clears it.
        let mut e = Env {
            release: 15,
            ..Env::default()
        };
        e.clock(10_000);
        e.hard_restart();
        e.gate(true);
        let levels = run_env(&mut e, 0.005, &mut c);
        assert!(levels.iter().take(5).any(|l| *l > 0));
    }

    #[test]
    fn the_cutoff_curves_span_each_chip() {
        assert!((cutoff_hz(Chip::Mos8580, 0, 0.0) - 30.0).abs() < 1.0);
        assert!((cutoff_hz(Chip::Mos8580, 2047, 0.0) - 12_000.0).abs() < 1.0);
        assert!((cutoff_hz(Chip::Mos6581, 0, 0.0) - 220.0).abs() < 1.0);
        assert!((cutoff_hz(Chip::Mos6581, 2047, 0.0) - 18_000.0).abs() < 1.0);
        let mut last = 0.0;
        for fc in (0..2048).step_by(64) {
            let hz = cutoff_hz(Chip::Mos6581, fc, 0.0);
            assert!(hz > last);
            last = hz;
        }
        assert!((damping(Chip::Mos8580, 0) - std::f32::consts::SQRT_2).abs() < 1e-6);
        assert!(damping(Chip::Mos8580, 15) < damping(Chip::Mos6581, 15));
    }

    /// A low-pass at 1 kHz passes 100 Hz and stops 10 kHz.
    #[test]
    fn the_filter_passes_and_stops() {
        let gain = |hz: f32, mode: u8| {
            let mut f = Filter::default();
            let fc = ((1000.0 - 30.0) / (12_000.0 - 30.0) * 2047.0) as u16;
            f.set(Chip::Mos8580, fc, 0, 0.0, SR);
            let mut peak = 0.0_f32;
            for i in 0..(SR as usize / 2) {
                let x = 0.1 * (std::f32::consts::TAU * hz * i as f32 / SR).sin();
                let y = f.process(Chip::Mos8580, x, mode);
                if i > SR as usize / 4 {
                    peak = peak.max(y.abs());
                }
            }
            peak / 0.1
        };
        assert!(gain(100.0, LP) > 0.9);
        assert!(gain(10_000.0, LP) < 0.03);
        assert!(gain(10_000.0, HP) > 0.9);
        assert!(gain(100.0, HP) < 0.03);
        assert!(gain(1000.0, BP) > 0.5);
    }
}
