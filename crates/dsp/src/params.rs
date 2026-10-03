//! The one parameter registry (ADR-0004).
//!
//! Every knob in the UI maps to `set_param(id, value)`. The ids here are the
//! source of truth; `web/src/audio/params.ts` mirrors them and a test below
//! fails if the two drift apart.

/// A parameter id as it crosses the C ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Param {
    /// Master output gain, 0..=1.
    MasterGain = 0,
    /// VCO 1 waveform id (`Waveform`), 0..=3.
    Vco1Wave = 1,
    /// VCO 1 coarse tune in semitones, −24..=24.
    Vco1Coarse = 2,
    /// VCO 1 fine tune in cents, −50..=50.
    Vco1Fine = 3,
    /// VCO 1 level into the mixer, 0..=1.
    Vco1Level = 4,
    /// VCO 2 waveform id (`Waveform`), 0..=3.
    Vco2Wave = 5,
    /// VCO 2 coarse tune in semitones, −24..=24.
    Vco2Coarse = 6,
    /// VCO 2 fine tune in cents, −50..=50.
    Vco2Fine = 7,
    /// VCO 2 level into the mixer, 0..=1.
    Vco2Level = 8,
    /// VCO 3 waveform id (`Waveform`), 0..=3.
    Vco3Wave = 9,
    /// VCO 3 coarse tune in semitones, −24..=24.
    Vco3Coarse = 10,
    /// VCO 3 fine tune in cents, −50..=50.
    Vco3Fine = 11,
    /// VCO 3 level into the mixer, 0..=1.
    Vco3Level = 12,
    /// Pulse width of every VCO, 0.05..=0.95.
    PulseWidth = 13,
    /// VCO 2 hard-syncs to VCO 1 when ≥ 0.5.
    Vco2Sync = 14,
    /// VCO 3 hard-syncs to VCO 1 when ≥ 0.5.
    Vco3Sync = 15,
    /// Noise level into the mixer, 0..=1.
    NoiseLevel = 16,
    /// Noise colour id (`NoiseColour`), 0..=1.
    NoiseColour = 17,
    /// Ladder cutoff in Hz, 20..=20000.
    Cutoff = 18,
    /// Ladder resonance, 0..=1; it self-oscillates from 0.8.
    Resonance = 19,
    /// Ladder drive into the saturator, 0..=1 (0 to +18 dB).
    Drive = 20,
    /// ADSR attack, decay and release in seconds, 0.001..=10.
    AdsrAttack = 21,
    AdsrDecay = 22,
    /// ADSR sustain level, 0..=1.
    AdsrSustain = 23,
    AdsrRelease = 24,
    /// AR attack and release in seconds, 0.001..=10.
    ArAttack = 25,
    ArRelease = 26,
    /// LFO rate in Hz, 0.01..=50.
    LfoRate = 27,
    /// LFO waveform id (`Waveform`; pulse is the square), 0..=3.
    LfoWave = 28,
    /// Mono note priority id (`NotePriority`: last, low, high), 0..=2.
    Priority = 29,
    /// Mono legato when ≥ 0.5: a new key while one is held keeps the envelope.
    Legato = 30,
    /// Mono glide time in seconds, 0..=5; 0 is off.
    Glide = 31,
    /// Patch slot 1 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch1Source = 32,
    /// Patch slot 1 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch1Dest = 33,
    /// Patch slot 1 amount, −1..=1.
    Patch1Amount = 34,
    /// Patch slot 2 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch2Source = 35,
    /// Patch slot 2 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch2Dest = 36,
    /// Patch slot 2 amount, −1..=1.
    Patch2Amount = 37,
    /// Patch slot 3 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch3Source = 38,
    /// Patch slot 3 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch3Dest = 39,
    /// Patch slot 3 amount, −1..=1.
    Patch3Amount = 40,
    /// Patch slot 4 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch4Source = 41,
    /// Patch slot 4 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch4Dest = 42,
    /// Patch slot 4 amount, −1..=1.
    Patch4Amount = 43,
    /// Patch slot 5 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch5Source = 44,
    /// Patch slot 5 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch5Dest = 45,
    /// Patch slot 5 amount, −1..=1.
    Patch5Amount = 46,
    /// Patch slot 6 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch6Source = 47,
    /// Patch slot 6 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch6Dest = 48,
    /// Patch slot 6 amount, −1..=1.
    Patch6Amount = 49,
    /// Patch slot 7 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch7Source = 50,
    /// Patch slot 7 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch7Dest = 51,
    /// Patch slot 7 amount, −1..=1.
    Patch7Amount = 52,
    /// Patch slot 8 source id (`ModSource`), 0..=255; unknown ids are ignored.
    Patch8Source = 53,
    /// Patch slot 8 destination id (`ModDest`), 0..=255; unknown ids are ignored.
    Patch8Dest = 54,
    /// Patch slot 8 amount, −1..=1.
    Patch8Amount = 55,
    /// Normalled ADSR → cutoff, −1..=1 (±4 octaves).
    EnvCutoff = 56,
    /// Normalled key → cutoff, 0..=1 (1 follows the key exactly).
    KeyTrack = 57,
    /// Normalled LFO → VCO pitch at full mod wheel, 0..=1 (±2 semitones).
    Vibrato = 58,
    /// The mod wheel, 0..=1, until MIDI input sends CC 1 (#10).
    ModWheel = 59,
    /// Mixer fader of a synth, 0..=1.
    Level = 60,
    /// Mixer pan of a synth, −1 (left)..=1 (right), equal power.
    Pan = 61,
    /// Post-fader send to the echo, 0..=1.
    EchoSend = 62,
    /// Post-fader send to the reverb, 0..=1.
    ReverbSend = 63,
    /// Silences the synth when ≥ 0.5.
    Mute = 64,
    /// When any synth is soloed (≥ 0.5), only soloed synths sound.
    Solo = 65,
    /// Drive insert mode id (`DriveMode`: off, overdrive, distortion, fuzz), 0..=3.
    DriveMode = 66,
    /// Drive amount, 0..=1 (0 to +40 dB into the shaper).
    DriveAmount = 67,
    /// Drive tone, 0..=1: the low-pass after the shaper, 200 Hz to 20 kHz.
    DriveTone = 68,
    /// Drive output level, 0..=1.
    DriveLevel = 69,
}

impl Param {
    /// Every parameter with the name the TypeScript mirror uses.
    pub const ALL: [(Param, &'static str); 70] = [
        (Param::MasterGain, "MasterGain"),
        (Param::Vco1Wave, "Vco1Wave"),
        (Param::Vco1Coarse, "Vco1Coarse"),
        (Param::Vco1Fine, "Vco1Fine"),
        (Param::Vco1Level, "Vco1Level"),
        (Param::Vco2Wave, "Vco2Wave"),
        (Param::Vco2Coarse, "Vco2Coarse"),
        (Param::Vco2Fine, "Vco2Fine"),
        (Param::Vco2Level, "Vco2Level"),
        (Param::Vco3Wave, "Vco3Wave"),
        (Param::Vco3Coarse, "Vco3Coarse"),
        (Param::Vco3Fine, "Vco3Fine"),
        (Param::Vco3Level, "Vco3Level"),
        (Param::PulseWidth, "PulseWidth"),
        (Param::Vco2Sync, "Vco2Sync"),
        (Param::Vco3Sync, "Vco3Sync"),
        (Param::NoiseLevel, "NoiseLevel"),
        (Param::NoiseColour, "NoiseColour"),
        (Param::Cutoff, "Cutoff"),
        (Param::Resonance, "Resonance"),
        (Param::Drive, "Drive"),
        (Param::AdsrAttack, "AdsrAttack"),
        (Param::AdsrDecay, "AdsrDecay"),
        (Param::AdsrSustain, "AdsrSustain"),
        (Param::AdsrRelease, "AdsrRelease"),
        (Param::ArAttack, "ArAttack"),
        (Param::ArRelease, "ArRelease"),
        (Param::LfoRate, "LfoRate"),
        (Param::LfoWave, "LfoWave"),
        (Param::Priority, "Priority"),
        (Param::Legato, "Legato"),
        (Param::Glide, "Glide"),
        (Param::Patch1Source, "Patch1Source"),
        (Param::Patch1Dest, "Patch1Dest"),
        (Param::Patch1Amount, "Patch1Amount"),
        (Param::Patch2Source, "Patch2Source"),
        (Param::Patch2Dest, "Patch2Dest"),
        (Param::Patch2Amount, "Patch2Amount"),
        (Param::Patch3Source, "Patch3Source"),
        (Param::Patch3Dest, "Patch3Dest"),
        (Param::Patch3Amount, "Patch3Amount"),
        (Param::Patch4Source, "Patch4Source"),
        (Param::Patch4Dest, "Patch4Dest"),
        (Param::Patch4Amount, "Patch4Amount"),
        (Param::Patch5Source, "Patch5Source"),
        (Param::Patch5Dest, "Patch5Dest"),
        (Param::Patch5Amount, "Patch5Amount"),
        (Param::Patch6Source, "Patch6Source"),
        (Param::Patch6Dest, "Patch6Dest"),
        (Param::Patch6Amount, "Patch6Amount"),
        (Param::Patch7Source, "Patch7Source"),
        (Param::Patch7Dest, "Patch7Dest"),
        (Param::Patch7Amount, "Patch7Amount"),
        (Param::Patch8Source, "Patch8Source"),
        (Param::Patch8Dest, "Patch8Dest"),
        (Param::Patch8Amount, "Patch8Amount"),
        (Param::EnvCutoff, "EnvCutoff"),
        (Param::KeyTrack, "KeyTrack"),
        (Param::Vibrato, "Vibrato"),
        (Param::ModWheel, "ModWheel"),
        (Param::Level, "Level"),
        (Param::Pan, "Pan"),
        (Param::EchoSend, "EchoSend"),
        (Param::ReverbSend, "ReverbSend"),
        (Param::Mute, "Mute"),
        (Param::Solo, "Solo"),
        (Param::DriveMode, "DriveMode"),
        (Param::DriveAmount, "DriveAmount"),
        (Param::DriveTone, "DriveTone"),
        (Param::DriveLevel, "DriveLevel"),
    ];

    /// The parameter for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Param> {
        Self::ALL
            .iter()
            .find(|(p, _)| *p as u32 == id)
            .map(|(p, _)| *p)
    }

    /// Clamp a value into this parameter's range.
    pub fn clamp(self, v: f32) -> f32 {
        let (lo, hi) = match self {
            Param::MasterGain => (0.0, 1.0),
            Param::Vco1Wave | Param::Vco2Wave | Param::Vco3Wave => (0.0, 3.0),
            Param::Vco1Coarse | Param::Vco2Coarse | Param::Vco3Coarse => (-24.0, 24.0),
            Param::Vco1Fine | Param::Vco2Fine | Param::Vco3Fine => (-50.0, 50.0),
            Param::Vco1Level | Param::Vco2Level | Param::Vco3Level => (0.0, 1.0),
            Param::PulseWidth => (0.05, 0.95),
            Param::Vco2Sync | Param::Vco3Sync => (0.0, 1.0),
            Param::NoiseLevel | Param::NoiseColour => (0.0, 1.0),
            Param::Cutoff => (20.0, 20_000.0),
            Param::Resonance | Param::Drive | Param::AdsrSustain => (0.0, 1.0),
            Param::AdsrAttack
            | Param::AdsrDecay
            | Param::AdsrRelease
            | Param::ArAttack
            | Param::ArRelease => (0.001, 10.0),
            Param::LfoRate => (0.01, 50.0),
            Param::LfoWave => (0.0, 3.0),
            Param::Priority => (0.0, 2.0),
            Param::Legato => (0.0, 1.0),
            Param::Glide => (0.0, 5.0),
            Param::Patch1Source | Param::Patch1Dest => (0.0, 255.0),
            Param::Patch2Source | Param::Patch2Dest => (0.0, 255.0),
            Param::Patch3Source | Param::Patch3Dest => (0.0, 255.0),
            Param::Patch4Source | Param::Patch4Dest => (0.0, 255.0),
            Param::Patch5Source | Param::Patch5Dest => (0.0, 255.0),
            Param::Patch6Source | Param::Patch6Dest => (0.0, 255.0),
            Param::Patch7Source | Param::Patch7Dest => (0.0, 255.0),
            Param::Patch8Source | Param::Patch8Dest => (0.0, 255.0),
            Param::Patch1Amount
            | Param::Patch2Amount
            | Param::Patch3Amount
            | Param::Patch4Amount
            | Param::Patch5Amount
            | Param::Patch6Amount
            | Param::Patch7Amount
            | Param::Patch8Amount
            | Param::EnvCutoff => (-1.0, 1.0),
            Param::KeyTrack | Param::Vibrato | Param::ModWheel => (0.0, 1.0),
            Param::Level | Param::EchoSend | Param::ReverbSend => (0.0, 1.0),
            Param::Pan => (-1.0, 1.0),
            Param::Mute | Param::Solo => (0.0, 1.0),
            Param::DriveMode => (0.0, 3.0),
            Param::DriveAmount | Param::DriveTone | Param::DriveLevel => (0.0, 1.0),
        };
        if v.is_nan() { lo } else { v.clamp(lo, hi) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for (i, (p, _)) in Param::ALL.iter().enumerate() {
            assert_eq!(Param::from_id(*p as u32), Some(*p));
            // Ids run from 0 without gaps: `param_count` and the engine's
            // value table rely on it.
            assert_eq!(*p as usize, i);
        }
        assert_eq!(Param::from_id(999), None);
    }

    #[test]
    fn clamp_rejects_nan_and_out_of_range() {
        assert_eq!(Param::MasterGain.clamp(f32::NAN), 0.0);
        assert_eq!(Param::MasterGain.clamp(7.0), 1.0);
        assert_eq!(Param::Cutoff.clamp(0.0), 20.0);
    }

    /// The `Name: id` entries of `export const {name} = { ... }`.
    fn ts_block(ts: &str, name: &str) -> Vec<(String, u32)> {
        let open = format!("export const {name} = {{");
        let body = ts
            .split(&open)
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .unwrap_or_else(|| panic!("params.ts has no `{open}`"));
        let mut entries: Vec<(String, u32)> = body
            .lines()
            .filter_map(|line| {
                let (k, v) = line.trim().trim_end_matches(',').split_once(": ")?;
                Some((k.to_string(), v.parse().ok()?))
            })
            .collect();
        entries.sort();
        entries
    }

    /// ADR-0004: every id list in Rust and its block in
    /// `web/src/audio/params.ts` hold exactly the same names and ids.
    #[test]
    fn typescript_mirror_matches() {
        use crate::fx::drive::DriveMode;
        use crate::mono::noise::NoiseColour;
        use crate::mono::osc::Waveform;
        use crate::mono::patch::{ModDest, ModSource};
        use crate::mono::preset::Preset;
        use crate::mono::voice::NotePriority;
        fn rust<T: Copy>(all: &[(T, &str)], id: impl Fn(T) -> u32) -> Vec<(String, u32)> {
            let mut v: Vec<_> = all.iter().map(|(x, n)| (n.to_string(), id(*x))).collect();
            v.sort();
            v
        }
        let ts = include_str!("../../../web/src/audio/params.ts");
        let lists = [
            ("Param", rust(&Param::ALL, |p| p as u32)),
            ("Waveform", rust(&Waveform::ALL, |w| w as u32)),
            ("NoiseColour", rust(&NoiseColour::ALL, |c| c as u32)),
            ("DriveMode", rust(&DriveMode::ALL, |m| m as u32)),
            ("Preset", rust(&Preset::ALL, |p| p as u32)),
            ("NotePriority", rust(&NotePriority::ALL, |p| p as u32)),
            ("ModSource", rust(&ModSource::ALL, |s| s as u32)),
            ("ModDest", rust(&ModDest::ALL, |d| d as u32)),
        ];
        for (name, want) in lists {
            assert_eq!(
                ts_block(ts, name),
                want,
                "params.ts `{name}` differs from Rust"
            );
        }
    }
}
