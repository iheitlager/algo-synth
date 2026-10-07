//! The synth models (spec 005, ADR-0009): which instrument a Mono synth is.
//!
//! One shared voice serves every model; what differs is each model's
//! definition in `crate::synth` (ADR-0025), read here as small `Copy`
//! answers `MonoVoice::render` matches on (ADR-0002: no boxing, no
//! allocation). This file keeps the ids and the filter voicings the
//! definitions share. Every parameter exists on every model; the panel shows
//! what the instrument has and the presets set the rest to neutral values.

use crate::mono::env::EnvTimes;
use crate::mono::ladder::MAX_K;
use crate::synth::Engine;

/// A synth model id; mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Model {
    #[default]
    Arp2600 = 0,
    Minimoog = 1,
    ProOne = 2,
    Ms20 = 3,
    Cs15 = 4,
    Sh101 = 5,
    Odyssey = 6,
    Prophet5 = 7,
    Juno106 = 8,
    Jupiter8 = 9,
    Matrix12 = 10,
    PpgWave = 11,
    D50 = 12,
    Dx7 = 13,
    PolyMoog = 14,
    /// The drum kit: eighteen synthesized pads, one voice each (#114, #148).
    Tr808 = 15,
    /// The multisampler (`sampler`): zones of the sample store.
    Sampler = 16,
    /// The drum/pad sampler (`padsampler`): sixteen pads playing samples.
    PadSampler = 17,
    /// The TR-909 (#148): the drum kit's pads with the 909's sounds.
    Tr909 = 18,
    /// A voice written in the song as a graph of unit generators (ADR-0020).
    Modular = 19,
}

impl Model {
    /// Every model with the name the TypeScript mirror uses.
    pub const ALL: [(Model, &'static str); 20] = [
        (Model::Arp2600, "Arp2600"),
        (Model::Minimoog, "Minimoog"),
        (Model::ProOne, "ProOne"),
        (Model::Ms20, "Ms20"),
        (Model::Cs15, "Cs15"),
        (Model::Sh101, "Sh101"),
        (Model::Odyssey, "Odyssey"),
        (Model::Prophet5, "Prophet5"),
        (Model::Juno106, "Juno106"),
        (Model::Jupiter8, "Jupiter8"),
        (Model::Matrix12, "Matrix12"),
        (Model::PpgWave, "PpgWave"),
        (Model::D50, "D50"),
        (Model::Dx7, "Dx7"),
        (Model::PolyMoog, "PolyMoog"),
        (Model::Tr808, "Tr808"),
        (Model::Sampler, "Sampler"),
        (Model::PadSampler, "PadSampler"),
        (Model::Tr909, "Tr909"),
        (Model::Modular, "Modular"),
    ];

    /// The model for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<Model> {
        Self::ALL
            .iter()
            .find(|(m, _)| *m as u32 == id)
            .map(|(m, _)| *m)
    }
}

/// How a ladder is voiced (spec 004 Req 13): the same filter, set up as the
/// instrument had it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LadderVoicing {
    /// Multiplies the drive into the saturator.
    pub drive: f32,
    /// How much of the bass resonance would take away is given back, per
    /// unit of feedback: 0 loses it as the Moog ladder does.
    pub comp: f32,
    /// Multiplies the feedback, so a voicing can stop short of the full
    /// resonance range.
    pub k_scale: f32,
    /// Where the ladder saturates besides its input.
    pub stages: Stages,
    /// Where on the resonance knob (0..1) the filter starts to whistle
    /// (#342): the feedback reaches the self-oscillation threshold there and
    /// the voicing's full feedback at the top. 0.8 with `k_scale` 1 is a
    /// straight line, as every model had before.
    pub onset: f32,
    /// A one-pole high-pass in the feedback path, as a MIDI note (0 for
    /// none): the AC coupling of a resonance loop, which takes the low end out
    /// of the resonance and stops the whistle at the lowest cutoffs (#342).
    pub loop_hp: f32,
}

/// The feedback at which a 4-pole ladder starts to self-oscillate.
pub const ONSET_K: f32 = 4.0;

impl LadderVoicing {
    /// The feedback for a resonance knob at `r` (0..1, clamped) (#342): up
    /// to the threshold at `onset`, on to `k_scale` of the full range at 1.
    pub fn feedback(&self, r: f32) -> f32 {
        let r = if r.is_nan() { 0.0 } else { r.clamp(0.0, 1.0) };
        let (rise, slope) = self.taper();
        if r <= self.onset {
            rise * r
        } else {
            ONSET_K + (r - self.onset) * slope
        }
    }

    /// The taper's slopes: feedback per unit of knob below the onset, and
    /// above it up to `k_scale` of the full range.
    pub fn taper(&self) -> (f32, f32) {
        let top = (MAX_K * self.k_scale).max(ONSET_K);
        (ONSET_K / self.onset, (top - ONSET_K) / (1.0 - self.onset))
    }
}

/// What saturates inside a 4-pole ladder's stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stages {
    /// Nothing: four linear one-poles after the input saturator.
    Linear,
    /// Each stage's differential pair, `g·(tanh(in) − tanh(out))`: the Moog
    /// transistor ladder (Huovilainen, DAFx 2004; #306).
    Transistor,
    /// Each OTA's input pair, `g·tanh(in − out)`: Roland's IR3109 cascade
    /// (#305).
    Ota,
    /// The CEM3320 (#321): transconductance stages as `Ota`, and resonance
    /// through its own VCA, which clips the feedback instead of the input.
    /// Input and resonance no longer squash each other, and that VCA sets
    /// the level of the self-oscillation.
    Cem3320,
    /// The SSM2040 (#321): transconductance stages with a wider linear
    /// range than the IR3109's, the feedback summed into the input as the
    /// external op-amp does.
    Ssm2040,
}

/// How a 12 dB state-variable filter is voiced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvfVoicing {
    /// The resonance (0..=1) at which the damping reaches 0 and the filter
    /// self-oscillates; above 1 it never does.
    pub osc_at: f32,
    /// The least damping, negative where the oscillation may grow into the
    /// saturator.
    pub k_min: f32,
    /// Where the saturating state clips: smaller is harder.
    pub ceiling: f32,
}

pub const MOOG: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.0,
    k_scale: 1.0,
    stages: Stages::Transistor,
    onset: 0.8,
    loop_hp: 0.0,
};
/// The Pro-One's CEM3320.
pub const PRO_ONE: LadderVoicing = LadderVoicing {
    drive: 1.1,
    comp: 0.3,
    k_scale: 1.0,
    stages: Stages::Cem3320,
    // Estimates (#342): where it starts to whistle on the knob.
    onset: 0.85,
    loop_hp: 0.0,
};
/// The Prophet-5 Rev 3's CEM3320: a little cleaner than the Pro-One's.
pub const PROPHET5_REV3: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.25,
    k_scale: 1.0,
    stages: Stages::Cem3320,
    // Estimates (#342): where it starts to whistle on the knob.
    onset: 0.85,
    loop_hp: 0.0,
};
/// The Prophet-5 Rev 1/2's SSM2040: fat, the bass kept under resonance,
/// a softer resonance short of the full range.
pub const PROPHET5_REV12: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.5,
    k_scale: 0.94,
    stages: Stages::Ssm2040,
    // Estimates (#342): where it starts to whistle on the knob.
    onset: 0.88,
    loop_hp: 0.0,
};
/// Roland's IR3109 OTA cascade: soft, a little bass kept, short of the full
/// range.
pub const SH101: LadderVoicing = LadderVoicing {
    drive: 0.7,
    comp: 0.15,
    k_scale: 0.95,
    stages: Stages::Ota,
    // Estimates (#342): where it starts to whistle on the knob, and a loop high-pass near 10 Hz.
    onset: 0.9,
    loop_hp: 3.5,
};
/// The Juno-106's 80017A: the IR3109's die trimmed a little hotter, with
/// more of the bass kept and a stronger whistle.
pub const JUNO106: LadderVoicing = LadderVoicing {
    drive: 0.75,
    comp: 0.2,
    k_scale: 0.97,
    stages: Stages::Ota,
    // Estimates (#342): where it starts to whistle on the knob, and a loop high-pass near 10 Hz.
    onset: 0.93,
    loop_hp: 3.5,
};
/// The Odyssey Rev 3's ARP 4075: brighter and cleaner than the Moog, a
/// little bass kept under resonance, short of the full range.
pub const ODYSSEY: LadderVoicing = LadderVoicing {
    drive: 0.85,
    comp: 0.2,
    k_scale: 0.97,
    stages: Stages::Linear,
    onset: 0.8,
    loop_hp: 0.0,
};
/// The Jupiter-8's four-pole, an IR3109: clean and a little bass kept under
/// resonance.
pub const JUPITER: LadderVoicing = LadderVoicing {
    drive: 0.9,
    comp: 0.25,
    k_scale: 0.98,
    stages: Stages::Ota,
    // Estimates (#342): where it starts to whistle on the knob, and a loop high-pass near 10 Hz.
    onset: 0.9,
    loop_hp: 3.5,
};
/// Its two-pole setting: resonant but short of oscillating.
pub const JUPITER12: SvfVoicing = SvfVoicing {
    osc_at: 1.1,
    k_min: 0.08,
    ceiling: 1.0,
};
/// The Matrix-12's four-pole: clean, with the bass kept under resonance.
pub const MATRIX: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.35,
    k_scale: 1.0,
    stages: Stages::Linear,
    // Estimates (#342): where it starts to whistle on the knob.
    onset: 0.85,
    loop_hp: 0.0,
};
/// Its two-pole setting: smooth, short of oscillating.
pub const MATRIX12: SvfVoicing = SvfVoicing {
    osc_at: 1.05,
    k_min: 0.05,
    ceiling: 1.1,
};
/// The PPG Wave's four-pole (an SSM-style ladder): clean, bass kept.
pub const PPG: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.2,
    k_scale: 1.0,
    stages: Stages::Linear,
    // Estimates (#342): where it starts to whistle on the knob.
    onset: 0.85,
    loop_hp: 0.0,
};
/// The D-50's partial filters: clean, a little bass kept.
pub const D50: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.2,
    k_scale: 0.98,
    stages: Stages::Linear,
    // Estimates (#342): where it starts to whistle on the knob.
    onset: 0.9,
    loop_hp: 0.0,
};
/// The Polymoog's resonator filter: strongly resonant, vocal rather than screaming.
pub const POLYMOOG: SvfVoicing = SvfVoicing {
    osc_at: 1.05,
    k_min: 0.04,
    ceiling: 1.0,
};
/// Sharp, and screaming at the top of the knob.
pub const MS20: SvfVoicing = SvfVoicing {
    osc_at: 0.9,
    k_min: -0.06,
    ceiling: 0.5,
};
/// The Odyssey Rev 1's ARP 4023, a two-pole: resonant, short of
/// oscillating.
pub const ODYSSEY_REV1: SvfVoicing = SvfVoicing {
    osc_at: 1.08,
    k_min: 0.06,
    ceiling: 1.0,
};
/// The Odyssey Rev 2's ARP 4035, a copy of the Moog transistor ladder.
pub const ODYSSEY_REV2: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.0,
    k_scale: 1.0,
    stages: Stages::Transistor,
    onset: 0.8,
    loop_hp: 0.0,
};
/// Resonant and smooth, never quite oscillating.
pub const CS15: SvfVoicing = SvfVoicing {
    osc_at: 1.15,
    k_min: 0.1,
    ceiling: 1.2,
};

/// How a model's oscillators are voiced (spec 004 Req 16, #339): how far
/// `Analog` lets them wander, and the shape of their waves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OscVoicing {
    /// Scales `Analog`'s static detune of each voice: 0 for a
    /// crystal-locked DCO or a digital oscillator.
    pub detune: f32,
    /// Scales `Analog`'s slow drift.
    pub drift: f32,
    /// Rounds the triangle's corners, 0 sharp to 1 flat at the peaks.
    pub tri_round: f32,
    /// Bows the saw's ramp as a charging capacitor does, 0 straight.
    pub saw_bend: f32,
}

/// Today's oscillator: `Analog` as spec 006 Req 3 has it, ideal waves. The
/// voicing of a model whose oscillators are not voiced.
pub const IDEAL_VCO: OscVoicing = OscVoicing {
    detune: 1.0,
    drift: 1.0,
    tri_round: 0.0,
    saw_bend: 0.0,
};
/// Discrete VCOs (Minimoog, ARP 2600 and Odyssey modules, MS-20, CS-15,
/// Jupiter-8): the most drift, rounded triangles, bowed saws.
pub const DISCRETE_VCO: OscVoicing = OscVoicing {
    detune: 1.0,
    drift: 1.5,
    tri_round: 0.3,
    saw_bend: 0.08,
};
/// The CEM3340 and CEM3374 (SH-101, Pro-One, Prophet-5 Rev 3, Matrix-12):
/// temperature-compensated, steadier and cleaner.
pub const CEM_VCO: OscVoicing = OscVoicing {
    detune: 0.6,
    drift: 0.5,
    tri_round: 0.15,
    saw_bend: 0.03,
};
/// A crystal-clocked DCO (Juno-106), a digital oscillator (PPG) or a
/// divide-down organ core (Polymoog): every voice in tune, ideal waves.
pub const LOCKED: OscVoicing = OscVoicing {
    detune: 0.0,
    drift: 0.0,
    tri_round: 0.0,
    saw_bend: 0.0,
};

impl OscVoicing {
    /// Whether wave `w` is shaped at all: per block, so a sample of an
    /// unshaped wave costs nothing.
    pub fn shapes(&self, w: crate::mono::osc::Waveform) -> bool {
        use crate::mono::osc::Waveform;
        match w {
            Waveform::Triangle => self.tri_round > 0.0,
            Waveform::Saw => self.saw_bend > 0.0,
            _ => false,
        }
    }

    /// A band-limited oscillator's output `y` of wave `w`, shaped. Two
    /// multiplies and an add; `y` passes untouched at 0.
    pub fn shape(&self, w: crate::mono::osc::Waveform, y: f32) -> f32 {
        use crate::mono::osc::Waveform;
        match w {
            // y·(1 + r/2) − r/2·y³ keeps ±1 at the peaks, where its slope
            // falls to 1 − r.
            Waveform::Triangle if self.tri_round > 0.0 => {
                let h = 0.5 * self.tri_round;
                y * (1.0 + h - h * y * y)
            }
            // A bow of (1 − y²) less its mean, 2/3: both ends move alike, so
            // the reset is still a step of 2 and no DC is added.
            Waveform::Saw if self.saw_bend > 0.0 => y + self.saw_bend * (1.0 / 3.0 - y * y),
            _ => y,
        }
    }
}

/// How a model's envelopes are voiced (spec 004 Req 17, #340): the curve of
/// the attack, each segment's shortest and longest time, and how high the
/// sustain goes. It shapes the ADSR and the filter envelope; the AR keeps
/// the knobs' full range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvVoicing {
    /// How far past its peak the attack aims: 0.3 is an RC charge (the
    /// CEM3310 charges toward 1.3× its peak), larger is straighter, as an
    /// envelope a CPU writes.
    pub attack_aim: f64,
    /// The shortest and longest attack, decay and release, in seconds.
    pub attack: [f32; 2],
    pub decay: [f32; 2],
    pub release: [f32; 2],
    /// The highest sustain, as a fraction of the peak.
    pub sustain_max: f32,
}

/// Today's envelope: an RC curve over the full range of the knobs. The
/// voicing of a model whose envelopes are not voiced.
pub const RC_ENV: EnvVoicing = EnvVoicing {
    attack_aim: 0.3,
    attack: [0.001, 10.0],
    decay: [0.001, 10.0],
    release: [0.001, 10.0],
    sustain_max: 1.0,
};
/// The CEM3310 (Prophet-5 Rev 3, Pro-One): an RC attack toward 1.3× its
/// peak, from 2 ms.
pub const CEM3310: EnvVoicing = EnvVoicing {
    attack: [0.002, 10.0],
    decay: [0.002, 10.0],
    release: [0.002, 10.0],
    ..RC_ENV
};

impl EnvVoicing {
    /// `t` (samples, at `sample_rate`) within the voicing's ranges, the
    /// sustain under its ceiling.
    pub fn clamp(&self, t: &EnvTimes, sample_rate: f32) -> EnvTimes {
        let within = |x: f32, [lo, hi]: [f32; 2]| x.clamp(lo * sample_rate, hi * sample_rate);
        EnvTimes {
            attack: within(t.attack, self.attack),
            decay: within(t.decay, self.decay),
            sustain: t.sustain * self.sustain_max,
            release: within(t.release, self.release),
        }
    }
}

/// How a model's VCA is voiced (spec 004 Req 18, #341): how hard its input
/// rounds off, and whether its envelope drives an exponential control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VcaVoicing {
    /// Drive into an OTA's input pair before its gain: 0 is a clean,
    /// linear multiply.
    pub sat: f32,
    /// The envelope drives an exponential control input, so a decay falls
    /// evenly in decibels.
    pub expo: bool,
}

/// Today's VCA: a clean, linear multiply. The voicing of a model whose
/// VCA is not voiced, and of the CEM3360 and CEM3372 used linearly.
pub const CLEAN_VCA: VcaVoicing = VcaVoicing {
    sat: 0.0,
    expo: false,
};
/// Roland's BA662 OTA (SH-101, Juno-106, Jupiter-8): a hot signal rounds
/// off.
pub const BA662: VcaVoicing = VcaVoicing {
    sat: 0.5,
    expo: false,
};
/// The CA3280 OTA (Prophet-5 Rev 3): a little cleaner than the BA662.
pub const CA3280: VcaVoicing = VcaVoicing {
    sat: 0.4,
    expo: false,
};
/// The decibels an exponential VCA spans over its envelope's travel.
const EXPO_DB: f32 = 60.0;

impl VcaVoicing {
    /// The gain for an envelope level `vca` in 0..=1: `vca` itself on a
    /// linear control; on an exponential one `EXPO_DB` of travel, put back
    /// to 0 at 0 and 1 at 1. A polynomial `exp2`, no transcendental.
    pub fn gain(&self, vca: f32) -> f32 {
        if !self.expo {
            return vca;
        }
        const OCTAVES: f32 = EXPO_DB / 6.020_6;
        let floor = crate::modular::fast_exp2(-OCTAVES);
        let g = crate::modular::fast_exp2(OCTAVES * (vca - 1.0));
        ((g - floor) / (1.0 - floor)).max(0.0)
    }

    /// An OTA's drive and its inverse, worked out once per block; `None`
    /// for a clean VCA.
    pub fn ota(&self) -> Option<(f32, f32)> {
        (self.sat > 0.0).then(|| (self.sat, 1.0 / self.sat))
    }

    /// The signal `y` through the VCA's input: an OTA's input pair rounds
    /// a hot signal off, about `tanh(y·sat)/sat`; a clean VCA passes it.
    pub fn input(&self, y: f32) -> f32 {
        match self.ota() {
            Some((k, inv)) => ota(y, k, inv),
            None => y,
        }
    }
}

/// `y` through an OTA of drive `k` (`inv` = 1/k): a cubic soft clip, flat
/// from ±1.5, smooth at the knee. Multiplies only, no division per sample.
pub fn ota(y: f32, k: f32, inv: f32) -> f32 {
    let x = (y * k).clamp(-1.5, 1.5);
    (x - (4.0 / 27.0) * x * x * x) * inv
}

/// A panel's filter switches: the slope switch at 12 dB (`Param::Slope`)
/// and the revision switch (`Param::FilterRev`, 1..=3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setting {
    pub slope12: bool,
    pub rev: u8,
}

impl Setting {
    /// The filter a model has by itself: 24 dB, and Rev 3, which is the own
    /// filter of both models with the revision switch.
    pub const OWN: Setting = Setting {
        slope12: false,
        rev: 3,
    };
    /// The slope switch at 12 dB.
    pub const SLOPE12: Setting = Setting {
        slope12: true,
        ..Setting::OWN
    };

    /// The revision switch at `rev`.
    pub const fn rev(rev: u8) -> Setting {
        Setting {
            rev,
            ..Setting::OWN
        }
    }

    /// Every setting a switch gives, past `OWN`.
    pub const SWITCHED: [Setting; 3] = [Setting::SLOPE12, Setting::rev(1), Setting::rev(2)];
}

/// The low-pass a model uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    Ladder(LadderVoicing),
    Svf(SvfVoicing),
}

/// The high-pass stage of a model, if it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hp {
    None,
    /// 12 dB with resonance, before the low-pass.
    Svf,
    /// 6 dB, after the low-pass.
    OnePole,
}

/// What each model decides, read from its definition (`crate::synth`, #330).
impl Model {
    /// How many voices the instrument has at most (spec 006 Req 1): the
    /// polyphonic models are limited to their own count, the monosynths to the
    /// pool's.
    pub fn voices(self) -> usize {
        self.def().voices
    }

    /// How its oscillators are voiced (#339).
    pub fn osc(self) -> OscVoicing {
        self.def().osc
    }

    /// How its envelopes are voiced (#340).
    pub fn env(self) -> EnvVoicing {
        self.def().env
    }

    /// How its VCA is voiced (#341).
    pub fn vca(self) -> VcaVoicing {
        self.def().vca
    }

    /// The filter and its voicing.
    pub fn filter(self) -> Filter {
        self.def().filter
    }

    /// The low-pass at the panel's switches (#321): the 12 dB setting of a
    /// model with the slope switch (Jupiter-8, Matrix-12), a revision of one
    /// with the revision switch (the Prophet-5 Rev 4's SSM2040 at Rev 1/2,
    /// the Odyssey reissue's 4023 and 4035 at Rev 1 and 2); `filter` where
    /// no switch applies.
    pub fn low_pass(self, s: Setting) -> Filter {
        self.def().low_pass(s)
    }

    /// Whether the voice is the DX7's six-operator FM voice (spec 006 Req 13).
    pub fn uses_fm(self) -> bool {
        self.def().engine == Engine::Fm
    }

    /// Whether the synth is the drum kit, whose keys hit pads (#114).
    pub fn uses_drums(self) -> bool {
        matches!(self.def().engine, Engine::Drums(_))
    }

    /// The drum machine a kit model is; the 808 for any other.
    pub fn drum_machine(self) -> crate::drums::Machine {
        match self.def().engine {
            Engine::Drums(m) => m,
            _ => crate::drums::Machine::Tr808,
        }
    }

    /// Whether the voice is the D-50's two-partial LA voice (spec 006 Req 12).
    pub fn uses_la(self) -> bool {
        self.def().engine == Engine::La
    }

    /// Whether the voice is a graph of unit generators (ADR-0020).
    pub fn uses_graph(self) -> bool {
        self.def().engine == Engine::Graph
    }

    /// Whether its voices are the Mono voice, monophonic or in a poly pool:
    /// the voices that read per-voice values (ADR-0023).
    pub fn uses_mono_voice(self) -> bool {
        self.def().engine == Engine::Mono
    }

    /// Whether the synth is the drum/pad sampler (#124).
    pub fn uses_pads(self) -> bool {
        self.def().engine == Engine::Pads
    }

    /// Whether the voice is the multisampler's (#123).
    pub fn uses_sampler(self) -> bool {
        self.def().engine == Engine::Sampler
    }

    /// Whether VCO 1 and VCO 2 are wavetable oscillators (spec 006 Req 11).
    pub fn uses_tables(self) -> bool {
        self.def().uses_tables
    }

    /// Whether the voice runs the second LFO and the ramp, the sources the
    /// Matrix-12's modulation matrix adds.
    pub fn has_matrix(self) -> bool {
        self.def().has_matrix
    }

    /// The high-pass stage.
    pub fn hp(self) -> Hp {
        self.def().hp
    }

    /// Whether the filter envelope source (`ModSource::Fenv`, so the poly-mod
    /// and high-pass envelope amounts) is the ADSR: the SH-101 has one
    /// envelope for filter and loudness.
    pub fn filter_env_is_adsr(self) -> bool {
        self.def().filter_env_is_adsr
    }

    /// Whether VCO 2 is the pulse output of VCO 1: phase-locked to it and at
    /// its pitch, as the SH-101's one oscillator gives saw and pulse together.
    pub fn pulse_locked(self) -> bool {
        self.def().pulse_locked
    }

    /// Whether a time set as decay is also the release, on the loudness and
    /// the filter ADSR: the Minimoog's contours have no release knob.
    pub fn decay_is_release(self) -> bool {
        self.def().decay_is_release
    }

    /// Whether VCO 3 is the modulation source of the normals (vibrato,
    /// `LfoCutoff`) instead of the LFO: the Minimoog has no LFO, Osc 3 in
    /// its low range does that job.
    pub fn modulates_with_osc3(self) -> bool {
        self.def().modulates_with_osc3
    }

    /// Whether the high-pass cutoff follows the AR envelope (the CS-15's
    /// own envelope for it) rather than the filter ADSR.
    pub fn hp_follows_ar(self) -> bool {
        self.def().hp_follows_ar
    }

    /// Whether the normalled cutoff follows the filter ADSR. The ARP 2600,
    /// the SH-101 and the Odyssey (whose ADSR is normalled to both filter
    /// and VCA; its AR is a patch source) follow the ADSR (spec 004 Req 12).
    pub fn cutoff_follows_filter_env(self) -> bool {
        self.def().cutoff_follows_filter_env
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for (i, (m, _)) in Model::ALL.iter().enumerate() {
            assert_eq!(Model::from_id(*m as u32), Some(*m));
            assert_eq!(*m as usize, i, "ids run from 0 without gaps");
        }
        assert_eq!(Model::from_id(99), None);
        assert_eq!(Model::default(), Model::Arp2600);
    }

    /// #339: discrete VCOs drift most, the CEM chips less, the DCO, the
    /// digital oscillators and the divide-down core not at all; a model
    /// whose oscillators are not voiced keeps today's `Analog`.
    #[test]
    fn models_voice_their_oscillators() {
        use crate::synth::Engine;
        for (m, name) in Model::ALL {
            let want = match m {
                Model::Arp2600
                | Model::Minimoog
                | Model::Ms20
                | Model::Cs15
                | Model::Odyssey
                | Model::Jupiter8 => DISCRETE_VCO,
                Model::Sh101 | Model::ProOne | Model::Prophet5 | Model::Matrix12 => CEM_VCO,
                Model::Juno106 | Model::PpgWave | Model::PolyMoog => LOCKED,
                _ => IDEAL_VCO,
            };
            assert_eq!(m.osc(), want, "{name}");
            if m.def().engine != Engine::Mono {
                assert_eq!(m.osc(), IDEAL_VCO, "{name}: not a Mono voice");
            }
        }
        const { assert!(DISCRETE_VCO.drift > CEM_VCO.drift && CEM_VCO.drift > LOCKED.drift) };
    }

    /// #340: each model's envelopes take the instrument's ranges and curve;
    /// one whose envelopes are not voiced keeps today's.
    #[test]
    fn models_voice_their_envelopes() {
        for (m, name) in Model::ALL {
            let v = m.env();
            let want: ([f32; 2], [f32; 2], [f32; 2], f64, f32) = match m {
                Model::Minimoog => ([0.01, 10.0], [0.01, 10.0], [0.01, 10.0], 0.3, 0.8),
                Model::Prophet5 | Model::ProOne => {
                    ([0.002, 10.0], [0.002, 10.0], [0.002, 10.0], 0.3, 1.0)
                }
                Model::Juno106 => ([0.0015, 3.0], [0.0015, 10.0], [0.0015, 10.0], 3.0, 1.0),
                Model::Sh101 => ([0.0015, 4.0], [0.002, 10.0], [0.002, 10.0], 0.3, 1.0),
                Model::Odyssey => ([0.005, 5.0], [0.01, 8.0], [0.015, 10.0], 0.3, 1.0),
                Model::Jupiter8 => ([0.0015, 6.0], [0.0015, 10.0], [0.0015, 10.0], 0.3, 1.0),
                _ => {
                    assert_eq!(v, RC_ENV, "{name}");
                    continue;
                }
            };
            assert_eq!(
                (v.attack, v.decay, v.release, v.attack_aim, v.sustain_max),
                want,
                "{name}"
            );
        }
        let t = EnvTimes {
            attack: 1.0,
            decay: 2.0,
            sustain: 0.5,
            release: 3.0,
        };
        assert_eq!(RC_ENV.clamp(&t, 1_000.0), t, "today's: untouched in range");
    }

    /// #341: the OTA VCAs round off, the ARP 2600's ADSR drives an
    /// exponential control, every other VCA is clean.
    #[test]
    fn models_voice_their_vcas() {
        for (m, name) in Model::ALL {
            let want = match m {
                Model::Sh101 | Model::Juno106 | Model::Jupiter8 => BA662,
                Model::Prophet5 => CA3280,
                Model::Arp2600 => VcaVoicing {
                    expo: true,
                    ..CLEAN_VCA
                },
                _ => CLEAN_VCA,
            };
            assert_eq!(m.vca(), want, "{name}");
        }
    }

    /// #341: a clean VCA passes the signal and the envelope as they are; an
    /// OTA leaves a soft signal alone and adds a third harmonic to a hot
    /// one; an exponential control runs 0 to 1, is about 30 dB down half
    /// way, and turns a straight fall into one even in decibels.
    #[test]
    fn vca_voicings_shape_as_they_say() {
        for i in 0..=1000 {
            let x = -2.0 + i as f32 * 0.004;
            assert_eq!(CLEAN_VCA.input(x), x);
            let v = i as f32 / 1000.0;
            assert_eq!(CLEAN_VCA.gain(v), v);
            assert_eq!(BA662.gain(v), v, "an OTA's control is linear");
        }
        // The third harmonic of a sine of `amp` through `v`, over its level.
        let third = |v: VcaVoicing, amp: f32| {
            let n = 4_800;
            let (mut re, mut im, mut all) = (0.0_f64, 0.0_f64, 0.0_f64);
            for k in 0..n {
                let ph = std::f64::consts::TAU * 10.0 * k as f64 / n as f64;
                let y = f64::from(v.input(amp * ph.sin() as f32));
                re += y * (3.0 * ph).cos();
                im += y * (3.0 * ph).sin();
                all += y * y;
            }
            (re * re + im * im).sqrt() * 2.0 / n as f64 / (2.0 * all / n as f64).sqrt()
        };
        for ota in [BA662, CA3280] {
            assert!(third(ota, 0.05) < 1.0e-3, "soft: {}", third(ota, 0.05));
            assert!(third(ota, 2.0) > 0.02, "hot: {}", third(ota, 2.0));
        }
        assert!(
            third(BA662, 2.0) > third(CA3280, 2.0),
            "the BA662 rounds harder"
        );

        let expo = VcaVoicing {
            expo: true,
            ..CLEAN_VCA
        };
        assert_eq!(expo.gain(0.0), 0.0);
        assert!((expo.gain(1.0) - 1.0).abs() < 1.0e-4);
        let db = |v: f32| 20.0 * expo.gain(v).log10();
        assert!((db(0.5) + 30.0).abs() < 0.5, "half way: {} dB", db(0.5));
        let steps: Vec<f32> = (5..10)
            .map(|i| db(i as f32 / 10.0) - db((i - 1) as f32 / 10.0))
            .collect();
        assert!(
            steps.iter().all(|s| (s - 6.0).abs() < 0.3),
            "even in dB: {steps:?}"
        );
    }

    /// #339: a rounded triangle keeps its peaks at ±1 and stays monotonic
    /// between them; a bowed saw adds no DC and its reset is still a step
    /// of 2, so the band-limiting still fits; at 0 both pass untouched.
    #[test]
    fn wave_shapes_keep_their_peaks_and_steps() {
        use crate::mono::osc::Waveform;
        let ys: Vec<f32> = (0..=2000).map(|i| -1.0 + i as f32 / 1000.0).collect();
        for v in [DISCRETE_VCO, CEM_VCO] {
            let tri: Vec<f32> = ys.iter().map(|&y| v.shape(Waveform::Triangle, y)).collect();
            assert!((tri[2000] - 1.0).abs() < 1e-6 && (tri[0] + 1.0).abs() < 1e-6);
            assert!(tri.windows(2).all(|w| w[1] >= w[0]), "monotonic");
            let saw: Vec<f32> = ys.iter().map(|&y| v.shape(Waveform::Saw, y)).collect();
            let mean = saw.iter().sum::<f32>() / saw.len() as f32;
            assert!(mean.abs() < 1e-3, "no DC: {mean}");
            assert!((saw[2000] - saw[0] - 2.0).abs() < 1e-6, "a step of 2");
            assert!(saw.iter().all(|y| y.abs() <= 1.1));
        }
        for w in [
            Waveform::Saw,
            Waveform::Triangle,
            Waveform::Pulse,
            Waveform::Sine,
        ] {
            for &y in &ys {
                assert_eq!(IDEAL_VCO.shape(w, y), y);
                assert_eq!(DISCRETE_VCO.shape(Waveform::Pulse, y), y);
            }
        }
    }

    /// Every model with a high-pass stage of 12 dB has the matching filter.
    #[test]
    fn models_pair_their_filters_and_stages() {
        for (m, name) in Model::ALL {
            if m.hp() == Hp::Svf {
                assert!(matches!(m.filter(), Filter::Svf(_)), "{name}");
            }
        }
        const {
            assert!(MS20.osc_at < 1.0 && MS20.k_min < 0.0);
            assert!(CS15.osc_at > 1.0 && CS15.k_min > 0.0);
        }
        assert_eq!(MOOG.comp, 0.0);
    }

    /// #306: the Minimoog and the ARP 2600 (whose 4012 copied the Moog
    /// ladder) saturate each stage of their ladder.
    #[test]
    fn moog_ladders_saturate_per_stage() {
        for m in [Model::Minimoog, Model::Arp2600] {
            let Filter::Ladder(v) = m.filter() else {
                panic!("{m:?}");
            };
            assert_eq!(v.stages, Stages::Transistor, "{m:?}");
        }
        assert_eq!(MOOG.stages, Stages::Transistor);
    }

    /// #305: the Roland four-poles are IR3109 OTA cascades, the Juno-106's
    /// voiced apart from the SH-101's; the Jupiter-8 keeps its 12 dB SVF.
    #[test]
    fn roland_ladders_are_ota_cascades() {
        for m in [Model::Sh101, Model::Juno106, Model::Jupiter8] {
            let Filter::Ladder(v) = m.filter() else {
                panic!("{m:?}");
            };
            assert_eq!(v.stages, Stages::Ota, "{m:?}");
        }
        assert_ne!(Model::Juno106.filter(), Model::Sh101.filter());
        assert_eq!(
            Model::Jupiter8.low_pass(Setting::SLOPE12),
            Filter::Svf(JUPITER12)
        );
    }

    #[test]
    fn polyphonic_models_have_their_own_voice_count() {
        assert_eq!(Model::Prophet5.voices(), 5);
        for (m, name) in Model::ALL {
            assert!(
                m.voices() >= 1 && m.voices() <= crate::poly::MAX_VOICES,
                "{name}"
            );
        }
        assert_eq!(
            Model::Minimoog.voices(),
            crate::poly::MAX_VOICES,
            "a monosynth is not limited"
        );
    }

    /// The models a switch changes, and what to: no model has both switches,
    /// so which one `low_pass` reads first does not matter.
    fn switched(s: Setting) -> Vec<Model> {
        Model::ALL
            .iter()
            .map(|(m, _)| *m)
            .filter(|m| m.low_pass(s) != m.filter())
            .collect()
    }

    /// Only the Jupiter-8 and the Matrix-12 have the slope switch, and their
    /// 12 dB setting is a state-variable filter beside the ladder.
    #[test]
    fn only_the_jupiter_and_matrix_have_a_slope_switch() {
        assert_eq!(
            switched(Setting::SLOPE12),
            [Model::Jupiter8, Model::Matrix12]
        );
        assert!(matches!(Model::Jupiter8.filter(), Filter::Ladder(_)));
        assert!(matches!(
            Model::Jupiter8.low_pass(Setting::SLOPE12),
            Filter::Svf(_)
        ));
    }

    /// #321: the Sequential four-poles are their chips: the CEM3320 in the
    /// Pro-One and the Prophet-5 Rev 3, the SSM2040 in its Rev 1/2.
    #[test]
    fn sequential_ladders_are_their_chips() {
        let stages = |f: Filter| match f {
            Filter::Ladder(v) => v.stages,
            Filter::Svf(_) => panic!("{f:?}"),
        };
        assert_eq!(stages(Model::ProOne.filter()), Stages::Cem3320);
        assert_eq!(stages(Model::Prophet5.filter()), Stages::Cem3320);
        for rev in [1, 2] {
            let f = Model::Prophet5.low_pass(Setting::rev(rev));
            assert_eq!(stages(f), Stages::Ssm2040);
        }
        assert_eq!(
            Model::Prophet5.low_pass(Setting::OWN),
            Model::Prophet5.filter(),
            "Rev 3 is its own"
        );
    }

    /// Only the Prophet-5 and the Odyssey have the revision switch; the
    /// Odyssey's Rev 1 is the 4023's two poles, its Rev 2 the 4035's ladder.
    /// No model has both switches.
    #[test]
    fn only_the_prophet_and_odyssey_have_a_rev_switch() {
        let mut revs = switched(Setting::rev(1));
        revs.extend(switched(Setting::rev(2)));
        revs.sort_by_key(|m| *m as u32);
        revs.dedup();
        assert_eq!(revs, [Model::Odyssey, Model::Prophet5]);
        assert!(revs.iter().all(|m| !switched(Setting::SLOPE12).contains(m)));
        let odyssey = |r| Model::Odyssey.low_pass(Setting::rev(r));
        assert_eq!(odyssey(1), Filter::Svf(ODYSSEY_REV1));
        assert_eq!(odyssey(2), Filter::Ladder(ODYSSEY_REV2));
        assert_eq!(ODYSSEY_REV2.stages, Stages::Transistor);
        assert_eq!(odyssey(3), Model::Odyssey.filter());
    }

    /// #321: the SH-101 has no high-pass.
    #[test]
    fn sh101_has_no_high_pass() {
        assert_eq!(Model::Sh101.hp(), Hp::None);
    }

    #[test]
    fn single_envelope_models_follow_the_adsr() {
        for (m, name) in Model::ALL {
            let single = matches!(
                m,
                Model::Arp2600 | Model::Sh101 | Model::Odyssey | Model::Juno106
            );
            assert_eq!(m.cutoff_follows_filter_env(), !single, "{name}");
        }
    }
}
