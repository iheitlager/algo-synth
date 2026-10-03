//! DX7 voices from SysEx (spec 006 Req 14): a single voice dump, a 32-voice bank,
//! or the same without their SysEx framing. The parser is total: it never panics,
//! whatever it is given, and out-of-range values are clamped as `FmPatch` clamps them.
//! Only the bytes of a file come from JavaScript; everything is read here.
//!
//! Layouts are the DX7's: the single voice is 155 bytes in the order of `FmPatch`
//! (operator 6 first, then the pitch envelope, algorithm and LFO, and a 10-character
//! name); the bank packs 32 voices of 128 bytes with several settings to a byte.

use crate::fm::patch::FmPatch;

/// Bytes in an unpacked voice, name included.
pub const VOICE_LEN: usize = 155;
/// Bytes in a packed voice.
pub const PACKED_LEN: usize = 128;
/// Voices in a bank.
pub const BANK: usize = 32;
/// Bytes of a message's header: F0, Yamaha, channel, format, and two length bytes.
const HEADER: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    /// A SysEx message that is not from Yamaha.
    NotYamaha,
    /// The message's format is not a single voice or a bank.
    Unsupported,
    /// Fewer bytes than the format needs.
    Truncated,
}

impl Error {
    /// Negative codes for `sysex_load`.
    pub fn code(self) -> i32 {
        match self {
            Error::Empty => -1,
            Error::NotYamaha => -2,
            Error::Unsupported => -3,
            Error::Truncated => -4,
        }
    }
}

/// A voice read from SysEx.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Voice {
    pub patch: FmPatch,
    /// Up to ten printable characters.
    pub name: String,
}

/// An unpacked voice (155 bytes) as a patch and a name.
fn voice_from(bytes: &[u8]) -> Voice {
    let mut patch = FmPatch::default();
    for op in 0..6 {
        for k in 0..21 {
            patch.set_op(
                op,
                k,
                f32::from(bytes.get(op * 21 + k).copied().unwrap_or(0)),
            );
        }
    }
    for k in 0..19 {
        patch.set_global(k, f32::from(bytes.get(126 + k).copied().unwrap_or(0)));
    }
    let name = bytes
        .get(145..155)
        .unwrap_or(&[])
        .iter()
        .map(|b| {
            if (32..127).contains(b) {
                char::from(*b)
            } else {
                ' '
            }
        })
        .collect::<String>()
        .trim_end()
        .to_string();
    Voice { patch, name }
}

/// Unpack one 128-byte voice of a bank into the 155-byte layout (the name last).
pub fn unpack(bulk: &[u8]) -> [u8; VOICE_LEN] {
    let b = |i: usize| bulk.get(i).copied().unwrap_or(0);
    let mut out = [0_u8; VOICE_LEN];
    for op in 0..6 {
        let (i, o) = (op * 17, op * 21);
        for k in 0..11 {
            if let Some(x) = out.get_mut(o + k) {
                *x = b(i + k);
            }
        }
        let set = |out: &mut [u8; VOICE_LEN], k: usize, v: u8| {
            if let Some(x) = out.get_mut(o + k) {
                *x = v;
            }
        };
        set(&mut out, 11, b(i + 11) & 3);
        set(&mut out, 12, (b(i + 11) >> 2) & 3);
        set(&mut out, 13, b(i + 12) & 7);
        set(&mut out, 20, (b(i + 12) >> 3) & 15);
        set(&mut out, 14, b(i + 13) & 3);
        set(&mut out, 15, (b(i + 13) >> 2) & 7);
        set(&mut out, 16, b(i + 14));
        set(&mut out, 17, b(i + 15) & 1);
        set(&mut out, 18, (b(i + 15) >> 1) & 31);
        set(&mut out, 19, b(i + 16));
    }
    for k in 0..9 {
        if let Some(x) = out.get_mut(126 + k) {
            *x = b(102 + k);
        }
    }
    let mut set = |k: usize, v: u8| {
        if let Some(x) = out.get_mut(k) {
            *x = v;
        }
    };
    set(135, b(111) & 7);
    set(136, (b(111) >> 3) & 1);
    for k in 0..4 {
        set(137 + k, b(112 + k));
    }
    set(141, b(116) & 1);
    set(142, (b(116) >> 1) & 7);
    set(143, (b(116) >> 4) & 7);
    for k in 0..11 {
        set(144 + k, b(117 + k));
    }
    out
}

/// Pack a 155-byte voice into the 128 bytes of a bank (the inverse of `unpack`).
pub fn pack(voice: &[u8; VOICE_LEN]) -> [u8; PACKED_LEN] {
    let v = |i: usize| voice.get(i).copied().unwrap_or(0);
    let mut out = [0_u8; PACKED_LEN];
    {
        let mut set = |k: usize, x: u8| {
            if let Some(s) = out.get_mut(k) {
                *s = x;
            }
        };
        for op in 0..6 {
            let (i, o) = (op * 17, op * 21);
            for k in 0..11 {
                set(i + k, v(o + k));
            }
            set(i + 11, (v(o + 11) & 3) | ((v(o + 12) & 3) << 2));
            set(i + 12, (v(o + 13) & 7) | ((v(o + 20) & 15) << 3));
            set(i + 13, (v(o + 14) & 3) | ((v(o + 15) & 7) << 2));
            set(i + 14, v(o + 16));
            set(i + 15, (v(o + 17) & 1) | ((v(o + 18) & 31) << 1));
            set(i + 16, v(o + 19));
        }
        for k in 0..9 {
            set(102 + k, v(126 + k));
        }
        set(111, (v(135) & 7) | ((v(136) & 1) << 3));
        for k in 0..4 {
            set(112 + k, v(137 + k));
        }
        set(
            116,
            (v(141) & 1) | ((v(142) & 7) << 1) | ((v(143) & 7) << 4),
        );
        for k in 0..11 {
            set(117 + k, v(144 + k));
        }
    }
    out
}

/// The 155 bytes of a patch and its name, as a single-voice dump holds them.
pub fn to_voice_bytes(patch: &FmPatch, name: &str) -> [u8; VOICE_LEN] {
    let mut out = [0_u8; VOICE_LEN];
    for op in 0..6 {
        for k in 0..21 {
            if let Some(x) = out.get_mut(op * 21 + k) {
                *x = patch.op_field(op, k);
            }
        }
    }
    for k in 0..19 {
        if let Some(x) = out.get_mut(126 + k) {
            *x = patch.global_field(k);
        }
    }
    for (k, c) in name
        .chars()
        .chain(std::iter::repeat(' '))
        .take(10)
        .enumerate()
    {
        if let Some(x) = out.get_mut(145 + k) {
            *x = if c.is_ascii() && (c as u32) >= 32 {
                c as u8
            } else {
                b' '
            };
        }
    }
    out
}

/// The checksum of a dump's data: the two's complement of its sum, in seven bits.
pub fn checksum(data: &[u8]) -> u8 {
    let sum = data
        .iter()
        .fold(0_u32, |a, b| a.wrapping_add(u32::from(*b)));
    (0_u32.wrapping_sub(sum) & 0x7f) as u8
}

/// The voices in `bytes`: a single-voice dump (one voice), a bank (up to 32), or either
/// without the SysEx framing (155, 128 or 4096 bytes). A wrong checksum is not an
/// error: many files carry one.
pub fn parse(bytes: &[u8]) -> Result<Vec<Voice>, Error> {
    let Some(first) = bytes.first() else {
        return Err(Error::Empty);
    };
    if *first == 0xf0 {
        if bytes.get(1) != Some(&0x43) {
            return Err(Error::NotYamaha);
        }
        let format = bytes.get(3).copied().ok_or(Error::Truncated)?;
        let body = bytes.get(HEADER..).ok_or(Error::Truncated)?;
        return match format {
            0 => Ok(vec![voice_from(
                body.get(..VOICE_LEN).ok_or(Error::Truncated)?,
            )]),
            9 => bank(body.get(..BANK * PACKED_LEN).ok_or(Error::Truncated)?),
            _ => Err(Error::Unsupported),
        };
    }
    match bytes.len() {
        VOICE_LEN => Ok(vec![voice_from(bytes)]),
        PACKED_LEN => Ok(vec![voice_from(&unpack(bytes))]),
        n if n == BANK * PACKED_LEN => bank(bytes),
        _ => Err(Error::Unsupported),
    }
}

fn bank(data: &[u8]) -> Result<Vec<Voice>, Error> {
    Ok(data
        .chunks(PACKED_LEN)
        .take(BANK)
        .map(|c| voice_from(&unpack(c)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A patch with every field at a distinct in-range value.
    fn patch() -> FmPatch {
        let mut p = FmPatch::default();
        for op in 0..6 {
            for k in 0..21 {
                p.set_op(op, k, (op * 7 + k * 3 + 5) as f32 % 90.0);
            }
        }
        for k in 0..19 {
            p.set_global(k, (k * 5 + 2) as f32);
        }
        p
    }

    fn single(patch: &FmPatch, name: &str) -> Vec<u8> {
        let data = to_voice_bytes(patch, name);
        let mut m = vec![0xf0, 0x43, 0x00, 0x00, 0x01, 0x1b];
        m.extend_from_slice(&data);
        m.push(checksum(&data));
        m.push(0xf7);
        m
    }

    fn bank_bytes(patch: &FmPatch) -> Vec<u8> {
        let mut data = Vec::new();
        for n in 0..BANK {
            let mut p = *patch;
            p.set_global(8, (n % 32) as f32);
            data.extend_from_slice(&pack(&to_voice_bytes(&p, &format!("VOICE {n:02}"))));
        }
        let mut m = vec![0xf0, 0x43, 0x00, 0x09, 0x20, 0x00];
        m.extend_from_slice(&data);
        m.push(checksum(&data));
        m.push(0xf7);
        m
    }

    #[test]
    fn packing_round_trips_every_field() {
        let p = patch();
        let bytes = to_voice_bytes(&p, "ROUNDTRIP");
        assert_eq!(unpack(&pack(&bytes)), bytes);
        let voice = voice_from(&bytes);
        assert_eq!(voice.patch, p);
        assert_eq!(voice.name, "ROUNDTRIP");
    }

    /// Spec 006 Req 14: a bank of 32 voices and a single voice are read, with their names.
    #[test]
    fn a_bank_and_a_single_voice_are_read() {
        let p = patch();
        let voices = parse(&bank_bytes(&p)).expect("a bank");
        assert_eq!(voices.len(), 32);
        assert_eq!(voices[3].name, "VOICE 03");
        assert_eq!(voices[7].patch.algorithm, 7);
        let one = parse(&single(&p, "ONE")).expect("a voice");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].patch, p);
        assert_eq!(one[0].name, "ONE");
    }

    #[test]
    fn framing_is_optional_and_a_bad_checksum_is_tolerated() {
        let p = patch();
        let bank = bank_bytes(&p);
        assert_eq!(
            parse(&bank[6..6 + BANK * PACKED_LEN])
                .expect("bare bank")
                .len(),
            32
        );
        let mut m = single(&p, "ONE");
        let n = m.len();
        m[n - 2] ^= 0x55;
        assert_eq!(parse(&m).expect("a wrong checksum").len(), 1);
        assert_eq!(
            parse(&to_voice_bytes(&p, "BARE"))
                .expect("a bare voice")
                .len(),
            1
        );
        assert_eq!(
            parse(&pack(&to_voice_bytes(&p, "PACKED"))).expect("a bare packed voice")[0]
                .patch
                .algorithm,
            p.algorithm
        );
    }

    /// Bad input is an error, never a panic; every prefix and every mutation is safe.
    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert_eq!(parse(&[]), Err(Error::Empty));
        assert_eq!(
            parse(&[0xf0, 0x41, 0x00, 0x09, 0x20, 0x00]),
            Err(Error::NotYamaha)
        );
        assert_eq!(
            parse(&[0xf0, 0x43, 0x00, 0x7e, 0x00, 0x00, 1, 2, 3]),
            Err(Error::Unsupported)
        );
        assert_eq!(parse(&[1, 2, 3]), Err(Error::Unsupported));
        let p = patch();
        for message in [single(&p, "X"), bank_bytes(&p)] {
            for n in 0..message.len() {
                drop(parse(&message[..n]));
            }
            for i in (0..message.len()).step_by(37) {
                let mut m = message.clone();
                m[i] = 0xff;
                if let Ok(voices) = parse(&m) {
                    for v in voices {
                        assert!(
                            v.patch.algorithm <= 31
                                && v.patch.feedback <= 7
                                && v.patch.transpose <= 48
                        );
                        assert!(v.name.chars().all(|c| c.is_ascii() && !c.is_control()));
                    }
                }
            }
        }
        // Noise of every length near the formats.
        let mut x = 0x1234_5678_u32;
        for len in [
            0, 1, 5, 6, 127, 128, 129, 154, 155, 156, 4095, 4096, 4097, 4104,
        ] {
            let junk: Vec<u8> = (0..len)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x as u8
                })
                .collect();
            drop(parse(&junk));
        }
    }

    #[test]
    fn the_checksum_is_the_twos_complement_in_seven_bits() {
        assert_eq!(checksum(&[1, 2, 3]), (128 - 6) as u8);
        assert_eq!(checksum(&[]), 0);
        let data = [10_u8; 155];
        assert_eq!(
            (data.iter().map(|b| u32::from(*b)).sum::<u32>() + u32::from(checksum(&data))) & 0x7f,
            0
        );
    }
}
