//! Mono presets (spec 004 Req 9): Rust data, selected by id.
//!
//! `DEFAULTS` gives every Mono parameter a value in the units `set_param`
//! takes; it is the voice's starting state (`MonoParams::new`,
//! `Engine::new`) and the base every preset starts from, so a preset fully
//! defines the voice, patch and normalled amounts included (spec 004 Req 7).

use crate::mono::model::Model;
use crate::params::Param;
use crate::synth::PresetDef;

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
    LaFantasia = 63,
    LaPluckPad = 64,
    LaBreathFlute = 65,
    LaRingBell = 66,
    LaThumpBass = 67,
    LaChoir = 68,
    FmElectricPiano = 69,
    FmBell = 70,
    FmBrass = 71,
    FmBass = 72,
    FmMarimba = 73,
    FmPad = 74,
    PolyStrings = 75,
    VoxHumana = 76,
    PolyFunk = 77,
    PolyBrass = 78,
    Kit808 = 79,
    TightKit = 80,
    SamplerKeys = 81,
    SamplerPad = 82,
    PadsLoud = 83,
    PadsSoft = 84,
    Kit909 = 85,
    Hard909 = 86,
    ModularBasic = 87,
    ModularHoover = 88,
    Heavy808 = 89,
    Heavy909 = 90,
    ModularKick = 91,
}

impl Preset {
    /// Every preset with the name the TypeScript mirror uses.
    pub const ALL: [(Preset, &'static str); 92] = [
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
        (Preset::LaFantasia, "LaFantasia"),
        (Preset::LaPluckPad, "LaPluckPad"),
        (Preset::LaBreathFlute, "LaBreathFlute"),
        (Preset::LaRingBell, "LaRingBell"),
        (Preset::LaThumpBass, "LaThumpBass"),
        (Preset::LaChoir, "LaChoir"),
        (Preset::FmElectricPiano, "FmElectricPiano"),
        (Preset::FmBell, "FmBell"),
        (Preset::FmBrass, "FmBrass"),
        (Preset::FmBass, "FmBass"),
        (Preset::FmMarimba, "FmMarimba"),
        (Preset::FmPad, "FmPad"),
        (Preset::PolyStrings, "PolyStrings"),
        (Preset::VoxHumana, "VoxHumana"),
        (Preset::PolyFunk, "PolyFunk"),
        (Preset::PolyBrass, "PolyBrass"),
        (Preset::Kit808, "Kit808"),
        (Preset::TightKit, "TightKit"),
        (Preset::SamplerKeys, "SamplerKeys"),
        (Preset::SamplerPad, "SamplerPad"),
        (Preset::PadsLoud, "PadsLoud"),
        (Preset::PadsSoft, "PadsSoft"),
        (Preset::Kit909, "Kit909"),
        (Preset::Hard909, "Hard909"),
        (Preset::ModularBasic, "ModularBasic"),
        (Preset::ModularHoover, "ModularHoover"),
        (Preset::Heavy808, "Heavy808"),
        (Preset::Heavy909, "Heavy909"),
        (Preset::ModularKick, "ModularKick"),
    ];

    /// The preset for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Preset> {
        Self::ALL
            .iter()
            .find(|(p, _)| *p as u32 == id)
            .map(|(p, _)| *p)
    }

    /// The model this preset is for: the one whose definition holds it.
    pub fn model(self) -> Model {
        Model::ALL
            .iter()
            .map(|(m, _)| *m)
            .find(|m| m.def().presets.iter().any(|d| d.preset == self))
            .unwrap_or_default()
    }

    /// The preset's definition, in its model's (`crate::synth`).
    fn def(self) -> Option<&'static PresetDef> {
        self.model().def().presets.iter().find(|d| d.preset == self)
    }

    /// A Modular preset's code: a SuperCollider SynthDef (ADR-0024).
    pub fn code(self) -> Option<&'static str> {
        self.def().and_then(|d| d.code)
    }

    /// What this preset changes from `DEFAULTS`.
    pub fn changes(self) -> &'static [(Param, f32)] {
        self.def().map_or(&[], |d| d.changes)
    }
}

/// Every Mono parameter's starting value: VCO 1 alone, a saw, through a
/// 4 kHz ladder, with a short attack.
pub const DEFAULTS: [(Param, f32); 443] = [
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
    (Param::Pcm1Sample, 0.0),
    (Param::Pcm2Sample, 0.0),
    (Param::Structure, 0.0),
    (Param::P2Cutoff, 4_000.0),
    (Param::P2Resonance, 0.0),
    (Param::P2EnvCutoff, 0.0),
    (Param::P2FenvAttack, 0.005),
    (Param::P2FenvDecay, 0.3),
    (Param::P2FenvSustain, 0.7),
    (Param::P2FenvRelease, 0.3),
    (Param::P2AdsrAttack, 0.005),
    (Param::P2AdsrDecay, 0.3),
    (Param::P2AdsrSustain, 0.7),
    (Param::P2AdsrRelease, 0.3),
    (Param::Op1R1, 99.0),
    (Param::Op1R2, 99.0),
    (Param::Op1R3, 99.0),
    (Param::Op1R4, 99.0),
    (Param::Op1L1, 99.0),
    (Param::Op1L2, 99.0),
    (Param::Op1L3, 99.0),
    (Param::Op1L4, 0.0),
    (Param::Op1BreakPoint, 39.0),
    (Param::Op1LeftDepth, 0.0),
    (Param::Op1RightDepth, 0.0),
    (Param::Op1LeftCurve, 0.0),
    (Param::Op1RightCurve, 0.0),
    (Param::Op1RateScale, 0.0),
    (Param::Op1AmpSens, 0.0),
    (Param::Op1VelSens, 0.0),
    (Param::Op1Level, 99.0),
    (Param::Op1Mode, 0.0),
    (Param::Op1Coarse, 1.0),
    (Param::Op1Fine, 0.0),
    (Param::Op1Detune, 7.0),
    (Param::Op2R1, 99.0),
    (Param::Op2R2, 99.0),
    (Param::Op2R3, 99.0),
    (Param::Op2R4, 99.0),
    (Param::Op2L1, 99.0),
    (Param::Op2L2, 99.0),
    (Param::Op2L3, 99.0),
    (Param::Op2L4, 0.0),
    (Param::Op2BreakPoint, 39.0),
    (Param::Op2LeftDepth, 0.0),
    (Param::Op2RightDepth, 0.0),
    (Param::Op2LeftCurve, 0.0),
    (Param::Op2RightCurve, 0.0),
    (Param::Op2RateScale, 0.0),
    (Param::Op2AmpSens, 0.0),
    (Param::Op2VelSens, 0.0),
    (Param::Op2Level, 0.0),
    (Param::Op2Mode, 0.0),
    (Param::Op2Coarse, 1.0),
    (Param::Op2Fine, 0.0),
    (Param::Op2Detune, 7.0),
    (Param::Op3R1, 99.0),
    (Param::Op3R2, 99.0),
    (Param::Op3R3, 99.0),
    (Param::Op3R4, 99.0),
    (Param::Op3L1, 99.0),
    (Param::Op3L2, 99.0),
    (Param::Op3L3, 99.0),
    (Param::Op3L4, 0.0),
    (Param::Op3BreakPoint, 39.0),
    (Param::Op3LeftDepth, 0.0),
    (Param::Op3RightDepth, 0.0),
    (Param::Op3LeftCurve, 0.0),
    (Param::Op3RightCurve, 0.0),
    (Param::Op3RateScale, 0.0),
    (Param::Op3AmpSens, 0.0),
    (Param::Op3VelSens, 0.0),
    (Param::Op3Level, 0.0),
    (Param::Op3Mode, 0.0),
    (Param::Op3Coarse, 1.0),
    (Param::Op3Fine, 0.0),
    (Param::Op3Detune, 7.0),
    (Param::Op4R1, 99.0),
    (Param::Op4R2, 99.0),
    (Param::Op4R3, 99.0),
    (Param::Op4R4, 99.0),
    (Param::Op4L1, 99.0),
    (Param::Op4L2, 99.0),
    (Param::Op4L3, 99.0),
    (Param::Op4L4, 0.0),
    (Param::Op4BreakPoint, 39.0),
    (Param::Op4LeftDepth, 0.0),
    (Param::Op4RightDepth, 0.0),
    (Param::Op4LeftCurve, 0.0),
    (Param::Op4RightCurve, 0.0),
    (Param::Op4RateScale, 0.0),
    (Param::Op4AmpSens, 0.0),
    (Param::Op4VelSens, 0.0),
    (Param::Op4Level, 0.0),
    (Param::Op4Mode, 0.0),
    (Param::Op4Coarse, 1.0),
    (Param::Op4Fine, 0.0),
    (Param::Op4Detune, 7.0),
    (Param::Op5R1, 99.0),
    (Param::Op5R2, 99.0),
    (Param::Op5R3, 99.0),
    (Param::Op5R4, 99.0),
    (Param::Op5L1, 99.0),
    (Param::Op5L2, 99.0),
    (Param::Op5L3, 99.0),
    (Param::Op5L4, 0.0),
    (Param::Op5BreakPoint, 39.0),
    (Param::Op5LeftDepth, 0.0),
    (Param::Op5RightDepth, 0.0),
    (Param::Op5LeftCurve, 0.0),
    (Param::Op5RightCurve, 0.0),
    (Param::Op5RateScale, 0.0),
    (Param::Op5AmpSens, 0.0),
    (Param::Op5VelSens, 0.0),
    (Param::Op5Level, 0.0),
    (Param::Op5Mode, 0.0),
    (Param::Op5Coarse, 1.0),
    (Param::Op5Fine, 0.0),
    (Param::Op5Detune, 7.0),
    (Param::Op6R1, 99.0),
    (Param::Op6R2, 99.0),
    (Param::Op6R3, 99.0),
    (Param::Op6R4, 99.0),
    (Param::Op6L1, 99.0),
    (Param::Op6L2, 99.0),
    (Param::Op6L3, 99.0),
    (Param::Op6L4, 0.0),
    (Param::Op6BreakPoint, 39.0),
    (Param::Op6LeftDepth, 0.0),
    (Param::Op6RightDepth, 0.0),
    (Param::Op6LeftCurve, 0.0),
    (Param::Op6RightCurve, 0.0),
    (Param::Op6RateScale, 0.0),
    (Param::Op6AmpSens, 0.0),
    (Param::Op6VelSens, 0.0),
    (Param::Op6Level, 0.0),
    (Param::Op6Mode, 0.0),
    (Param::Op6Coarse, 1.0),
    (Param::Op6Fine, 0.0),
    (Param::Op6Detune, 7.0),
    (Param::PitchR1, 99.0),
    (Param::PitchR2, 99.0),
    (Param::PitchR3, 99.0),
    (Param::PitchR4, 99.0),
    (Param::PitchL1, 50.0),
    (Param::PitchL2, 50.0),
    (Param::PitchL3, 50.0),
    (Param::PitchL4, 50.0),
    (Param::Algorithm, 0.0),
    (Param::Feedback, 0.0),
    (Param::OscSync, 1.0),
    (Param::LfoSpeed, 35.0),
    (Param::LfoDelay, 0.0),
    (Param::LfoPitchDepth, 0.0),
    (Param::LfoAmpDepth, 0.0),
    (Param::LfoSync, 1.0),
    (Param::LfoShape, 0.0),
    (Param::PitchSens, 3.0),
    (Param::Transpose, 24.0),
    (Param::BdTune, 0.0),
    (Param::BdDecay, 1.0),
    (Param::BdTone, 0.5),
    (Param::BdLevel, 0.8),
    (Param::BdDrive, 0.0),
    (Param::Ctl1, 0.0),
    (Param::Ctl2, 0.0),
    (Param::Ctl3, 0.0),
    (Param::Ctl4, 0.0),
    (Param::Ctl5, 0.0),
    (Param::Ctl6, 0.0),
    (Param::Ctl7, 0.0),
    (Param::Ctl8, 0.0),
    (Param::Ctl9, 0.0),
    (Param::Ctl10, 0.0),
    (Param::Ctl11, 0.0),
    (Param::Ctl12, 0.0),
    (Param::Ctl13, 0.0),
    (Param::Ctl14, 0.0),
    (Param::Ctl15, 0.0),
    (Param::Ctl16, 0.0),
    (Param::Ctl17, 0.0),
    (Param::Ctl18, 0.0),
    (Param::Ctl19, 0.0),
    (Param::Ctl20, 0.0),
    (Param::Ctl21, 0.0),
    (Param::Ctl22, 0.0),
    (Param::Ctl23, 0.0),
    (Param::Ctl24, 0.0),
    (Param::Ctl25, 0.0),
    (Param::Ctl26, 0.0),
    (Param::Ctl27, 0.0),
    (Param::Ctl28, 0.0),
    (Param::Ctl29, 0.0),
    (Param::Ctl30, 0.0),
    (Param::Ctl31, 0.0),
    (Param::Ctl32, 0.0),
    (Param::Vco1On, 1.0),
    (Param::Vco2On, 1.0),
    (Param::Vco3On, 1.0),
    (Param::NoiseOn, 1.0),
    (Param::GlideOn, 1.0),
    (Param::DecayRelease, 1.0),
    (Param::OscModOn, 1.0),
    (Param::FilterModOn, 1.0),
    (Param::A440, 0.0),
    (Param::FilterRev, 3.0),
    (Param::SnTune, 0.0),
    (Param::SnDecay, 1.0),
    (Param::SnTone, 0.5),
    (Param::SnLevel, 0.8),
    (Param::CpTune, 0.0),
    (Param::CpDecay, 1.0),
    (Param::CpTone, 0.5),
    (Param::CpLevel, 0.8),
    (Param::ChTune, 0.0),
    (Param::ChDecay, 1.0),
    (Param::ChTone, 0.5),
    (Param::ChLevel, 0.8),
    (Param::OhTune, 0.0),
    (Param::OhDecay, 1.0),
    (Param::OhTone, 0.5),
    (Param::OhLevel, 0.8),
    (Param::LtTune, 0.0),
    (Param::LtDecay, 1.0),
    (Param::LtTone, 0.5),
    (Param::LtLevel, 0.8),
    (Param::HtTune, 0.0),
    (Param::HtDecay, 1.0),
    (Param::HtTone, 0.5),
    (Param::HtLevel, 0.8),
    (Param::CbTune, 0.0),
    (Param::CbDecay, 1.0),
    (Param::CbTone, 0.5),
    (Param::CbLevel, 0.8),
    (Param::DrumAccent, 0.5),
    (Param::RsTune, 0.0),
    (Param::RsDecay, 1.0),
    (Param::RsTone, 0.5),
    (Param::RsLevel, 0.8),
    (Param::ClTune, 0.0),
    (Param::ClDecay, 1.0),
    (Param::ClTone, 0.5),
    (Param::ClLevel, 0.8),
    (Param::MaTune, 0.0),
    (Param::MaDecay, 1.0),
    (Param::MaTone, 0.5),
    (Param::MaLevel, 0.8),
    (Param::CyTune, 0.0),
    (Param::CyDecay, 1.0),
    (Param::CyTone, 0.5),
    (Param::CyLevel, 0.8),
    (Param::MtTune, 0.0),
    (Param::MtDecay, 1.0),
    (Param::MtTone, 0.5),
    (Param::MtLevel, 0.8),
    (Param::LcTune, 0.0),
    (Param::LcDecay, 1.0),
    (Param::LcTone, 0.5),
    (Param::LcLevel, 0.8),
    (Param::McTune, 0.0),
    (Param::McDecay, 1.0),
    (Param::McTone, 0.5),
    (Param::McLevel, 0.8),
    (Param::HcTune, 0.0),
    (Param::HcDecay, 1.0),
    (Param::HcTone, 0.5),
    (Param::HcLevel, 0.8),
    (Param::BdOut, 0.0),
    (Param::BdPan, 0.0),
    (Param::SnOut, 0.0),
    (Param::SnPan, 0.0),
    (Param::CpOut, 0.0),
    (Param::CpPan, 0.0),
    (Param::ChOut, 0.0),
    (Param::ChPan, 0.0),
    (Param::OhOut, 0.0),
    (Param::OhPan, 0.0),
    (Param::LtOut, 0.0),
    (Param::LtPan, 0.0),
    (Param::HtOut, 0.0),
    (Param::HtPan, 0.0),
    (Param::CbOut, 0.0),
    (Param::CbPan, 0.0),
    (Param::RsOut, 0.0),
    (Param::RsPan, 0.0),
    (Param::ClOut, 0.0),
    (Param::ClPan, 0.0),
    (Param::MaOut, 0.0),
    (Param::MaPan, 0.0),
    (Param::CyOut, 0.0),
    (Param::CyPan, 0.0),
    (Param::MtOut, 0.0),
    (Param::MtPan, 0.0),
    (Param::LcOut, 0.0),
    (Param::LcPan, 0.0),
    (Param::McOut, 0.0),
    (Param::McPan, 0.0),
    (Param::HcOut, 0.0),
    (Param::HcPan, 0.0),
    (Param::CrTune, 0.0),
    (Param::CrDecay, 1.0),
    (Param::CrTone, 0.5),
    (Param::CrLevel, 0.8),
    (Param::CrOut, 0.0),
    (Param::CrPan, 0.0),
    (Param::RdTune, 0.0),
    (Param::RdDecay, 1.0),
    (Param::RdTone, 0.5),
    (Param::RdLevel, 0.8),
    (Param::RdOut, 0.0),
    (Param::RdPan, 0.0),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR-0004: the view's preset list per model (`ModelDef.presets` in
    /// `web/src/audio/models.ts`, which the composer's picker offers, #213)
    /// holds exactly the presets of that model, in the engine's order.
    #[test]
    fn the_views_presets_per_model_match() {
        let ts = include_str!("../../../../web/src/audio/models.ts");
        let mut model: Option<&str> = None;
        let mut seen = Vec::new();
        for line in ts.lines() {
            if let Some(m) = line.trim().strip_prefix("id: Model.") {
                model = Some(m.trim_end_matches(','));
            }
            let Some(list) = line.trim().strip_prefix("presets: [") else {
                continue;
            };
            let Some(m) = model.take() else {
                continue;
            };
            let view: Vec<&str> = list
                .trim_end_matches("],")
                .split(',')
                .map(|w| w.trim().trim_matches('\''))
                .filter(|w| !w.is_empty())
                .collect();
            let engine: Vec<&str> = Preset::ALL
                .iter()
                .filter(|(p, _)| Model::ALL.iter().any(|(q, n)| *n == m && p.model() == *q))
                .map(|(_, n)| *n)
                .collect();
            assert_eq!(view, engine, "{m}");
            seen.push(m);
        }
        assert_eq!(
            seen.len(),
            Model::ALL.len(),
            "every model lists its presets"
        );
    }
    use crate::engine::{BLOCK, Engine};

    /// The parameters that aren't Mono's: global, or the mixer's.
    fn is_shared(p: Param) -> bool {
        p.is_global() || p.is_strip() || p.is_arp()
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
            // A sampler with nothing loaded is silent; `sampler::tests` and `padsampler::tests` play them.
            if preset.model().uses_sampler() || preset.model().uses_pads() {
                return;
            }
            for note in [24, 48, 72, 96, 108] {
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

    /// ADR-0014: the defaults a user preset starts from leave the strip alone.
    #[test]
    fn synth_defaults_reset_the_sound_not_the_strip() {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::JunoPad);
        e.set_param(0, Param::Level, 0.3);
        e.set_param(0, Param::Send2, 0.6);
        e.synth_defaults(0);
        for (p, v) in DEFAULTS {
            assert_eq!(e.param_value(0, p), v, "{p:?}");
        }
        assert_eq!(e.param_value(0, Param::Level), 0.3);
        assert_eq!(e.param_value(0, Param::Send2), 0.6);
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
    /// centre pan. Re-taken when the ladder's stages began to saturate
    /// (#306), which rounds the peaks, and when the discrete VCOs' saws were
    /// bowed and set drifting (#339), which moves the samples but not the
    /// level.
    #[test]
    fn arp_presets_keep_their_sound() {
        let gold: [(Preset, [f64; 4]); 4] = [
            (Preset::Bass, [0.111399, 0.446056, 0.030734, 0.138210]),
            (Preset::Lead, [0.166185, 0.456667, -0.269997, 0.198758]),
            (Preset::SyncLead, [0.143545, 0.328541, -0.087489, 0.202874]),
            (
                Preset::BowedString,
                [0.092674, 0.223676, -0.109591, 0.163225],
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
            if voices < 2 || preset.model().uses_sampler() || preset.model().uses_pads() {
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

    /// A preset's model is its definition's; its changes never set it.
    #[test]
    fn every_preset_sets_its_model() {
        for (preset, name) in Preset::ALL {
            assert!(
                preset.changes().iter().all(|(p, _)| *p != Param::Model),
                "{name} sets the model"
            );
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
    /// Every Modular preset's code builds (ADR-0024).
    #[test]
    fn every_modular_voice_compiles() {
        for (p, name) in Preset::ALL {
            assert_eq!(p.code().is_some(), p.model() == Model::Modular, "{name}");
            if let Some(text) = p.code() {
                let built = crate::modular::sc::compile(text);
                assert!(built.is_ok(), "{name}: {built:?}");
            }
        }
    }

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

    /// A four-note chord at the default master gain stays bounded: the voices
    /// sum before the strip, so a hot preset times four must not clip.
    #[test]
    fn a_chord_at_default_gain_stays_bounded() {
        for preset in [Preset::SyncLead, Preset::BowedString] {
            let mut e = Engine::new(48_000.0);
            e.preset(0, preset);
            for note in [48, 52, 55, 60] {
                e.note_on(0, note, 1.0);
            }
            for _ in 0..(48_000 / BLOCK) {
                e.render(BLOCK);
                for s in e.output() {
                    assert!(s.is_finite() && s.abs() <= 1.0, "{preset:?}: {s}");
                }
            }
        }
    }
}
