//! The ARP 2600-style Mono voice (spec 004).
//!
//! `osc` holds the VCOs, `noise` the noise source, `ladder` the filter,
//! `env` the ADSR and AR, `lfo` the LFO and sample-and-hold, `preset` the
//! defaults and presets. The settings
//! here are Mono-wide until tracks address parameters per instance
//! (spec 002 Req 1).

pub mod env;
pub mod ladder;
pub mod lfo;
pub mod noise;
pub mod osc;
pub mod preset;

use crate::params::Param;
use env::EnvTimes;
use ladder::{MAX_K, hz_to_note};
use noise::NoiseColour;
use osc::Waveform;

/// Number of VCOs per Mono voice.
pub const VCOS: usize = 3;

/// The Mono parameters, with what `render` needs precomputed.
#[derive(Clone, Copy)]
pub struct MonoParams {
    pub wave: [Waveform; VCOS],
    /// Coarse tune in semitones and fine tune in cents, per VCO.
    coarse: [f32; VCOS],
    fine: [f32; VCOS],
    /// Frequency ratio from coarse and fine, computed when either changes.
    pub ratio: [f32; VCOS],
    pub level: [f32; VCOS],
    pub pulse_width: f32,
    /// Whether VCO 2 and VCO 3 reset with VCO 1; VCO 1's entry is unused.
    pub sync: [bool; VCOS],
    pub noise_level: f32,
    pub noise_colour: NoiseColour,
    /// Ladder cutoff as a MIDI note, feedback, and input gain.
    pub cutoff: f32,
    pub k: f32,
    pub drive: f32,
    /// Envelope times in samples, so the sample rate is kept to convert.
    sample_rate: f32,
    pub adsr: EnvTimes,
    pub ar: EnvTimes,
    /// LFO cycles per sample, and its waveform.
    pub lfo_inc: f32,
    pub lfo_wave: Waveform,
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
            wave: [Waveform::Saw; VCOS],
            coarse: [0.0; VCOS],
            fine: [0.0; VCOS],
            ratio: [1.0; VCOS],
            level: [0.0; VCOS],
            pulse_width: 0.5,
            sync: [false; VCOS],
            noise_level: 0.0,
            noise_colour: NoiseColour::White,
            cutoff: 0.0,
            k: 0.0,
            drive: 1.0,
            sample_rate,
            adsr: off,
            ar: off,
            lfo_inc: 0.0,
            lfo_wave: Waveform::Sine,
        };
        for (param, v) in preset::DEFAULTS {
            p.set(param, param.clamp(v));
        }
        p
    }

    /// Apply an already clamped value; parameters that aren't Mono's are
    /// ignored.
    pub fn set(&mut self, param: Param, v: f32) {
        use Param::*;
        let (vco, what) = match param {
            Vco1Wave | Vco1Coarse | Vco1Fine | Vco1Level => (0, param),
            Vco2Wave | Vco2Coarse | Vco2Fine | Vco2Level | Vco2Sync => (1, param),
            Vco3Wave | Vco3Coarse | Vco3Fine | Vco3Level | Vco3Sync => (2, param),
            PulseWidth => {
                self.pulse_width = v;
                return;
            }
            NoiseLevel => {
                self.noise_level = v;
                return;
            }
            Param::NoiseColour => {
                if let Some(c) = noise::NoiseColour::from_id(v.round() as u32) {
                    self.noise_colour = c;
                }
                return;
            }
            AdsrAttack | AdsrDecay | AdsrSustain | AdsrRelease | ArAttack | ArRelease | LfoRate
            | LfoWave => {
                self.set_modulation(param, v);
                return;
            }
            Cutoff => {
                self.cutoff = hz_to_note(v);
                return;
            }
            Resonance => {
                self.k = v * MAX_K;
                return;
            }
            Drive => {
                // 0..=1 is 0 to +18 dB: 1 + 7·v.
                self.drive = 1.0 + 7.0 * v;
                return;
            }
            MasterGain | Attack | Release => return,
        };
        match what {
            Vco1Wave | Vco2Wave | Vco3Wave => {
                if let (Some(w), Some(slot)) =
                    (Waveform::from_id(v.round() as u32), self.wave.get_mut(vco))
                {
                    *slot = w;
                }
            }
            Vco1Coarse | Vco2Coarse | Vco3Coarse => {
                if let Some(c) = self.coarse.get_mut(vco) {
                    *c = v.round();
                }
                self.retune(vco);
            }
            Vco1Fine | Vco2Fine | Vco3Fine => {
                if let Some(f) = self.fine.get_mut(vco) {
                    *f = v;
                }
                self.retune(vco);
            }
            Vco1Level | Vco2Level | Vco3Level => {
                if let Some(l) = self.level.get_mut(vco) {
                    *l = v;
                }
            }
            Vco2Sync | Vco3Sync => {
                if let Some(s) = self.sync.get_mut(vco) {
                    *s = v >= 0.5;
                }
            }
            _ => {}
        }
    }

    /// Envelope times arrive in seconds and are kept in samples.
    fn set_modulation(&mut self, param: Param, v: f32) {
        let samples = v * self.sample_rate;
        match param {
            Param::AdsrAttack => self.adsr.attack = samples,
            Param::AdsrDecay => self.adsr.decay = samples,
            Param::AdsrSustain => self.adsr.sustain = v,
            Param::AdsrRelease => self.adsr.release = samples,
            Param::ArAttack => self.ar.attack = samples,
            Param::ArRelease => self.ar.release = samples,
            Param::LfoRate => self.lfo_inc = v / self.sample_rate,
            Param::LfoWave => {
                if let Some(w) = Waveform::from_id(v.round() as u32) {
                    self.lfo_wave = w;
                }
            }
            _ => {}
        }
    }

    /// Per parameter change, not per sample, so `exp2` is fine.
    fn retune(&mut self, vco: usize) {
        let semis = self.coarse.get(vco).copied().unwrap_or(0.0)
            + self.fine.get(vco).copied().unwrap_or(0.0) / 100.0;
        if let Some(r) = self.ratio.get_mut(vco) {
            *r = (semis / 12.0).exp2();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coarse_and_fine_set_the_ratio() {
        let mut p = MonoParams::default();
        p.set(Param::Vco2Coarse, Param::Vco2Coarse.clamp(12.0));
        assert!((p.ratio[1] - 2.0).abs() < 1.0e-6);
        p.set(Param::Vco2Fine, Param::Vco2Fine.clamp(-50.0));
        let expected = (11.5_f32 / 12.0).exp2();
        assert!((p.ratio[1] - expected).abs() < 1.0e-6);
        assert_eq!(p.ratio[0], 1.0);
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
