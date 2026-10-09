//! The Mono voice (spec 004): one shared voice, seven models (spec 005).
//!
//! `osc` holds the VCOs, `noise` the noise source, `ladder` the filter,
//! `env` the ADSR and AR, `lfo` the LFO and sample-and-hold, `voice` one
//! voice per owner with its keys and glide, `preset` the defaults and
//! presets, `model` which instrument a synth is, `svf` the 12 dB filters.
//! The settings here belong to one synth (spec 004 Req 10).

pub mod env;
pub mod ladder;
pub mod lfo;
pub mod model;
pub mod noise;
pub mod osc;
pub mod patch;
pub mod preset;
pub mod svf;
pub mod voice;

use crate::drums::{PADS, Pad, PadOut, PadParams};
use crate::fm::patch::FmPatch;
use crate::padsampler::PadKit;
use crate::params::Param;
use env::EnvTimes;
use ladder::{MAX_K, hz_to_note};
use model::{EnvVoicing, Filter, Model, OscVoicing, Setting};
use noise::NoiseColour;
use osc::Waveform;
use patch::{DESTS, Normals, Patch};
use voice::NotePriority;

/// Number of VCOs per Mono voice.
pub const VCOS: usize = 3;

/// The Mono parameters, with what `render` needs precomputed.
#[derive(Clone, Copy)]
pub struct MonoParams {
    /// Which instrument this synth is (spec 005).
    pub model: Model,
    /// Voices the synth plays at once (spec 006): 1 is monophonic.
    pub polyphony: usize,
    /// Unison assignment, its detune spread in cents either side, and the analog variance.
    pub unison: bool,
    pub unison_cents: f32,
    pub analog: f32,
    /// The stereo chorus mode (0 off); the engine runs it after the voices.
    pub chorus_mode: usize,
    /// The filter switches: slope and revision (#321).
    pub filter_switches: Setting,
    /// The VCO and envelope revisions, on a model with those switches (#343).
    pub vco_rev: u8,
    pub env_rev: u8,
    /// The second LFO (cycles per sample, waveform) and the ramp's step per sample.
    pub lfo2_inc: f32,
    pub lfo2_wave: Waveform,
    pub ramp_inc: f32,
    /// The wavetable oscillators (spec 006 Req 11): table and position of each, stepped
    /// positions, and the filter envelope's and the LFO's reach on the position.
    pub wt_table: [usize; 2],
    pub wt_pos: [f32; 2],
    pub wt_steps: bool,
    pub env_wt: f32,
    pub lfo_wt: f32,
    /// The D-50's partials (spec 006 Req 12): each one's PCM attack (0 is synthesised), how
    /// the pair combines, and partial 2's own filter and envelopes (partial 1 uses the
    /// synth's: `cutoff`, `adsr`, `fadsr`).
    pub pcm: [usize; 2],
    pub structure: usize,
    pub p2_cutoff: f32,
    pub p2_k: f32,
    pub p2_env_cutoff: f32,
    pub p2_fadsr: EnvTimes,
    pub p2_adsr: EnvTimes,
    /// The DX7 voice (spec 006 Req 13).
    pub fm: FmPatch,
    /// The drum kit's pads and its accent (`Model::Tr808`).
    pub drums: [PadParams; PADS],
    pub drum_accent: f32,
    /// Where each pad goes (#162).
    pub pad_outs: [PadOut; PADS],
    /// The drum/pad sampler's sixteen pads (#124); set through `padsampler::PadField`, not parameters.
    pub pad_kit: PadKit,
    pub wave: [Waveform; VCOS],
    /// Coarse tune in semitones and fine tune in cents, per VCO.
    coarse: [f32; VCOS],
    fine: [f32; VCOS],
    /// Coarse plus fine tune in semitones, added to the voice's pitch.
    pub tune: [f32; VCOS],
    pub level: [f32; VCOS],
    pub pulse_width: f32,
    /// Whether VCO 2 and VCO 3 reset with VCO 1; VCO 1's entry is unused.
    pub sync: [bool; VCOS],
    pub noise_level: f32,
    pub noise_colour: NoiseColour,
    /// Ring modulator and sub-oscillator levels, and the sub's frequency as
    /// a share of VCO 1's (a half or a quarter).
    pub ring_level: f32,
    pub sub_level: f32,
    pub sub_ratio: f32,
    /// Whether VCO 3 follows the key, and whether it is in the low range.
    pub vco3_follow: bool,
    pub vco3_low: bool,
    /// Ladder cutoff as a MIDI note, feedback, and input gain.
    pub cutoff: f32,
    pub k: f32,
    /// High-pass cutoff as a MIDI note, and resonance 0..=1.
    pub hp_cutoff: f32,
    pub hp_res: f32,
    pub drive: f32,
    /// Envelope times in samples, so the sample rate is kept to convert.
    sample_rate: f32,
    pub adsr: EnvTimes,
    pub ar: EnvTimes,
    /// The filter ADSR (spec 004 Req 12).
    pub fadsr: EnvTimes,
    /// LFO cycles per sample, and its waveform.
    pub lfo_inc: f32,
    pub lfo_wave: Waveform,
    /// Which held key sounds, legato on or off, glide time in samples.
    pub priority: NotePriority,
    pub legato: bool,
    pub glide: f32,
    /// The patch, which destinations it takes over, the normalled amounts
    /// and the mod wheel.
    pub patch: Patch,
    pub taken: [bool; DESTS],
    pub normals: Normals,
    pub mod_wheel: f32,
    /// The pitch wheel (−1..1) and its range in semitones; `bend` is their
    /// product, added to every voice's pitch (#10).
    pitch_bend: f32,
    bend_range: f32,
    pub bend: f32,
    /// A Modular synth's voice (ADR-0020): a preset's or the song's; each
    /// note takes a copy when it starts.
    pub graph: crate::modular::Program,
    /// The most voices a Modular program may sound at once, from its cost.
    pub graph_cap: usize,
    /// The values of a Modular voice's controls (`Param::Ctl1`…), in their own units.
    pub ctl: [f32; crate::params::CTLS],
    /// The knobs under the Minimoog's on/off switches (#308): `level`,
    /// `noise_level`, `glide` and the vibrato and filter modulation normals
    /// are these, or 0 where their switch is off.
    knob_level: [f32; VCOS],
    knob_noise: f32,
    knob_glide: f32,
    knob_vibrato: f32,
    knob_lfo_cutoff: f32,
    vco_on: [bool; VCOS],
    noise_on: bool,
    glide_on: bool,
    osc_mod_on: bool,
    filter_mod_on: bool,
    /// Whether decay is the release where `Model::decay_is_release`; off, the
    /// contours release at once.
    pub decay_release: bool,
    /// The 440 Hz reference tone at the synth's output.
    pub a440: bool,
}

impl Default for MonoParams {
    fn default() -> MonoParams {
        MonoParams::new(48_000.0)
    }
}

impl MonoParams {
    /// The sample rate the parameters were converted for.
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// The low-pass in use: the model's at the panel's switches.
    pub fn filter(&self) -> Filter {
        self.model.low_pass(self.filter_switches)
    }

    /// The oscillators in use: the model's at its VCO switch (#343).
    pub fn osc(&self) -> OscVoicing {
        self.model.def().osc_at(self.vco_rev)
    }

    /// The envelopes in use: the model's at its envelope switch (#343).
    pub fn env(&self) -> EnvVoicing {
        self.model.def().env_at(self.env_rev)
    }

    /// The voices this synth plays at once: `Polyphony`, at most its model's.
    pub fn voices(&self) -> usize {
        let most = if self.model.uses_graph() {
            self.model.voices().min(self.graph_cap)
        } else {
            self.model.voices()
        };
        self.polyphony.clamp(1, most.max(1))
    }

    /// The voice at `preset::DEFAULTS`: zeroed, then every default set.
    pub fn new(sample_rate: f32) -> MonoParams {
        let off = EnvTimes {
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.0,
        };
        let mut p = MonoParams {
            model: Model::Arp2600,
            polyphony: 1,
            unison: false,
            unison_cents: 0.0,
            analog: 0.0,
            chorus_mode: 0,
            filter_switches: Setting::OWN,
            vco_rev: 3,
            env_rev: 3,
            lfo2_inc: 0.0,
            lfo2_wave: Waveform::Sine,
            ramp_inc: 0.0,
            wt_table: [0; 2],
            wt_pos: [0.0; 2],
            wt_steps: false,
            env_wt: 0.0,
            lfo_wt: 0.0,
            pcm: [0; 2],
            structure: 0,
            p2_cutoff: 0.0,
            p2_k: 0.0,
            p2_env_cutoff: 0.0,
            p2_fadsr: off,
            p2_adsr: off,
            fm: FmPatch::default(),
            drums: [PadParams::default(); PADS],
            drum_accent: 0.5,
            pad_outs: [PadOut::default(); PADS],
            pad_kit: PadKit::default(),
            wave: [Waveform::Saw; VCOS],
            coarse: [0.0; VCOS],
            fine: [0.0; VCOS],
            tune: [0.0; VCOS],
            level: [0.0; VCOS],
            pulse_width: 0.5,
            sync: [false; VCOS],
            noise_level: 0.0,
            noise_colour: NoiseColour::White,
            ring_level: 0.0,
            sub_level: 0.0,
            sub_ratio: 0.5,
            vco3_follow: true,
            vco3_low: false,
            cutoff: 0.0,
            k: 0.0,
            hp_cutoff: 0.0,
            hp_res: 0.0,
            drive: 1.0,
            sample_rate,
            graph: crate::modular::Program::default(),
            graph_cap: crate::poly::MAX_VOICES,
            ctl: [0.0; crate::params::CTLS],
            knob_level: [0.0; VCOS],
            knob_noise: 0.0,
            knob_glide: 0.0,
            knob_vibrato: 0.0,
            knob_lfo_cutoff: 0.0,
            vco_on: [true; VCOS],
            noise_on: true,
            glide_on: true,
            osc_mod_on: true,
            filter_mod_on: true,
            decay_release: true,
            a440: false,
            adsr: off,
            ar: off,
            fadsr: off,
            lfo_inc: 0.0,
            lfo_wave: Waveform::Sine,
            priority: NotePriority::Last,
            legato: false,
            glide: 0.0,
            patch: Patch::default(),
            taken: [false; DESTS],
            normals: Normals::default(),
            mod_wheel: 0.0,
            pitch_bend: 0.0,
            bend_range: 2.0,
            bend: 0.0,
        };
        for (param, v) in preset::DEFAULTS {
            p.set(param, param.clamp(v));
        }
        p
    }

    /// Apply an already clamped value; parameters that aren't Mono's are
    /// ignored.
    pub fn set(&mut self, param: Param, v: f32) {
        self.set_one(param, v);
        self.apply_switches();
    }

    /// The mixer switches of VCO 1, 2, 3 and noise as gains, 1 or 0 (#308).
    pub fn mixer_on(&self) -> [f32; 4] {
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        let [a, b, c] = self.vco_on;
        [on(a), on(b), on(c), on(self.noise_on)]
    }

    /// What sounds of the knobs under the on/off switches (#308).
    fn apply_switches(&mut self) {
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        for ((level, knob), &is_on) in self.level.iter_mut().zip(self.knob_level).zip(&self.vco_on)
        {
            *level = knob * on(is_on);
        }
        self.noise_level = self.knob_noise * on(self.noise_on);
        self.glide = self.knob_glide * on(self.glide_on);
        self.normals.vibrato = self.knob_vibrato * on(self.osc_mod_on);
        self.normals.lfo_cutoff = self.knob_lfo_cutoff * on(self.filter_mod_on);
    }

    /// The match is exhaustive on purpose: a new `Param` doesn't compile
    /// until it is handled here.
    fn set_one(&mut self, param: Param, v: f32) {
        let samples = v * self.sample_rate;
        match param {
            Param::Ctl1
            | Param::Ctl2
            | Param::Ctl3
            | Param::Ctl4
            | Param::Ctl5
            | Param::Ctl6
            | Param::Ctl7
            | Param::Ctl8
            | Param::Ctl9
            | Param::Ctl10
            | Param::Ctl11
            | Param::Ctl12
            | Param::Ctl13
            | Param::Ctl14
            | Param::Ctl15
            | Param::Ctl16
            | Param::Ctl17
            | Param::Ctl18
            | Param::Ctl19
            | Param::Ctl20
            | Param::Ctl21
            | Param::Ctl22
            | Param::Ctl23
            | Param::Ctl24
            | Param::Ctl25
            | Param::Ctl26
            | Param::Ctl27
            | Param::Ctl28
            | Param::Ctl29
            | Param::Ctl30
            | Param::Ctl31
            | Param::Ctl32 => {
                if let Some(c) = param.ctl().and_then(|i| self.ctl.get_mut(i)) {
                    *c = v;
                }
            }
            Param::Vco1Wave => self.set_wave(0, v),
            Param::Vco2Wave => self.set_wave(1, v),
            Param::Vco3Wave => self.set_wave(2, v),
            Param::Vco1Coarse => self.set_tune(0, Some(v.round()), None),
            Param::Vco2Coarse => self.set_tune(1, Some(v.round()), None),
            Param::Vco3Coarse => self.set_tune(2, Some(v.round()), None),
            Param::Vco1Fine => self.set_tune(0, None, Some(v)),
            Param::Vco2Fine => self.set_tune(1, None, Some(v)),
            Param::Vco3Fine => self.set_tune(2, None, Some(v)),
            Param::Vco1Level => self.knob_level[0] = v,
            Param::Vco2Level => self.knob_level[1] = v,
            Param::Vco3Level => self.knob_level[2] = v,
            Param::Vco1On => self.vco_on[0] = v >= 0.5,
            Param::Vco2On => self.vco_on[1] = v >= 0.5,
            Param::Vco3On => self.vco_on[2] = v >= 0.5,
            Param::PulseWidth => self.pulse_width = v,
            Param::Vco2Sync => self.sync[1] = v >= 0.5,
            Param::Vco3Sync => self.sync[2] = v >= 0.5,
            Param::NoiseLevel => self.knob_noise = v,
            Param::NoiseOn => self.noise_on = v >= 0.5,
            Param::RingLevel => self.ring_level = v,
            Param::SubLevel => self.sub_level = v,
            Param::SubOctave => self.sub_ratio = if v >= 0.5 { 0.25 } else { 0.5 },
            Param::Vco3KeyFollow => self.vco3_follow = v >= 0.5,
            Param::Vco3Low => self.vco3_low = v >= 0.5,
            Param::NoiseColour => {
                if let Some(c) = NoiseColour::from_id(v.round() as u32) {
                    self.noise_colour = c;
                }
            }
            Param::Cutoff => self.cutoff = hz_to_note(v),
            Param::Resonance => self.k = v * MAX_K,
            Param::HpCutoff => self.hp_cutoff = hz_to_note(v),
            Param::HpResonance => self.hp_res = v,
            // 0..=1 is 0 to +18 dB: 1 + 7·v.
            Param::Drive => self.drive = 1.0 + 7.0 * v,
            // Times arrive in seconds and are kept in samples.
            Param::AdsrAttack => self.adsr.attack = samples,
            Param::AdsrDecay => self.adsr.decay = samples,
            Param::AdsrSustain => self.adsr.sustain = v,
            Param::AdsrRelease => self.adsr.release = samples,
            Param::ArAttack => self.ar.attack = samples,
            Param::ArRelease => self.ar.release = samples,
            Param::FenvAttack => self.fadsr.attack = samples,
            Param::FenvDecay => self.fadsr.decay = samples,
            Param::FenvSustain => self.fadsr.sustain = v,
            Param::FenvRelease => self.fadsr.release = samples,
            Param::LfoRate => self.lfo_inc = v / self.sample_rate,
            Param::LfoWave => {
                if let Some(w) = Waveform::from_id(v.round() as u32) {
                    self.lfo_wave = w;
                }
            }
            Param::Priority => {
                if let Some(pr) = NotePriority::from_id(v.round() as u32) {
                    self.priority = pr;
                }
            }
            Param::Legato => self.legato = v >= 0.5,
            Param::Glide => self.knob_glide = samples,
            Param::GlideOn => self.glide_on = v >= 0.5,
            Param::DecayRelease => self.decay_release = v >= 0.5,
            Param::A440 => self.a440 = v >= 0.5,
            Param::Patch1Source => self.patch.set_source(0, v),
            Param::Patch1Dest => self.patch.set_dest(0, v),
            Param::Patch1Amount => self.patch.set_amount(0, v),
            Param::Patch2Source => self.patch.set_source(1, v),
            Param::Patch2Dest => self.patch.set_dest(1, v),
            Param::Patch2Amount => self.patch.set_amount(1, v),
            Param::Patch3Source => self.patch.set_source(2, v),
            Param::Patch3Dest => self.patch.set_dest(2, v),
            Param::Patch3Amount => self.patch.set_amount(2, v),
            Param::Patch4Source => self.patch.set_source(3, v),
            Param::Patch4Dest => self.patch.set_dest(3, v),
            Param::Patch4Amount => self.patch.set_amount(3, v),
            Param::Patch5Source => self.patch.set_source(4, v),
            Param::Patch5Dest => self.patch.set_dest(4, v),
            Param::Patch5Amount => self.patch.set_amount(4, v),
            Param::Patch6Source => self.patch.set_source(5, v),
            Param::Patch6Dest => self.patch.set_dest(5, v),
            Param::Patch6Amount => self.patch.set_amount(5, v),
            Param::Patch7Source => self.patch.set_source(6, v),
            Param::Patch7Dest => self.patch.set_dest(6, v),
            Param::Patch7Amount => self.patch.set_amount(6, v),
            Param::Patch8Source => self.patch.set_source(7, v),
            Param::Patch8Dest => self.patch.set_dest(7, v),
            Param::Patch8Amount => self.patch.set_amount(7, v),
            // Semitones of cutoff at full ADSR, and of VCO pitch at full LFO.
            Param::EnvCutoff => self.normals.env_cutoff = 48.0 * v,
            Param::EnvHpCutoff => self.normals.env_hp_cutoff = 48.0 * v,
            Param::KeyTrack => self.normals.key_track = v,
            Param::Vibrato => self.knob_vibrato = 2.0 * v,
            Param::LfoCutoff => self.knob_lfo_cutoff = 24.0 * v,
            Param::OscModOn => self.osc_mod_on = v >= 0.5,
            Param::FilterModOn => self.filter_mod_on = v >= 0.5,
            Param::LfoPw => self.normals.lfo_pw = 0.45 * v,
            Param::EnvFreq2 => self.normals.env_freq2 = 24.0 * v,
            Param::OscFreq2 => self.normals.osc_freq2 = 24.0 * v,
            Param::EnvPw => self.normals.env_pw = 0.45 * v,
            Param::OscPw => self.normals.osc_pw = 0.45 * v,
            Param::OscCutoff => self.normals.osc_cutoff = 48.0 * v,
            Param::ModWheel => self.mod_wheel = v,
            Param::PitchBend => {
                self.pitch_bend = v;
                self.bend = v * self.bend_range;
            }
            Param::BendRange => {
                self.bend_range = v;
                self.bend = self.pitch_bend * v;
            }
            // The mixer's (`mixer::Mixer`), not the voice's.
            Param::Level
            | Param::Pan
            | Param::Send1
            | Param::Send2
            | Param::Send3
            | Param::Send4
            | Param::Mute
            | Param::Solo
            | Param::I1Type
            | Param::I1A
            | Param::I1B
            | Param::I1C
            | Param::I1D
            | Param::I1E
            | Param::I2Type
            | Param::I2A
            | Param::I2B
            | Param::I2C
            | Param::I2D
            | Param::I2E
            | Param::I3Type
            | Param::I3A
            | Param::I3B
            | Param::I3C
            | Param::I3D
            | Param::I3E
            | Param::Out
            | Param::Send1Pre
            | Param::Send2Pre
            | Param::Send3Pre
            | Param::Send4Pre
            | Param::Send1On
            | Param::Send2On
            | Param::Send3On
            | Param::Send4On
            | Param::Key
            | Param::ArpOn
            | Param::ArpMode
            | Param::ArpOctaves
            | Param::ArpRate
            | Param::ArpGate
            | Param::ArpLatch
            | Param::ArpFree
            | Param::ArpSeed
            | Param::P2In
            | Param::P3In
            | Param::P4In
            | Param::P1Type
            | Param::P1Return
            | Param::P1A
            | Param::P1B
            | Param::P1C
            | Param::P1D
            | Param::P1E
            | Param::P2Type
            | Param::P2Return
            | Param::P2A
            | Param::P2B
            | Param::P2C
            | Param::P2D
            | Param::P2E
            | Param::P3Type
            | Param::P3Return
            | Param::P3A
            | Param::P3B
            | Param::P3C
            | Param::P3D
            | Param::P3E
            | Param::P4Type
            | Param::P4Return
            | Param::P4A
            | Param::P4B
            | Param::P4C
            | Param::P4D
            | Param::P4E
            | Param::CompThreshold
            | Param::CompRatio
            | Param::CompAttack
            | Param::CompRelease
            | Param::CompMakeup
            | Param::EqLowFreq
            | Param::EqLowGain
            | Param::EqMid1Freq
            | Param::EqMid1Gain
            | Param::EqMid1Q
            | Param::EqMid2Freq
            | Param::EqMid2Gain
            | Param::EqMid2Q
            | Param::EqHighFreq
            | Param::EqHighGain => {}
            Param::Polyphony => self.polyphony = v.round().max(1.0) as usize,
            Param::Assign => self.unison = v >= 0.5,
            Param::UnisonDetune => self.unison_cents = 50.0 * v,
            Param::Analog => self.analog = v,
            Param::ChorusMode => self.chorus_mode = v.round() as usize,
            Param::XMod => self.normals.xmod = 24.0 * v,
            Param::Slope => self.filter_switches.slope12 = v < 0.5,
            Param::FilterRev => self.filter_switches.rev = v.round().clamp(1.0, 3.0) as u8,
            Param::VcoRev => self.vco_rev = v.round().clamp(1.0, 3.0) as u8,
            Param::EnvRev => self.env_rev = v.round().clamp(1.0, 3.0) as u8,
            // The engine sets the switches it stands for (#343).
            Param::Revision => {}
            Param::Patch9Source => self.patch.set_source(8, v),
            Param::Patch9Dest => self.patch.set_dest(8, v),
            Param::Patch9Amount => self.patch.set_amount(8, v),
            Param::Patch10Source => self.patch.set_source(9, v),
            Param::Patch10Dest => self.patch.set_dest(9, v),
            Param::Patch10Amount => self.patch.set_amount(9, v),
            Param::Patch11Source => self.patch.set_source(10, v),
            Param::Patch11Dest => self.patch.set_dest(10, v),
            Param::Patch11Amount => self.patch.set_amount(10, v),
            Param::Patch12Source => self.patch.set_source(11, v),
            Param::Patch12Dest => self.patch.set_dest(11, v),
            Param::Patch12Amount => self.patch.set_amount(11, v),
            Param::Patch13Source => self.patch.set_source(12, v),
            Param::Patch13Dest => self.patch.set_dest(12, v),
            Param::Patch13Amount => self.patch.set_amount(12, v),
            Param::Patch14Source => self.patch.set_source(13, v),
            Param::Patch14Dest => self.patch.set_dest(13, v),
            Param::Patch14Amount => self.patch.set_amount(13, v),
            Param::Patch15Source => self.patch.set_source(14, v),
            Param::Patch15Dest => self.patch.set_dest(14, v),
            Param::Patch15Amount => self.patch.set_amount(14, v),
            Param::Patch16Source => self.patch.set_source(15, v),
            Param::Patch16Dest => self.patch.set_dest(15, v),
            Param::Patch16Amount => self.patch.set_amount(15, v),
            Param::Patch17Source => self.patch.set_source(16, v),
            Param::Patch17Dest => self.patch.set_dest(16, v),
            Param::Patch17Amount => self.patch.set_amount(16, v),
            Param::Patch18Source => self.patch.set_source(17, v),
            Param::Patch18Dest => self.patch.set_dest(17, v),
            Param::Patch18Amount => self.patch.set_amount(17, v),
            Param::Patch19Source => self.patch.set_source(18, v),
            Param::Patch19Dest => self.patch.set_dest(18, v),
            Param::Patch19Amount => self.patch.set_amount(18, v),
            Param::Patch20Source => self.patch.set_source(19, v),
            Param::Patch20Dest => self.patch.set_dest(19, v),
            Param::Patch20Amount => self.patch.set_amount(19, v),
            Param::Wt1Table => self.wt_table[0] = v.round() as usize,
            Param::Wt2Table => self.wt_table[1] = v.round() as usize,
            Param::Wt1Pos => self.wt_pos[0] = v,
            Param::Wt2Pos => self.wt_pos[1] = v,
            Param::WtSteps => self.wt_steps = v >= 0.5,
            Param::EnvWt => self.env_wt = v,
            Param::LfoWt => self.lfo_wt = v,
            Param::Pcm1Sample => self.pcm[0] = v.round() as usize,
            Param::Pcm2Sample => self.pcm[1] = v.round() as usize,
            Param::Structure => self.structure = v.round() as usize,
            Param::P2Cutoff => self.p2_cutoff = hz_to_note(v),
            Param::P2Resonance => self.p2_k = v * MAX_K,
            Param::P2EnvCutoff => self.p2_env_cutoff = 48.0 * v,
            Param::P2FenvAttack => self.p2_fadsr.attack = samples,
            Param::P2FenvDecay => self.p2_fadsr.decay = samples,
            Param::P2FenvSustain => self.p2_fadsr.sustain = v,
            Param::P2FenvRelease => self.p2_fadsr.release = samples,
            Param::P2AdsrAttack => self.p2_adsr.attack = samples,
            Param::P2AdsrDecay => self.p2_adsr.decay = samples,
            Param::P2AdsrSustain => self.p2_adsr.sustain = v,
            Param::P2AdsrRelease => self.p2_adsr.release = samples,
            Param::Op1R1 => self.fm.set_op(5, 0, v),
            Param::Op1R2 => self.fm.set_op(5, 1, v),
            Param::Op1R3 => self.fm.set_op(5, 2, v),
            Param::Op1R4 => self.fm.set_op(5, 3, v),
            Param::Op1L1 => self.fm.set_op(5, 4, v),
            Param::Op1L2 => self.fm.set_op(5, 5, v),
            Param::Op1L3 => self.fm.set_op(5, 6, v),
            Param::Op1L4 => self.fm.set_op(5, 7, v),
            Param::Op1BreakPoint => self.fm.set_op(5, 8, v),
            Param::Op1LeftDepth => self.fm.set_op(5, 9, v),
            Param::Op1RightDepth => self.fm.set_op(5, 10, v),
            Param::Op1LeftCurve => self.fm.set_op(5, 11, v),
            Param::Op1RightCurve => self.fm.set_op(5, 12, v),
            Param::Op1RateScale => self.fm.set_op(5, 13, v),
            Param::Op1AmpSens => self.fm.set_op(5, 14, v),
            Param::Op1VelSens => self.fm.set_op(5, 15, v),
            Param::Op1Level => self.fm.set_op(5, 16, v),
            Param::Op1Mode => self.fm.set_op(5, 17, v),
            Param::Op1Coarse => self.fm.set_op(5, 18, v),
            Param::Op1Fine => self.fm.set_op(5, 19, v),
            Param::Op1Detune => self.fm.set_op(5, 20, v),
            Param::Op2R1 => self.fm.set_op(4, 0, v),
            Param::Op2R2 => self.fm.set_op(4, 1, v),
            Param::Op2R3 => self.fm.set_op(4, 2, v),
            Param::Op2R4 => self.fm.set_op(4, 3, v),
            Param::Op2L1 => self.fm.set_op(4, 4, v),
            Param::Op2L2 => self.fm.set_op(4, 5, v),
            Param::Op2L3 => self.fm.set_op(4, 6, v),
            Param::Op2L4 => self.fm.set_op(4, 7, v),
            Param::Op2BreakPoint => self.fm.set_op(4, 8, v),
            Param::Op2LeftDepth => self.fm.set_op(4, 9, v),
            Param::Op2RightDepth => self.fm.set_op(4, 10, v),
            Param::Op2LeftCurve => self.fm.set_op(4, 11, v),
            Param::Op2RightCurve => self.fm.set_op(4, 12, v),
            Param::Op2RateScale => self.fm.set_op(4, 13, v),
            Param::Op2AmpSens => self.fm.set_op(4, 14, v),
            Param::Op2VelSens => self.fm.set_op(4, 15, v),
            Param::Op2Level => self.fm.set_op(4, 16, v),
            Param::Op2Mode => self.fm.set_op(4, 17, v),
            Param::Op2Coarse => self.fm.set_op(4, 18, v),
            Param::Op2Fine => self.fm.set_op(4, 19, v),
            Param::Op2Detune => self.fm.set_op(4, 20, v),
            Param::Op3R1 => self.fm.set_op(3, 0, v),
            Param::Op3R2 => self.fm.set_op(3, 1, v),
            Param::Op3R3 => self.fm.set_op(3, 2, v),
            Param::Op3R4 => self.fm.set_op(3, 3, v),
            Param::Op3L1 => self.fm.set_op(3, 4, v),
            Param::Op3L2 => self.fm.set_op(3, 5, v),
            Param::Op3L3 => self.fm.set_op(3, 6, v),
            Param::Op3L4 => self.fm.set_op(3, 7, v),
            Param::Op3BreakPoint => self.fm.set_op(3, 8, v),
            Param::Op3LeftDepth => self.fm.set_op(3, 9, v),
            Param::Op3RightDepth => self.fm.set_op(3, 10, v),
            Param::Op3LeftCurve => self.fm.set_op(3, 11, v),
            Param::Op3RightCurve => self.fm.set_op(3, 12, v),
            Param::Op3RateScale => self.fm.set_op(3, 13, v),
            Param::Op3AmpSens => self.fm.set_op(3, 14, v),
            Param::Op3VelSens => self.fm.set_op(3, 15, v),
            Param::Op3Level => self.fm.set_op(3, 16, v),
            Param::Op3Mode => self.fm.set_op(3, 17, v),
            Param::Op3Coarse => self.fm.set_op(3, 18, v),
            Param::Op3Fine => self.fm.set_op(3, 19, v),
            Param::Op3Detune => self.fm.set_op(3, 20, v),
            Param::Op4R1 => self.fm.set_op(2, 0, v),
            Param::Op4R2 => self.fm.set_op(2, 1, v),
            Param::Op4R3 => self.fm.set_op(2, 2, v),
            Param::Op4R4 => self.fm.set_op(2, 3, v),
            Param::Op4L1 => self.fm.set_op(2, 4, v),
            Param::Op4L2 => self.fm.set_op(2, 5, v),
            Param::Op4L3 => self.fm.set_op(2, 6, v),
            Param::Op4L4 => self.fm.set_op(2, 7, v),
            Param::Op4BreakPoint => self.fm.set_op(2, 8, v),
            Param::Op4LeftDepth => self.fm.set_op(2, 9, v),
            Param::Op4RightDepth => self.fm.set_op(2, 10, v),
            Param::Op4LeftCurve => self.fm.set_op(2, 11, v),
            Param::Op4RightCurve => self.fm.set_op(2, 12, v),
            Param::Op4RateScale => self.fm.set_op(2, 13, v),
            Param::Op4AmpSens => self.fm.set_op(2, 14, v),
            Param::Op4VelSens => self.fm.set_op(2, 15, v),
            Param::Op4Level => self.fm.set_op(2, 16, v),
            Param::Op4Mode => self.fm.set_op(2, 17, v),
            Param::Op4Coarse => self.fm.set_op(2, 18, v),
            Param::Op4Fine => self.fm.set_op(2, 19, v),
            Param::Op4Detune => self.fm.set_op(2, 20, v),
            Param::Op5R1 => self.fm.set_op(1, 0, v),
            Param::Op5R2 => self.fm.set_op(1, 1, v),
            Param::Op5R3 => self.fm.set_op(1, 2, v),
            Param::Op5R4 => self.fm.set_op(1, 3, v),
            Param::Op5L1 => self.fm.set_op(1, 4, v),
            Param::Op5L2 => self.fm.set_op(1, 5, v),
            Param::Op5L3 => self.fm.set_op(1, 6, v),
            Param::Op5L4 => self.fm.set_op(1, 7, v),
            Param::Op5BreakPoint => self.fm.set_op(1, 8, v),
            Param::Op5LeftDepth => self.fm.set_op(1, 9, v),
            Param::Op5RightDepth => self.fm.set_op(1, 10, v),
            Param::Op5LeftCurve => self.fm.set_op(1, 11, v),
            Param::Op5RightCurve => self.fm.set_op(1, 12, v),
            Param::Op5RateScale => self.fm.set_op(1, 13, v),
            Param::Op5AmpSens => self.fm.set_op(1, 14, v),
            Param::Op5VelSens => self.fm.set_op(1, 15, v),
            Param::Op5Level => self.fm.set_op(1, 16, v),
            Param::Op5Mode => self.fm.set_op(1, 17, v),
            Param::Op5Coarse => self.fm.set_op(1, 18, v),
            Param::Op5Fine => self.fm.set_op(1, 19, v),
            Param::Op5Detune => self.fm.set_op(1, 20, v),
            Param::Op6R1 => self.fm.set_op(0, 0, v),
            Param::Op6R2 => self.fm.set_op(0, 1, v),
            Param::Op6R3 => self.fm.set_op(0, 2, v),
            Param::Op6R4 => self.fm.set_op(0, 3, v),
            Param::Op6L1 => self.fm.set_op(0, 4, v),
            Param::Op6L2 => self.fm.set_op(0, 5, v),
            Param::Op6L3 => self.fm.set_op(0, 6, v),
            Param::Op6L4 => self.fm.set_op(0, 7, v),
            Param::Op6BreakPoint => self.fm.set_op(0, 8, v),
            Param::Op6LeftDepth => self.fm.set_op(0, 9, v),
            Param::Op6RightDepth => self.fm.set_op(0, 10, v),
            Param::Op6LeftCurve => self.fm.set_op(0, 11, v),
            Param::Op6RightCurve => self.fm.set_op(0, 12, v),
            Param::Op6RateScale => self.fm.set_op(0, 13, v),
            Param::Op6AmpSens => self.fm.set_op(0, 14, v),
            Param::Op6VelSens => self.fm.set_op(0, 15, v),
            Param::Op6Level => self.fm.set_op(0, 16, v),
            Param::Op6Mode => self.fm.set_op(0, 17, v),
            Param::Op6Coarse => self.fm.set_op(0, 18, v),
            Param::Op6Fine => self.fm.set_op(0, 19, v),
            Param::Op6Detune => self.fm.set_op(0, 20, v),
            Param::PitchR1 => self.fm.set_global(0, v),
            Param::PitchR2 => self.fm.set_global(1, v),
            Param::PitchR3 => self.fm.set_global(2, v),
            Param::PitchR4 => self.fm.set_global(3, v),
            Param::PitchL1 => self.fm.set_global(4, v),
            Param::PitchL2 => self.fm.set_global(5, v),
            Param::PitchL3 => self.fm.set_global(6, v),
            Param::PitchL4 => self.fm.set_global(7, v),
            Param::Algorithm => self.fm.set_global(8, v),
            Param::Feedback => self.fm.set_global(9, v),
            Param::OscSync => self.fm.set_global(10, v),
            Param::LfoSpeed => self.fm.set_global(11, v),
            Param::LfoDelay => self.fm.set_global(12, v),
            Param::LfoPitchDepth => self.fm.set_global(13, v),
            Param::LfoAmpDepth => self.fm.set_global(14, v),
            Param::LfoSync => self.fm.set_global(15, v),
            Param::LfoShape => self.fm.set_global(16, v),
            Param::PitchSens => self.fm.set_global(17, v),
            Param::Transpose => self.fm.set_global(18, v),
            Param::BdTune => self.pad(Pad::Bd, |p| p.tune = v),
            Param::BdDecay => self.pad(Pad::Bd, |p| p.decay = v),
            Param::BdTone => self.pad(Pad::Bd, |p| p.tone = v),
            Param::BdLevel => self.pad(Pad::Bd, |p| p.level = v),
            Param::BdDrive => self.pad(Pad::Bd, |p| p.drive = v),
            Param::SnTune => self.pad(Pad::Sn, |p| p.tune = v),
            Param::SnDecay => self.pad(Pad::Sn, |p| p.decay = v),
            Param::SnTone => self.pad(Pad::Sn, |p| p.tone = v),
            Param::SnLevel => self.pad(Pad::Sn, |p| p.level = v),
            Param::CpTune => self.pad(Pad::Cp, |p| p.tune = v),
            Param::CpDecay => self.pad(Pad::Cp, |p| p.decay = v),
            Param::CpTone => self.pad(Pad::Cp, |p| p.tone = v),
            Param::CpLevel => self.pad(Pad::Cp, |p| p.level = v),
            Param::ChTune => self.pad(Pad::Ch, |p| p.tune = v),
            Param::ChDecay => self.pad(Pad::Ch, |p| p.decay = v),
            Param::ChTone => self.pad(Pad::Ch, |p| p.tone = v),
            Param::ChLevel => self.pad(Pad::Ch, |p| p.level = v),
            Param::OhTune => self.pad(Pad::Oh, |p| p.tune = v),
            Param::OhDecay => self.pad(Pad::Oh, |p| p.decay = v),
            Param::OhTone => self.pad(Pad::Oh, |p| p.tone = v),
            Param::OhLevel => self.pad(Pad::Oh, |p| p.level = v),
            Param::LtTune => self.pad(Pad::Lt, |p| p.tune = v),
            Param::LtDecay => self.pad(Pad::Lt, |p| p.decay = v),
            Param::LtTone => self.pad(Pad::Lt, |p| p.tone = v),
            Param::LtLevel => self.pad(Pad::Lt, |p| p.level = v),
            Param::HtTune => self.pad(Pad::Ht, |p| p.tune = v),
            Param::HtDecay => self.pad(Pad::Ht, |p| p.decay = v),
            Param::HtTone => self.pad(Pad::Ht, |p| p.tone = v),
            Param::HtLevel => self.pad(Pad::Ht, |p| p.level = v),
            Param::CbTune => self.pad(Pad::Cb, |p| p.tune = v),
            Param::CbDecay => self.pad(Pad::Cb, |p| p.decay = v),
            Param::CbTone => self.pad(Pad::Cb, |p| p.tone = v),
            Param::CbLevel => self.pad(Pad::Cb, |p| p.level = v),
            Param::RsTune => self.pad(Pad::Rs, |p| p.tune = v),
            Param::RsDecay => self.pad(Pad::Rs, |p| p.decay = v),
            Param::RsTone => self.pad(Pad::Rs, |p| p.tone = v),
            Param::RsLevel => self.pad(Pad::Rs, |p| p.level = v),
            Param::ClTune => self.pad(Pad::Cl, |p| p.tune = v),
            Param::ClDecay => self.pad(Pad::Cl, |p| p.decay = v),
            Param::ClTone => self.pad(Pad::Cl, |p| p.tone = v),
            Param::ClLevel => self.pad(Pad::Cl, |p| p.level = v),
            Param::MaTune => self.pad(Pad::Ma, |p| p.tune = v),
            Param::MaDecay => self.pad(Pad::Ma, |p| p.decay = v),
            Param::MaTone => self.pad(Pad::Ma, |p| p.tone = v),
            Param::MaLevel => self.pad(Pad::Ma, |p| p.level = v),
            Param::CyTune => self.pad(Pad::Cy, |p| p.tune = v),
            Param::CyDecay => self.pad(Pad::Cy, |p| p.decay = v),
            Param::CyTone => self.pad(Pad::Cy, |p| p.tone = v),
            Param::CyLevel => self.pad(Pad::Cy, |p| p.level = v),
            Param::MtTune => self.pad(Pad::Mt, |p| p.tune = v),
            Param::MtDecay => self.pad(Pad::Mt, |p| p.decay = v),
            Param::MtTone => self.pad(Pad::Mt, |p| p.tone = v),
            Param::MtLevel => self.pad(Pad::Mt, |p| p.level = v),
            Param::LcTune => self.pad(Pad::Lc, |p| p.tune = v),
            Param::LcDecay => self.pad(Pad::Lc, |p| p.decay = v),
            Param::LcTone => self.pad(Pad::Lc, |p| p.tone = v),
            Param::LcLevel => self.pad(Pad::Lc, |p| p.level = v),
            Param::McTune => self.pad(Pad::Mc, |p| p.tune = v),
            Param::McDecay => self.pad(Pad::Mc, |p| p.decay = v),
            Param::McTone => self.pad(Pad::Mc, |p| p.tone = v),
            Param::McLevel => self.pad(Pad::Mc, |p| p.level = v),
            Param::HcTune => self.pad(Pad::Hc, |p| p.tune = v),
            Param::HcDecay => self.pad(Pad::Hc, |p| p.decay = v),
            Param::HcTone => self.pad(Pad::Hc, |p| p.tone = v),
            Param::HcLevel => self.pad(Pad::Hc, |p| p.level = v),
            Param::DrumAccent => self.drum_accent = v,
            Param::CrTune => self.pad(Pad::Cr, |p| p.tune = v),
            Param::CrDecay => self.pad(Pad::Cr, |p| p.decay = v),
            Param::CrTone => self.pad(Pad::Cr, |p| p.tone = v),
            Param::CrLevel => self.pad(Pad::Cr, |p| p.level = v),
            Param::CrOut => self.pad_out(Pad::Cr, |o| o.group = v.round() as usize),
            Param::CrPan => self.pad_out(Pad::Cr, |o| o.set_pan(v)),
            Param::RdTune => self.pad(Pad::Rd, |p| p.tune = v),
            Param::RdDecay => self.pad(Pad::Rd, |p| p.decay = v),
            Param::RdTone => self.pad(Pad::Rd, |p| p.tone = v),
            Param::RdLevel => self.pad(Pad::Rd, |p| p.level = v),
            Param::RdOut => self.pad_out(Pad::Rd, |o| o.group = v.round() as usize),
            Param::RdPan => self.pad_out(Pad::Rd, |o| o.set_pan(v)),
            Param::BdOut => self.pad_out(Pad::Bd, |o| o.group = v.round() as usize),
            Param::BdPan => self.pad_out(Pad::Bd, |o| o.set_pan(v)),
            Param::SnOut => self.pad_out(Pad::Sn, |o| o.group = v.round() as usize),
            Param::SnPan => self.pad_out(Pad::Sn, |o| o.set_pan(v)),
            Param::CpOut => self.pad_out(Pad::Cp, |o| o.group = v.round() as usize),
            Param::CpPan => self.pad_out(Pad::Cp, |o| o.set_pan(v)),
            Param::ChOut => self.pad_out(Pad::Ch, |o| o.group = v.round() as usize),
            Param::ChPan => self.pad_out(Pad::Ch, |o| o.set_pan(v)),
            Param::OhOut => self.pad_out(Pad::Oh, |o| o.group = v.round() as usize),
            Param::OhPan => self.pad_out(Pad::Oh, |o| o.set_pan(v)),
            Param::LtOut => self.pad_out(Pad::Lt, |o| o.group = v.round() as usize),
            Param::LtPan => self.pad_out(Pad::Lt, |o| o.set_pan(v)),
            Param::HtOut => self.pad_out(Pad::Ht, |o| o.group = v.round() as usize),
            Param::HtPan => self.pad_out(Pad::Ht, |o| o.set_pan(v)),
            Param::CbOut => self.pad_out(Pad::Cb, |o| o.group = v.round() as usize),
            Param::CbPan => self.pad_out(Pad::Cb, |o| o.set_pan(v)),
            Param::RsOut => self.pad_out(Pad::Rs, |o| o.group = v.round() as usize),
            Param::RsPan => self.pad_out(Pad::Rs, |o| o.set_pan(v)),
            Param::ClOut => self.pad_out(Pad::Cl, |o| o.group = v.round() as usize),
            Param::ClPan => self.pad_out(Pad::Cl, |o| o.set_pan(v)),
            Param::MaOut => self.pad_out(Pad::Ma, |o| o.group = v.round() as usize),
            Param::MaPan => self.pad_out(Pad::Ma, |o| o.set_pan(v)),
            Param::CyOut => self.pad_out(Pad::Cy, |o| o.group = v.round() as usize),
            Param::CyPan => self.pad_out(Pad::Cy, |o| o.set_pan(v)),
            Param::MtOut => self.pad_out(Pad::Mt, |o| o.group = v.round() as usize),
            Param::MtPan => self.pad_out(Pad::Mt, |o| o.set_pan(v)),
            Param::LcOut => self.pad_out(Pad::Lc, |o| o.group = v.round() as usize),
            Param::LcPan => self.pad_out(Pad::Lc, |o| o.set_pan(v)),
            Param::McOut => self.pad_out(Pad::Mc, |o| o.group = v.round() as usize),
            Param::McPan => self.pad_out(Pad::Mc, |o| o.set_pan(v)),
            Param::HcOut => self.pad_out(Pad::Hc, |o| o.group = v.round() as usize),
            Param::HcPan => self.pad_out(Pad::Hc, |o| o.set_pan(v)),
            Param::Lfo2Rate => self.lfo2_inc = v / self.sample_rate,
            Param::Lfo2Wave => {
                if let Some(w) = Waveform::from_id(v.round() as u32) {
                    self.lfo2_wave = w;
                }
            }
            Param::RampTime => self.ramp_inc = 1.0 / (v * self.sample_rate),

            Param::Model => {
                if let Some(m) = Model::from_id(v.round() as u32) {
                    self.model = m;
                    self.normals.cutoff_from_fenv = m.cutoff_follows_filter_env();
                    self.normals.hp_from_ar = m.hp_follows_ar();
                    self.normals.mod_from_osc3 = m.modulates_with_osc3();
                }
            }
            Param::MasterGain => {}
        }
        // A modulation matrix adds to its destinations; a patch panel replaces the normals.
        self.taken = if self.model.has_matrix() {
            [false; DESTS]
        } else {
            self.patch.overridden()
        };
    }

    /// Change where a pad goes.
    fn pad_out(&mut self, pad: Pad, f: impl FnOnce(&mut PadOut)) {
        if let Some(o) = self.pad_outs.get_mut(pad as usize) {
            f(o);
        }
    }

    /// The groups the drum kit's or pad sampler's pads go to, as a bit per group (bit 0 is
    /// group 1); none unless the synth is one of them.
    pub fn pad_groups(&self) -> u8 {
        if self.model.uses_pads() {
            // A pad sampler's pads (#220).
            return (0..crate::padsampler::PADS)
                .filter_map(|i| self.pad_kit.pad(i))
                .filter(|c| (1..=8).contains(&c.out))
                .fold(0, |m, c| m | 1 << (c.out - 1));
        }
        if !self.model.uses_drums() {
            return 0;
        }
        self.pad_outs
            .iter()
            .filter(|o| (1..=8).contains(&o.group))
            .fold(0, |m, o| m | 1 << (o.group - 1))
    }

    /// Change a pad's knobs.
    fn pad(&mut self, pad: Pad, f: impl FnOnce(&mut PadParams)) {
        if let Some(p) = self.drums.get_mut(pad as usize) {
            f(p);
        }
    }

    fn set_wave(&mut self, vco: usize, v: f32) {
        if let (Some(w), Some(slot)) = (Waveform::from_id(v.round() as u32), self.wave.get_mut(vco))
        {
            *slot = w;
        }
    }

    /// Set coarse and/or fine tune; the voice adds the sum to its pitch.
    fn set_tune(&mut self, vco: usize, coarse: Option<f32>, fine: Option<f32>) {
        if let (Some(c), Some(slot)) = (coarse, self.coarse.get_mut(vco)) {
            *slot = c;
        }
        if let (Some(f), Some(slot)) = (fine, self.fine.get_mut(vco)) {
            *slot = f;
        }
        let semis = self.coarse.get(vco).copied().unwrap_or(0.0)
            + self.fine.get(vco).copied().unwrap_or(0.0) / 100.0;
        if let Some(t) = self.tune.get_mut(vco) {
            *t = semis;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #308: an on/off switch zeroes what its knob sets and gives it back.
    #[test]
    fn switches_gate_their_knobs() {
        let mut p = MonoParams::new(48_000.0);
        let mut set = |param: Param, v: f32| p.set(param, param.clamp(v));
        set(Param::Vco2Level, 0.6);
        set(Param::NoiseLevel, 0.4);
        set(Param::Glide, 0.5);
        set(Param::Vibrato, 0.5);
        set(Param::LfoCutoff, 0.5);
        for (switch, v) in [
            (Param::Vco2On, 0.0),
            (Param::NoiseOn, 0.0),
            (Param::GlideOn, 0.0),
            (Param::OscModOn, 0.0),
            (Param::FilterModOn, 0.0),
        ] {
            set(switch, v);
        }
        let off = (
            p.level[1],
            p.noise_level,
            p.glide,
            p.normals.vibrato,
            p.normals.lfo_cutoff,
        );
        assert_eq!(off, (0.0, 0.0, 0.0, 0.0, 0.0));
        let mut set = |param: Param, v: f32| p.set(param, param.clamp(v));
        for switch in [
            Param::Vco2On,
            Param::NoiseOn,
            Param::GlideOn,
            Param::OscModOn,
            Param::FilterModOn,
        ] {
            set(switch, 1.0);
        }
        assert_eq!(
            (
                p.level[1],
                p.noise_level,
                p.glide,
                p.normals.vibrato,
                p.normals.lfo_cutoff
            ),
            (0.6, 0.4, 24_000.0, 1.0, 12.0)
        );
    }

    /// Each parameter lands in the voice in the units `render` uses.
    #[test]
    fn values_are_converted_for_render() {
        let mut p = MonoParams::new(48_000.0);
        let mut set = |param: Param, v: f32| p.set(param, param.clamp(v));
        set(Param::Resonance, 0.8);
        set(Param::Drive, 1.0);
        set(Param::AdsrAttack, 0.01);
        set(Param::AdsrSustain, 0.25);
        set(Param::ArRelease, 2.0);
        set(Param::LfoRate, 4.0);
        set(Param::LfoWave, 1.0);
        set(Param::Cutoff, 440.0);
        set(Param::Vco3Level, 0.6);
        set(Param::PulseWidth, 0.2);
        assert!(
            (p.k - 4.0).abs() < 1.0e-6,
            "resonance 0.8 is the self-oscillation threshold"
        );
        assert_eq!(p.drive, 8.0);
        assert_eq!(p.adsr.attack, 480.0);
        assert_eq!(p.adsr.sustain, 0.25);
        assert_eq!(p.ar.release, 96_000.0);
        assert_eq!(p.lfo_inc, 4.0 / 48_000.0);
        assert_eq!(p.lfo_wave, Waveform::Pulse);
        assert!((p.cutoff - 69.0).abs() < 1.0e-4);
        assert_eq!((p.level[2], p.pulse_width), (0.6, 0.2));
    }

    #[test]
    fn coarse_and_fine_set_the_tune() {
        let mut p = MonoParams::default();
        p.set(Param::Vco2Coarse, Param::Vco2Coarse.clamp(12.4));
        assert_eq!(p.tune[1], 12.0, "coarse is whole semitones");
        p.set(Param::Vco2Fine, Param::Vco2Fine.clamp(-50.0));
        assert!((p.tune[1] - 11.5).abs() < 1.0e-6);
        assert_eq!(p.tune[0], 0.0);
        p.set(Param::Glide, 0.1);
        p.set(Param::Priority, 2.0);
        p.set(Param::Legato, 1.0);
        assert_eq!(
            (p.glide, p.priority, p.legato),
            (4_800.0, NotePriority::High, true)
        );
    }

    #[test]
    fn ids_and_switches_come_from_their_values() {
        let mut p = MonoParams::default();
        p.set(Param::Vco3Wave, Param::Vco3Wave.clamp(2.4));
        assert_eq!(p.wave[2], Waveform::Triangle);
        p.set(Param::Vco3Sync, Param::Vco3Sync.clamp(1.0));
        assert!(p.sync[2] && !p.sync[1]);
        // Values outside the range are clamped first: 7 becomes Sine.
        p.set(Param::Vco1Wave, Param::Vco1Wave.clamp(7.0));
        assert_eq!(p.wave[0], Waveform::Sine);
        p.set(Param::NoiseColour, Param::NoiseColour.clamp(1.0));
        p.set(Param::NoiseLevel, Param::NoiseLevel.clamp(0.5));
        assert_eq!((p.noise_colour, p.noise_level), (NoiseColour::Pink, 0.5));
    }
}
