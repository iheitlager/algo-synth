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
    /// Test-voice attack time in seconds, 0.001..=5.
    Attack = 1,
    /// Test-voice release time in seconds, 0.005..=10.
    Release = 2,
    /// VCO 1 waveform id (`Waveform`), 0..=3.
    Vco1Wave = 3,
    /// VCO 1 coarse tune in semitones, −24..=24.
    Vco1Coarse = 4,
    /// VCO 1 fine tune in cents, −50..=50.
    Vco1Fine = 5,
    /// VCO 1 level into the mixer, 0..=1.
    Vco1Level = 6,
    /// VCO 2 waveform id (`Waveform`), 0..=3.
    Vco2Wave = 7,
    /// VCO 2 coarse tune in semitones, −24..=24.
    Vco2Coarse = 8,
    /// VCO 2 fine tune in cents, −50..=50.
    Vco2Fine = 9,
    /// VCO 2 level into the mixer, 0..=1.
    Vco2Level = 10,
    /// VCO 3 waveform id (`Waveform`), 0..=3.
    Vco3Wave = 11,
    /// VCO 3 coarse tune in semitones, −24..=24.
    Vco3Coarse = 12,
    /// VCO 3 fine tune in cents, −50..=50.
    Vco3Fine = 13,
    /// VCO 3 level into the mixer, 0..=1.
    Vco3Level = 14,
    /// Pulse width of every VCO, 0.05..=0.95.
    PulseWidth = 15,
    /// VCO 2 hard-syncs to VCO 1 when ≥ 0.5.
    Vco2Sync = 16,
    /// VCO 3 hard-syncs to VCO 1 when ≥ 0.5.
    Vco3Sync = 17,
    /// Noise level into the mixer, 0..=1.
    NoiseLevel = 18,
    /// Noise colour id (`NoiseColour`), 0..=1.
    NoiseColour = 19,
    /// Ladder cutoff in Hz, 20..=20000.
    Cutoff = 20,
    /// Ladder resonance, 0..=1; it self-oscillates from 0.8.
    Resonance = 21,
    /// Ladder drive into the saturator, 0..=1 (0 to +18 dB).
    Drive = 22,
    /// ADSR attack, decay and release in seconds, 0.001..=10.
    AdsrAttack = 23,
    AdsrDecay = 24,
    /// ADSR sustain level, 0..=1.
    AdsrSustain = 25,
    AdsrRelease = 26,
    /// AR attack and release in seconds, 0.001..=10.
    ArAttack = 27,
    ArRelease = 28,
    /// LFO rate in Hz, 0.01..=50.
    LfoRate = 29,
    /// LFO waveform id (`Waveform`; pulse is the square), 0..=3.
    LfoWave = 30,
}

impl Param {
    /// Every parameter with the name the TypeScript mirror uses.
    pub const ALL: [(Param, &'static str); 31] = [
        (Param::MasterGain, "MasterGain"),
        (Param::Attack, "Attack"),
        (Param::Release, "Release"),
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
            Param::Attack => (0.001, 5.0),
            Param::Release => (0.005, 10.0),
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
        };
        if v.is_nan() { lo } else { v.clamp(lo, hi) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for (p, _) in Param::ALL {
            assert_eq!(Param::from_id(p as u32), Some(p));
        }
        assert_eq!(Param::from_id(999), None);
    }

    #[test]
    fn clamp_rejects_nan_and_out_of_range() {
        assert_eq!(Param::MasterGain.clamp(f32::NAN), 0.0);
        assert_eq!(Param::MasterGain.clamp(7.0), 1.0);
        assert_eq!(Param::Attack.clamp(0.0), 0.001);
    }

    /// ADR-0004: the TypeScript mirror names every id exactly as Rust does.
    #[test]
    fn typescript_mirror_matches() {
        let ts = include_str!("../../../web/src/audio/params.ts");
        for (p, name) in Param::ALL {
            let line = format!("{name}: {},", p as u32);
            assert!(ts.contains(&line), "web/src/audio/params.ts lacks `{line}`");
        }
    }
}
