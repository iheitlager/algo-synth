//! A song bounced offline to a stereo WAV (#705): rendered from the top,
//! faster than realtime, the same bytes on every run.
//!
//! `Bounce::start` stops the song, works out where its last bar ends and sizes
//! the whole file up front; `step` renders blocks and writes them straight
//! into it. At the end of the bars the song pauses on that exact sample, so
//! its notes let go as they would at a stop, and a tail renders until the
//! output stays quiet or `MAX_TAIL` passes: reverbs and delays ring out.
//!
//! A song's length is its arrangement. One with a `loop` line plays to the
//! loop's end once and marks the loop in a `smpl` chunk, as samplers and
//! game engines read it, rather than repeating it; one without an
//! arrangement has no end of its own and plays `FREE_SECONDS`, to a bar.
//!
//! The master limiter keeps every sample inside ±1, so nothing clips; 16- and
//! 24-bit files are dithered (TPDF) when the float output is rounded.

use crate::algo::Rng;
use crate::engine::{BLOCK, Engine};
use crate::song::STEPS_PER_BAR;

/// Longest tail after the last bar, in seconds.
pub const MAX_TAIL: f64 = 10.0;
/// The tail ends after this long below `QUIET`, in seconds.
const QUIET_FOR: f64 = 0.5;
/// −90 dBFS.
const QUIET: f32 = 3.162e-5;
/// How long a song without an arrangement plays, in seconds.
pub const FREE_SECONDS: f64 = 300.0;
/// A `smpl` chunk with one loop: header, 36 bytes, a 24-byte loop.
const SMPL_LEN: usize = 8 + 36 + 24;
/// Largest file a bounce writes: 1 GiB.
pub const MAX_BYTES: u64 = 1 << 30;
/// Most blocks one `step` renders, so a caller can show progress and stop.
pub const MAX_STEP: usize = 1024;
/// The dither's seed: the same file every run.
const SEED: u32 = 0x0705_2026;

/// How the samples are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// 16-bit PCM, dithered.
    Pcm16,
    /// 24-bit PCM, dithered.
    Pcm24,
    /// 32-bit IEEE float, as rendered.
    Float32,
}

impl Format {
    /// The format of `bits` per sample: 16, 24 or 32 (float).
    pub fn from_bits(bits: u32) -> Option<Format> {
        match bits {
            16 => Some(Format::Pcm16),
            24 => Some(Format::Pcm24),
            32 => Some(Format::Float32),
            _ => None,
        }
    }

    fn bytes(self) -> usize {
        match self {
            Format::Pcm16 => 2,
            Format::Pcm24 => 3,
            Format::Float32 => 4,
        }
    }

    /// Bytes before the samples: RIFF, `fmt ` and `data`'s header, and for
    /// the extensible formats a `fact` chunk.
    fn header_len(self) -> usize {
        match self {
            Format::Pcm16 => 44,
            _ => 80,
        }
    }
}

/// Why a bounce didn't start. `code` is what crosses the C ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The file would pass `MAX_BYTES`.
    TooLong,
    /// The memory for the file couldn't be had.
    NoMemory,
    /// Bits other than 16, 24 or 32.
    Unsupported,
}

impl Error {
    pub fn code(self) -> i32 {
        match self {
            Error::TooLong => -2,
            Error::NoMemory => -3,
            Error::Unsupported => -4,
        }
    }
}

/// Interleaved stereo frames written as a WAV, header kept current.
#[derive(Debug)]
pub struct Writer {
    format: Format,
    wav: Vec<u8>,
    /// Bytes of samples written.
    data: usize,
    rng: Rng,
    peak: f32,
}

impl Writer {
    /// An empty file at `rate` with room for `frames`.
    pub fn new(format: Format, rate: u32, frames: u64) -> Result<Writer, Error> {
        let block = 2 * format.bytes();
        let len = frames
            .checked_mul(block as u64)
            .and_then(|n| n.checked_add((format.header_len() + SMPL_LEN) as u64))
            .filter(|n| *n <= MAX_BYTES)
            .ok_or(Error::TooLong)?;
        let mut wav = Vec::new();
        wav.try_reserve_exact(len as usize)
            .map_err(|_| Error::NoMemory)?;
        let bits = 8 * format.bytes() as u16;
        wav.extend(b"RIFF\0\0\0\0WAVEfmt ");
        let tag: u16 = match format {
            Format::Pcm16 => 1,
            _ => 0xFFFE,
        };
        wav.extend(if tag == 1 { 16u32 } else { 40 }.to_le_bytes());
        wav.extend(tag.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(rate.to_le_bytes());
        wav.extend((rate * block as u32).to_le_bytes());
        wav.extend((block as u16).to_le_bytes());
        wav.extend(bits.to_le_bytes());
        if tag == 0xFFFE {
            wav.extend(22u16.to_le_bytes());
            wav.extend(bits.to_le_bytes()); // valid bits
            wav.extend(3u32.to_le_bytes()); // front left, front right
            let sub: u16 = if format == Format::Float32 { 3 } else { 1 };
            wav.extend(sub.to_le_bytes());
            wav.extend([
                0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
            ]);
            wav.extend(b"fact\x04\0\0\0\0\0\0\0");
        }
        wav.extend(b"data\0\0\0\0");
        let mut w = Writer {
            format,
            wav,
            data: 0,
            rng: Rng::new(SEED),
            peak: 0.0,
        };
        w.patch();
        Ok(w)
    }

    /// Append one frame.
    pub fn push(&mut self, l: f32, r: f32) {
        for v in [l, r] {
            let v = if v.is_finite() { v } else { 0.0 };
            self.peak = self.peak.max(v.abs());
            match self.format {
                Format::Pcm16 => {
                    let q = self.quantise(v, 32_767.0) as i16;
                    self.put(&q.to_le_bytes());
                }
                Format::Pcm24 => {
                    let q = self.quantise(v, 8_388_607.0) as i32;
                    self.put(&q.to_le_bytes()[..3]);
                }
                Format::Float32 => self.put(&v.to_le_bytes()),
            }
        }
    }

    /// `v` scaled to `full`, dithered by triangular noise of ±1 step, rounded
    /// and held inside the range.
    fn quantise(&mut self, v: f32, full: f32) -> f32 {
        let u = |r: &mut Rng| r.next_u32() as f32 / 4_294_967_296.0;
        let d = u(&mut self.rng) - u(&mut self.rng);
        (v * full + d).round().clamp(-full, full)
    }

    /// Append within the capacity reserved; past it the bytes are dropped
    /// rather than the buffer grown.
    fn put(&mut self, b: &[u8]) {
        if self.wav.capacity() - self.wav.len() >= b.len() + SMPL_LEN {
            self.wav.extend_from_slice(b);
            self.data += b.len();
        }
    }

    /// End the file, with the frames `start..end` marked as its loop.
    pub fn close(&mut self, rate: u32, looped: Option<(u32, u32)>) {
        if let Some((start, end)) = looped.filter(|(s, e)| s < e) {
            let mut c = [0u8; SMPL_LEN];
            let mut set = |at: usize, v: u32| {
                if let Some(b) = c.get_mut(at..at + 4) {
                    b.copy_from_slice(&v.to_le_bytes());
                }
            };
            set(0, u32::from_le_bytes(*b"smpl"));
            set(4, (SMPL_LEN - 8) as u32);
            set(8 + 8, 1_000_000_000 / rate.max(1)); // nanoseconds a sample
            set(8 + 12, 60); // unity note
            set(8 + 28, 1); // one loop, forward
            set(8 + 36 + 8, start);
            set(8 + 36 + 12, end - 1); // its last frame
            self.wav.extend_from_slice(&c);
        }
        self.patch();
    }

    /// Bring the RIFF, `fact` and `data` sizes up to the frames written.
    pub(crate) fn patch(&mut self) {
        let head = self.format.header_len();
        let data = self.data as u32;
        let frames = data / (2 * self.format.bytes() as u32);
        let riff = (self.wav.len() - 8) as u32;
        let mut set = |at: usize, v: u32| {
            if let Some(b) = self.wav.get_mut(at..at + 4) {
                b.copy_from_slice(&v.to_le_bytes());
            }
        };
        set(4, riff);
        set(head - 4, data);
        if head == 80 {
            set(68, frames);
        }
    }

    /// The file so far, a whole WAV.
    pub fn wav(&self) -> &[u8] {
        &self.wav
    }

    /// The loudest sample written, linear.
    pub fn peak(&self) -> f32 {
        self.peak
    }
}

/// A bounce in progress over an engine.
#[derive(Debug)]
pub struct Bounce {
    out: Writer,
    /// The frame the last bar ends on.
    end: u64,
    /// Frames rendered.
    pos: u64,
    /// Frames in a row below `QUIET`, in the tail.
    quiet: u64,
    quiet_for: u64,
    max_tail: u64,
    rate: u32,
    /// The loop's first frame, for a song with a `loop` line.
    loop_from: Option<u64>,
    done: bool,
}

impl Bounce {
    /// Stop `e`'s song, then bounce `bars` of it from the top, or with 0 its
    /// own length (see the module). Bars past the arrangement's end are
    /// silence. Sizing the file allocates; nothing after does.
    pub fn start(e: &mut Engine, bars: u64, format: Format) -> Result<Bounce, Error> {
        let song = e.song();
        let (bars, loop_from) = match (bars, song.loop_bars) {
            (0, Some((from, to))) => (u64::from(to), Some(u64::from(from.max(1) - 1))),
            (0, None) if song.bars() > 0 => (song.bars(), None),
            // A bar is 4 beats.
            (0, None) => (
                ((FREE_SECONDS * f64::from(song.tempo) / 240.0) as u64).max(1),
                None,
            ),
            (n, _) => (n, None),
        };
        e.song_stop();
        let steps = bars.checked_mul(STEPS_PER_BAR).ok_or(Error::TooLong)?;
        let end = e.clock().step_sample(steps);
        let loop_from = loop_from.map(|bar| e.clock().step_sample(bar * STEPS_PER_BAR));
        let rate = f64::from(e.sample_rate());
        let max_tail = (MAX_TAIL * rate) as u64;
        let frames = end.checked_add(max_tail).ok_or(Error::TooLong)?;
        let out = Writer::new(format, rate.round() as u32, frames)?;
        e.song_play();
        Ok(Bounce {
            out,
            end,
            pos: 0,
            quiet: 0,
            quiet_for: (QUIET_FOR * rate) as u64,
            max_tail,
            rate: rate.round() as u32,
            loop_from,
            done: false,
        })
    }

    /// Render up to `blocks` blocks (at most `MAX_STEP`) of the bounce.
    pub fn step(&mut self, e: &mut Engine, blocks: usize) {
        for _ in 0..blocks.min(MAX_STEP) {
            if self.done {
                break;
            }
            let n = if self.pos < self.end {
                (self.end - self.pos).min(BLOCK as u64) as usize
            } else {
                BLOCK
            };
            e.render(n);
            let (left, right) = e.output().split_at(BLOCK);
            let mut loud = false;
            for (&l, &r) in left.iter().zip(right).take(n) {
                loud |= l.abs() > QUIET || r.abs() > QUIET;
                self.out.push(l, r);
            }
            let tail = self.pos >= self.end;
            self.pos += n as u64;
            if !tail && self.pos == self.end {
                // On the last bar's end, before its step fires.
                e.song_pause();
            } else if tail {
                self.quiet = if loud { 0 } else { self.quiet + n as u64 };
                self.done = self.quiet >= self.quiet_for || self.pos - self.end >= self.max_tail;
                if self.done {
                    let frame = |f: u64| u32::try_from(f).unwrap_or(u32::MAX);
                    let looped = self.loop_from.map(|s| (frame(s), frame(self.end)));
                    self.out.close(self.rate, looped);
                    return;
                }
            }
        }
        self.out.patch();
    }

    /// Frames rendered so far.
    pub fn frames(&self) -> u64 {
        self.pos
    }

    /// The frame the bars end on; the tail follows.
    pub fn end(&self) -> u64 {
        self.end
    }

    pub fn done(&self) -> bool {
        self.done
    }

    /// The file so far, a whole WAV.
    pub fn wav(&self) -> &[u8] {
        self.out.wav()
    }

    /// The loudest sample, linear.
    pub fn peak(&self) -> f32 {
        self.out.peak()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample;

    const RATE: f32 = 48_000.0;
    /// Two bars of 808 at 240 BPM: 2 s.
    const BEAT: &str = "tempo 240\ntrack kit drums\nclip b = kit\n  bd x...x...x...x...\n  sn ....x.......x...\nscene a 2: b\narrange a\n";

    fn engine(text: &str) -> Engine {
        let mut e = Engine::new(RATE);
        e.song_buffer(text.len())
            .expect("fits")
            .copy_from_slice(text.as_bytes());
        e.load_song().expect("parses");
        e
    }

    fn bounce(text: &str, bars: u64, format: Format) -> Bounce {
        let mut e = engine(text);
        let mut b = Bounce::start(&mut e, bars, format).expect("starts");
        while !b.done() {
            b.step(&mut e, MAX_STEP);
        }
        b
    }

    #[test]
    fn the_bars_then_a_tail_that_ends() {
        let b = bounce(BEAT, 0, Format::Float32);
        // 2 bars × 16 steps of 1/16 beat at 240 BPM.
        assert_eq!(b.end(), 2 * 16 * 48_000 * 60 / 240 / 4);
        assert!(b.frames() > b.end(), "a tail");
        assert!(b.frames() <= b.end() + (MAX_TAIL as u64) * 48_000);
        assert!(b.peak() > 0.1 && b.peak() <= 1.0, "heard: {}", b.peak());
    }

    #[test]
    fn every_format_reads_back() {
        let float = bounce(BEAT, 1, Format::Float32);
        let want = sample::parse(float.wav(), RATE).expect("float parses");
        assert_eq!(want.channels, 2);
        assert_eq!(want.frames() as u64, float.frames());
        assert!(want.data.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        for (format, step) in [
            (Format::Pcm16, 1.0 / 32_767.0),
            (Format::Pcm24, 1.0 / 8_388_607.0),
        ] {
            let b = bounce(BEAT, 1, format);
            assert_eq!(
                b.wav().len() as u64,
                format.header_len() as u64 + b.frames() * 2 * format.bytes() as u64
            );
            let got = sample::parse(b.wav(), RATE).expect("pcm parses");
            assert_eq!(got.frames(), want.frames(), "{format:?}");
            let err = got
                .data
                .iter()
                .zip(&want.data)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max);
            assert!(err <= 2.0 * step, "{format:?} off by {err}");
        }
    }

    #[test]
    fn the_same_bytes_every_run() {
        let a = bounce(BEAT, 0, Format::Pcm16);
        let b = bounce(BEAT, 0, Format::Pcm16);
        assert!(a.wav() == b.wav());
    }

    #[test]
    fn a_silent_song_is_silent_but_for_the_dither() {
        let text = "tempo 240\ntrack kit drums\nclip b = kit\n  bd ....\nscene a 1: b\narrange a\n";
        let b = bounce(text, 0, Format::Pcm16);
        assert_eq!(b.peak(), 0.0);
        let s = sample::parse(b.wav(), RATE).expect("parses");
        assert!(s.data.iter().all(|v| v.abs() <= 1.5 / 32_767.0));
        // Quiet from the first frame of the tail: half a second of it, in blocks.
        assert!((24_000..24_000 + BLOCK as u64).contains(&(b.frames() - b.end())));
    }

    #[test]
    fn a_looping_song_plays_to_its_loop_end_and_marks_the_loop() {
        // Scenes a, b, a; the loop is bar 2 (b), so the last a is never heard.
        let looped = "tempo 240\ntrack kit drums\nclip x = kit\n  bd x...\nscene a 1: x\nscene b 1: x\narrange a b a\nloop 2 2\n";
        let b = bounce(looped, 0, Format::Pcm24);
        assert_eq!(b.end(), 2 * 16 * 3_000);
        let s = sample::parse(b.wav(), RATE).expect("parses");
        assert_eq!(s.frames() as u64, b.frames());
        assert_eq!(s.loop_range, Some((16 * 3_000, 2 * 16 * 3_000)));
        // Asked for bars, it plays those and marks nothing.
        let b = bounce(looped, 1, Format::Pcm24);
        let s = sample::parse(b.wav(), RATE).expect("parses");
        assert_eq!(s.loop_range, None);
    }

    #[test]
    fn a_song_without_an_arrangement_bounces_five_minutes_or_its_bars() {
        let free = "tempo 240\ntrack kit drums\nclip b = kit\n  bd x...\n";
        let mut e = engine(free);
        let b = Bounce::start(&mut e, 0, Format::Pcm16).expect("starts");
        // 300 s at 240 BPM, a second a bar.
        assert_eq!(b.end(), 300 * 48_000);
        let b = bounce(free, 3, Format::Pcm24);
        assert_eq!(b.end(), 3 * 16 * 3_000);
        assert!(b.done() && b.peak() > 0.01, "heard: {}", b.peak());
    }

    #[test]
    fn too_long_is_refused_before_allocating() {
        let mut e = engine(BEAT);
        assert_eq!(
            Bounce::start(&mut e, u64::MAX, Format::Pcm16).err(),
            Some(Error::TooLong)
        );
        assert_eq!(
            Bounce::start(&mut e, 1_000_000, Format::Pcm16).err(),
            Some(Error::TooLong)
        );
        assert_eq!(Format::from_bits(8), None);
    }

    #[test]
    fn dither_keeps_a_signal_below_one_step() {
        // A 1 kHz sine at −100 dBFS, a third of a 16-bit step.
        let mut w = Writer::new(Format::Pcm16, 48_000, 48_000).expect("fits");
        let x: Vec<f32> = (0..48_000)
            .map(|i| 1.0e-5 * (std::f32::consts::TAU * 1_000.0 * i as f32 / 48_000.0).sin())
            .collect();
        for v in &x {
            w.push(*v, *v);
        }
        w.patch();
        let s = sample::parse(w.wav(), 48_000.0).expect("parses");
        assert!(s.data.iter().any(|v| *v != 0.0), "not rounded away");
        // The sine is still in there: the output correlates with it.
        let dot: f32 = (0..48_000).map(|i| s.frame(i).0 * x[i]).sum();
        assert!(dot > 0.0, "{dot}");
    }
}
