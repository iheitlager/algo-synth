//! The sample store: WAV files parsed and resampled at load, held in bounded
//! memory for the samplers (plan.md samplers epic).
//!
//! JavaScript forwards the file's bytes; everything below is Rust. Loading
//! allocates, so it is a control-thread call and never runs from `render`
//! (ADR-0002); playback only reads. `parse` is total: any byte string gives a
//! `Sample` or an `Error`, never a panic.

/// Slots in the store; a sample is named by its slot index.
pub const SLOTS: usize = 64;

/// Largest WAV file accepted: 32 MiB.
pub const MAX_WAV: usize = 32 << 20;

/// The store's cap, in `f32` values across all slots (64 MiB of samples).
pub const MAX_VALUES: usize = 16 << 20;

/// Why a file was rejected. `code` is what crosses the C ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not a RIFF/WAVE file.
    NotWav,
    /// A chunk runs past the end of the file, or `fmt ` or `data` is missing.
    Truncated,
    /// A format other than PCM 16/24 or 32-bit float, or more than 2 channels.
    Unsupported,
    /// No audio frames, or a zero sample rate.
    Empty,
    /// The file, or the store with it, would pass the memory cap.
    TooLarge,
    /// The slot index is outside the store.
    NoSlot,
}

impl Error {
    /// Negative codes for `sample_load`.
    pub fn code(self) -> i32 {
        match self {
            Error::NotWav => -1,
            Error::Truncated => -2,
            Error::Unsupported => -3,
            Error::Empty => -4,
            Error::TooLarge => -6,
            Error::NoSlot => -7,
        }
    }
}

/// A sample at the engine's rate: `frames` of `channels` (1 or 2)
/// interleaved values.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub channels: u8,
    pub data: Vec<f32>,
    /// Sustain loop `start..end` in frames, from the file's `smpl` chunk.
    pub loop_range: Option<(usize, usize)>,
    /// MIDI note the recording sounds at, from the `smpl` chunk (60 if absent).
    pub root: u8,
}

impl Sample {
    pub fn frames(&self) -> usize {
        self.data.len() / usize::from(self.channels)
    }

    /// Frame `i` as (left, right); mono repeats on both sides. Out of range
    /// reads silence.
    pub fn frame(&self, i: usize) -> (f32, f32) {
        if self.channels == 2 {
            let l = self.data.get(2 * i).copied().unwrap_or(0.0);
            let r = self.data.get(2 * i + 1).copied().unwrap_or(0.0);
            (l, r)
        } else {
            let m = self.data.get(i).copied().unwrap_or(0.0);
            (m, m)
        }
    }
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        b.get(i..i.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(i..i.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// The WAV container's chunks that matter here.
struct Chunks<'a> {
    /// Format tag (1 PCM, 3 float; extensible resolved to its subformat).
    tag: u16,
    channels: u16,
    rate: u32,
    bits: u16,
    data: &'a [u8],
    root: Option<u8>,
    loop_range: Option<(u32, u32)>,
}

fn chunks(bytes: &[u8]) -> Result<Chunks<'_>, Error> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(Error::NotWav);
    }
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    let mut root = None;
    let mut loop_range = None;
    let mut at = 12;
    while let (Some(id), Some(len)) = (bytes.get(at..at + 4), u32_at(bytes, at + 4)) {
        let body = at + 8;
        let end = body.checked_add(len as usize).ok_or(Error::Truncated)?;
        let chunk = bytes.get(body..end).ok_or(Error::Truncated)?;
        match id {
            b"fmt " => {
                let mut tag = u16_at(chunk, 0).ok_or(Error::Truncated)?;
                if tag == 0xFFFE {
                    // WAVE_FORMAT_EXTENSIBLE: the subformat's first two bytes.
                    tag = u16_at(chunk, 24).ok_or(Error::Truncated)?;
                }
                let channels = u16_at(chunk, 2).ok_or(Error::Truncated)?;
                let rate = u32_at(chunk, 4).ok_or(Error::Truncated)?;
                let bits = u16_at(chunk, 14).ok_or(Error::Truncated)?;
                fmt = Some((tag, channels, rate, bits));
            }
            b"data" => data = Some(chunk),
            b"smpl" => {
                // 36-byte header; unity note at 12, loop count at 28, then
                // 24-byte loops of (id, type, start, end, fraction, count).
                root = u32_at(chunk, 12)
                    .and_then(|n| u8::try_from(n).ok())
                    .filter(|n| *n < 128);
                if u32_at(chunk, 28).is_some_and(|n| n > 0) {
                    if let (Some(s), Some(e)) = (u32_at(chunk, 44), u32_at(chunk, 48)) {
                        loop_range = Some((s, e));
                    }
                }
            }
            _ => {}
        }
        // Chunks are word-aligned.
        at = end.saturating_add(end & 1);
    }
    let (tag, channels, rate, bits) = fmt.ok_or(Error::Truncated)?;
    Ok(Chunks {
        tag,
        channels,
        rate,
        bits,
        data: data.ok_or(Error::Truncated)?,
        root,
        loop_range,
    })
}

/// Parse `bytes` and resample to `target_rate` (Catmull-Rom). A sample
/// recorded above the engine's rate is not low-passed first.
pub fn parse(bytes: &[u8], target_rate: f32) -> Result<Sample, Error> {
    let c = chunks(bytes)?;
    if !(1..=2).contains(&c.channels) {
        return Err(Error::Unsupported);
    }
    let width = match (c.tag, c.bits) {
        (1, 16) => 2,
        (1, 24) => 3,
        (3, 32) => 4,
        _ => return Err(Error::Unsupported),
    };
    if c.rate == 0 || !(target_rate.is_finite() && target_rate > 0.0) {
        return Err(Error::Empty);
    }
    let channels = usize::from(c.channels);
    let frames = c.data.len() / (width * channels);
    if frames == 0 {
        return Err(Error::Empty);
    }
    let ratio = f64::from(c.rate) / f64::from(target_rate);
    let out_frames = (frames as f64 / ratio).floor() as usize;
    if out_frames == 0 {
        return Err(Error::Empty);
    }
    // Check the cap before allocating anything.
    if out_frames.saturating_mul(channels) > MAX_VALUES {
        return Err(Error::TooLarge);
    }

    let decoded: Vec<f32> = c
        .data
        .chunks_exact(width)
        .take(frames * channels)
        .map(|b| {
            let byte = |k: usize| b.get(k).copied().unwrap_or(0);
            match width {
                2 => f32::from(i16::from_le_bytes([byte(0), byte(1)])) / 32_768.0,
                3 => (i32::from_le_bytes([0, byte(0), byte(1), byte(2)]) >> 8) as f32 / 8_388_608.0,
                _ => f32::from_le_bytes([byte(0), byte(1), byte(2), byte(3)]),
            }
        })
        // A float file may carry NaN or runaway values; keep playback bounded.
        .map(|v| {
            if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        })
        .collect();

    let (data, scale) = if (ratio - 1.0).abs() < 1.0e-9 {
        (decoded, 1.0)
    } else {
        let mut out = Vec::with_capacity(out_frames * channels);
        for n in 0..out_frames {
            let pos = n as f64 * ratio;
            let i = pos as usize;
            let t = (pos - i as f64) as f32;
            for ch in 0..channels {
                let at = |k: isize| {
                    let j = (i as isize + k).clamp(0, frames as isize - 1) as usize;
                    decoded.get(j * channels + ch).copied().unwrap_or(0.0)
                };
                let (p0, p1, p2, p3) = (at(-1), at(0), at(1), at(2));
                out.push(
                    p1 + 0.5
                        * t
                        * (p2 - p0
                            + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3
                                + t * (3.0 * (p1 - p2) + p3 - p0))),
                );
            }
        }
        (out, 1.0 / ratio)
    };

    let loop_range = c.loop_range.and_then(|(s, e)| {
        let s = (f64::from(s) * scale) as usize;
        let e = ((f64::from(e) * scale) as usize + 1).min(out_frames);
        (s < e).then_some((s, e))
    });
    Ok(Sample {
        channels: c.channels as u8,
        data,
        loop_range,
        root: c.root.unwrap_or(60),
    })
}

/// The slots, and the memory they hold.
pub struct SampleStore {
    slots: Vec<Option<Sample>>,
    values: usize,
}

impl Default for SampleStore {
    fn default() -> Self {
        SampleStore::new()
    }
}

impl SampleStore {
    pub fn new() -> SampleStore {
        SampleStore {
            slots: vec![None; SLOTS],
            values: 0,
        }
    }

    /// Parse `bytes` into `slot`, replacing what was there. On any error the
    /// slot keeps its old sample.
    pub fn load(&mut self, slot: usize, bytes: &[u8], rate: f32) -> Result<&Sample, Error> {
        let cell = self.slots.get_mut(slot).ok_or(Error::NoSlot)?;
        let old_values = cell.as_ref().map_or(0, |s| s.data.len());
        let sample = parse(bytes, rate)?;
        let values = self.values - old_values + sample.data.len();
        if values > MAX_VALUES {
            return Err(Error::TooLarge);
        }
        self.values = values;
        Ok(cell.insert(sample))
    }

    pub fn get(&self, slot: usize) -> Option<&Sample> {
        self.slots.get(slot)?.as_ref()
    }

    pub fn clear(&mut self, slot: usize) {
        if let Some(cell) = self.slots.get_mut(slot) {
            if let Some(s) = cell.take() {
                self.values -= s.data.len();
            }
        }
    }

    /// `f32` values held across all slots.
    pub fn values(&self) -> usize {
        self.values
    }
}

/// A mono 16-bit WAV of `values` (−1..=1) at `rate`, with a `smpl` chunk of
/// root note, loop start and loop end when given; for tests that need a
/// loaded sample.
#[cfg(test)]
pub(crate) fn test_wav(rate: u32, values: &[f32], smpl: Option<(u32, u32, u32)>) -> Vec<u8> {
    let data: Vec<u8> = values
        .iter()
        .flat_map(|v| ((v * 32_767.0) as i16).to_le_bytes())
        .collect();
    let mut body = b"WAVEfmt ".to_vec();
    body.extend(16u32.to_le_bytes());
    for field in [1u16, 1] {
        body.extend(field.to_le_bytes());
    }
    body.extend(rate.to_le_bytes());
    body.extend((rate * 2).to_le_bytes());
    body.extend(2u16.to_le_bytes());
    body.extend(16u16.to_le_bytes());
    body.extend(b"data");
    body.extend((data.len() as u32).to_le_bytes());
    body.extend(data);
    if let Some((root, start, end)) = smpl {
        let mut c = vec![0u8; 36];
        c[12..16].copy_from_slice(&root.to_le_bytes());
        c[28..32].copy_from_slice(&1u32.to_le_bytes());
        let mut l = vec![0u8; 24];
        l[8..12].copy_from_slice(&start.to_le_bytes());
        l[12..16].copy_from_slice(&end.to_le_bytes());
        c.extend(l);
        body.extend(b"smpl");
        body.extend((c.len() as u32).to_le_bytes());
        body.extend(c);
    }
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV file around `data`, with optional extra chunks.
    fn wav(tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8], extra: &[u8]) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend(tag.to_le_bytes());
        fmt.extend(channels.to_le_bytes());
        fmt.extend(rate.to_le_bytes());
        let block = channels * bits / 8;
        fmt.extend((rate * u32::from(block)).to_le_bytes());
        fmt.extend(block.to_le_bytes());
        fmt.extend(bits.to_le_bytes());
        let mut body = b"WAVE".to_vec();
        body.extend(b"fmt ");
        body.extend((fmt.len() as u32).to_le_bytes());
        body.extend(&fmt);
        body.extend(b"data");
        body.extend((data.len() as u32).to_le_bytes());
        body.extend(data);
        if data.len() % 2 == 1 {
            body.push(0);
        }
        body.extend(extra);
        let mut out = b"RIFF".to_vec();
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        out
    }

    fn pcm16(values: &[i16]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn smpl(root: u32, start: u32, end: u32) -> Vec<u8> {
        let mut c = vec![0u8; 36];
        c[12..16].copy_from_slice(&root.to_le_bytes());
        c[28..32].copy_from_slice(&1u32.to_le_bytes());
        let mut l = vec![0u8; 24];
        l[8..12].copy_from_slice(&start.to_le_bytes());
        l[12..16].copy_from_slice(&end.to_le_bytes());
        c.extend(l);
        let mut out = b"smpl".to_vec();
        out.extend((c.len() as u32).to_le_bytes());
        out.extend(c);
        out
    }

    #[test]
    fn pcm16_mono_reads_back() {
        let s = parse(
            &wav(1, 1, 48_000, 16, &pcm16(&[0, 16_384, -32_768]), &[]),
            48_000.0,
        )
        .unwrap();
        assert_eq!(s.channels, 1);
        assert_eq!(s.data, vec![0.0, 0.5, -1.0]);
        assert_eq!(s.root, 60);
        assert_eq!(s.loop_range, None);
    }

    #[test]
    fn pcm24_stereo_reads_back() {
        // Left +0.5, right -0.5.
        let data = [0x00, 0x00, 0x40, 0x00, 0x00, 0xC0];
        let s = parse(&wav(1, 2, 48_000, 24, &data, &[]), 48_000.0).unwrap();
        assert_eq!(s.frames(), 1);
        assert_eq!(s.frame(0), (0.5, -0.5));
    }

    #[test]
    fn float_is_bounded() {
        let data: Vec<u8> = [0.25f32, f32::NAN, 9.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let s = parse(&wav(3, 1, 48_000, 32, &data, &[]), 48_000.0).unwrap();
        assert_eq!(s.data, vec![0.25, 0.0, 1.0]);
    }

    #[test]
    fn smpl_chunk_gives_root_and_loop() {
        let s = parse(
            &wav(1, 1, 48_000, 16, &pcm16(&[0; 100]), &smpl(64, 10, 49)),
            48_000.0,
        )
        .unwrap();
        assert_eq!(s.root, 64);
        assert_eq!(s.loop_range, Some((10, 50)));
    }

    #[test]
    fn resampling_keeps_pitch_and_scales_loop() {
        // A 441 Hz sine at 44.1 kHz, played at 48 kHz, is still 441 Hz.
        let n = 4410;
        let src: Vec<i16> = (0..n)
            .map(|i| ((i as f32 / 100.0 * std::f32::consts::TAU).sin() * 20_000.0) as i16)
            .collect();
        let file = wav(1, 1, 44_100, 16, &pcm16(&src), &smpl(60, 1000, 2000));
        let s = parse(&file, 48_000.0).unwrap();
        assert_eq!(s.frames(), 4800);
        let crossings = s
            .data
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        assert_eq!(
            crossings, 45,
            "44.1 cycles, the first from the zero at frame 0"
        );
        let (a, b) = s.loop_range.unwrap();
        assert_eq!(a, 1088);
        assert!((2177..=2178).contains(&b));
        assert!(s.data.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }

    #[test]
    fn bad_files_are_errors_not_panics() {
        assert_eq!(parse(b"", 48_000.0), Err(Error::NotWav));
        assert_eq!(parse(b"RIFFxxxxWAVE", 48_000.0), Err(Error::Truncated));
        let good = wav(1, 1, 48_000, 16, &pcm16(&[1, 2, 3, 4]), &[]);
        for cut in 0..good.len() {
            drop(parse(&good[..cut], 48_000.0));
        }
        assert_eq!(
            parse(&wav(1, 3, 48_000, 16, &pcm16(&[1; 6]), &[]), 48_000.0),
            Err(Error::Unsupported)
        );
        assert_eq!(
            parse(&wav(1, 1, 48_000, 8, &[1, 2], &[]), 48_000.0),
            Err(Error::Unsupported)
        );
        assert_eq!(
            parse(&wav(1, 1, 48_000, 16, &[], &[]), 48_000.0),
            Err(Error::Empty)
        );
        assert_eq!(
            parse(&wav(1, 1, 0, 16, &pcm16(&[1]), &[]), 48_000.0),
            Err(Error::Empty)
        );
    }

    #[test]
    fn oversized_data_chunk_length_is_truncated() {
        let mut f = wav(1, 1, 48_000, 16, &pcm16(&[1, 2]), &[]);
        let at = f.windows(4).position(|w| w == b"data").unwrap() + 4;
        f[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&f, 48_000.0), Err(Error::Truncated));
    }

    #[test]
    fn store_replaces_clears_and_caps() {
        let mut store = SampleStore::new();
        let small = wav(1, 1, 48_000, 16, &pcm16(&[1; 100]), &[]);
        store.load(3, &small, 48_000.0).unwrap();
        assert_eq!(store.values(), 100);
        store.load(3, &small, 48_000.0).unwrap();
        assert_eq!(store.values(), 100, "a reload replaces");
        assert_eq!(
            store.load(SLOTS, &small, 48_000.0).err(),
            Some(Error::NoSlot)
        );
        assert!(store.get(3).is_some());
        store.clear(3);
        assert_eq!(store.values(), 0);
        assert!(store.get(3).is_none());

        // A failed load leaves the slot as it was.
        store.load(0, &small, 48_000.0).unwrap();
        assert!(store.load(0, b"nope", 48_000.0).is_err());
        assert_eq!(store.get(0).unwrap().frames(), 100);

        // Fill the store to its cap: the next load is refused.
        let big = wav(1, 1, 48_000, 16, &vec![0u8; 2 * (MAX_VALUES / 2)], &[]);
        store.clear(0);
        store.load(1, &big, 48_000.0).unwrap();
        store.load(2, &big, 48_000.0).unwrap();
        assert_eq!(store.values(), MAX_VALUES);
        assert_eq!(store.load(4, &small, 48_000.0).err(), Some(Error::TooLarge));
        assert_eq!(store.values(), MAX_VALUES);
    }
}
