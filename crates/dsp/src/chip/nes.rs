//! The Ricoh 2A03's sound (the NES APU, #112, ADR-0034), at register
//! precision: two pulses, a triangle, a noise and the delta-modulation
//! channel (DMC), with their envelopes, sweep and counters, and the
//! console's non-linear mixer. The instrument (`chip::nes_voice`) and the
//! Modular's NES unit generators share them.
//!
//! Written from the NESdev wiki's public documentation; no emulator's code.
//! Every channel is a timer counted in CPU cycles (16.16 fixed point, run
//! once per output sample for the cycles that sample holds), so the steps
//! land where the chip puts them and each channel reports them, with the
//! time they fall at, for band-limiting:
//!
//! - pulse: an 11-bit timer clocking an 8-step duty sequence every
//!   `2 (t + 1)` cycles (`f = CPU / 16 (t + 1)`), 4-bit volume, muted under
//!   t = 8 or when the sweep would pass 0x7FF;
//! - triangle: a 32-step 4-bit sequence every `t + 1` cycles, no volume;
//! - noise: a 15-bit LFSR, long (bit 1) or short (bit 6, 93 steps), at one
//!   of 16 periods;
//! - DMC: a 7-bit counter moved ±2 a bit at one of 16 rates, or set at once
//!   (`$4011`).

/// CPU clocks in Hz.
pub const NTSC: f64 = 1_789_773.0;
pub const PAL: f64 = 1_662_607.0;
/// CPU cycles per frame-counter sequence (4-step mode): the quarter frames
/// clock envelopes and the linear counter, the halves the sweeps. About
/// 60 Hz on NTSC, 50 on PAL.
pub const NTSC_SEQUENCE: u32 = 29_830;
pub const PAL_SEQUENCE: u32 = 33_252;

/// Fractional bits of the cycle counts.
const FRAC: u32 = 16;
const ONE: u64 = 1 << FRAC;

/// The duty sequences, in the order the sequencer plays them.
pub const DUTY: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1, 0, 0, 0],
    [1, 0, 0, 1, 1, 1, 1, 1],
];

/// The noise periods in CPU cycles.
pub const NOISE_NTSC: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];
pub const NOISE_PAL: [u16; 16] = [
    4, 8, 14, 30, 60, 88, 118, 148, 188, 236, 354, 472, 708, 944, 1890, 3778,
];
/// The DMC rates in CPU cycles per bit.
pub const DMC_NTSC: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];
pub const DMC_PAL: [u16; 16] = [
    398, 354, 316, 298, 276, 236, 210, 198, 176, 148, 132, 118, 98, 78, 66, 50,
];
/// The longest sample the DMC plays: 4081 bytes (`L · 16 + 1`, L = 255),
/// eight bits each.
pub const DMC_MAX_BITS: u32 = 4081 * 8;

/// NTSC or PAL: the CPU clock and the tables that follow it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Region {
    #[default]
    Ntsc,
    Pal,
}

impl Region {
    pub fn from_value(v: f32) -> Region {
        if v >= 0.5 { Region::Pal } else { Region::Ntsc }
    }

    pub fn clock(self) -> f64 {
        match self {
            Region::Ntsc => NTSC,
            Region::Pal => PAL,
        }
    }

    pub fn sequence(self) -> u32 {
        match self {
            Region::Ntsc => NTSC_SEQUENCE,
            Region::Pal => PAL_SEQUENCE,
        }
    }

    pub fn noise_period(self, i: u8) -> u16 {
        let t = match self {
            Region::Ntsc => &NOISE_NTSC,
            Region::Pal => &NOISE_PAL,
        };
        t.get(usize::from(i & 15)).copied().unwrap_or(4)
    }

    pub fn dmc_period(self, i: u8) -> u16 {
        let t = match self {
            Region::Ntsc => &DMC_NTSC,
            Region::Pal => &DMC_PAL,
        };
        t.get(usize::from(i & 15)).copied().unwrap_or(428)
    }
}

/// CPU cycles per output sample in 16.16.
pub fn cycles_per_sample(region: Region, sample_rate: f32) -> u64 {
    (region.clock() / f64::from(sample_rate.max(1.0)) * ONE as f64).round() as u64
}

/// The pulse timer for `hz`: `CPU / 16 f − 1`, 11 bits.
pub fn pulse_timer(hz: f32, region: Region) -> u16 {
    timer(hz, region, 16.0)
}

/// The triangle timer for `hz`: `CPU / 32 f − 1`, 11 bits (an octave below
/// the pulse at the same value).
pub fn triangle_timer(hz: f32, region: Region) -> u16 {
    timer(hz, region, 32.0)
}

fn timer(hz: f32, region: Region, steps: f64) -> u16 {
    if hz <= 0.0 || !hz.is_finite() {
        return 0x7FF;
    }
    let t = (region.clock() / (steps * f64::from(hz))).round() - 1.0;
    t.clamp(0.0, 2047.0) as u16
}

/// The noise period index nearest a pitch: the period whose clock is 16
/// times the note, as trackers map it.
pub fn noise_index(hz: f32, region: Region) -> u8 {
    let want = region.clock() / (16.0 * f64::from(hz.max(1.0)));
    (0..16_u8)
        .min_by(|a, b| {
            let d = |i: u8| (f64::from(region.noise_period(i)) / want).ln().abs();
            d(*a).total_cmp(&d(*b))
        })
        .unwrap_or(0)
}

/// The two mixer outputs, from the NESdev wiki's formulas: the pulses' sum
/// 0..30, and the triangle 0..15, noise 0..15 and DMC 0..127.
pub fn pulse_out(p1: u8, p2: u8) -> f32 {
    let n = f32::from(p1) + f32::from(p2);
    if n == 0.0 {
        0.0
    } else {
        95.88 / (8128.0 / n + 100.0)
    }
}

pub fn tnd_out(t: u8, n: u8, d: u8) -> f32 {
    let x = f32::from(t) / 8227.0 + f32::from(n) / 12241.0 + f32::from(d) / 22638.0;
    if x == 0.0 {
        0.0
    } else {
        159.79 / (1.0 / x + 100.0)
    }
}

/// The mixed output of all five levels.
pub fn mix(levels: [u8; 5]) -> f32 {
    let [p1, p2, t, n, d] = levels;
    pulse_out(p1, p2) + tnd_out(t, n, d)
}

/// Where in a sample's run a channel stepped, as the time from the step to
/// the end of the sample in samples, and its new level.
pub type Event = (f32, u8);

/// Up to 32 steps in one sample, from every channel, kept as they come.
#[derive(Clone, Copy, Debug)]
pub struct Events {
    pub at: [(f32, usize, u8); 32],
    pub len: usize,
}

impl Default for Events {
    fn default() -> Events {
        Events {
            at: [(0.0, 0, 0); 32],
            len: 0,
        }
    }
}

impl Events {
    pub fn clear(&mut self) {
        self.len = 0;
    }

    fn push(&mut self, after: f32, ch: usize, level: u8) {
        if let Some(e) = self.at.get_mut(self.len) {
            *e = (after, ch, level);
            self.len += 1;
        } else if let Some(e) = self.at.last_mut() {
            // Past the room: the last one's level wins at its time.
            e.2 = level;
        }
    }

    /// The events earliest first (the most time after them first).
    pub fn sort(&mut self) {
        let n = self.len.min(self.at.len());
        if let Some(s) = self.at.get_mut(..n) {
            s.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(f32, usize, u8)> {
        self.at.iter().take(self.len)
    }
}

/// A timer counting down CPU cycles in 16.16 to its next clock.
#[derive(Clone, Copy, Debug, Default)]
struct Timer {
    left: u64,
}

impl Timer {
    /// Run `run` cycles of a timer clocking every `period` cycles, calling
    /// `clock` at each with the time after it as a share of the run. At most
    /// `cap` clocks; beyond them the timer only keeps time.
    fn run(&mut self, run: u64, period: u64, cap: usize, mut clock: impl FnMut(f32)) {
        if period == 0 {
            return;
        }
        let mut rest = run;
        let mut n = 0;
        while self.left <= rest {
            rest -= self.left;
            self.left = period;
            if n < cap {
                clock(rest as f32 / run.max(1) as f32);
            }
            n += 1;
            if n > 4096 {
                break;
            }
        }
        self.left -= rest.min(self.left);
    }
}

/// The volume envelope of a pulse or the noise: a 4-bit decay clocked on
/// quarter frames, or the constant volume.
#[derive(Clone, Copy, Debug, Default)]
pub struct Envelope {
    /// The register's nibble: the volume, or the decay's divider period.
    pub volume: u8,
    pub constant: bool,
    pub looped: bool,
    start: bool,
    divider: u8,
    decay: u8,
}

impl Envelope {
    pub fn restart(&mut self) {
        self.start = true;
    }

    pub fn quarter(&mut self) {
        if self.start {
            self.start = false;
            self.decay = 15;
            self.divider = self.volume;
        } else if self.divider == 0 {
            self.divider = self.volume;
            if self.decay > 0 {
                self.decay -= 1;
            } else if self.looped {
                self.decay = 15;
            }
        } else {
            self.divider -= 1;
        }
    }

    pub fn level(&self) -> u8 {
        if self.constant {
            self.volume & 15
        } else {
            self.decay
        }
    }
}

/// A pulse channel.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pulse {
    /// The 11-bit timer and the duty (0..3).
    pub period: u16,
    pub duty: u8,
    pub env: Envelope,
    /// Sweep: shift 0..7 (0 off), negate, divider period 0..7; pulse 1
    /// negates in ones' complement, pulse 2 in two's.
    pub shift: u8,
    pub negate: bool,
    pub sweep_period: u8,
    pub ones: bool,
    pub on: bool,
    step: u8,
    timer: Timer,
    sweep_div: u8,
    sweep_reload: bool,
    level: u8,
}

impl Pulse {
    pub fn restart(&mut self) {
        self.env.restart();
        self.sweep_reload = true;
    }

    fn target(&self) -> u16 {
        let change = self.period >> self.shift;
        if self.negate {
            let back = if self.ones { change + 1 } else { change };
            self.period.saturating_sub(back)
        } else {
            self.period + change
        }
    }

    pub fn muted(&self) -> bool {
        self.period < 8 || (!self.negate && self.target() > 0x7FF)
    }

    pub fn half(&mut self) {
        if self.sweep_div == 0 && self.shift > 0 && !self.muted() {
            self.period = self.target().min(0x7FF);
        }
        if self.sweep_div == 0 || self.sweep_reload {
            self.sweep_div = self.sweep_period;
            self.sweep_reload = false;
        } else {
            self.sweep_div -= 1;
        }
    }

    fn out(&self) -> u8 {
        let bit = DUTY
            .get(usize::from(self.duty & 3))
            .and_then(|d| d.get(usize::from(self.step & 7)))
            .copied()
            .unwrap_or(0);
        if self.on && bit == 1 && !self.muted() {
            self.env.level()
        } else {
            0
        }
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    /// Run `run` cycles, pushing each change of level as channel `ch`.
    pub fn run(&mut self, run: u64, ch: usize, ev: &mut Events) {
        self.settle(ch, ev);
        let period = 2 * (u64::from(self.period) + 1) * ONE;
        let mut timer = self.timer;
        timer.run(run, period, 8, |after| {
            self.step = (self.step + 1) & 7;
            let out = self.out();
            if out != self.level {
                self.level = out;
                ev.push(after, ch, out);
            }
        });
        self.timer = timer;
    }

    /// A change of volume or duty between samples shows at the sample's start.
    fn settle(&mut self, ch: usize, ev: &mut Events) {
        let out = self.out();
        if out != self.level {
            self.level = out;
            ev.push(1.0, ch, out);
        }
    }
}

/// The triangle's 32 steps: 15 down to 0, 0 up to 15.
fn tri_step(i: u8) -> u8 {
    let i = i & 31;
    if i < 16 { 15 - i } else { i - 16 }
}

/// The triangle channel.
#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub period: u16,
    /// The sequencer runs while this is set (the linear and length counters).
    pub running: bool,
    step: u8,
    timer: Timer,
    level: u8,
}

impl Default for Triangle {
    /// At step 16, level 0: a note starts from silence, not a pop.
    fn default() -> Triangle {
        Triangle {
            period: 0x7FF,
            running: false,
            step: 16,
            timer: Timer::default(),
            level: 0,
        }
    }
}

impl Triangle {
    pub fn level(&self) -> u8 {
        self.level
    }

    /// Run `run` cycles. Stopped, it holds its step; under t = 2 (above
    /// about 37 kHz) it holds too, as an ultrasonic note would only add a
    /// constant.
    pub fn run(&mut self, run: u64, ch: usize, ev: &mut Events) {
        if !self.running || self.period < 2 {
            return;
        }
        let period = (u64::from(self.period) + 1) * ONE;
        let mut timer = self.timer;
        timer.run(run, period, 24, |after| {
            self.step = (self.step + 1) & 31;
            let out = tri_step(self.step);
            self.level = out;
            ev.push(after, ch, out);
        });
        self.timer = timer;
    }
}

/// The noise channel.
#[derive(Clone, Copy, Debug)]
pub struct Noise {
    /// Period index 0..15 and the short mode.
    pub period: u8,
    pub short: bool,
    pub env: Envelope,
    pub on: bool,
    lfsr: u16,
    timer: Timer,
    level: u8,
}

impl Default for Noise {
    fn default() -> Noise {
        Noise {
            period: 0,
            short: false,
            env: Envelope::default(),
            on: false,
            lfsr: 1,
            timer: Timer::default(),
            level: 0,
        }
    }
}

impl Noise {
    /// Shift the LFSR: feedback from bit 0 and bit 1, or bit 6 in short mode.
    pub fn shift(&mut self) {
        let tap = if self.short { 6 } else { 1 };
        let fb = (self.lfsr ^ (self.lfsr >> tap)) & 1;
        self.lfsr = (self.lfsr >> 1) | (fb << 14);
    }

    fn out(&self) -> u8 {
        if self.on && self.lfsr & 1 == 0 {
            self.env.level()
        } else {
            0
        }
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn run(&mut self, run: u64, region: Region, ch: usize, ev: &mut Events) {
        let out = self.out();
        if out != self.level {
            self.level = out;
            ev.push(1.0, ch, out);
        }
        let period = u64::from(region.noise_period(self.period)) * ONE;
        let mut timer = self.timer;
        timer.run(run, period, 8, |after| {
            self.shift();
            let out = self.out();
            if out != self.level {
                self.level = out;
                ev.push(after, ch, out);
            }
        });
        self.timer = timer;
    }
}

/// The DMC: a 7-bit counter moved ±2 for each bit at the rate, from a
/// stream of input it delta-encodes as it plays (a 1-bit sample made on the
/// fly), or set to it at each bit (`direct`, the `$4011` trick: 7-bit PCM).
#[derive(Clone, Copy, Debug, Default)]
pub struct Dmc {
    pub rate: u8,
    pub direct: bool,
    counter: u8,
    timer: Timer,
    /// Bits played of the current sample.
    pub bits: u32,
}

impl Dmc {
    /// Set the counter at once (`$4011`), 7 bits.
    pub fn load(&mut self, level: u8) {
        self.counter = level & 0x7F;
    }

    pub fn restart(&mut self) {
        self.bits = 0;
    }

    pub fn level(&self) -> u8 {
        self.counter
    }

    /// The 7-bit level a signal in −1..1 stands for.
    pub fn target(x: f32) -> u8 {
        ((x.clamp(-1.0, 1.0) * 0.5 + 0.5) * 127.0).round() as u8
    }

    /// One bit: up 2 if the input is above the counter, else down 2, held
    /// within 0..127 as the chip does; or the input itself, direct.
    pub fn bit(&mut self, x: f32) {
        let t = Dmc::target(x);
        if self.direct {
            self.counter = t;
        } else if t > self.counter {
            if self.counter <= 125 {
                self.counter += 2;
            }
        } else if self.counter >= 2 {
            self.counter -= 2;
        }
        self.bits = self.bits.saturating_add(1);
    }

    /// Run `run` cycles; `input(k)` gives the signal at the k-th bit of
    /// this run, `None` once the sample has ended.
    pub fn run(
        &mut self,
        run: u64,
        region: Region,
        ch: usize,
        ev: &mut Events,
        mut input: impl FnMut(u32) -> Option<f32>,
    ) {
        let period = u64::from(region.dmc_period(self.rate)) * ONE;
        let mut timer = self.timer;
        let mut k = 0;
        timer.run(run, period, 4, |after| {
            if let Some(x) = input(k) {
                let before = self.counter;
                self.bit(x);
                if self.counter != before {
                    ev.push(after, ch, self.counter);
                }
            }
            k += 1;
        });
        self.timer = timer;
    }
}

/// The frame counter: quarter and half frames from CPU cycles, in 16.16 so
/// NTSC's 7457.5-cycle quarter keeps time.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameCounter {
    at: u64,
    quarter: u8,
}

/// What a sample's run of the frame counter clocked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clocks {
    pub quarters: u8,
    pub halves: u8,
    /// Whole sequences: the player's frames.
    pub frames: u8,
}

impl FrameCounter {
    pub fn run(&mut self, run: u64, region: Region) -> Clocks {
        let quarter = u64::from(region.sequence()) * ONE / 4;
        let mut c = Clocks::default();
        self.at += run;
        while self.at >= quarter {
            self.at -= quarter;
            self.quarter = (self.quarter + 1) & 3;
            c.quarters = c.quarters.saturating_add(1);
            if self.quarter & 1 == 0 {
                c.halves = c.halves.saturating_add(1);
            }
            if self.quarter == 0 {
                c.frames = c.frames.saturating_add(1);
            }
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// A4 is t = 253 on an NTSC pulse (440.4 Hz) and 126 on the triangle.
    #[test]
    fn timers_are_11_bits() {
        assert_eq!(pulse_timer(440.0, Region::Ntsc), 253);
        assert_eq!(triangle_timer(440.0, Region::Ntsc), 126);
        assert_eq!(pulse_timer(20.0, Region::Ntsc), 2047);
        assert_eq!(pulse_timer(20_000.0, Region::Ntsc), 5);
        let hz = NTSC / (16.0 * 254.0);
        assert!((hz - 440.4).abs() < 0.1);
    }

    fn run_pulse(p: &mut Pulse, samples: usize) -> Vec<u8> {
        let per = cycles_per_sample(Region::Ntsc, SR);
        let mut ev = Events::default();
        (0..samples)
            .map(|_| {
                ev.clear();
                p.run(per, 0, &mut ev);
                p.level()
            })
            .collect()
    }

    /// The duty sequence's share of high steps: 1, 2, 4 and 6 of 8.
    #[test]
    fn the_duties_are_the_chips() {
        for (duty, want) in [(0, 0.125), (1, 0.25), (2, 0.5), (3, 0.75)] {
            let mut p = Pulse {
                period: 400,
                duty,
                on: true,
                ..Pulse::default()
            };
            p.env.constant = true;
            p.env.volume = 15;
            let x = run_pulse(&mut p, 48_000);
            let high = x.iter().filter(|l| **l == 15).count() as f32 / x.len() as f32;
            assert!((high - want).abs() < 0.01, "duty {duty}: {high}");
        }
    }

    /// The pulse is muted below t = 8 and when its sweep would pass 0x7FF.
    #[test]
    fn the_pulse_mutes() {
        let mut p = Pulse {
            period: 7,
            on: true,
            ..Pulse::default()
        };
        assert!(p.muted());
        p.period = 0x700;
        p.shift = 1;
        assert!(p.muted(), "target 0xA80");
        p.negate = true;
        assert!(!p.muted());
    }

    /// Pulse 1's sweep goes one lower than pulse 2's when it falls.
    #[test]
    fn the_sweeps_negate_differently() {
        let mk = |ones| Pulse {
            period: 0x100,
            shift: 2,
            negate: true,
            ones,
            ..Pulse::default()
        };
        assert_eq!(mk(true).target(), 0x100 - 0x40 - 1);
        assert_eq!(mk(false).target(), 0x100 - 0x40);
        let mut p = mk(false);
        p.restart();
        for _ in 0..4 {
            p.half();
        }
        assert!(p.period < 0x100);
    }

    /// The envelope decays 15 to 0 a step every V + 1 quarter frames, and
    /// loops when asked.
    #[test]
    fn the_envelope_decays_and_loops() {
        let mut e = Envelope {
            volume: 2,
            ..Envelope::default()
        };
        e.restart();
        e.quarter();
        assert_eq!(e.level(), 15);
        let mut n = 0;
        while e.level() > 0 && n < 1000 {
            e.quarter();
            n += 1;
        }
        assert_eq!(n, 15 * 3);
        e.looped = true;
        for _ in 0..3 {
            e.quarter();
        }
        assert_eq!(e.level(), 15);
    }

    /// The triangle steps 15..0, 0..15, at a 32nd of its timer's rate.
    #[test]
    fn the_triangle_has_32_steps() {
        let seq: Vec<u8> = (0..32).map(tri_step).collect();
        assert_eq!(seq.first(), Some(&15));
        assert_eq!(seq.get(15), Some(&0));
        assert_eq!(seq.get(16), Some(&0));
        assert_eq!(seq.last(), Some(&15));
        let mut t = Triangle {
            period: triangle_timer(220.0, Region::Ntsc),
            running: true,
            ..Triangle::default()
        };
        let per = cycles_per_sample(Region::Ntsc, SR);
        let mut ev = Events::default();
        let mut peaks = 0;
        let mut last = t.level();
        for _ in 0..48_000 {
            ev.clear();
            t.run(per, 2, &mut ev);
            if t.level() == 15 && last != 15 {
                peaks += 1;
            }
            last = t.level();
        }
        let hz = NTSC / (32.0 * (f64::from(t.period) + 1.0));
        assert!((f64::from(peaks) - hz).abs() <= 1.0, "{peaks} vs {hz}");
    }

    /// The long LFSR repeats after 32767 shifts, the short after 93.
    #[test]
    fn the_noise_runs_its_lengths() {
        let period = |short| {
            let mut n = Noise {
                short,
                ..Noise::default()
            };
            for _ in 0..200 {
                n.shift();
            }
            let start = n.lfsr;
            let mut k = 0;
            loop {
                n.shift();
                k += 1;
                if n.lfsr == start || k > 40_000 {
                    return k;
                }
            }
        };
        assert_eq!(period(false), 32_767);
        assert_eq!(period(true), 93);
    }

    /// The DMC moves 2 a bit, held in 0..127; direct sets the level.
    #[test]
    fn the_dmc_steps_by_two() {
        let mut d = Dmc::default();
        d.load(126);
        d.bit(1.0);
        assert_eq!(d.level(), 126, "no step past 127");
        d.load(64);
        d.bit(1.0);
        assert_eq!(d.level(), 66);
        d.bit(-1.0);
        assert_eq!(d.level(), 64);
        d.load(1);
        d.bit(-1.0);
        assert_eq!(d.level(), 1, "no step under 0");
        d.direct = true;
        d.bit(0.0);
        assert_eq!(d.level(), 64);
        assert_eq!(Dmc::target(1.0), 127);
    }

    /// A DMC at rate 15 plays 33 144 bits a second on NTSC.
    #[test]
    fn the_dmc_keeps_its_rate() {
        let mut d = Dmc {
            rate: 15,
            ..Dmc::default()
        };
        let per = cycles_per_sample(Region::Ntsc, SR);
        let mut ev = Events::default();
        for _ in 0..48_000 {
            ev.clear();
            d.run(per, Region::Ntsc, 4, &mut ev, |_| Some(0.0));
        }
        let want = NTSC / 54.0;
        assert!(
            (f64::from(d.bits) - want).abs() <= 2.0,
            "{} vs {want}",
            d.bits
        );
    }

    /// The mixer is the console's: pulses compress, the sum stays under 1.
    #[test]
    fn the_mixer_is_non_linear() {
        assert_eq!(mix([0; 5]), 0.0);
        let one = pulse_out(15, 0);
        let two = pulse_out(15, 15);
        assert!(two < 2.0 * one && two > 1.5 * one);
        assert!((mix([15, 15, 15, 15, 127]) - (0.2584 + 0.7419)).abs() < 0.01);
    }

    /// Quarter frames come four to a sequence, at about 240 Hz on NTSC.
    #[test]
    fn the_frame_counter_keeps_time() {
        let mut f = FrameCounter::default();
        let per = cycles_per_sample(Region::Ntsc, SR);
        let mut c = Clocks::default();
        for _ in 0..48_000 {
            let k = f.run(per, Region::Ntsc);
            c.quarters += k.quarters.min(1);
            c.frames = c.frames.saturating_add(0);
        }
        let want = 4.0 * NTSC / f64::from(NTSC_SEQUENCE);
        assert!(
            (f64::from(c.quarters) - want).abs() <= 1.0,
            "{}",
            c.quarters
        );
    }

    #[test]
    fn noise_follows_a_pitch() {
        assert_eq!(noise_index(440.0, Region::Ntsc), 9);
        assert!(noise_index(4000.0, Region::Ntsc) < noise_index(100.0, Region::Ntsc));
    }
}
