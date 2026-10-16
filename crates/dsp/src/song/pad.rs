//! A pad of a pad sampler as the song holds it (#527): which sample it plays,
//! by the source a zone names (`@<hash>` of a dropped file or
//! `<pack>/<sample>`), and its fields away from their defaults. The song
//! reads and prints it as `pad kit 1 = audiophob/kick : tune -2, choke 1`;
//! the engine writes it from the pads the hands set.

use crate::padsampler::{PADS, PadCfg};

/// A sound of the track's Strudel sample map (#728): its name, and maybe
/// `:n` for its n-th file (0 when left out), as Strudel's `s("hungup:1")`.
pub fn is_map_sound(s: &str) -> bool {
    map_sound(s).is_some()
}

/// The name and file index of a map sound.
pub fn map_sound(s: &str) -> Option<(&str, u32)> {
    let (name, index) = match s.split_once(':') {
        Some((n, i)) => (n, i.parse::<u32>().ok().filter(|i| *i < 1000)?),
        None => (s, 0),
    };
    let ok = !name.is_empty()
        && name.len() <= 64
        && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    ok.then_some((name, index))
}

/// The fields by the names a pad line gives them, in print order.
pub const FIELD_NAMES: [&str; 9] = [
    "tune", "level", "pan", "decay", "choke", "vellevel", "velstart", "oneshot", "out",
];

#[derive(Clone, Debug, PartialEq)]
pub struct SongPad {
    pub track: usize,
    /// 0 to `PADS - 1`; the line writes it from 1.
    pub pad: usize,
    pub source: String,
    /// Its fields; `sample` is the engine's slot and stays `None` here.
    pub fields: PadCfg,
}

pub fn is_pad_number(n: usize) -> bool {
    (1..=PADS).contains(&n)
}

impl SongPad {
    /// The line as the song prints it: only the fields away from their defaults.
    pub fn print(&self, track: &str) -> String {
        let d = PadCfg::default();
        let p = &self.fields;
        let mut changes = Vec::new();
        let mut real = |name: &str, v: f32, dv: f32| {
            if v != dv {
                changes.push(format!("{name} {v}"));
            }
        };
        real("tune", p.tune, d.tune);
        real("level", p.level, d.level);
        real("pan", p.pan, d.pan);
        real("decay", p.decay, d.decay);
        if p.choke != d.choke {
            changes.push(format!("choke {}", p.choke));
        }
        if p.vel_level != d.vel_level {
            changes.push(format!("vellevel {}", p.vel_level));
        }
        if p.vel_start != d.vel_start {
            changes.push(format!("velstart {}", p.vel_start));
        }
        if p.one_shot != d.one_shot {
            changes.push(format!("oneshot {}", u8::from(p.one_shot)));
        }
        if p.out != d.out {
            changes.push(format!("out {}", p.out));
        }
        let line = format!("pad {track} {} = {}", self.pad + 1, self.source);
        if changes.is_empty() {
            line
        } else {
            format!("{line} : {}", changes.join(", "))
        }
    }
}

/// Set field `name` of `p` from its written value, or say what it takes.
pub fn set_field(p: &mut PadCfg, name: &str, v: &str) -> Result<(), &'static str> {
    let real = |lo: f32, hi: f32, msg| {
        v.parse::<f32>()
            .ok()
            .filter(|x| x.is_finite() && (lo..=hi).contains(x))
            .ok_or(msg)
    };
    let int = |hi: u8, msg| v.parse::<u8>().ok().filter(|n| *n <= hi).ok_or(msg);
    match name {
        "tune" => p.tune = real(-24.0, 24.0, "a tune is −24 to 24 semitones")?,
        "level" => p.level = real(0.0, 2.0, "a level is 0 to 2")?,
        "pan" => p.pan = real(-1.0, 1.0, "a pan is −1 to 1")?,
        "decay" => p.decay = real(0.0, 10.0, "a decay is 0 to 10 seconds")?,
        "choke" => p.choke = int(8, "a choke group is 0 to 8")?,
        "vellevel" => p.vel_level = real(0.0, 1.0, "vellevel is 0 to 1")?,
        "velstart" => p.vel_start = real(0.0, 1.0, "velstart is 0 to 1")?,
        "oneshot" => p.one_shot = int(1, "oneshot is 0 or 1")? == 1,
        "out" => p.out = int(8, "out is 0 (the strip) or a group, 1 to 8")?,
        _ => {
            return Err("a pad field: tune level pan decay choke vellevel velstart oneshot out");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pad_prints_only_its_changes() {
        let mut p = SongPad {
            track: 0,
            pad: 2,
            source: "audiophob/kick".into(),
            fields: PadCfg::default(),
        };
        assert_eq!(p.print("kit"), "pad kit 3 = audiophob/kick");
        for (k, v) in [
            ("tune", "-2"),
            ("choke", "1"),
            ("oneshot", "0"),
            ("out", "3"),
        ] {
            assert_eq!(set_field(&mut p.fields, k, v), Ok(()), "{k}");
        }
        assert_eq!(
            p.print("kit"),
            "pad kit 3 = audiophob/kick : tune -2, choke 1, oneshot 0, out 3"
        );
    }

    #[test]
    fn bad_fields_say_what_they_take() {
        let mut p = PadCfg::default();
        for (k, v) in [
            ("tune", "25"),
            ("level", "x"),
            ("pan", "2"),
            ("decay", "-1"),
            ("choke", "9"),
            ("vellevel", "inf"),
            ("oneshot", "2"),
            ("out", "-1"),
            ("nope", "1"),
        ] {
            assert!(set_field(&mut p, k, v).is_err(), "{k} {v}");
        }
        assert_eq!(p, PadCfg::default());
        assert_eq!(
            FIELD_NAMES.len(),
            crate::padsampler::PadField::ALL.len() - 1
        );
    }
}
