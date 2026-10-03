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

use crate::params::Param;
use env::EnvTimes;
use ladder::{MAX_K, hz_to_note};
use model::Model;
use noise::NoiseColour;
use osc::Waveform;
use patch::{Normals, Patch};
use voice::NotePriority;

/// Number of VCOs per Mono voice.
pub const VCOS: usize = 3;

/// The Mono parameters, with what `render` needs precomputed.
#[derive(Clone, Copy)]
pub struct MonoParams {
    /// Which instrument this synth is (spec 005).
    pub model: Model,
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
    pub taken: [bool; 8],
    pub normals: Normals,
    pub mod_wheel: f32,
}

impl Default for MonoParams {
    fn default() -> MonoParams {
        MonoParams::new(48_000.0)
    }
}

impl MonoParams {
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
            adsr: off,
            ar: off,
            fadsr: off,
            lfo_inc: 0.0,
            lfo_wave: Waveform::Sine,
            priority: NotePriority::Last,
            legato: false,
            glide: 0.0,
            patch: Patch::default(),
            taken: [false; 8],
            normals: Normals::default(),
            mod_wheel: 0.0,
        };
        for (param, v) in preset::DEFAULTS {
            p.set(param, param.clamp(v));
        }
        p
    }

    /// Apply an already clamped value; parameters that aren't Mono's are
    /// ignored. The match is exhaustive on purpose: a new `Param` doesn't
    /// compile until it is handled here.
    pub fn set(&mut self, param: Param, v: f32) {
        let samples = v * self.sample_rate;
        match param {
            Param::Vco1Wave => self.set_wave(0, v),
            Param::Vco2Wave => self.set_wave(1, v),
            Param::Vco3Wave => self.set_wave(2, v),
            Param::Vco1Coarse => self.set_tune(0, Some(v.round()), None),
            Param::Vco2Coarse => self.set_tune(1, Some(v.round()), None),
            Param::Vco3Coarse => self.set_tune(2, Some(v.round()), None),
            Param::Vco1Fine => self.set_tune(0, None, Some(v)),
            Param::Vco2Fine => self.set_tune(1, None, Some(v)),
            Param::Vco3Fine => self.set_tune(2, None, Some(v)),
            Param::Vco1Level => self.level[0] = v,
            Param::Vco2Level => self.level[1] = v,
            Param::Vco3Level => self.level[2] = v,
            Param::PulseWidth => self.pulse_width = v,
            Param::Vco2Sync => self.sync[1] = v >= 0.5,
            Param::Vco3Sync => self.sync[2] = v >= 0.5,
            Param::NoiseLevel => self.noise_level = v,
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
            Param::Glide => self.glide = samples,
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
            Param::Vibrato => self.normals.vibrato = 2.0 * v,
            Param::LfoCutoff => self.normals.lfo_cutoff = 24.0 * v,
            Param::LfoPw => self.normals.lfo_pw = 0.45 * v,
            Param::EnvFreq2 => self.normals.env_freq2 = 24.0 * v,
            Param::OscFreq2 => self.normals.osc_freq2 = 24.0 * v,
            Param::EnvPw => self.normals.env_pw = 0.45 * v,
            Param::OscPw => self.normals.osc_pw = 0.45 * v,
            Param::OscCutoff => self.normals.osc_cutoff = 48.0 * v,
            Param::ModWheel => self.mod_wheel = v,
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
        self.taken = self.patch.overridden();
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
