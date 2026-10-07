//! One definition per instrument (#330, ADR-0025): what each `Model` is,
//! and its presets, in `synth/<model>.rs`. `Model::def` is the one place
//! that goes from a model to its definition; the voice reads the rest.

use crate::drums::Machine;
use crate::mono::model::{D50, Filter, Hp, IDEAL_VCO, Model, OscVoicing, Setting};
use crate::mono::preset::Preset;
use crate::params::Param;

mod arp2600;
mod cs15;
mod d50;
mod dx7;
mod juno106;
mod jupiter8;
mod matrix12;
mod minimoog;
mod modular;
mod ms20;
mod odyssey;
mod padsampler;
mod polymoog;
mod ppgwave;
mod proone;
mod prophet5;
mod sampler;
mod sh101;
mod tr808;
mod tr909;

/// The voice a model plays with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// The Mono voice, monophonic or in a poly pool.
    Mono,
    /// The D-50's two-partial LA voice (spec 006 Req 12).
    La,
    /// The DX7's six-operator FM voice (spec 006 Req 13).
    Fm,
    /// A drum machine's pads (#114, #148).
    Drums(Machine),
    /// The multisampler (#123).
    Sampler,
    /// The drum/pad sampler (#124).
    Pads,
    /// A graph of unit generators (ADR-0020).
    Graph,
}

/// What a model is: plain data, read by the voice; nothing is boxed or
/// dispatched (ADR-0002, ADR-0009).
#[derive(Clone, Copy, Debug)]
pub struct ModelDef {
    /// How many voices the instrument has at most (spec 006 Req 1).
    pub voices: usize,
    pub engine: Engine,
    /// The low-pass and its voicing; on a model without one, unused.
    pub filter: Filter,
    /// The 12 dB setting of the slope switch, if it has one.
    pub slope12: Option<Filter>,
    /// The low-pass at Rev 1 and Rev 2 of the revision switch, if it has
    /// one; Rev 3 is `filter` (#321).
    pub revs: [Option<Filter>; 2],
    /// The high-pass stage.
    pub hp: Hp,
    /// How the oscillators are voiced (#339).
    pub osc: OscVoicing,
    /// VCO 1 and VCO 2 are wavetable oscillators (spec 006 Req 11).
    pub uses_tables: bool,
    /// The second LFO and the ramp of the Matrix-12's modulation matrix.
    pub has_matrix: bool,
    /// The filter envelope source (`ModSource::Fenv`) is the ADSR.
    pub filter_env_is_adsr: bool,
    /// VCO 2 is VCO 1's pulse output, phase-locked and at its pitch.
    pub pulse_locked: bool,
    /// A time set as decay is also the release.
    pub decay_is_release: bool,
    /// VCO 3, not the LFO, modulates the normals.
    pub modulates_with_osc3: bool,
    /// The high-pass cutoff follows the AR envelope, not the filter ADSR.
    pub hp_follows_ar: bool,
    /// The normalled cutoff follows the filter ADSR (spec 004 Req 12).
    pub cutoff_follows_filter_env: bool,
    /// Its presets, in the order the view lists them.
    pub presets: &'static [PresetDef],
}

impl ModelDef {
    /// A Mono voice with none of the switches and quirks: what a
    /// definition does not say.
    pub const MONO: ModelDef = ModelDef {
        voices: crate::poly::MAX_VOICES,
        engine: Engine::Mono,
        filter: Filter::Ladder(D50),
        slope12: None,
        revs: [None; 2],
        hp: Hp::None,
        osc: IDEAL_VCO,
        uses_tables: false,
        has_matrix: false,
        filter_env_is_adsr: false,
        pulse_locked: false,
        decay_is_release: false,
        modulates_with_osc3: false,
        hp_follows_ar: false,
        cutoff_follows_filter_env: true,
        presets: &[],
    };

    /// The low-pass at the panel's switches; `filter` where none applies.
    pub fn low_pass(&self, s: Setting) -> Filter {
        let slope = self.slope12.filter(|_| s.slope12);
        let rev = match s.rev {
            1 => self.revs[0],
            2 => self.revs[1],
            _ => None,
        };
        slope.or(rev).unwrap_or(self.filter)
    }
}

/// A preset: what it changes from `preset::DEFAULTS`, and a Modular
/// preset's SuperCollider code (ADR-0024).
#[derive(Clone, Copy, Debug)]
pub struct PresetDef {
    pub preset: Preset,
    pub changes: &'static [(Param, f32)],
    pub code: Option<&'static str>,
}

impl PresetDef {
    pub const fn of(preset: Preset, changes: &'static [(Param, f32)]) -> PresetDef {
        PresetDef {
            preset,
            changes,
            code: None,
        }
    }
}

impl Model {
    /// What the model is.
    pub fn def(self) -> &'static ModelDef {
        match self {
            Model::Arp2600 => &arp2600::DEF,
            Model::Minimoog => &minimoog::DEF,
            Model::ProOne => &proone::DEF,
            Model::Ms20 => &ms20::DEF,
            Model::Cs15 => &cs15::DEF,
            Model::Sh101 => &sh101::DEF,
            Model::Odyssey => &odyssey::DEF,
            Model::Prophet5 => &prophet5::DEF,
            Model::Juno106 => &juno106::DEF,
            Model::Jupiter8 => &jupiter8::DEF,
            Model::Matrix12 => &matrix12::DEF,
            Model::PpgWave => &ppgwave::DEF,
            Model::D50 => &d50::DEF,
            Model::Dx7 => &dx7::DEF,
            Model::PolyMoog => &polymoog::DEF,
            Model::Tr808 => &tr808::DEF,
            Model::Sampler => &sampler::DEF,
            Model::PadSampler => &padsampler::DEF,
            Model::Tr909 => &tr909::DEF,
            Model::Modular => &modular::DEF,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every preset is in exactly one definition, so its model is found.
    #[test]
    fn every_preset_has_one_home() {
        for (p, name) in Preset::ALL {
            let homes = Model::ALL
                .iter()
                .filter(|(m, _)| m.def().presets.iter().any(|d| d.preset == p))
                .count();
            assert_eq!(homes, 1, "{name}");
        }
    }

    /// Only a model with an engine of its own leaves the Mono voice.
    #[test]
    fn engines_name_their_models() {
        for (m, name) in Model::ALL {
            let mono = matches!(
                m,
                Model::D50
                    | Model::Dx7
                    | Model::Tr808
                    | Model::Tr909
                    | Model::Sampler
                    | Model::PadSampler
                    | Model::Modular
            );
            assert_eq!(m.def().engine == Engine::Mono, !mono, "{name}");
        }
    }
}
