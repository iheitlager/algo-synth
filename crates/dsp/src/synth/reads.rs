//! Which parameters a model reads (#673, ADR-0037): one its track's model
//! never reads is a parse error, not silence. Every model reads the strip,
//! the arpeggiator and the voice pool; the rest goes by engine, and on the
//! Mono voice by what the model has.

use super::Engine;
use crate::mono::model::Model;
use crate::params::Param;
use crate::song::param_name;

/// What part of an instrument a parameter belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    /// The strip, the master, the arpeggiator and the voice pool.
    Any,
    ModWheel,
    Bend,
    /// The DX7's operators, pitch envelope and LFO.
    Fm,
    /// The D-50's second partial and its PCM attacks.
    La,
    /// The PPG's wavetables.
    Tables,
    /// The Matrix-12's second LFO, ramp and patch slots 9 to 20.
    Matrix,
    /// The LFO of the Mono voice.
    Lfo,
    Drums,
    Graph,
    Sid,
    Nes,
    Vl,
    /// The Mono voice: oscillators, filters, envelopes, patch slots 1 to 8.
    Mono,
}

/// The two-letter drum pads; a pad's parameters are its name, then one of
/// `PAD_FIELDS`.
const PADS: [&str; 18] = [
    "Bd", "Sn", "Cp", "Ch", "Oh", "Lt", "Ht", "Cb", "Rs", "Cl", "Ma", "Cy", "Mt", "Lc", "Mc", "Hc",
    "Cr", "Rd",
];
const PAD_FIELDS: [&str; 6] = ["Tune", "Decay", "Tone", "Level", "Out", "Pan"];

/// What the multisampler reads of the Mono voice's parameters.
const SAMPLER: [Param; 15] = [
    Param::Vco1Coarse,
    Param::Vco1Fine,
    Param::Vco1Level,
    Param::Cutoff,
    Param::Resonance,
    Param::EnvCutoff,
    Param::KeyTrack,
    Param::AdsrAttack,
    Param::AdsrDecay,
    Param::AdsrSustain,
    Param::AdsrRelease,
    Param::FenvAttack,
    Param::FenvDecay,
    Param::FenvSustain,
    Param::FenvRelease,
];

const ADSR: [Param; 4] = [
    Param::AdsrAttack,
    Param::AdsrDecay,
    Param::AdsrSustain,
    Param::AdsrRelease,
];

fn family(p: Param) -> Family {
    use Param::*;
    if p.is_strip() || p.is_global() || p.is_arp() {
        return Family::Any;
    }
    match p {
        Model | Out | Polyphony | Assign | UnisonDetune | Analog | Legato | Priority | Glide
        | GlideOn | ChorusMode | Detune | Humanize => return Family::Any,
        ModWheel => return Family::ModWheel,
        PitchBend | BendRange => return Family::Bend,
        Algorithm | Feedback | OscSync | LfoSpeed | LfoDelay | LfoPitchDepth | LfoAmpDepth
        | LfoSync | LfoShape | PitchSens | Transpose => return Family::Fm,
        Structure | Pcm1Sample | Pcm2Sample => return Family::La,
        Wt1Table | Wt1Pos | Wt2Table | Wt2Pos | WtSteps | EnvWt | LfoWt => return Family::Tables,
        Lfo2Rate | Lfo2Wave | RampTime => return Family::Matrix,
        LfoRate | LfoWave => return Family::Lfo,
        DrumAccent | BdDrive => return Family::Drums,
        _ => {}
    }
    if p.ctl().is_some() {
        return Family::Graph;
    }
    if p.sid().is_some() {
        return Family::Sid;
    }
    if p.nes().is_some() {
        return Family::Nes;
    }
    if p.vl().is_some() {
        return Family::Vl;
    }
    let name = param_name(p);
    let rest = |prefix: &str| name.strip_prefix(prefix);
    if rest("Op").is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
        || rest("Pitch").is_some_and(|r| r.starts_with(['R', 'L']))
    {
        return Family::Fm;
    }
    if rest("P2").is_some() {
        return Family::La;
    }
    if rest("Patch")
        .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse::<u32>().ok())
        .is_some_and(|n| n > 8)
    {
        return Family::Matrix;
    }
    if PADS
        .iter()
        .any(|pad| rest(pad).is_some_and(|r| PAD_FIELDS.contains(&r)))
    {
        return Family::Drums;
    }
    Family::Mono
}

impl Model {
    /// Does the model read `p`? Its voice, or its strip, arpeggiator or
    /// voice pool (#673).
    pub fn reads(self, p: Param) -> bool {
        let d = self.def();
        let mono = matches!(d.engine, Engine::Mono | Engine::La);
        match family(p) {
            Family::Any => true,
            Family::ModWheel => {
                mono || matches!(
                    d.engine,
                    Engine::Fm | Engine::Sid | Engine::Vl | Engine::Graph
                )
            }
            Family::Bend => !matches!(d.engine, Engine::Drums(_) | Engine::Pads),
            Family::Fm => d.engine == Engine::Fm,
            Family::La => d.engine == Engine::La,
            Family::Tables => mono && d.uses_tables,
            Family::Matrix => mono && d.has_matrix,
            Family::Lfo => mono && !d.modulates_with_osc3,
            Family::Drums => matches!(d.engine, Engine::Drums(_)),
            Family::Graph => d.engine == Engine::Graph,
            Family::Sid => d.engine == Engine::Sid,
            Family::Nes => d.engine == Engine::Nes,
            Family::Vl => d.engine == Engine::Vl,
            Family::Mono => match d.engine {
                Engine::Mono | Engine::La => true,
                Engine::Sampler => SAMPLER.contains(&p),
                Engine::Pads => p == Param::Vco1Level,
                Engine::Graph => ADSR.contains(&p),
                Engine::Sid | Engine::Vl => p == Param::Vibrato,
                Engine::Fm | Engine::Drums(_) | Engine::Nes => false,
            },
        }
    }

    /// Why the model doesn't read `p`, with its own parameter for the job
    /// where it has one.
    pub fn unread(self, p: Param) -> &'static str {
        let d = self.def();
        let fam = family(p);
        if d.modulates_with_osc3
            && matches!(fam, Family::Lfo | Family::Fm)
            && param_name(p).starts_with("Lfo")
        {
            return "the Minimoog's vibrato is VCO 3: vco3coarse";
        }
        match (fam, d.engine) {
            (Family::Fm, Engine::Mono | Engine::La) if param_name(p).starts_with("Lfo") => {
                "a DX7 parameter: this synth's LFO is lforate"
            }
            (Family::Fm, _) => "a DX7 parameter: this track's synth is not a DX7",
            (Family::Mono, Engine::Fm) if ADSR.contains(&p) => {
                "the DX7's envelopes are its operators' rates and levels: op1r1"
            }
            (Family::Mono, Engine::Sid) if ADSR.contains(&p) => {
                "the SID's envelopes are its voices': sid1attack"
            }
            (Family::Mono, Engine::Vl) if ADSR.contains(&p) => {
                "the VL-1's envelope is its code's digits: vlattack"
            }
            (Family::La, _) => "a D-50 parameter: this track's synth is not a D-50",
            (Family::Tables, _) => "a PPG Wave parameter: this track's synth has no wavetables",
            (Family::Matrix, _) => {
                "a Matrix-12 parameter: this track's synth has one LFO and 8 patch slots"
            }
            (Family::Drums, _) => "a drum machine's parameter: this track's synth has no pads",
            (Family::Graph, _) => "a Modular knob: this track's synth is not the Modular",
            (Family::Sid, _) => "a SID parameter: this track's synth is not the SID",
            (Family::Nes, _) => "a NES parameter: this track's synth is not the NES",
            (Family::Vl, _) => "a VL-1 parameter: this track's synth is not the VL-Tone",
            (Family::ModWheel, _) => "this track's synth has no mod wheel",
            (Family::Bend, _) => "this track's synth has no pitch bend",
            (_, Engine::Drums(_)) => "a drum machine reads its pads' parameters: bdtune",
            (_, Engine::Pads) => {
                "the Pad Sampler reads its level, vco1level; its pads are set in the view"
            }
            (_, Engine::Graph) => "the Modular reads its knobs, ctl1 to ctl32, and its envelope",
            (_, Engine::Sampler) => "the sampler does not read this parameter",
            (_, Engine::Fm) => "the DX7 does not read this parameter",
            (_, Engine::Sid) => "the SID does not read this parameter",
            (_, Engine::Nes) => "the NES does not read this parameter",
            (_, Engine::Vl) => "the VL-1 does not read this parameter",
            (_, Engine::Mono | Engine::La) => "this track's synth does not read this parameter",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The issue's case (#673): the DX7's LFO and the general one on a
    /// Minimoog, whose vibrato is VCO 3.
    #[test]
    fn a_minimoog_has_no_lfo_and_a_juno_has_no_dx7_lfo() {
        for p in [Param::LfoSpeed, Param::LfoRate, Param::LfoWave] {
            assert!(!Model::Minimoog.reads(p), "{p:?}");
            assert_eq!(
                Model::Minimoog.unread(p),
                "the Minimoog's vibrato is VCO 3: vco3coarse"
            );
        }
        assert!(Model::Minimoog.reads(Param::Vco3Coarse));
        assert!(Model::Juno106.reads(Param::LfoRate));
        assert!(!Model::Juno106.reads(Param::LfoSpeed));
        assert_eq!(
            Model::Juno106.unread(Param::LfoSpeed),
            "a DX7 parameter: this synth's LFO is lforate"
        );
        assert!(Model::Dx7.reads(Param::LfoSpeed));
        assert!(!Model::Dx7.reads(Param::LfoRate));
    }

    #[test]
    fn every_model_reads_its_strip_arp_and_voice_pool() {
        for (m, _) in Model::ALL {
            for p in [
                Param::Level,
                Param::Pan,
                Param::Send1,
                Param::Send2,
                Param::ArpOn,
                Param::Polyphony,
                Param::Glide,
            ] {
                assert!(m.reads(p), "{m:?} {p:?}");
            }
        }
    }

    #[test]
    fn engines_keep_to_their_own() {
        assert!(!Model::Tr808.reads(Param::Cutoff));
        assert!(Model::Tr909.reads(Param::CrTune));
        assert!(!Model::Prophet5.reads(Param::BdTune));
        assert!(!Model::Prophet5.reads(Param::Patch9Source));
        assert!(Model::Matrix12.reads(Param::Patch20Amount));
        assert!(!Model::Prophet5.reads(Param::Wt1Pos));
        assert!(Model::PpgWave.reads(Param::Wt1Pos));
        assert!(Model::D50.reads(Param::P2Cutoff));
        assert!(!Model::Juno106.reads(Param::P2Cutoff));
        assert!(Model::Sid.reads(Param::Vibrato));
        assert!(!Model::Sid.reads(Param::AdsrAttack));
        assert!(!Model::Nes.reads(Param::ModWheel));
        assert!(Model::Nes.reads(Param::PitchBend));
        assert!(Model::Modular.reads(Param::Ctl32));
        assert!(!Model::Dx7.reads(Param::AdsrAttack));
    }

    /// Every preset sets only what its model reads.
    #[test]
    fn presets_set_what_their_model_reads() {
        for (m, _) in Model::ALL {
            for pre in m.def().presets {
                for (p, _) in pre.changes {
                    assert!(m.reads(*p), "{:?} on {m:?} ({:?})", p, pre.preset);
                }
            }
        }
    }

    /// Every control on a model's panel is a parameter the model reads, so
    /// the view and the engine can't drift (#673). Controls made in shared
    /// helpers outside a model's block are not seen.
    #[test]
    fn the_panels_show_what_their_model_reads() {
        let ts = include_str!("../../../../web/src/audio/models.ts");
        let mut seen = 0;
        for block in ts.split(": ModelDef = {").skip(1) {
            let block = block.split("\nconst ").next().unwrap_or(block);
            let Some(id) = block
                .split("id: Model.")
                .nth(1)
                .and_then(|r| r.split(|c: char| !c.is_ascii_alphanumeric()).next())
            else {
                continue;
            };
            let m = Model::ALL
                .iter()
                .find(|(_, n)| *n == id)
                .map(|(m, _)| *m)
                .unwrap_or_else(|| panic!("no model {id}"));
            for name in block
                .split("Param.")
                .skip(1)
                .filter_map(|r| r.split(|c: char| !c.is_ascii_alphanumeric()).next())
            {
                let p = Param::by_name(name).unwrap_or_else(|| panic!("no param {name}"));
                assert!(m.reads(p), "{id}'s panel shows {name}");
                seen += 1;
            }
        }
        assert!(seen > 500, "the panels were read: {seen}");
    }
}
