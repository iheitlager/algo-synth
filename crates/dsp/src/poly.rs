//! The voice pool (spec 006, ADR-0011): each synth owns `MAX_VOICES` voices.
//!
//! A *monophonic* synth (`Polyphony` 1) keeps one voice per owner (live keys,
//! each MIDI channel), exactly as before the pool: the voice holds that owner's
//! key stack and glides, and its noise seed is the owner's. A *polyphonic* synth
//! presses every note on a voice of its own: a free one, rotating from the last
//! used, else the one in release that has sounded longest, else the oldest held
//! note. A voice remembers its owner and note, so a note-off finds its own
//! voice and live keys and MIDI channels do not release each other.
//!
//! A poly synth's pool runs one LFO and one sample-and-hold for all its voices.
//! Everything is allocated up front (ADR-0002): the voices are an enum with no
//! heap data, so a model change rebuilds one without allocating.

use crate::engine::SYNTHS;
use crate::mono::MonoParams;
use crate::mono::ladder::LadderTables;
use crate::mono::lfo::Lfo;
use crate::mono::noise::Noise;
use crate::mono::osc::Blep;
use crate::mono::voice::{MonoCtx, MonoVoice, PitchTable, SharedMod};
use crate::voice::Owner;

/// Voices in a pool.
pub const MAX_VOICES: usize = 16;
/// Voices that may sound at once across every synth (spec 006 Req 5).
pub const VOICE_BUDGET: usize = 64;

/// One voice of a pool: the Mono voice now; other engines join as further variants.
#[derive(Clone, Copy)]
pub enum PolyVoice {
    Mono(MonoVoice),
}

impl PolyVoice {
    fn new(seed: u32) -> PolyVoice {
        PolyVoice::Mono(MonoVoice::new(seed))
    }

    /// Sounding: gated, releasing, or about to start.
    pub fn active(&self) -> bool {
        match self {
            PolyVoice::Mono(v) => v.active(),
        }
    }

    /// A key is held on it.
    pub fn gated(&self) -> bool {
        match self {
            PolyVoice::Mono(v) => v.gated(),
        }
    }

    fn release_all(&mut self) {
        match self {
            PolyVoice::Mono(v) => v.release_all(),
        }
    }
}

/// Who a voice plays for and what it plays.
#[derive(Clone, Copy, Default)]
struct Slot {
    owner: Option<Owner>,
    note: u8,
    /// The engine's note counter when it started: lower is older.
    age: u64,
}

/// The seed of an owner's noise, as it was when each owner had a voice of its own.
fn owner_seed(owner: Owner) -> u32 {
    let index = match owner {
        Owner::Live(s) => usize::from(s),
        Owner::Channel(ch) => SYNTHS + usize::from(ch),
    };
    (index as u32 + 1).wrapping_mul(2_654_435_761)
}

pub struct Pool {
    voices: [PolyVoice; MAX_VOICES],
    slots: [Slot; MAX_VOICES],
    /// The voice a poly note took last, so the next rotates on from it.
    last: usize,
    /// The shared LFO and sample-and-hold of a poly synth.
    lfo: Lfo,
    noise: Noise,
    shared: SharedMod,
}

impl Pool {
    pub fn new(index: usize) -> Pool {
        let seed = |i: usize| ((index * MAX_VOICES + i) as u32 + 1).wrapping_mul(0x9E37_79B1);
        Pool {
            voices: std::array::from_fn(|i| PolyVoice::new(seed(i))),
            slots: [Slot::default(); MAX_VOICES],
            last: MAX_VOICES - 1,
            lfo: Lfo::default(),
            noise: Noise::new(seed(MAX_VOICES) | 1),
            shared: SharedMod::new(),
        }
    }

    /// Voices sounding now.
    pub fn active(&self) -> usize {
        self.voices.iter().filter(|v| v.active()).count()
    }

    /// Voices with a key held (for tests).
    pub fn held(&self) -> usize {
        self.voices.iter().filter(|v| v.gated()).count()
    }

    /// The Mono voice playing for `owner`, if it has one.
    pub fn voice(&self, owner: Owner) -> Option<&MonoVoice> {
        let i = self.slots.iter().position(|s| s.owner == Some(owner))?;
        match self.voices.get(i)? {
            PolyVoice::Mono(v) => Some(v),
        }
    }

    /// The voice in release that has sounded longest: its slot and age.
    pub fn oldest_release(&self) -> Option<(usize, u64)> {
        self.slots
            .iter()
            .zip(&self.voices)
            .enumerate()
            .filter(|(_, (_, v))| v.active() && !v.gated())
            .map(|(i, (s, _))| (i, s.age))
            .min_by_key(|(_, age)| *age)
    }

    /// The held voice that has sounded longest: its slot.
    fn oldest_held(&self, limit: usize) -> Option<usize> {
        self.slots
            .iter()
            .zip(&self.voices)
            .enumerate()
            .take(limit)
            .filter(|(_, (_, v))| v.gated())
            .map(|(i, (s, _))| (i, s.age))
            .min_by_key(|(_, age)| *age)
            .map(|(i, _)| i)
    }

    /// Silence a voice at once and free its slot (a stolen voice restarts clean).
    pub fn silence(&mut self, i: usize) {
        if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
            *v = PolyVoice::new(owner_seed_of_slot(i, s.owner));
            *s = Slot::default();
        }
    }

    /// Silence the oldest held note; false when nothing is held.
    pub fn silence_oldest_held(&mut self) -> bool {
        match self.oldest_held(MAX_VOICES) {
            Some(i) => {
                self.silence(i);
                true
            }
            None => false,
        }
    }

    /// Whether pressing `note` for `owner` would add a sounding voice, rather than
    /// re-use or steal one of this pool's own.
    pub fn adds_a_voice(&self, owner: Owner, note: u8, p: &MonoParams) -> bool {
        let limit = p.polyphony.clamp(1, MAX_VOICES);
        if limit == 1 {
            return !self
                .slots
                .iter()
                .zip(&self.voices)
                .any(|(s, v)| s.owner == Some(owner) && v.active());
        }
        let again = self
            .slots
            .iter()
            .zip(&self.voices)
            .take(limit)
            .any(|(s, v)| s.owner == Some(owner) && s.note == note && v.gated());
        !again && self.voices.iter().take(limit).any(|v| !v.active())
    }

    /// Press `note` for `owner`. A monophonic synth keeps the owner's voice and
    /// its key stack; a polyphonic one gives the note a voice of its own.
    pub fn note_on(&mut self, owner: Owner, note: u8, velocity: f32, p: &MonoParams, clock: u64) {
        let limit = p.polyphony.clamp(1, MAX_VOICES);
        if limit == 1 {
            self.press_mono(owner, note, velocity, p, clock);
        } else {
            self.press_poly(owner, note, velocity, p, clock, limit);
        }
    }

    /// The owner's voice, or a free one made for it (else the oldest).
    fn press_mono(&mut self, owner: Owner, note: u8, velocity: f32, p: &MonoParams, clock: u64) {
        let i = match self.slots.iter().position(|s| s.owner == Some(owner)) {
            Some(i) => i,
            None => {
                let i = self
                    .voices
                    .iter()
                    .position(|v| !v.active())
                    .or_else(|| self.oldest_release().map(|(i, _)| i))
                    .or_else(|| self.oldest_held(MAX_VOICES))
                    .unwrap_or(0);
                if let Some(v) = self.voices.get_mut(i) {
                    *v = PolyVoice::new(owner_seed(owner));
                }
                i
            }
        };
        if let (Some(PolyVoice::Mono(v)), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i))
        {
            v.press(note, velocity, p);
            *s = Slot {
                owner: Some(owner),
                note,
                age: clock,
            };
        }
    }

    fn press_poly(
        &mut self,
        owner: Owner,
        note: u8,
        velocity: f32,
        p: &MonoParams,
        clock: u64,
        limit: usize,
    ) {
        // The same note again on its owner re-uses its voice, and retriggers.
        let again = self
            .slots
            .iter()
            .zip(&self.voices)
            .take(limit)
            .position(|(s, v)| s.owner == Some(owner) && s.note == note && v.gated());
        let i = match again {
            Some(i) => {
                if let Some(v) = self.voices.get_mut(i) {
                    v.release_all();
                }
                i
            }
            None => self.allocate(limit),
        };
        if let (Some(PolyVoice::Mono(v)), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i))
        {
            v.press(note, velocity, p);
            *s = Slot {
                owner: Some(owner),
                note,
                age: clock,
            };
        }
        self.last = i;
    }

    /// A voice for a new note among the first `limit`: free, rotating on from the
    /// last one used; else the one in release longest; else the oldest held note.
    fn allocate(&mut self, limit: usize) -> usize {
        let free = (1..=limit)
            .map(|k| (self.last + k) % limit)
            .find(|i| self.voices.get(*i).is_some_and(|v| !v.active()));
        if let Some(i) = free {
            return i;
        }
        let steal = self
            .slots
            .iter()
            .zip(&self.voices)
            .enumerate()
            .take(limit)
            .filter(|(_, (_, v))| !v.gated())
            .map(|(i, (s, _))| (i, s.age))
            .min_by_key(|(_, age)| *age)
            .map(|(i, _)| i)
            .or_else(|| self.oldest_held(limit))
            .unwrap_or(0);
        self.silence(steal);
        steal
    }

    /// Release `note` for `owner`: a monophonic owner's voice falls back to its next
    /// held key; a polyphonic note releases the voice that plays it.
    pub fn note_off(&mut self, owner: Owner, note: u8, p: &MonoParams) {
        if p.polyphony <= 1 {
            if let Some(i) = self.slots.iter().position(|s| s.owner == Some(owner)) {
                if let Some(PolyVoice::Mono(v)) = self.voices.get_mut(i) {
                    v.release(note, p);
                }
            }
            return;
        }
        let held = self
            .slots
            .iter()
            .zip(&self.voices)
            .position(|(s, v)| s.owner == Some(owner) && s.note == note && v.gated());
        if let Some(PolyVoice::Mono(v)) = held.and_then(|i| self.voices.get_mut(i)) {
            v.release(note, p);
        }
    }

    /// Release every voice `owner` holds.
    pub fn release_owner(&mut self, owner: Owner) {
        for (s, v) in self.slots.iter().zip(self.voices.iter_mut()) {
            if s.owner == Some(owner) {
                v.release_all();
            }
        }
    }

    /// Release every voice played for a MIDI channel.
    pub fn release_channels(&mut self) {
        for (s, v) in self.slots.iter().zip(self.voices.iter_mut()) {
            if matches!(s.owner, Some(Owner::Channel(_))) {
                v.release_all();
            }
        }
    }

    pub fn release_all(&mut self) {
        for v in self.voices.iter_mut() {
            v.release_all();
        }
    }

    /// Add every sounding voice into `out`.
    pub fn render(
        &mut self,
        p: &MonoParams,
        sine: &[f32],
        blep: &Blep,
        ladder: &LadderTables,
        pitch: &PitchTable,
        out: &mut [f32],
    ) {
        let poly = p.polyphony > 1;
        if poly {
            self.shared
                .fill(&mut self.lfo, &mut self.noise, p, sine, out.len());
        }
        let ctx = MonoCtx {
            params: p,
            sine,
            blep,
            ladder,
            pitch,
            shared: if poly { Some(&self.shared) } else { None },
        };
        for v in self.voices.iter_mut().filter(|v| v.active()) {
            match v {
                PolyVoice::Mono(m) => m.render(&ctx, out),
            }
        }
    }
}

/// The noise seed of a slot that is being cleared: its owner's if it had one.
fn owner_seed_of_slot(i: usize, owner: Option<Owner>) -> u32 {
    owner.map_or_else(|| ((i as u32) + 1).wrapping_mul(0x9E37_79B1), owner_seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::BLOCK;
    use crate::voice::sine_table;

    const SR: f32 = 48_000.0;

    struct Rig {
        pool: Pool,
        params: MonoParams,
        sine: Vec<f32>,
        blep: Blep,
        ladder: LadderTables,
        pitch: PitchTable,
        clock: u64,
    }

    impl Rig {
        fn new(polyphony: f32) -> Rig {
            let mut params = MonoParams::new(SR);
            params.set(crate::params::Param::Polyphony, polyphony);
            Rig {
                pool: Pool::new(0),
                params,
                sine: sine_table(),
                blep: Blep::new(),
                ladder: LadderTables::new(SR),
                pitch: PitchTable::new(SR),
                clock: 0,
            }
        }

        fn on(&mut self, owner: Owner, note: u8) {
            self.clock += 1;
            self.pool
                .note_on(owner, note, 1.0, &self.params, self.clock);
        }

        fn off(&mut self, owner: Owner, note: u8) {
            self.pool.note_off(owner, note, &self.params);
        }

        fn run(&mut self, blocks: usize) -> Vec<f32> {
            let mut all = Vec::new();
            for _ in 0..blocks {
                let mut out = vec![0.0; BLOCK];
                self.pool.render(
                    &self.params,
                    &self.sine,
                    &self.blep,
                    &self.ladder,
                    &self.pitch,
                    &mut out,
                );
                all.extend(out);
            }
            all
        }

        fn held(&self) -> Vec<u8> {
            let mut v: Vec<u8> = self
                .pool
                .slots
                .iter()
                .zip(&self.pool.voices)
                .filter(|(_, v)| v.gated())
                .map(|(s, _)| s.note)
                .collect();
            v.sort_unstable();
            v
        }
    }

    const LIVE: Owner = Owner::Live(0);

    #[test]
    fn a_chord_sounds_one_voice_per_note_and_releases_its_own() {
        let mut r = Rig::new(6.0);
        for n in [60, 64, 67, 71] {
            r.on(LIVE, n);
        }
        r.run(2);
        assert_eq!(r.pool.active(), 4);
        r.off(LIVE, 64);
        assert_eq!(r.held(), vec![60, 67, 71]);
        r.run(400);
        assert_eq!(
            r.pool.active(),
            3,
            "the released voice fell silent, the others hold"
        );
    }

    #[test]
    fn a_monophonic_synth_keeps_one_voice_per_owner() {
        let mut r = Rig::new(1.0);
        r.on(LIVE, 60);
        r.on(LIVE, 64);
        r.on(Owner::Channel(2), 67);
        r.run(2);
        assert_eq!(
            r.pool.active(),
            2,
            "live keys are one voice, the channel another"
        );
        assert_eq!(
            r.pool.voice(LIVE).map(|v| v.note()),
            Some(64),
            "last note wins on the owner's voice"
        );
        r.off(LIVE, 64);
        assert_eq!(
            r.pool.voice(LIVE).map(|v| v.note()),
            Some(60),
            "and falls back"
        );
    }

    #[test]
    fn owners_do_not_release_each_other() {
        let mut r = Rig::new(6.0);
        r.on(LIVE, 60);
        r.on(Owner::Channel(1), 60);
        r.off(Owner::Channel(1), 60);
        assert_eq!(r.held(), vec![60], "the live key is still held");
        r.on(Owner::Channel(1), 62);
        r.pool.release_owner(Owner::Channel(1));
        assert_eq!(r.held(), vec![60]);
        r.pool.release_all();
        assert!(r.held().is_empty());
    }

    #[test]
    fn a_full_pool_steals_the_release_first_then_the_oldest_held() {
        let mut r = Rig::new(4.0);
        for n in [60, 62, 64, 65] {
            r.on(LIVE, n);
        }
        r.run(1);
        // All held: the oldest (60) is stolen.
        r.on(LIVE, 70);
        assert_eq!(r.held(), vec![62, 64, 65, 70]);
        // One in release: that one goes first, whoever is oldest.
        r.off(LIVE, 64);
        r.run(1);
        r.on(LIVE, 72);
        assert_eq!(
            r.held(),
            vec![62, 65, 70, 72],
            "the released 64 was taken, not the older 62"
        );
    }

    #[test]
    fn rotation_does_not_cut_a_released_tail_first() {
        let mut r = Rig::new(3.0);
        r.params.set(crate::params::Param::AdsrRelease, 2.0);
        r.on(LIVE, 60);
        r.run(1);
        r.off(LIVE, 60);
        r.run(1);
        // The released 60 still sounds; two new notes take the free voices.
        r.on(LIVE, 62);
        r.on(LIVE, 64);
        r.run(1);
        assert_eq!(
            r.pool.active(),
            3,
            "the tail of 60 and both new notes sound"
        );
    }

    #[test]
    fn the_same_note_again_retriggers_its_voice() {
        let mut r = Rig::new(4.0);
        r.on(LIVE, 60);
        r.run(1);
        r.on(LIVE, 60);
        assert_eq!(r.pool.active(), 1);
        assert_eq!(r.held(), vec![60]);
    }

    /// Spec 006 Req 4: every voice reads the one LFO, so a voice started later
    /// has the same vibrato as one already sounding.
    #[test]
    fn the_lfo_is_shared_by_every_voice() {
        use crate::params::Param;
        let mut r = Rig::new(4.0);
        for (p, v) in [
            (Param::Vibrato, 1.0),
            (Param::ModWheel, 1.0),
            (Param::LfoRate, 5.0),
            (Param::LfoWave, 3.0),
        ] {
            r.params.set(p, p.clamp(v));
        }
        r.on(LIVE, 60);
        r.run(300);
        // A quarter of an LFO cycle later (5 Hz: 0.05 s is 19 blocks of 128).
        r.on(LIVE, 64);
        r.run(19);
        let pitch_mods: Vec<f32> = r
            .pool
            .voices
            .iter()
            .filter(|v| v.active())
            .map(|v| match v {
                PolyVoice::Mono(m) => m.mods().pitch[0],
            })
            .collect();
        assert_eq!(pitch_mods.len(), 2);
        assert!(
            pitch_mods[0].abs() > 0.01,
            "the vibrato moves: {pitch_mods:?}"
        );
        assert_eq!(
            pitch_mods[0], pitch_mods[1],
            "and is the same on both voices"
        );
    }

    #[test]
    fn every_sample_is_finite_and_bounded_at_full_polyphony() {
        let mut r = Rig::new(16.0);
        for n in 0..24 {
            r.on(LIVE, 36 + 3 * n);
        }
        let out = r.run(80);
        assert!(out.iter().all(|s| s.is_finite() && s.abs() < 16.0));
        assert_eq!(r.pool.active(), MAX_VOICES);
    }
}
