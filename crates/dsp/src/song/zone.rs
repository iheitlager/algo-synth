//! A Sampler zone as the song holds it (#522): which sample it plays, by the
//! source a recipe names (`@<hash>` of a dropped file or `<pack>/<sample>`),
//! and its fields away from their defaults. The song reads and prints it as
//! `zone keys 1 = @3fa9… : keylo c2, keyhi e3, root c3`; the engine writes it
//! from the zones the hands set.

use super::recipe::{Source, note_number, parse_source};
use crate::notes::note_name;
use crate::sampler::{LoopMode, ZONES, Zone};

/// The fields by the names a zone line gives them, in print order.
pub const FIELD_NAMES: [&str; 13] = [
    "keylo",
    "keyhi",
    "vello",
    "velhi",
    "root",
    "tune",
    "level",
    "loop",
    "loopstart",
    "loopend",
    "seqlen",
    "seqpos",
    "release",
];
/// A zone's loop, by the names a zone line gives it, in `LoopMode` order.
pub const LOOP_NAMES: [&str; 3] = ["off", "loop", "sustain"];

#[derive(Clone, Debug, PartialEq)]
pub struct SongZone {
    pub track: usize,
    /// 0 to `ZONES - 1`; the line writes it from 1.
    pub zone: usize,
    pub source: String,
    /// Its fields; `sample` is the engine's slot and stays `None` here.
    pub fields: Zone,
}

/// A zone's source: a dropped file's hash or a pack's sample.
pub fn is_zone_source(s: &str) -> bool {
    matches!(parse_source(s), Some(Source::Hash(_) | Source::Pack(..)))
}

pub fn is_zone_number(n: usize) -> bool {
    (1..=ZONES).contains(&n)
}

impl SongZone {
    /// The line as the song prints it: only the fields away from their defaults.
    pub fn print(&self, track: &str) -> String {
        let d = Zone::default();
        let z = &self.fields;
        let mut changes = Vec::new();
        let mut note = |name: &str, v: u8, dv: u8| {
            if v != dv {
                changes.push(format!("{name} {}", note_name(v)));
            }
        };
        note("keylo", z.key_lo, d.key_lo);
        note("keyhi", z.key_hi, d.key_hi);
        if z.vel_lo != d.vel_lo {
            changes.push(format!("vello {}", z.vel_lo));
        }
        if z.vel_hi != d.vel_hi {
            changes.push(format!("velhi {}", z.vel_hi));
        }
        if let Some(r) = z.root {
            changes.push(format!("root {}", note_name(r)));
        }
        if z.tune != d.tune {
            changes.push(format!("tune {}", z.tune));
        }
        if z.level != d.level {
            changes.push(format!("level {}", z.level));
        }
        if z.loop_mode != d.loop_mode {
            let name = LOOP_NAMES
                .get(z.loop_mode as usize)
                .copied()
                .unwrap_or("off");
            changes.push(format!("loop {name}"));
        }
        if z.loop_start != d.loop_start {
            changes.push(format!("loopstart {}", z.loop_start));
        }
        if z.loop_end != d.loop_end {
            changes.push(format!("loopend {}", z.loop_end));
        }
        if z.seq_len != d.seq_len {
            changes.push(format!("seqlen {}", z.seq_len));
        }
        if z.seq_pos != d.seq_pos {
            changes.push(format!("seqpos {}", z.seq_pos));
        }
        if z.release {
            changes.push("release 1".to_string());
        }
        let line = format!("zone {track} {} = {}", self.zone + 1, self.source);
        if changes.is_empty() {
            line
        } else {
            format!("{line} : {}", changes.join(", "))
        }
    }
}

/// Set field `name` of `z` from its written value, or say what it takes.
pub fn set_field(z: &mut Zone, name: &str, v: &str) -> Result<(), &'static str> {
    let int = |lo: u32, hi: u32| v.parse::<u32>().ok().filter(|n| (lo..=hi).contains(n));
    let key = || {
        note_number(v)
            .or_else(|| int(0, 127).map(|n| n as u8))
            .ok_or("a key is a note, as c4, or 0 to 127")
    };
    let vel = || int(1, 127).map(|n| n as u8).ok_or("a velocity is 1 to 127");
    let seq = || {
        int(1, 16)
            .map(|n| n as u8)
            .ok_or("a round-robin is 1 to 16")
    };
    let frames = || {
        v.parse::<usize>()
            .ok()
            .ok_or("a loop point is a frame, 0 or more")
    };
    let real = |lo: f32, hi: f32, msg| {
        v.parse::<f32>()
            .ok()
            .filter(|x| x.is_finite() && (lo..=hi).contains(x))
            .ok_or(msg)
    };
    match name {
        "keylo" => z.key_lo = key()?,
        "keyhi" => z.key_hi = key()?,
        "vello" => z.vel_lo = vel()?,
        "velhi" => z.vel_hi = vel()?,
        "root" => z.root = Some(key()?),
        "tune" => z.tune = real(-1200.0, 1200.0, "a tune is −1200 to 1200 cents")?,
        "level" => z.level = real(0.0, 2.0, "a level is 0 to 2")?,
        "loop" => {
            z.loop_mode = match v {
                "off" => LoopMode::Off,
                "loop" => LoopMode::Loop,
                "sustain" => LoopMode::Sustain,
                _ => return Err("a loop is off, loop or sustain"),
            }
        }
        "loopstart" => z.loop_start = frames()?,
        "loopend" => z.loop_end = frames()?,
        "seqlen" => z.seq_len = seq()?,
        "seqpos" => z.seq_pos = seq()?,
        "release" => z.release = int(0, 1).ok_or("release is 0 or 1")? == 1,
        _ => {
            return Err(
                "a zone field: keylo keyhi vello velhi root tune level loop loopstart loopend seqlen seqpos release",
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zone_prints_only_its_changes() {
        let mut z = SongZone {
            track: 0,
            zone: 0,
            source: "upright-piano-kw/C4vH".into(),
            fields: Zone::default(),
        };
        assert_eq!(z.print("keys"), "zone keys 1 = upright-piano-kw/C4vH");
        for (k, v) in [
            ("keylo", "c2"),
            ("keyhi", "64"),
            ("root", "c#3"),
            ("loop", "sustain"),
            ("release", "1"),
        ] {
            assert_eq!(set_field(&mut z.fields, k, v), Ok(()), "{k}");
        }
        assert_eq!(
            z.print("keys"),
            "zone keys 1 = upright-piano-kw/C4vH : keylo c2, keyhi e4, root c#3, loop sustain, release 1"
        );
    }

    #[test]
    fn bad_fields_say_what_they_take() {
        let mut z = Zone::default();
        for (k, v) in [
            ("keylo", "h4"),
            ("keyhi", "128"),
            ("vello", "0"),
            ("tune", "2000"),
            ("level", "nan"),
            ("loop", "ping"),
            ("seqlen", "17"),
            ("release", "2"),
            ("nope", "1"),
        ] {
            assert!(set_field(&mut z, k, v).is_err(), "{k} {v}");
        }
        assert_eq!(z, Zone::default());
    }

    #[test]
    fn a_zone_source_is_a_hash_or_a_pack_sample() {
        assert!(is_zone_source("upright-piano-kw/C4vH"));
        assert!(is_zone_source(&format!(
            "@{}",
            "0123456789abcdef".repeat(2)
        )));
        assert!(!is_zone_source("synth:LaChoir:c4"));
        assert!(!is_zone_source("plaits:0.3:0.5:0.8"));
        assert_eq!(FIELD_NAMES.len(), crate::sampler::ZoneField::ALL.len() - 1);
    }
}
