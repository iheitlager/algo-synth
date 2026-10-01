//! The ARP 2600-style Mono voice (spec 004).
//!
//! `osc` holds the VCOs. The settings here are Mono-wide until tracks address
//! parameters per instance (spec 002 Req 1).

pub mod osc;

use crate::params::Param;
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
}

impl Default for MonoParams {
    /// VCO 1 alone, a saw: the voice sounds as it did before the VCOs.
    fn default() -> MonoParams {
        MonoParams {
            wave: [Waveform::Saw; VCOS],
            coarse: [0.0; VCOS],
            fine: [0.0; VCOS],
            ratio: [1.0; VCOS],
            level: [1.0, 0.0, 0.0],
            pulse_width: 0.5,
            sync: [false; VCOS],
        }
    }
}

impl MonoParams {
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
    fn wave_and_sync_come_from_their_values() {
        let mut p = MonoParams::default();
        p.set(Param::Vco3Wave, Param::Vco3Wave.clamp(2.4));
        assert_eq!(p.wave[2], Waveform::Triangle);
        p.set(Param::Vco3Sync, Param::Vco3Sync.clamp(1.0));
        assert!(p.sync[2] && !p.sync[1]);
        // Values outside the range are clamped first: 7 becomes Sine.
        p.set(Param::Vco1Wave, Param::Vco1Wave.clamp(7.0));
        assert_eq!(p.wave[0], Waveform::Sine);
    }
}
