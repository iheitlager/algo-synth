//! Normalled routing and the patch (spec 004 Req 7), as on the 2600.
//!
//! The audio path is wired: VCO 1-3 and noise into the mixer, the mixer
//! into the ladder, the ladder into the VCA, and the key into every VCO's
//! pitch. Four modulation connections are *normalled*, each with an amount
//! the panel sets:
//!
//! - ADSR → cutoff (`EnvCutoff`) and key → cutoff (`KeyTrack`),
//! - LFO → VCO 1-3 pitch, scaled by the mod wheel (`Vibrato`, `ModWheel`),
//! - ADSR → VCA.
//!
//! A *patch* is 8 overrides (source, destination, amount). An override
//! replaces every normalled connection to its destination; several
//! overrides to one destination add up. The AR envelope and the S&H have no
//! normalled destination.

/// A modulation source; the ids are mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum ModSource {
    /// An empty slot.
    #[default]
    None = 0,
    Vco1 = 1,
    Vco2 = 2,
    Vco3 = 3,
    Noise = 4,
    Adsr = 5,
    Ar = 6,
    Lfo = 7,
    SampleHold = 8,
    ModWheel = 9,
    Velocity = 10,
    /// The key relative to middle C, −1..1 over ±5 octaves.
    Key = 11,
    /// The filter ADSR (spec 004 Req 11).
    Fenv = 12,
}

impl ModSource {
    pub const ALL: [(ModSource, &'static str); 13] = [
        (ModSource::None, "None"),
        (ModSource::Vco1, "Vco1"),
        (ModSource::Vco2, "Vco2"),
        (ModSource::Vco3, "Vco3"),
        (ModSource::Noise, "Noise"),
        (ModSource::Adsr, "Adsr"),
        (ModSource::Ar, "Ar"),
        (ModSource::Lfo, "Lfo"),
        (ModSource::SampleHold, "SampleHold"),
        (ModSource::ModWheel, "ModWheel"),
        (ModSource::Velocity, "Velocity"),
        (ModSource::Key, "Key"),
        (ModSource::Fenv, "Fenv"),
    ];

    /// The source for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<ModSource> {
        Self::ALL
            .iter()
            .find(|(s, _)| *s as u32 == id)
            .map(|(s, _)| *s)
    }
}

/// A modulation destination; the ids are mirrored in `params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum ModDest {
    /// An empty slot.
    #[default]
    None = 0,
    Vco1Pitch = 1,
    Vco2Pitch = 2,
    Vco3Pitch = 3,
    PulseWidth = 4,
    Cutoff = 5,
    Resonance = 6,
    Vca = 7,
    LfoRate = 8,
}

impl ModDest {
    pub const ALL: [(ModDest, &'static str); 9] = [
        (ModDest::None, "None"),
        (ModDest::Vco1Pitch, "Vco1Pitch"),
        (ModDest::Vco2Pitch, "Vco2Pitch"),
        (ModDest::Vco3Pitch, "Vco3Pitch"),
        (ModDest::PulseWidth, "PulseWidth"),
        (ModDest::Cutoff, "Cutoff"),
        (ModDest::Resonance, "Resonance"),
        (ModDest::Vca, "Vca"),
        (ModDest::LfoRate, "LfoRate"),
    ];

    /// The destination for a raw id, or `None` for an unknown one.
    pub fn from_id(id: u32) -> Option<ModDest> {
        Self::ALL
            .iter()
            .find(|(d, _)| *d as u32 == id)
            .map(|(d, _)| *d)
    }

    /// Index into `Mods`, or `None` for an empty slot.
    fn index(self) -> Option<usize> {
        (self as usize).checked_sub(1)
    }
}

/// Destinations that take modulation (all but `ModDest::None`).
const DESTS: usize = 8;
/// Slots in a patch.
pub const SLOTS: usize = 8;

/// What amount 1 at full source does to each destination, in its units:
/// semitones of pitch, pulse width, semitones of cutoff, a share of the
/// full resonance, VCA gain, octaves of LFO rate.
const SCALE: [f32; DESTS] = [24.0, 24.0, 24.0, 0.45, 48.0, 1.0, 1.0, 4.0];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slot {
    pub source: ModSource,
    pub dest: ModDest,
    /// −1..=1.
    pub amount: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Patch {
    pub slots: [Slot; SLOTS],
}

impl Patch {
    /// Set a slot's source from a raw id; unknown ids leave it as it was.
    pub fn set_source(&mut self, slot: usize, id: f32) {
        if let (Some(s), Some(slot)) = (
            ModSource::from_id(id.round() as u32),
            self.slots.get_mut(slot),
        ) {
            slot.source = s;
        }
    }

    /// Set a slot's destination from a raw id; unknown ids leave it.
    pub fn set_dest(&mut self, slot: usize, id: f32) {
        if let (Some(d), Some(slot)) = (
            ModDest::from_id(id.round() as u32),
            self.slots.get_mut(slot),
        ) {
            slot.dest = d;
        }
    }

    /// Set a slot's amount, clamped to −1..=1; NaN is 0.
    pub fn set_amount(&mut self, slot: usize, amount: f32) {
        if let Some(slot) = self.slots.get_mut(slot) {
            slot.amount = if amount.is_nan() {
                0.0
            } else {
                amount.clamp(-1.0, 1.0)
            };
        }
    }

    /// Which destinations an override takes over from their normals.
    pub fn overridden(&self) -> [bool; DESTS] {
        let mut taken = [false; DESTS];
        for s in self.slots.iter().filter(|s| s.source != ModSource::None) {
            if let Some(t) = s.dest.index().and_then(|i| taken.get_mut(i)) {
                *t = true;
            }
        }
        taken
    }
}

/// Whether a patch slot takes `dest` over from its normals.
pub fn is_taken(taken: &[bool; DESTS], dest: ModDest) -> bool {
    dest.index()
        .and_then(|i| taken.get(i).copied())
        .unwrap_or(false)
}

/// The value of every source for one sample, indexed by `ModSource` id.
pub type Sources = [f32; SOURCES];
/// Number of modulation sources, `ModSource::None` included.
pub const SOURCES: usize = 13;

/// The modulation each destination receives, in its own units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mods {
    pub pitch: [f32; 3],
    pub pulse_width: f32,
    pub cutoff: f32,
    /// Semitones of high-pass cutoff (a normal only, no patch destination).
    pub hp_cutoff: f32,
    pub resonance: f32,
    /// The VCA gain, 0..=1.
    pub vca: f32,
    pub lfo_rate: f32,
}

/// The normalled amounts, in destination units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Normals {
    /// Semitones of cutoff at full ADSR (or filter ADSR).
    pub env_cutoff: f32,
    /// Whether the normalled cutoff follows the filter ADSR, not the ADSR.
    pub cutoff_from_fenv: bool,
    /// Semitones of high-pass cutoff at full envelope, and whether that
    /// envelope is the AR (else the filter ADSR).
    pub env_hp_cutoff: f32,
    pub hp_from_ar: bool,
    /// Semitones of cutoff per semitone of key.
    pub key_track: f32,
    /// Semitones of VCO pitch at full LFO and full mod wheel.
    pub vibrato: f32,
}

/// Sum the normalled connections and the patch for one sample. `key` is the
/// voice's pitch relative to middle C in semitones.
pub fn modulate(
    patch: &Patch,
    taken: &[bool; DESTS],
    normals: &Normals,
    src: &Sources,
    key: f32,
) -> Mods {
    let at = |s: ModSource| src.get(s as usize).copied().unwrap_or(0.0);
    let mut d = [0.0; DESTS];
    let [_, _, _, _, cutoff_taken, _, vca_taken, _] = *taken;
    let vibrato = at(ModSource::Lfo) * at(ModSource::ModWheel) * normals.vibrato;
    // The three VCO pitches come first in `ModDest` order.
    for (v, t) in d.iter_mut().zip(taken).take(3) {
        if !t {
            *v = vibrato;
        }
    }
    if !cutoff_taken {
        let env = if normals.cutoff_from_fenv {
            at(ModSource::Fenv)
        } else {
            at(ModSource::Adsr)
        };
        d[4] = env * normals.env_cutoff + key * normals.key_track;
    }
    if !vca_taken {
        d[6] = at(ModSource::Adsr);
    }
    for s in patch.slots.iter().filter(|s| s.source != ModSource::None) {
        if let Some(i) = s.dest.index() {
            let scale = SCALE.get(i).copied().unwrap_or(0.0);
            if let Some(v) = d.get_mut(i) {
                *v += at(s.source) * s.amount * scale;
            }
        }
    }
    let [p1, p2, p3, pulse_width, cutoff, resonance, vca, lfo_rate] = d;
    let hp_env = if normals.hp_from_ar {
        at(ModSource::Ar)
    } else {
        at(ModSource::Fenv)
    };
    Mods {
        pitch: [p1, p2, p3],
        pulse_width,
        cutoff,
        hp_cutoff: hp_env * normals.env_hp_cutoff,
        resonance,
        vca: vca.clamp(0.0, 1.0),
        lfo_rate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(pairs: &[(ModSource, f32)]) -> Sources {
        let mut s = [0.0; SOURCES];
        for (src, v) in pairs {
            s[*src as usize] = *v;
        }
        s
    }

    const NORMALS: Normals = Normals {
        env_cutoff: 24.0,
        cutoff_from_fenv: false,
        env_hp_cutoff: 12.0,
        hp_from_ar: false,
        key_track: 0.5,
        vibrato: 2.0,
    };

    fn run(patch: &Patch, src: &Sources, key: f32) -> Mods {
        modulate(patch, &patch.overridden(), &NORMALS, src, key)
    }

    /// The model chooses which envelope the normalled cutoff follows; a
    /// patch can read either.
    #[test]
    fn normalled_cutoff_follows_the_chosen_envelope() {
        let src = sources(&[(ModSource::Adsr, 1.0), (ModSource::Fenv, 0.25)]);
        let by_adsr = run(&Patch::default(), &src, 0.0);
        assert_eq!(by_adsr.cutoff, 24.0);
        let normals = Normals {
            cutoff_from_fenv: true,
            ..NORMALS
        };
        let patch = Patch::default();
        let by_fenv = modulate(&patch, &patch.overridden(), &normals, &src, 0.0);
        assert_eq!(by_fenv.cutoff, 6.0);
        assert_eq!(by_fenv.vca, 1.0, "the VCA stays on the ADSR");
        let mut p = Patch::default();
        p.slots[0] = Slot {
            source: ModSource::Fenv,
            dest: ModDest::Vca,
            amount: 1.0,
        };
        assert_eq!(run(&p, &src, 0.0).vca, 0.25);
    }

    /// The high-pass cutoff follows the filter ADSR, or the AR on the
    /// model that has an envelope for it; it takes no patch destination.
    #[test]
    fn high_pass_follows_the_chosen_envelope() {
        let src = sources(&[(ModSource::Fenv, 0.5), (ModSource::Ar, 1.0)]);
        let patch = Patch::default();
        let hp = |hp_from_ar| {
            let normals = Normals {
                hp_from_ar,
                ..NORMALS
            };
            modulate(&patch, &patch.overridden(), &normals, &src, 0.0).hp_cutoff
        };
        assert_eq!(hp(false), 6.0);
        assert_eq!(hp(true), 12.0);
    }

    #[test]
    fn empty_patch_is_normalled() {
        let src = sources(&[
            (ModSource::Adsr, 0.5),
            (ModSource::Lfo, 1.0),
            (ModSource::ModWheel, 0.5),
        ]);
        let m = run(&Patch::default(), &src, 12.0);
        assert_eq!(m.cutoff, 0.5 * 24.0 + 12.0 * 0.5, "ADSR and key → cutoff");
        assert_eq!(m.pitch, [1.0; 3], "LFO × mod wheel → every VCO");
        assert_eq!(m.vca, 0.5, "ADSR → VCA");
        assert_eq!((m.pulse_width, m.resonance, m.lfo_rate), (0.0, 0.0, 0.0));
        // Half-empty slots change nothing.
        let mut half = Patch::default();
        half.slots[0] = Slot {
            source: ModSource::None,
            dest: ModDest::Cutoff,
            amount: 1.0,
        };
        half.slots[1] = Slot {
            source: ModSource::Lfo,
            dest: ModDest::None,
            amount: 1.0,
        };
        assert_eq!(run(&half, &src, 12.0), m);
    }

    #[test]
    fn override_replaces_destination() {
        let mut patch = Patch::default();
        patch.slots[0] = Slot {
            source: ModSource::SampleHold,
            dest: ModDest::Cutoff,
            amount: 0.5,
        };
        let src = sources(&[(ModSource::Adsr, 1.0), (ModSource::SampleHold, -0.5)]);
        let m = run(&patch, &src, 12.0);
        assert_eq!(m.cutoff, -0.5 * 0.5 * 48.0, "neither ADSR nor key moves it");
        assert_eq!(m.vca, 1.0, "other normals stay");
        // A second slot to the same destination adds.
        patch.slots[1] = Slot {
            source: ModSource::Adsr,
            dest: ModDest::Cutoff,
            amount: 0.25,
        };
        assert_eq!(run(&patch, &src, 12.0).cutoff, -12.0 + 12.0);
        // Patching the VCA replaces the ADSR there; a pitch override takes
        // only its own VCO.
        patch.slots[2] = Slot {
            source: ModSource::Ar,
            dest: ModDest::Vca,
            amount: 1.0,
        };
        patch.slots[3] = Slot {
            source: ModSource::Key,
            dest: ModDest::Vco2Pitch,
            amount: 0.5,
        };
        let src = sources(&[
            (ModSource::Adsr, 1.0),
            (ModSource::Ar, 0.25),
            (ModSource::Lfo, 1.0),
            (ModSource::ModWheel, 1.0),
            (ModSource::Key, 0.5),
        ]);
        let m = run(&patch, &src, 30.0);
        assert_eq!(m.vca, 0.25);
        assert_eq!(m.pitch, [2.0, 0.5 * 0.5 * 24.0, 2.0]);
        assert!(is_taken(&patch.overridden(), ModDest::Vca));
        assert!(!is_taken(&patch.overridden(), ModDest::Vco1Pitch));
    }

    #[test]
    fn bad_ids_are_ignored() {
        let mut patch = Patch::default();
        patch.set_source(0, 7.0);
        patch.set_dest(0, 5.0);
        patch.set_source(0, 99.0);
        patch.set_dest(0, 42.0);
        patch.set_source(99, 1.0);
        patch.set_amount(0, 5.0);
        assert_eq!(
            patch.slots[0],
            Slot {
                source: ModSource::Lfo,
                dest: ModDest::Cutoff,
                amount: 1.0
            }
        );
        patch.set_amount(0, f32::NAN);
        assert_eq!(patch.slots[0].amount, 0.0);
    }
}
