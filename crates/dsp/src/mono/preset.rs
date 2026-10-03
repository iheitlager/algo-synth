//! Mono presets (spec 004 Req 9): Rust data, selected by id.
//!
//! `DEFAULTS` gives every Mono parameter a value in the units `set_param`
//! takes; it is the voice's starting state (`MonoParams::new`,
//! `Engine::new`) and the base every preset starts from, so a preset fully
//! defines the voice, patch and normalled amounts included (spec 004 Req 7).

use crate::mono::model::Model;
use crate::params::Param;

/// A preset id; mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Preset {
    Bass = 0,
    Lead = 1,
    SyncLead = 2,
    BowedString = 3,
    MiniBass = 4,
    MiniLead = 5,
    ProLead = 6,
    ProBass = 7,
    Ms20Lead = 8,
    Ms20Wobble = 9,
    Cs15Brass = 10,
    Cs15Lead = 11,
    Sh101Bass = 12,
    Sh101Lead = 13,
    R2D2 = 14,
    ShArp = 15,
    SolinaStrings = 16,
    LuckyMan = 17,
    FunkBass = 18,
    MoogStrings = 19,
    SyncSweep = 20,
    PolyModBell = 21,
    ProStrings = 22,
    Ms20Squelch = 23,
    JetSweep = 24,
    Ms20Strings = 25,
    BladeBrass = 26,
    Cs15Strings = 27,
    AcidBass = 28,
    SubPluck = 29,
    Sh101Strings = 30,
    CurrieLead = 31,
    OdysseySync = 32,
    P5Brass = 33,
    P5Strings = 34,
    P5Bass = 35,
    P5SyncLead = 36,
    P5Bell = 37,
    P5Pad = 38,
    JunoPad = 39,
    JunoStrings = 40,
    JunoBrass = 41,
    JunoBass = 42,
    JunoPluck = 43,
    JunoPoly = 44,
    JupiterBrass = 45,
    JupiterStrings = 46,
    JupiterBass = 47,
    JupiterSync = 48,
    JupiterXMod = 49,
    JupiterPad = 50,
    MatrixPad = 51,
    MatrixSweep = 52,
    MatrixBrass = 53,
    MatrixPunch = 54,
    MatrixBells = 55,
    MatrixLead = 56,
    PpgSweepPad = 57,
    PpgGlassBell = 58,
    PpgFormant = 59,
    PpgPulseBass = 60,
    PpgDigitalPluck = 61,
    PpgOrganWave = 62,
}

impl Preset {
    /// Every preset with the name the TypeScript mirror uses.
    pub const ALL: [(Preset, &'static str); 63] = [
        (Preset::Bass, "Bass"),
        (Preset::Lead, "Lead"),
        (Preset::SyncLead, "SyncLead"),
        (Preset::BowedString, "BowedString"),
        (Preset::MiniBass, "MiniBass"),
        (Preset::MiniLead, "MiniLead"),
        (Preset::ProLead, "ProLead"),
        (Preset::ProBass, "ProBass"),
        (Preset::Ms20Lead, "Ms20Lead"),
        (Preset::Ms20Wobble, "Ms20Wobble"),
        (Preset::Cs15Brass, "Cs15Brass"),
        (Preset::Cs15Lead, "Cs15Lead"),
        (Preset::Sh101Bass, "Sh101Bass"),
        (Preset::Sh101Lead, "Sh101Lead"),
        (Preset::R2D2, "R2D2"),
        (Preset::ShArp, "ShArp"),
        (Preset::SolinaStrings, "SolinaStrings"),
        (Preset::LuckyMan, "LuckyMan"),
        (Preset::FunkBass, "FunkBass"),
        (Preset::MoogStrings, "MoogStrings"),
        (Preset::SyncSweep, "SyncSweep"),
        (Preset::PolyModBell, "PolyModBell"),
        (Preset::ProStrings, "ProStrings"),
        (Preset::Ms20Squelch, "Ms20Squelch"),
        (Preset::JetSweep, "JetSweep"),
        (Preset::Ms20Strings, "Ms20Strings"),
        (Preset::BladeBrass, "BladeBrass"),
        (Preset::Cs15Strings, "Cs15Strings"),
        (Preset::AcidBass, "AcidBass"),
        (Preset::SubPluck, "SubPluck"),
        (Preset::Sh101Strings, "Sh101Strings"),
        (Preset::CurrieLead, "CurrieLead"),
        (Preset::OdysseySync, "OdysseySync"),
        (Preset::P5Brass, "P5Brass"),
        (Preset::P5Strings, "P5Strings"),
        (Preset::P5Bass, "P5Bass"),
        (Preset::P5SyncLead, "P5SyncLead"),
        (Preset::P5Bell, "P5Bell"),
        (Preset::P5Pad, "P5Pad"),
        (Preset::JunoPad, "JunoPad"),
        (Preset::JunoStrings, "JunoStrings"),
        (Preset::JunoBrass, "JunoBrass"),
        (Preset::JunoBass, "JunoBass"),
        (Preset::JunoPluck, "JunoPluck"),
        (Preset::JunoPoly, "JunoPoly"),
        (Preset::JupiterBrass, "JupiterBrass"),
        (Preset::JupiterStrings, "JupiterStrings"),
        (Preset::JupiterBass, "JupiterBass"),
        (Preset::JupiterSync, "JupiterSync"),
        (Preset::JupiterXMod, "JupiterXMod"),
        (Preset::JupiterPad, "JupiterPad"),
        (Preset::MatrixPad, "MatrixPad"),
        (Preset::MatrixSweep, "MatrixSweep"),
        (Preset::MatrixBrass, "MatrixBrass"),
        (Preset::MatrixPunch, "MatrixPunch"),
        (Preset::MatrixBells, "MatrixBells"),
        (Preset::MatrixLead, "MatrixLead"),
        (Preset::PpgSweepPad, "PpgSweepPad"),
        (Preset::PpgGlassBell, "PpgGlassBell"),
        (Preset::PpgFormant, "PpgFormant"),
        (Preset::PpgPulseBass, "PpgPulseBass"),
        (Preset::PpgDigitalPluck, "PpgDigitalPluck"),
        (Preset::PpgOrganWave, "PpgOrganWave"),
    ];

    /// The preset for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Preset> {
        Self::ALL
            .iter()
            .find(|(p, _)| *p as u32 == id)
            .map(|(p, _)| *p)
    }

    /// The model this preset is for; `changes` sets it.
    pub fn model(self) -> Model {
        match self {
            Preset::PpgSweepPad
            | Preset::PpgGlassBell
            | Preset::PpgFormant
            | Preset::PpgPulseBass
            | Preset::PpgDigitalPluck
            | Preset::PpgOrganWave => Model::PpgWave,
            Preset::MatrixPad
            | Preset::MatrixSweep
            | Preset::MatrixBrass
            | Preset::MatrixPunch
            | Preset::MatrixBells
            | Preset::MatrixLead => Model::Matrix12,
            Preset::JupiterBrass
            | Preset::JupiterStrings
            | Preset::JupiterBass
            | Preset::JupiterSync
            | Preset::JupiterXMod
            | Preset::JupiterPad => Model::Jupiter8,
            Preset::JunoPad
            | Preset::JunoStrings
            | Preset::JunoBrass
            | Preset::JunoBass
            | Preset::JunoPluck
            | Preset::JunoPoly => Model::Juno106,
            Preset::P5Brass
            | Preset::P5Strings
            | Preset::P5Bass
            | Preset::P5SyncLead
            | Preset::P5Bell
            | Preset::P5Pad => Model::Prophet5,
            Preset::Bass | Preset::Lead | Preset::SyncLead | Preset::BowedString => Model::Arp2600,
            Preset::MiniBass | Preset::MiniLead => Model::Minimoog,
            Preset::ProLead | Preset::ProBass => Model::ProOne,
            Preset::Ms20Lead | Preset::Ms20Wobble => Model::Ms20,
            Preset::Cs15Brass | Preset::Cs15Lead => Model::Cs15,
            Preset::Sh101Bass | Preset::Sh101Lead => Model::Sh101,
            Preset::R2D2 | Preset::ShArp | Preset::SolinaStrings => Model::Arp2600,
            Preset::LuckyMan | Preset::FunkBass | Preset::MoogStrings => Model::Minimoog,
            Preset::SyncSweep | Preset::PolyModBell | Preset::ProStrings => Model::ProOne,
            Preset::Ms20Squelch | Preset::JetSweep | Preset::Ms20Strings => Model::Ms20,
            Preset::BladeBrass | Preset::Cs15Strings => Model::Cs15,
            Preset::AcidBass | Preset::SubPluck | Preset::Sh101Strings => Model::Sh101,
            Preset::CurrieLead | Preset::OdysseySync => Model::Odyssey,
        }
    }

    /// What this preset changes from `DEFAULTS`.
    pub fn changes(self) -> &'static [(Param, f32)] {
        use Param::*;
        match self {
            // Saw and a pulse an octave down, into a low, resonant ladder.
            Preset::Bass => &[
                (Vco2Wave, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Level, 0.7),
                (PulseWidth, 0.35),
                (Cutoff, 600.0),
                (Resonance, 0.35),
                (Drive, 0.4),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.45),
                (AdsrRelease, 0.12),
                // The filter opens with each note and follows the key.
                (EnvCutoff, 0.4),
                (KeyTrack, 0.5),
            ],
            // Two saws a few cents apart and a sub saw, a medium cutoff.
            Preset::Lead => &[
                (Vco2Fine, 7.0),
                (Vco2Level, 0.8),
                (Vco3Coarse, -12.0),
                (Vco3Level, 0.3),
                (Cutoff, 2_500.0),
                (Resonance, 0.3),
                (Drive, 0.2),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.75),
                (AdsrRelease, 0.3),
                (EnvCutoff, 0.25),
                (KeyTrack, 0.5),
                // A little vibrato, the wheel half up.
                (Vibrato, 0.15),
                (ModWheel, 0.5),
            ],
            // VCO 2 hard-synced to a silent VCO 1, an octave and a fifth up.
            // The ADSR sweeps VCO 2's pitch: the classic sync sweep.
            Preset::SyncLead => &[
                (Vco1Level, 0.0),
                (Vco2Coarse, 19.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (Cutoff, 5_000.0),
                (Resonance, 0.2),
                (Drive, 0.3),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.6),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.3),
                (Patch1Source, 5.0),
                (Patch1Dest, 2.0),
                (Patch1Amount, 0.5),
            ],
            // Two saws and a pulse an octave under, overdriven into a low
            // ladder that the contour opens: the Minimoog's bass. Low note
            // priority, as on the instrument.
            Preset::MiniBass => &[
                (Model, 1.0),
                (Vco1Coarse, -12.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 4.0),
                (Vco2Level, 1.0),
                (Vco3Wave, 1.0),
                (Vco3Coarse, -24.0),
                (Vco3Level, 0.8),
                (Cutoff, 450.0),
                (Resonance, 0.3),
                (Drive, 0.5),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.8),
                (FenvAttack, 0.002),
                (FenvDecay, 0.35),
                (FenvSustain, 0.2),
                (EnvCutoff, 0.55),
                (KeyTrack, 0.33),
                (Priority, 1.0),
                (Glide, 0.04),
            ],
            // Two saws a few cents apart and a triangle; VCO 3 in its low
            // range is the vibrato, the wheel half up. Glide, legato.
            Preset::MiniLead => &[
                (Model, 1.0),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.8),
                (Vco3Wave, 2.0),
                (Vco3Coarse, -3.0),
                (Vco3Level, 0.0),
                (Vco3Low, 1.0),
                (Vco3KeyFollow, 0.0),
                (Cutoff, 1_800.0),
                (Resonance, 0.35),
                (Drive, 0.25),
                (AdsrAttack, 0.02),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.8),
                (FenvAttack, 0.02),
                (FenvDecay, 0.5),
                (FenvSustain, 0.5),
                (EnvCutoff, 0.3),
                (KeyTrack, 0.67),
                (Vibrato, 0.2),
                (ModWheel, 0.6),
                (Glide, 0.12),
                (Legato, 1.0),
                (Priority, 1.0),
            ],
            // Oscillator A (VCO 2) synced to a silent B (VCO 1), a fifth
            // over: the filter envelope sweeps A's pitch, the classic
            // poly-mod sync lead. Low note priority, a little glide.
            Preset::ProLead => &[
                (Model, 2.0),
                (Vco1Level, 0.0),
                (Vco2Coarse, 12.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (Cutoff, 3_500.0),
                (Resonance, 0.2),
                (Drive, 0.2),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.3),
                (FenvAttack, 0.005),
                (FenvDecay, 0.7),
                (FenvSustain, 0.3),
                (EnvFreq2, 0.35),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
                (LfoWave, 2.0),
                (LfoRate, 5.5),
                (Vibrato, 0.15),
                (ModWheel, 0.5),
                (Priority, 1.0),
                (Glide, 0.03),
            ],
            // A saw an octave down and a narrow pulse (A) whose width the
            // LFO moves; full key tracking, as on the Pro-One.
            Preset::ProBass => &[
                (Model, 2.0),
                (Vco1Coarse, -12.0),
                (Vco2Wave, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 5.0),
                (Vco2Level, 0.8),
                (PulseWidth, 0.35),
                (LfoWave, 2.0),
                (LfoRate, 0.8),
                (LfoPw, 0.25),
                (Cutoff, 700.0),
                (Resonance, 0.4),
                (Drive, 0.3),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.15),
                (FenvAttack, 0.002),
                (FenvDecay, 0.3),
                (FenvSustain, 0.15),
                (EnvCutoff, 0.5),
                (KeyTrack, 1.0),
                (Priority, 1.0),
            ],
            // Two saws through a thin high-pass and a peaking low-pass that
            // the envelope pushes toward self-oscillation: the MS-20 scream.
            Preset::Ms20Lead => &[
                (Model, 3.0),
                (Vco2Fine, 8.0),
                (Vco2Level, 0.6),
                (HpCutoff, 120.0),
                (HpResonance, 0.3),
                (Cutoff, 1_800.0),
                (Resonance, 0.78),
                (Drive, 0.3),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.25),
                (FenvAttack, 0.01),
                (FenvDecay, 0.5),
                (FenvSustain, 0.4),
                (EnvCutoff, 0.35),
                (EnvHpCutoff, 0.1),
                (KeyTrack, 0.5),
                (Glide, 0.08),
                (Legato, 1.0),
            ],
            // A pulse and a saw an octave down, ring-modulated, with the
            // sample-and-hold stepping the cutoff on the patch panel.
            Preset::Ms20Wobble => &[
                (Model, 3.0),
                (Vco1Wave, 1.0),
                (Vco1Coarse, -12.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 6.0),
                (Vco2Level, 0.7),
                (RingLevel, 0.35),
                (HpCutoff, 60.0),
                (Cutoff, 500.0),
                (Resonance, 0.85),
                (Drive, 0.2),
                (AdsrAttack, 0.003),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.2),
                (LfoWave, 0.0),
                (LfoRate, 6.0),
                (Patch1Source, 8.0),
                (Patch1Dest, 5.0),
                (Patch1Amount, 0.5),
            ],
            // Two saws a few cents apart; the low-pass swells with its
            // envelope as a brass note does, the high-pass opens a little
            // with the AR: the CS-15's brass.
            Preset::Cs15Brass => &[
                (Model, 4.0),
                (Vco2Fine, 9.0),
                (Vco2Level, 0.9),
                (HpCutoff, 150.0),
                (Cutoff, 900.0),
                (Resonance, 0.35),
                (AdsrAttack, 0.08),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.85),
                (AdsrRelease, 0.2),
                (FenvAttack, 0.12),
                (FenvDecay, 0.4),
                (FenvSustain, 0.6),
                (EnvCutoff, 0.6),
                (ArAttack, 0.15),
                (EnvHpCutoff, 0.2),
                (KeyTrack, 0.4),
                (LfoWave, 2.0),
                (LfoRate, 5.5),
                (Vibrato, 0.1),
                (ModWheel, 0.3),
            ],
            // A pulse and a saw an octave up with a little ring mod, a
            // snappy low-pass and glide: a thin, vocal lead.
            Preset::Cs15Lead => &[
                (Model, 4.0),
                (Vco1Wave, 1.0),
                (PulseWidth, 0.4),
                (Vco2Coarse, 12.0),
                (Vco2Level, 0.5),
                (RingLevel, 0.2),
                (HpCutoff, 300.0),
                (HpResonance, 0.2),
                (Cutoff, 3_000.0),
                (Resonance, 0.4),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.75),
                (AdsrRelease, 0.2),
                (FenvAttack, 0.005),
                (FenvDecay, 0.25),
                (FenvSustain, 0.35),
                (EnvCutoff, 0.4),
                (ArAttack, 0.2),
                (EnvHpCutoff, 0.15),
                (Glide, 0.06),
                (Legato, 1.0),
            ],
            // Saw and pulse together, and a sub two octaves down, through a
            // resonant low-pass that the one envelope opens on every note.
            Preset::Sh101Bass => &[
                (Model, 5.0),
                (Vco1Level, 0.6),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.6),
                (SubLevel, 0.9),
                (SubOctave, 1.0),
                (Cutoff, 500.0),
                (Resonance, 0.45),
                (Drive, 0.3),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.3),
                (AdsrRelease, 0.15),
                (EnvCutoff, 0.6),
                (KeyTrack, 0.5),
            ],
            // Saw with a pulse whose width the LFO moves, a sub an octave
            // down, vibrato on the wheel, glide: the 101 lead.
            Preset::Sh101Lead => &[
                (Model, 5.0),
                (Vco1Level, 1.0),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.8),
                (SubLevel, 0.4),
                (HpCutoff, 120.0),
                (Cutoff, 2_500.0),
                (Resonance, 0.35),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.3),
                (KeyTrack, 0.5),
                (LfoWave, 2.0),
                (LfoRate, 5.5),
                (LfoPw, 0.3),
                (Vibrato, 0.12),
                (ModWheel, 1.0),
                (Glide, 0.1),
                (Legato, 1.0),
            ],
            // Fast sample-and-hold steps on both pitches and the cutoff, a
            // thin pulse and a sine an octave up: the chirping robot.
            Preset::R2D2 => &[
                (Vco1Wave, 1.0),
                (Vco1Level, 1.0),
                (PulseWidth, 0.3),
                (Vco2Wave, 3.0),
                (Vco2Coarse, 12.0),
                (Vco2Level, 0.8),
                (Cutoff, 3_500.0),
                (Resonance, 0.5),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.12),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.1),
                (LfoWave, 0.0),
                (LfoRate, 9.0),
                (Patch1Source, 8.0),
                (Patch1Dest, 1.0),
                (Patch1Amount, 0.5),
                (Patch2Source, 8.0),
                (Patch2Dest, 2.0),
                (Patch2Amount, 0.7),
                (Patch3Source, 8.0),
                (Patch3Dest, 5.0),
                (Patch3Amount, 0.3),
            ],
            // Two saws stepping to random pitches at the LFO's rate, through a
            // resonant filter the envelope opens: the sample-and-hold arpeggio.
            Preset::ShArp => &[
                (Vco2Fine, 6.0),
                (Vco2Level, 0.8),
                (Cutoff, 1_800.0),
                (Resonance, 0.55),
                (Drive, 0.2),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.15),
                (EnvCutoff, 0.3),
                (KeyTrack, 0.5),
                (LfoWave, 0.0),
                (LfoRate, 5.5),
                (Patch1Source, 8.0),
                (Patch1Dest, 1.0),
                (Patch1Amount, 0.5),
                (Patch2Source, 8.0),
                (Patch2Dest, 2.0),
                (Patch2Amount, 0.5),
            ],
            // Two detuned saws and a pulse whose width a slow LFO moves, a
            // slow attack and a long release: an ensemble's shimmer.
            Preset::SolinaStrings => &[
                (Vco1Level, 0.8),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.7),
                (Vco3Wave, 1.0),
                (Vco3Level, 0.5),
                (Cutoff, 2_800.0),
                (Resonance, 0.05),
                (AdsrAttack, 0.5),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.7),
                (EnvCutoff, 0.1),
                (KeyTrack, 0.5),
                (LfoWave, 2.0),
                (LfoRate, 0.7),
                (LfoPw, 0.35),
            ],
            // Two saws and a square, a long glide with legato and low note
            // priority, Osc 3 as the vibrato: the singing portamento solo.
            Preset::LuckyMan => &[
                (Model, 1.0),
                (Vco2Fine, 5.0),
                (Vco2Level, 0.9),
                (Vco3Wave, 2.0),
                (Vco3Coarse, -3.0),
                (Vco3Level, 0.0),
                (Vco3Low, 1.0),
                (Vco3KeyFollow, 0.0),
                (Cutoff, 3_000.0),
                (Resonance, 0.3),
                (Drive, 0.3),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.85),
                (FenvAttack, 0.01),
                (FenvDecay, 0.3),
                (FenvSustain, 0.6),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.67),
                (Vibrato, 0.15),
                (ModWheel, 0.7),
                (Glide, 0.35),
                (Legato, 1.0),
                (Priority, 1.0),
            ],
            // Three oscillators down at 16' and 32' into a low, resonant
            // filter that a short contour snaps open: a plucked funk bass.
            Preset::FunkBass => &[
                (Model, 1.0),
                (Vco1Coarse, -12.0),
                (Vco2Wave, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Level, 0.9),
                (Vco3Coarse, -24.0),
                (Vco3Level, 0.6),
                (Cutoff, 300.0),
                (Resonance, 0.45),
                (Drive, 0.4),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.22),
                (AdsrSustain, 0.6),
                (FenvAttack, 0.001),
                (FenvDecay, 0.18),
                (FenvSustain, 0.0),
                (EnvCutoff, 0.8),
                (KeyTrack, 0.67),
                (Priority, 1.0),
            ],
            // Three saws a few cents apart, a slow attack and a decay that is
            // also a long release: the Minimoog as a string section.
            Preset::MoogStrings => &[
                (Model, 1.0),
                (Vco2Fine, 8.0),
                (Vco2Level, 0.8),
                (Vco3Fine, -8.0),
                (Vco3Level, 0.8),
                (Cutoff, 1_600.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.4),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.9),
                (FenvAttack, 0.5),
                (FenvDecay, 0.6),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.25),
                (KeyTrack, 0.67),
            ],
            // A slow poly-mod sweep of the synced oscillator on every note,
            // from a high start down to the pitch: the sync sweep.
            Preset::SyncSweep => &[
                (Model, 2.0),
                (Vco1Level, 0.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (Cutoff, 6_000.0),
                (Resonance, 0.15),
                (AdsrAttack, 0.003),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.25),
                (FenvAttack, 0.003),
                (FenvDecay, 1.2),
                (FenvSustain, 0.0),
                (EnvFreq2, 0.8),
                (KeyTrack, 0.5),
                (Priority, 1.0),
            ],
            // A sine B at an inharmonic ratio frequency-modulating a sine A
            // (poly-mod), the sound dying with the loudness: a bell.
            Preset::PolyModBell => &[
                (Model, 2.0),
                (Vco1Wave, 3.0),
                (Vco1Coarse, 22.0),
                (Vco1Level, 0.0),
                (Vco2Wave, 3.0),
                (Vco2Level, 1.0),
                (OscFreq2, 0.9),
                (Cutoff, 6_000.0),
                (AdsrAttack, 0.001),
                (AdsrDecay, 1.6),
                (AdsrSustain, 0.0),
                (AdsrRelease, 1.2),
                (KeyTrack, 1.0),
            ],
            // A saw and a pulse (A) that the LFO sweeps, slightly apart, with
            // a slow attack and release.
            Preset::ProStrings => &[
                (Model, 2.0),
                (Vco1Fine, 9.0),
                (Vco1Level, 0.8),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.8),
                (LfoWave, 2.0),
                (LfoRate, 0.5),
                (LfoPw, 0.3),
                (Cutoff, 2_200.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.3),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.5),
                (FenvAttack, 0.4),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
            ],
            // A low saw into a nearly self-oscillating low-pass that a short
            // envelope throws open: the squelch.
            Preset::Ms20Squelch => &[
                (Model, 3.0),
                (Vco1Coarse, -12.0),
                (HpCutoff, 60.0),
                (Cutoff, 400.0),
                (Resonance, 0.9),
                (Drive, 0.5),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.6),
                (AdsrRelease, 0.1),
                (FenvAttack, 0.001),
                (FenvDecay, 0.22),
                (FenvSustain, 0.05),
                (EnvCutoff, 0.85),
                (KeyTrack, 0.3),
                (Glide, 0.06),
                (Legato, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 4.0),
                (Vco2Level, 0.8),
            ],
            // Noise through the high-pass and a resonant low-pass that a very
            // slow LFO sweeps: a jet going over.
            Preset::JetSweep => &[
                (Model, 3.0),
                (Vco1Level, 0.0),
                (NoiseLevel, 1.0),
                (HpCutoff, 300.0),
                (HpResonance, 0.5),
                (Cutoff, 1_200.0),
                (Resonance, 0.7),
                (AdsrAttack, 1.2),
                (AdsrDecay, 0.5),
                (AdsrSustain, 1.0),
                (AdsrRelease, 1.0),
                (LfoWave, 3.0),
                (LfoRate, 0.15),
                (LfoCutoff, 0.9),
            ],
            // Two detuned saws through a gentle high-pass and low-pass, a slow
            // attack, vibrato on the wheel.
            Preset::Ms20Strings => &[
                (Model, 3.0),
                (Vco2Fine, 10.0),
                (Vco2Level, 0.8),
                (HpCutoff, 200.0),
                (HpResonance, 0.1),
                (Cutoff, 2_400.0),
                (Resonance, 0.25),
                (AdsrAttack, 0.35),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (FenvAttack, 0.5),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.2),
                (LfoWave, 3.0),
                (LfoRate, 5.0),
                (Vibrato, 0.12),
                (ModWheel, 1.0),
            ],
            // Two saws, a slow swell of the low-pass and a little glide, with
            // vibrato: a big, slow brass.
            Preset::BladeBrass => &[
                (Model, 4.0),
                (Vco2Fine, 8.0),
                (Vco2Level, 0.9),
                (HpCutoff, 100.0),
                (Cutoff, 600.0),
                (Resonance, 0.4),
                (AdsrAttack, 0.2),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (FenvAttack, 0.35),
                (FenvDecay, 0.8),
                (FenvSustain, 0.6),
                (EnvCutoff, 0.75),
                (ArAttack, 0.3),
                (EnvHpCutoff, 0.15),
                (LfoWave, 3.0),
                (LfoRate, 5.0),
                (Vibrato, 0.1),
                (ModWheel, 1.0),
                (Glide, 0.1),
            ],
            // A saw and a pulse whose width the LFO moves, through the two
            // filters: synth strings.
            Preset::Cs15Strings => &[
                (Model, 4.0),
                (Vco2Wave, 1.0),
                (Vco2Fine, -6.0),
                (Vco2Level, 0.7),
                (LfoWave, 2.0),
                (LfoRate, 0.7),
                (LfoPw, 0.3),
                (HpCutoff, 180.0),
                (Cutoff, 2_800.0),
                (Resonance, 0.15),
                (AdsrAttack, 0.3),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.5),
                (FenvAttack, 0.3),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.15),
            ],
            // A saw into a snapping, near-self-oscillating filter, with a
            // short slide between legato notes: an acid bassline.
            Preset::AcidBass => &[
                (Model, 5.0),
                (Cutoff, 350.0),
                (Resonance, 0.85),
                (Drive, 0.4),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.25),
                (AdsrSustain, 0.5),
                (AdsrRelease, 0.1),
                (EnvCutoff, 0.8),
                (KeyTrack, 0.3),
                (Glide, 0.07),
                (Legato, 1.0),
            ],
            // The sub-oscillator carrying a short pluck of a low-pass sweep.
            Preset::SubPluck => &[
                (Model, 5.0),
                (Vco1Level, 0.6),
                (SubLevel, 1.0),
                (Cutoff, 900.0),
                (Resonance, 0.2),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.8),
                (AdsrSustain, 0.1),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.7),
                (KeyTrack, 0.5),
            ],
            // Two saws a few cents apart into a bright, slightly resonant
            // ladder driven into its saturator; legato glide and a quick
            // vibrato on the wheel: a singing late-70s Odyssey lead in the
            // manner of Billy Currie. An overdrive insert on the strip adds the
            // grit; the bends are the player's (#10).
            Preset::CurrieLead => &[
                (Model, 6.0),
                (Vco1Level, 0.9),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.8),
                (HpCutoff, 60.0),
                (Cutoff, 3_200.0),
                (Resonance, 0.35),
                (Drive, 0.45),
                (AdsrAttack, 0.008),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.3),
                (EnvCutoff, 0.25),
                (KeyTrack, 0.5),
                (LfoRate, 5.5),
                (Vibrato, 0.25),
                (ModWheel, 0.6),
                (Glide, 0.12),
                (Legato, 1.0),
            ],
            // VCO 2 hard-synced to a silent VCO 1, its pitch swept down by the
            // ADSR on every note, through an open ladder: the Odyssey's sync
            // lead.
            Preset::OdysseySync => &[
                (Model, 6.0),
                (Vco1Level, 0.0),
                (Vco2Coarse, 12.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (HpCutoff, 60.0),
                (Cutoff, 4_500.0),
                (Resonance, 0.25),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.4),
                (Vibrato, 0.2),
                (ModWheel, 0.4),
                (Patch1Source, 5.0),
                (Patch1Dest, 2.0),
                (Patch1Amount, 0.45),
            ],
            // Saw and a pulse whose width the LFO moves, a sub, a high-pass and
            // a slow attack that opens the filter with it.
            Preset::Sh101Strings => &[
                (Model, 5.0),
                (Vco1Level, 0.7),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.7),
                (SubLevel, 0.4),
                (HpCutoff, 150.0),
                (Cutoff, 2_600.0),
                (Resonance, 0.15),
                (AdsrAttack, 0.4),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (EnvCutoff, 0.15),
                (LfoWave, 2.0),
                (LfoRate, 0.5),
                (LfoPw, 0.4),
            ],
            // Two saws a few cents apart, the filter opened by its envelope on every
            // note and followed by the key: the Prophet brass. Five voices, a little drift.
            Preset::P5Brass => &[
                (Model, 7.0),
                (Polyphony, 5.0),
                (Analog, 0.5),
                (Vco1Level, 0.9),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.9),
                (Cutoff, 1_200.0),
                (Resonance, 0.25),
                (AdsrAttack, 0.06),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.25),
                (FenvAttack, 0.08),
                (FenvDecay, 0.5),
                (FenvSustain, 0.6),
                (FenvRelease, 0.3),
                (EnvCutoff, 0.55),
                (KeyTrack, 1.0),
            ],
            // A saw and a pulse whose width a slow LFO sweeps, a slow attack and a long
            // release: the Prophet string pad.
            Preset::P5Strings => &[
                (Model, 7.0),
                (Polyphony, 5.0),
                (Analog, 0.6),
                (Vco1Level, 0.8),
                (Vco1Fine, 6.0),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.8),
                (LfoWave, 2.0),
                (LfoRate, 0.5),
                (LfoPw, 0.3),
                (Cutoff, 2_400.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.4),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (FenvAttack, 0.4),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
            ],
            // Unison on all five voices, two saws an octave down, a snapping filter
            // envelope: a fat unison bass.
            Preset::P5Bass => &[
                (Model, 7.0),
                (Polyphony, 5.0),
                (Assign, 1.0),
                (UnisonDetune, 0.12),
                (Analog, 0.4),
                (Vco1Coarse, -12.0),
                (Vco1Level, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 4.0),
                (Vco2Level, 0.9),
                (Cutoff, 500.0),
                (Resonance, 0.35),
                (Drive, 0.3),
                (AdsrAttack, 0.003),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.15),
                (FenvAttack, 0.003),
                (FenvDecay, 0.3),
                (FenvSustain, 0.15),
                (EnvCutoff, 0.6),
                (KeyTrack, 1.0),
            ],
            // A synced to B with the filter envelope sweeping A, in unison with a
            // little spread: a Prophet sync lead.
            Preset::P5SyncLead => &[
                (Model, 7.0),
                (Polyphony, 5.0),
                (Assign, 1.0),
                (UnisonDetune, 0.1),
                (Analog, 0.4),
                (Vco1Level, 0.0),
                (Vco2Coarse, 12.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (Cutoff, 3_500.0),
                (Resonance, 0.2),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.3),
                (FenvAttack, 0.005),
                (FenvDecay, 0.7),
                (FenvSustain, 0.3),
                (EnvFreq2, 0.35),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
            ],
            // B frequency-modulating A through poly-mod at an inharmonic ratio, the
            // sound dying with the loudness: a poly-mod bell, five voices of it.
            Preset::P5Bell => &[
                (Model, 7.0),
                (Polyphony, 5.0),
                (Analog, 0.3),
                (Vco1Wave, 3.0),
                (Vco1Coarse, 22.0),
                (Vco1Level, 0.0),
                (Vco2Wave, 3.0),
                (Vco2Level, 1.0),
                (OscFreq2, 0.9),
                (Cutoff, 6_000.0),
                (AdsrAttack, 0.001),
                (AdsrDecay, 1.6),
                (AdsrSustain, 0.0),
                (AdsrRelease, 1.2),
                (KeyTrack, 1.0),
            ],
            // Two detuned saws, a slow swell and the LFO breathing the filter: a warm pad.
            Preset::P5Pad => &[
                (Model, 7.0),
                (Polyphony, 5.0),
                (Analog, 0.7),
                (Vco1Level, 0.8),
                (Vco1Fine, -8.0),
                (Vco2Fine, 8.0),
                (Vco2Level, 0.8),
                (Cutoff, 1_400.0),
                (Resonance, 0.2),
                (AdsrAttack, 0.8),
                (AdsrDecay, 0.8),
                (AdsrSustain, 0.85),
                (AdsrRelease, 1.0),
                (FenvAttack, 0.9),
                (FenvSustain, 0.7),
                (EnvCutoff, 0.25),
                (LfoWave, 3.0),
                (LfoRate, 0.3),
                (LfoCutoff, 0.3),
                (KeyTrack, 0.5),
            ],
            // A saw and a pulse that the LFO sweeps, a sub, a slow swell and chorus II:
            // the lush Juno pad.
            Preset::JunoPad => &[
                (Model, 8.0),
                (Polyphony, 6.0),
                (Analog, 0.5),
                (ChorusMode, 2.0),
                (Vco1Level, 0.6),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.6),
                (SubLevel, 0.3),
                (LfoWave, 2.0),
                (LfoRate, 0.6),
                (LfoPw, 0.4),
                (Cutoff, 1_800.0),
                (Resonance, 0.15),
                (AdsrAttack, 0.5),
                (AdsrDecay, 0.7),
                (AdsrSustain, 0.85),
                (AdsrRelease, 0.9),
                (EnvCutoff, 0.15),
                (KeyTrack, 0.5),
            ],
            // Saw and a swept pulse with both chorus buttons in: the fast, shallow
            // shimmer of the Juno strings.
            Preset::JunoStrings => &[
                (Model, 8.0),
                (Polyphony, 6.0),
                (Analog, 0.5),
                (ChorusMode, 3.0),
                (Vco1Level, 0.9),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.5),
                (LfoWave, 2.0),
                (LfoRate, 0.6),
                (LfoPw, 0.3),
                (Cutoff, 3_000.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.25),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.5),
                (EnvCutoff, 0.1),
                (KeyTrack, 0.5),
            ],
            // A saw, a pulse and a sub through a filter the one envelope sweeps on
            // every note, with chorus I: the Juno brass.
            Preset::JunoBrass => &[
                (Model, 8.0),
                (Polyphony, 6.0),
                (Analog, 0.4),
                (ChorusMode, 1.0),
                (Vco1Level, 1.0),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.6),
                (SubLevel, 0.3),
                (Cutoff, 900.0),
                (Resonance, 0.3),
                (AdsrAttack, 0.05),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.25),
                (EnvCutoff, 0.6),
                (KeyTrack, 0.5),
            ],
            // The sub-oscillator at full level under a saw and a pulse, no chorus:
            // the Juno bass.
            Preset::JunoBass => &[
                (Model, 8.0),
                (Polyphony, 6.0),
                (Analog, 0.3),
                (Vco1Level, 0.7),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.5),
                (SubLevel, 1.0),
                (Cutoff, 600.0),
                (Resonance, 0.3),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.35),
                (AdsrSustain, 0.5),
                (AdsrRelease, 0.2),
                (EnvCutoff, 0.5),
                (KeyTrack, 0.5),
            ],
            // A narrow pulse with PWM and a filter that closes quickly, chorus I: a
            // plucked Juno.
            Preset::JunoPluck => &[
                (Model, 8.0),
                (Polyphony, 6.0),
                (Analog, 0.4),
                (ChorusMode, 1.0),
                (Vco1Level, 0.0),
                (Vco2Wave, 1.0),
                (Vco2Level, 0.9),
                (SubLevel, 0.3),
                (PulseWidth, 0.3),
                (LfoWave, 2.0),
                (LfoRate, 1.2),
                (LfoPw, 0.35),
                (Cutoff, 1_500.0),
                (Resonance, 0.4),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.45),
                (AdsrSustain, 0.0),
                (AdsrRelease, 0.3),
                (EnvCutoff, 0.55),
                (KeyTrack, 0.5),
            ],
            // A bright saw with a little resonance and the high-pass in its first step,
            // chorus II: the classic poly-synth chord sound.
            Preset::JunoPoly => &[
                (Model, 8.0),
                (Polyphony, 6.0),
                (Analog, 0.5),
                (ChorusMode, 2.0),
                (Vco1Level, 1.0),
                (HpCutoff, 240.0),
                (Cutoff, 2_200.0),
                (Resonance, 0.45),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.6),
                (AdsrRelease, 0.3),
                (EnvCutoff, 0.3),
                (KeyTrack, 0.5),
            ],
            // Two saws a few cents apart, the filter opened by its envelope on every
            // note: the Jupiter brass.
            Preset::JupiterBrass => &[
                (Model, 9.0),
                (Polyphony, 8.0),
                (Analog, 0.4),
                (Vco1Level, 0.9),
                (Vco2Fine, 8.0),
                (Vco2Level, 0.9),
                (Cutoff, 1_000.0),
                (Resonance, 0.2),
                (Slope, 1.0),
                (AdsrAttack, 0.05),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.25),
                (FenvAttack, 0.07),
                (FenvDecay, 0.5),
                (FenvSustain, 0.55),
                (EnvCutoff, 0.6),
                (KeyTrack, 0.6),
            ],
            // A saw and a pulse swept by the LFO, a slow attack and the filter at
            // 12 dB: a soft string pad.
            Preset::JupiterStrings => &[
                (Model, 9.0),
                (Polyphony, 8.0),
                (Analog, 0.5),
                (Vco1Level, 0.8),
                (Vco2Wave, 1.0),
                (Vco2Fine, 6.0),
                (Vco2Level, 0.7),
                (LfoWave, 2.0),
                (LfoRate, 0.5),
                (LfoPw, 0.3),
                (Cutoff, 2_600.0),
                (Resonance, 0.1),
                (Slope, 0.0),
                (AdsrAttack, 0.35),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (FenvAttack, 0.4),
                (FenvSustain, 0.8),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
            ],
            // Unison on all eight voices, two saws an octave down, a 24 dB filter
            // snapping shut: the big Jupiter bass.
            Preset::JupiterBass => &[
                (Model, 9.0),
                (Polyphony, 8.0),
                (Assign, 1.0),
                (UnisonDetune, 0.15),
                (Analog, 0.4),
                (Vco1Coarse, -12.0),
                (Vco1Level, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 5.0),
                (Vco2Level, 0.9),
                (Cutoff, 450.0),
                (Resonance, 0.3),
                (Slope, 1.0),
                (Drive, 0.3),
                (AdsrAttack, 0.003),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.15),
                (FenvAttack, 0.003),
                (FenvDecay, 0.3),
                (FenvSustain, 0.15),
                (EnvCutoff, 0.65),
                (KeyTrack, 1.0),
            ],
            // VCO 2 synced to VCO 1 and swept by the filter envelope through
            // cross-modulation: the Jupiter sync lead, in unison.
            Preset::JupiterSync => &[
                (Model, 9.0),
                (Polyphony, 8.0),
                (Assign, 1.0),
                (UnisonDetune, 0.1),
                (Analog, 0.4),
                (Vco1Level, 0.0),
                (Vco2Coarse, 7.0),
                (Vco2Level, 1.0),
                (Vco2Sync, 1.0),
                (EnvFreq2, 0.45),
                (Cutoff, 3_800.0),
                (Resonance, 0.2),
                (Slope, 1.0),
                (AdsrAttack, 0.005),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.3),
                (FenvAttack, 0.005),
                (FenvDecay, 0.6),
                (FenvSustain, 0.3),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
            ],
            // VCO 2 frequency-modulating VCO 1: a metallic, clangorous sound that
            // the envelope tames.
            Preset::JupiterXMod => &[
                (Model, 9.0),
                (Polyphony, 8.0),
                (Analog, 0.4),
                (Vco1Wave, 3.0),
                (Vco1Level, 0.9),
                (Vco2Wave, 3.0),
                (Vco2Coarse, 7.0),
                (Vco2Level, 0.0),
                (XMod, 0.6),
                (Cutoff, 5_000.0),
                (Resonance, 0.15),
                (Slope, 1.0),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.9),
                (AdsrSustain, 0.1),
                (AdsrRelease, 0.6),
                (FenvAttack, 0.002),
                (FenvDecay, 0.5),
                (FenvSustain, 0.2),
                (EnvCutoff, 0.3),
                (KeyTrack, 1.0),
            ],
            // Two saws, a slow swell, and the LFO breathing the filter: a warm pad.
            Preset::JupiterPad => &[
                (Model, 9.0),
                (Polyphony, 8.0),
                (Analog, 0.6),
                (Vco1Level, 0.8),
                (Vco1Fine, -9.0),
                (Vco2Fine, 9.0),
                (Vco2Level, 0.8),
                (Cutoff, 1_500.0),
                (Resonance, 0.2),
                (Slope, 1.0),
                (AdsrAttack, 0.9),
                (AdsrDecay, 0.8),
                (AdsrSustain, 0.85),
                (AdsrRelease, 1.1),
                (FenvAttack, 1.0),
                (FenvSustain, 0.7),
                (EnvCutoff, 0.25),
                (LfoWave, 3.0),
                (LfoRate, 0.35),
                (LfoCutoff, 0.3),
                (KeyTrack, 0.5),
            ],
            // A saw and a pulse, a second LFO moving the pulse width and breathing the
            // filter through the matrix: a slow, wide pad.
            Preset::MatrixPad => &[
                (Model, 10.0),
                (Polyphony, 12.0),
                (Analog, 0.5),
                (Vco1Level, 0.8),
                (Vco2Wave, 1.0),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.7),
                (Cutoff, 1_800.0),
                (Resonance, 0.15),
                (Slope, 1.0),
                (AdsrAttack, 0.6),
                (AdsrDecay, 0.7),
                (AdsrSustain, 0.85),
                (AdsrRelease, 1.0),
                (FenvAttack, 0.7),
                (FenvSustain, 0.7),
                (EnvCutoff, 0.2),
                (KeyTrack, 0.5),
                (Lfo2Rate, 0.3),
                (Lfo2Wave, 2.0),
                (Patch1Source, 13.0),
                (Patch1Dest, 4.0),
                (Patch1Amount, 0.35),
                (Patch2Source, 13.0),
                (Patch2Dest, 5.0),
                (Patch2Amount, 0.18),
            ],
            // A long ramp from every note's start into the cutoff and resonance: the
            // filter sweeps open and the resonance rises as the note ages.
            Preset::MatrixSweep => &[
                (Model, 10.0),
                (Polyphony, 12.0),
                (Analog, 0.4),
                (Vco1Level, 0.9),
                (Vco2Fine, 6.0),
                (Vco2Level, 0.8),
                (Cutoff, 500.0),
                (Resonance, 0.2),
                (Slope, 1.0),
                (AdsrAttack, 0.05),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.6),
                (EnvCutoff, 0.0),
                (KeyTrack, 0.6),
                (RampTime, 3.0),
                (Patch1Source, 14.0),
                (Patch1Dest, 5.0),
                (Patch1Amount, 0.7),
                (Patch2Source, 14.0),
                (Patch2Dest, 6.0),
                (Patch2Amount, 0.25),
            ],
            // Two saws, the filter opened by its envelope and by how hard the key is
            // struck, through the matrix: velocity-sensitive brass.
            Preset::MatrixBrass => &[
                (Model, 10.0),
                (Polyphony, 12.0),
                (Analog, 0.4),
                (Vco1Level, 0.9),
                (Vco2Fine, 8.0),
                (Vco2Level, 0.9),
                (Cutoff, 900.0),
                (Resonance, 0.2),
                (Slope, 1.0),
                (AdsrAttack, 0.05),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.25),
                (FenvAttack, 0.07),
                (FenvDecay, 0.5),
                (FenvSustain, 0.55),
                (EnvCutoff, 0.5),
                (KeyTrack, 0.6),
                (Patch1Source, 10.0),
                (Patch1Dest, 5.0),
                (Patch1Amount, 0.4),
            ],
            // A short third envelope bending the pitch into every note, a sub-heavy
            // saw and a snapping filter: a punchy bass.
            Preset::MatrixPunch => &[
                (Model, 10.0),
                (Polyphony, 12.0),
                (Analog, 0.3),
                (Vco1Coarse, -12.0),
                (Vco1Level, 1.0),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 4.0),
                (Vco2Level, 0.8),
                (Cutoff, 500.0),
                (Resonance, 0.3),
                (Slope, 1.0),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.6),
                (AdsrRelease, 0.15),
                (FenvAttack, 0.002),
                (FenvDecay, 0.3),
                (FenvSustain, 0.1),
                (EnvCutoff, 0.6),
                (KeyTrack, 1.0),
                (ArAttack, 0.001),
                (ArRelease, 0.07),
                (Patch1Source, 6.0),
                (Patch1Dest, 1.0),
                (Patch1Amount, 0.5),
                (Patch2Source, 6.0),
                (Patch2Dest, 2.0),
                (Patch2Amount, 0.5),
            ],
            // VCO 2 frequency-modulating VCO 1 with the third envelope shaping the
            // modulation through the matrix: a struck, clangorous bell.
            Preset::MatrixBells => &[
                (Model, 10.0),
                (Polyphony, 12.0),
                (Analog, 0.3),
                (Vco1Wave, 3.0),
                (Vco1Level, 0.9),
                (Vco2Wave, 3.0),
                (Vco2Coarse, 19.0),
                (Vco2Level, 0.0),
                (XMod, 0.5),
                (Cutoff, 6_000.0),
                (Resonance, 0.1),
                (Slope, 1.0),
                (AdsrAttack, 0.001),
                (AdsrDecay, 1.8),
                (AdsrSustain, 0.0),
                (AdsrRelease, 1.4),
                (KeyTrack, 1.0),
            ],
            // Unison with a second LFO as the vibrato, delayed by nothing but the wheel:
            // a singing mono-style lead on all twelve voices.
            Preset::MatrixLead => &[
                (Model, 10.0),
                (Polyphony, 12.0),
                (Assign, 1.0),
                (UnisonDetune, 0.12),
                (Analog, 0.4),
                (Vco1Level, 0.9),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.9),
                (Cutoff, 2_800.0),
                (Resonance, 0.3),
                (Slope, 1.0),
                (AdsrAttack, 0.01),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.8),
                (AdsrRelease, 0.3),
                (FenvAttack, 0.01),
                (FenvSustain, 0.6),
                (EnvCutoff, 0.25),
                (KeyTrack, 0.5),
                (Lfo2Rate, 5.5),
                (Lfo2Wave, 3.0),
                (Patch1Source, 13.0),
                (Patch1Dest, 1.0),
                (Patch1Amount, 0.025),
                (Patch2Source, 13.0),
                (Patch2Dest, 2.0),
                (Patch2Amount, 0.025),
            ],
            // The Sweep table slowly opened by the filter envelope and breathed by the
            // LFO, crossfaded, with chorus II: the sweeping digital pad.
            Preset::PpgSweepPad => &[
                (Model, 11.0),
                (Polyphony, 8.0),
                (Analog, 0.5),
                (ChorusMode, 2.0),
                (Wt1Table, 0.0),
                (Wt1Pos, 0.15),
                (Vco1Level, 0.8),
                (Wt2Table, 0.0),
                (Wt2Pos, 0.25),
                (Vco2Fine, 7.0),
                (Vco2Level, 0.7),
                (EnvWt, 0.7),
                (LfoWt, 0.1),
                (Cutoff, 3_000.0),
                (Resonance, 0.15),
                (AdsrAttack, 0.9),
                (AdsrDecay, 0.8),
                (AdsrSustain, 0.85),
                (AdsrRelease, 1.0),
                (FenvAttack, 1.4),
                (FenvDecay, 1.0),
                (FenvSustain, 0.6),
                (EnvCutoff, 0.15),
                (KeyTrack, 0.5),
                (LfoWave, 2.0),
                (LfoRate, 0.3),
            ],
            // The Bell table thrown open at the strike and falling back as the note
            // dies, in digital steps: a glassy bell.
            Preset::PpgGlassBell => &[
                (Model, 11.0),
                (Polyphony, 8.0),
                (Analog, 0.3),
                (WtSteps, 1.0),
                (Wt1Table, 7.0),
                (Wt1Pos, 0.2),
                (Vco1Level, 0.9),
                (Wt2Table, 7.0),
                (Wt2Pos, 0.1),
                (Vco2Coarse, 12.0),
                (Vco2Level, 0.5),
                (EnvWt, 0.7),
                (Cutoff, 7_000.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.001),
                (AdsrDecay, 1.8),
                (AdsrSustain, 0.0),
                (AdsrRelease, 1.3),
                (FenvAttack, 0.001),
                (FenvDecay, 1.2),
                (FenvSustain, 0.0),
                (EnvCutoff, 0.1),
                (KeyTrack, 1.0),
            ],
            // The Formant table swept by a slow LFO from oo to ee: a vocal, breathing
            // choir pad.
            Preset::PpgFormant => &[
                (Model, 11.0),
                (Polyphony, 8.0),
                (Analog, 0.5),
                (ChorusMode, 1.0),
                (Wt1Table, 2.0),
                (Wt1Pos, 0.4),
                (Vco1Level, 0.9),
                (Wt2Table, 2.0),
                (Wt2Pos, 0.45),
                (Vco2Fine, 6.0),
                (Vco2Level, 0.7),
                (LfoWt, 0.35),
                (Cutoff, 4_000.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.5),
                (AdsrDecay, 0.6),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.8),
                (KeyTrack, 0.5),
                (LfoWave, 2.0),
                (LfoRate, 0.4),
            ],
            // A pulse table that narrows with the envelope into a low, closing filter:
            // a hollow digital bass.
            Preset::PpgPulseBass => &[
                (Model, 11.0),
                (Polyphony, 8.0),
                (Analog, 0.3),
                (Wt1Table, 1.0),
                (Wt1Pos, 0.1),
                (Vco1Coarse, -12.0),
                (Vco1Level, 1.0),
                (Wt2Table, 5.0),
                (Wt2Pos, 0.2),
                (Vco2Coarse, -12.0),
                (Vco2Fine, 5.0),
                (Vco2Level, 0.7),
                (EnvWt, 0.5),
                (Cutoff, 700.0),
                (Resonance, 0.3),
                (Drive, 0.2),
                (AdsrAttack, 0.002),
                (AdsrDecay, 0.4),
                (AdsrSustain, 0.7),
                (AdsrRelease, 0.15),
                (FenvAttack, 0.002),
                (FenvDecay, 0.3),
                (FenvSustain, 0.1),
                (EnvCutoff, 0.6),
                (KeyTrack, 1.0),
            ],
            // The seeded Digital table in steps, closing quickly: a gritty, stepped
            // pluck.
            Preset::PpgDigitalPluck => &[
                (Model, 11.0),
                (Polyphony, 8.0),
                (Analog, 0.3),
                (WtSteps, 1.0),
                (Wt1Table, 6.0),
                (Wt1Pos, 0.6),
                (Vco1Level, 1.0),
                (Wt2Table, 6.0),
                (Wt2Pos, 0.3),
                (Vco2Coarse, 12.0),
                (Vco2Level, 0.5),
                (EnvWt, -0.5),
                (Cutoff, 2_500.0),
                (Resonance, 0.25),
                (AdsrAttack, 0.001),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.0),
                (AdsrRelease, 0.3),
                (FenvAttack, 0.001),
                (FenvDecay, 0.35),
                (FenvSustain, 0.0),
                (EnvCutoff, 0.5),
                (KeyTrack, 0.8),
            ],
            // The Organ table with the LFO shifting its drawbars, chorus I: a moving,
            // wave-organ pad.
            Preset::PpgOrganWave => &[
                (Model, 11.0),
                (Polyphony, 8.0),
                (Analog, 0.4),
                (ChorusMode, 1.0),
                (Wt1Table, 4.0),
                (Wt1Pos, 0.5),
                (Vco1Level, 0.9),
                (Wt2Table, 4.0),
                (Wt2Pos, 0.2),
                (Vco2Coarse, 12.0),
                (Vco2Level, 0.4),
                (LfoWt, 0.4),
                (Cutoff, 5_000.0),
                (Resonance, 0.05),
                (AdsrAttack, 0.02),
                (AdsrDecay, 0.3),
                (AdsrSustain, 0.9),
                (AdsrRelease, 0.3),
                (KeyTrack, 0.5),
                (LfoWave, 2.0),
                (LfoRate, 0.7),
            ],
            // Three detuned saws and a breath of pink noise for the bow,
            // a slow attack and a long release: the ensemble's start.
            Preset::BowedString => &[
                (Vco1Level, 0.8),
                (Vco2Fine, 6.0),
                (Vco2Level, 0.7),
                (Vco3Fine, -5.0),
                (Vco3Level, 0.6),
                (NoiseColour, 1.0),
                (NoiseLevel, 0.05),
                (Cutoff, 1_800.0),
                (Resonance, 0.1),
                (AdsrAttack, 0.25),
                (AdsrDecay, 0.5),
                (AdsrSustain, 0.85),
                (AdsrRelease, 0.6),
                // Vibrato at 5.5 Hz, the wheel all the way up; bow pressure
                // (velocity) brightens the tone.
                (LfoRate, 5.5),
                (Vibrato, 0.1),
                (ModWheel, 1.0),
                (KeyTrack, 0.6),
                (EnvCutoff, 0.15),
                (Patch1Source, 10.0),
                (Patch1Dest, 5.0),
                (Patch1Amount, 0.2),
            ],
        }
    }
}

/// Every Mono parameter's starting value: VCO 1 alone, a saw, through a
/// 4 kHz ladder, with a short attack.
pub const DEFAULTS: [(Param, f32); 132] = [
    (Param::Vco1Wave, 0.0),
    (Param::Vco1Coarse, 0.0),
    (Param::Vco1Fine, 0.0),
    (Param::Vco1Level, 1.0),
    (Param::Vco2Wave, 0.0),
    (Param::Vco2Coarse, 0.0),
    (Param::Vco2Fine, 0.0),
    (Param::Vco2Level, 0.0),
    (Param::Vco3Wave, 0.0),
    (Param::Vco3Coarse, 0.0),
    (Param::Vco3Fine, 0.0),
    (Param::Vco3Level, 0.0),
    (Param::PulseWidth, 0.5),
    (Param::Vco2Sync, 0.0),
    (Param::Vco3Sync, 0.0),
    (Param::NoiseLevel, 0.0),
    (Param::NoiseColour, 0.0),
    (Param::Cutoff, 4_000.0),
    (Param::Resonance, 0.0),
    (Param::Drive, 0.0),
    (Param::AdsrAttack, 0.005),
    (Param::AdsrDecay, 0.3),
    (Param::AdsrSustain, 0.7),
    (Param::AdsrRelease, 0.3),
    (Param::ArAttack, 0.005),
    (Param::ArRelease, 0.3),
    (Param::LfoRate, 4.0),
    (Param::LfoWave, 3.0),
    (Param::Priority, 0.0),
    (Param::Legato, 0.0),
    (Param::Glide, 0.0),
    (Param::Patch1Source, 0.0),
    (Param::Patch1Dest, 0.0),
    (Param::Patch1Amount, 0.0),
    (Param::Patch2Source, 0.0),
    (Param::Patch2Dest, 0.0),
    (Param::Patch2Amount, 0.0),
    (Param::Patch3Source, 0.0),
    (Param::Patch3Dest, 0.0),
    (Param::Patch3Amount, 0.0),
    (Param::Patch4Source, 0.0),
    (Param::Patch4Dest, 0.0),
    (Param::Patch4Amount, 0.0),
    (Param::Patch5Source, 0.0),
    (Param::Patch5Dest, 0.0),
    (Param::Patch5Amount, 0.0),
    (Param::Patch6Source, 0.0),
    (Param::Patch6Dest, 0.0),
    (Param::Patch6Amount, 0.0),
    (Param::Patch7Source, 0.0),
    (Param::Patch7Dest, 0.0),
    (Param::Patch7Amount, 0.0),
    (Param::Patch8Source, 0.0),
    (Param::Patch8Dest, 0.0),
    (Param::Patch8Amount, 0.0),
    (Param::EnvCutoff, 0.0),
    (Param::KeyTrack, 0.0),
    (Param::Vibrato, 0.0),
    (Param::ModWheel, 0.0),
    (Param::Model, 0.0),
    (Param::FenvAttack, 0.005),
    (Param::FenvDecay, 0.3),
    (Param::FenvSustain, 0.7),
    (Param::FenvRelease, 0.3),
    (Param::HpCutoff, 20.0),
    (Param::HpResonance, 0.0),
    (Param::EnvHpCutoff, 0.0),
    (Param::RingLevel, 0.0),
    (Param::SubLevel, 0.0),
    (Param::SubOctave, 0.0),
    (Param::Vco3KeyFollow, 1.0),
    (Param::Vco3Low, 0.0),
    (Param::LfoCutoff, 0.0),
    (Param::LfoPw, 0.0),
    (Param::EnvFreq2, 0.0),
    (Param::OscFreq2, 0.0),
    (Param::EnvPw, 0.0),
    (Param::OscPw, 0.0),
    (Param::OscCutoff, 0.0),
    (Param::Polyphony, 1.0),
    (Param::Assign, 0.0),
    (Param::UnisonDetune, 0.3),
    (Param::Analog, 0.0),
    (Param::ChorusMode, 0.0),
    (Param::XMod, 0.0),
    (Param::Slope, 1.0),
    (Param::Lfo2Rate, 2.0),
    (Param::Lfo2Wave, 2.0),
    (Param::RampTime, 1.0),
    (Param::Patch9Source, 0.0),
    (Param::Patch9Dest, 0.0),
    (Param::Patch9Amount, 0.0),
    (Param::Patch10Source, 0.0),
    (Param::Patch10Dest, 0.0),
    (Param::Patch10Amount, 0.0),
    (Param::Patch11Source, 0.0),
    (Param::Patch11Dest, 0.0),
    (Param::Patch11Amount, 0.0),
    (Param::Patch12Source, 0.0),
    (Param::Patch12Dest, 0.0),
    (Param::Patch12Amount, 0.0),
    (Param::Patch13Source, 0.0),
    (Param::Patch13Dest, 0.0),
    (Param::Patch13Amount, 0.0),
    (Param::Patch14Source, 0.0),
    (Param::Patch14Dest, 0.0),
    (Param::Patch14Amount, 0.0),
    (Param::Patch15Source, 0.0),
    (Param::Patch15Dest, 0.0),
    (Param::Patch15Amount, 0.0),
    (Param::Patch16Source, 0.0),
    (Param::Patch16Dest, 0.0),
    (Param::Patch16Amount, 0.0),
    (Param::Patch17Source, 0.0),
    (Param::Patch17Dest, 0.0),
    (Param::Patch17Amount, 0.0),
    (Param::Patch18Source, 0.0),
    (Param::Patch18Dest, 0.0),
    (Param::Patch18Amount, 0.0),
    (Param::Patch19Source, 0.0),
    (Param::Patch19Dest, 0.0),
    (Param::Patch19Amount, 0.0),
    (Param::Patch20Source, 0.0),
    (Param::Patch20Dest, 0.0),
    (Param::Patch20Amount, 0.0),
    (Param::Wt1Table, 0.0),
    (Param::Wt1Pos, 0.0),
    (Param::Wt2Table, 0.0),
    (Param::Wt2Pos, 0.0),
    (Param::WtSteps, 0.0),
    (Param::EnvWt, 0.0),
    (Param::LfoWt, 0.0),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{BLOCK, Engine};

    /// The parameters that aren't Mono's: global, or the mixer's.
    fn is_shared(p: Param) -> bool {
        p.is_global() || p.is_strip()
    }

    #[test]
    fn defaults_cover_every_mono_parameter_once() {
        for (p, _) in Param::ALL {
            let n = DEFAULTS.iter().filter(|(d, _)| *d == p).count();
            let want = usize::from(!is_shared(p));
            assert_eq!(n, want, "{p:?} in DEFAULTS {n} times");
        }
    }

    #[test]
    fn every_value_is_in_range() {
        let changes = Preset::ALL.iter().flat_map(|(p, _)| p.changes());
        for (p, v) in DEFAULTS.iter().chain(changes) {
            assert_eq!(p.clamp(*v), *v, "{p:?} = {v} is out of range");
            assert!(!is_shared(*p), "a preset sets {p:?}");
        }
    }

    /// Run `check` on every preset, spread over the machine's threads: each check
    /// renders seconds of audio, and a failure in any thread fails the test.
    fn for_every_preset(check: impl Fn(Preset, &str) + Sync) {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let per = Preset::ALL.len().div_ceil(threads);
        std::thread::scope(|scope| {
            for chunk in Preset::ALL.chunks(per) {
                let check = &check;
                scope.spawn(move || {
                    for (preset, name) in chunk {
                        check(*preset, name);
                    }
                });
            }
        });
    }

    #[test]
    fn every_preset_is_bounded() {
        for_every_preset(|preset, name| {
            for note in [24, 48, 72, 96] {
                let mut e = Engine::new(48_000.0);
                e.set_param(0, Param::MasterGain, 1.0);
                e.preset(0, preset);
                e.note_on(0, note, 1.0);
                let mut heard = 0.0_f32;
                for i in 0..(48_000 * 5 / 2 / BLOCK) {
                    if i == 48_000 / BLOCK {
                        e.note_off(0, note);
                    }
                    e.render(BLOCK);
                    for s in e.output() {
                        assert!(s.is_finite() && s.abs() <= 1.0, "{name} {note}: {s}");
                        heard = heard.max(s.abs());
                    }
                }
                assert!(heard > 0.02, "{name} {note} is silent");
                assert_eq!(e.active_voices(), 0, "{name} {note} still sounds");
            }
        });
    }

    #[test]
    fn a_preset_sets_every_mono_parameter() {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::Bass);
        e.preset(0, Preset::Lead);
        for (p, v) in DEFAULTS {
            let want = Preset::Lead
                .changes()
                .iter()
                .find(|(c, _)| *c == p)
                .map_or(v, |(_, c)| *c);
            assert_eq!(e.param_value(0, p), want, "{p:?}");
        }
    }

    /// No leftovers: a preset after another sounds like it does from fresh.
    #[test]
    fn a_preset_after_another_renders_like_a_fresh_one() {
        let render = |presets: &[Preset]| {
            let mut e = Engine::new(48_000.0);
            for p in presets {
                e.preset(0, *p);
            }
            e.note_on(0, 60, 1.0);
            let mut out = Vec::new();
            for _ in 0..50 {
                e.render(BLOCK);
                out.extend_from_slice(e.output());
            }
            out
        };
        // After the first, the middle and the last preset of the list: one of
        // each kind of leftover, without rendering every pair.
        let n = Preset::ALL.len();
        let befores = [Preset::ALL[0].0, Preset::ALL[n / 2].0, Preset::ALL[n - 1].0];
        for (preset, name) in Preset::ALL {
            let fresh = render(&[preset]);
            for before in befores {
                assert!(
                    render(&[before, preset]) == fresh,
                    "{name} after {before:?}"
                );
            }
        }
    }

    /// The ARP 2600 voice sounds as it did before models (spec 005 Req 2):
    /// rms, peak and two samples of half a second of A3, per preset, from the
    /// last release before the model was added, then scaled by the mixer's
    /// centre pan.
    #[test]
    fn arp_presets_keep_their_sound() {
        let gold: [(Preset, [f64; 4]); 4] = [
            (Preset::Bass, [0.117948, 0.479209, 0.040528, 0.162096]),
            (Preset::Lead, [0.169469, 0.478455, -0.246227, -0.055599]),
            (Preset::SyncLead, [0.150151, 0.386691, -0.188180, -0.129467]),
            (
                Preset::BowedString,
                [0.093402, 0.229120, -0.096020, 0.048850],
            ),
        ];
        for (preset, want) in gold {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::MasterGain, 1.0);
            e.preset(0, preset);
            e.note_on(0, 57, 0.8);
            let mut out = Vec::new();
            for _ in 0..(48_000 / 2 / BLOCK) {
                e.render(BLOCK);
                out.extend_from_slice(e.output().get(..BLOCK).unwrap_or(&[]));
            }
            let rms =
                (out.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / out.len() as f64).sqrt();
            let peak = out.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
            let got = [
                rms,
                f64::from(peak),
                f64::from(out[7000]),
                f64::from(out[15000]),
            ];
            // The mixer's centre pan is equal power (spec 002 Req 2): x 1/sqrt 2.
            for (g, w) in got
                .iter()
                .zip(want.map(|w| w * std::f64::consts::FRAC_1_SQRT_2))
            {
                assert!((g - w).abs() < 2.0e-5, "{preset:?}: {got:?} vs {want:?}");
            }
        }
    }

    /// Spec 006 Req 15: every polyphonic preset plays a full chord, bounded,
    /// audible, and silent once released.
    #[test]
    fn every_poly_preset_plays_a_full_chord() {
        for_every_preset(|preset, name| {
            let mut probe = Engine::new(48_000.0);
            probe.preset(0, preset);
            let voices =
                (probe.param_value(0, Param::Polyphony) as usize).min(preset.model().voices());
            if voices < 2 {
                return;
            }
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::MasterGain, 1.0);
            e.preset(0, preset);
            let chord: Vec<u8> = (0..voices).map(|k| (40 + 5 * k).min(100) as u8).collect();
            for n in &chord {
                e.note_on(0, *n, 1.0);
            }
            let mut heard = 0.0_f32;
            for i in 0..(48_000 * 3 / BLOCK) {
                if i == 24_000 / BLOCK {
                    for n in &chord {
                        e.note_off(0, *n);
                    }
                }
                e.render(BLOCK);
                for s in e.output() {
                    assert!(s.is_finite() && s.abs() <= 1.0, "{name}: {s}");
                    heard = heard.max(s.abs());
                }
            }
            assert!(heard > 0.05, "{name} is silent");
            assert_eq!(e.active_voices(), 0, "{name} still sounds");
        });
    }

    #[test]
    fn every_preset_sets_its_model() {
        for (preset, name) in Preset::ALL {
            let mut e = Engine::new(48_000.0);
            e.preset(0, preset);
            assert_eq!(
                e.param_value(0, Param::Model),
                preset.model() as u32 as f32,
                "{name}"
            );
        }
    }

    /// Spec 005 Req 1: every model has at least two presets of its own.
    #[test]
    fn every_model_has_at_least_two_presets() {
        for (model, name) in Model::ALL {
            let n = Preset::ALL
                .iter()
                .filter(|(p, _)| p.model() == model)
                .count();
            assert!(n >= 2, "{name} has {n} presets");
        }
    }

    /// A new or reset synth is an ARP 2600, and a preset after another
    /// model's leaves nothing of it.
    #[test]
    fn a_new_synth_is_an_arp_2600() {
        let mut e = Engine::new(48_000.0);
        assert_eq!(e.param_value(5, Param::Model), 0.0);
        e.set_param(5, Param::Model, 3.0);
        assert_eq!(e.param_value(5, Param::Model), 3.0);
        e.reset(5);
        assert_eq!(e.param_value(5, Param::Model), 0.0);
        e.set_param(5, Param::Model, 3.0);
        e.preset(5, Preset::Bass);
        assert_eq!(e.param_value(5, Param::Model), 0.0);
    }

    #[test]
    fn unknown_ids_are_none() {
        assert_eq!(Preset::from_id(99), None);
    }
}
