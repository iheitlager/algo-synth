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
//! A drum kit (`Model::Tr808`) hits pads instead: a key plays the pad General
//! MIDI puts there, on the voice already playing that pad or a new one, with
//! no note-off, and the closed hat chokes the open hat.
//!
//! A poly synth's pool runs one LFO and one sample-and-hold for all its voices.
//! Everything is allocated up front (ADR-0002): the voices are an enum with no
//! heap data, so a model change rebuilds one without allocating.

use crate::drums::{ACCENT_VELOCITY, Pad, PadVoice};
use crate::engine::SYNTHS;
use crate::fm::FmVoice;
use crate::la::LaVoice;
use crate::mono::MonoParams;
use crate::mono::lfo::Lfo;
use crate::mono::noise::Noise;
use crate::mono::voice::{MonoCtx, MonoVoice, SharedMod, Tools};
use crate::padsampler::{PadVoice as SampledPad, pad_of};
use crate::sample::SampleStore;
use crate::sampler::SamplerVoice;
use crate::voice::Owner;

/// Voices in a pool.
pub const MAX_VOICES: usize = 16;
/// Voices that may sound at once across every synth (spec 006 Req 5).
pub const VOICE_BUDGET: usize = 64;

/// One voice of a pool: the Mono voice now; other engines join as further variants.
#[derive(Clone, Copy)]
// Boxing the larger variant would allocate when a voice is rebuilt on a model change; the
// pool is allocated once and its voices are plain values (ADR-0011).
#[allow(clippy::large_enum_variant)]
pub enum PolyVoice {
    Mono(MonoVoice),
    /// The D-50's two-partial voice.
    La(LaVoice),
    /// The DX7's six-operator FM voice.
    Fm(FmVoice),
    /// A pad of the drum kit.
    Drum(PadVoice),
    /// The multisampler's voice.
    Sampler(SamplerVoice),
    /// A pad of the drum/pad sampler.
    Pad(SampledPad),
}

impl PolyVoice {
    /// A voice of the kind a synth's model plays with.
    fn new(seed: u32, p: Option<&MonoParams>) -> PolyVoice {
        match p {
            Some(p) if p.model.uses_la() => PolyVoice::La(LaVoice::new()),
            Some(p) if p.model.uses_fm() => PolyVoice::Fm(FmVoice::new(p.sample_rate())),
            Some(p) if p.model.uses_drums() => PolyVoice::Drum(PadVoice::new(Pad::Bd, seed)),
            Some(p) if p.model.uses_sampler() => PolyVoice::Sampler(SamplerVoice::new()),
            Some(p) if p.model.uses_pads() => PolyVoice::Pad(SampledPad::default()),
            _ => PolyVoice::Mono(MonoVoice::new(seed)),
        }
    }

    /// Whether this voice is of the kind `p`'s model needs.
    fn fits(&self, p: &MonoParams) -> bool {
        match self {
            PolyVoice::Mono(_) => {
                !p.model.uses_la()
                    && !p.model.uses_fm()
                    && !p.model.uses_drums()
                    && !p.model.uses_sampler()
                    && !p.model.uses_pads()
            }
            PolyVoice::La(_) => p.model.uses_la(),
            PolyVoice::Fm(_) => p.model.uses_fm(),
            PolyVoice::Drum(_) => p.model.uses_drums(),
            PolyVoice::Sampler(_) => p.model.uses_sampler(),
            PolyVoice::Pad(_) => p.model.uses_pads(),
        }
    }

    /// Sounding: gated, releasing, or about to start.
    pub fn active(&self) -> bool {
        match self {
            PolyVoice::Mono(v) => v.active(),
            PolyVoice::La(v) => v.active(),
            PolyVoice::Fm(v) => v.active(),
            PolyVoice::Drum(v) => v.active(),
            PolyVoice::Sampler(v) => v.active(),
            PolyVoice::Pad(v) => v.active(),
        }
    }

    /// A key is held on it.
    pub fn gated(&self) -> bool {
        match self {
            PolyVoice::Mono(v) => v.gated(),
            PolyVoice::La(v) => v.gated(),
            PolyVoice::Fm(v) => v.gated(),
            // A hit has no key to hold.
            PolyVoice::Drum(_) => false,
            PolyVoice::Sampler(v) => v.gated(),
            // A pad has no key to hold.
            PolyVoice::Pad(_) => false,
        }
    }

    fn release_all(&mut self) {
        match self {
            PolyVoice::Mono(v) => v.release_all(),
            PolyVoice::La(v) => v.release_all(),
            PolyVoice::Fm(v) => v.release_all(),
            // A hit rings out.
            PolyVoice::Drum(_) => {}
            PolyVoice::Sampler(v) => v.release_all(),
            PolyVoice::Pad(v) => v.release_all(),
        }
    }

    /// Press a key on a voice of the right kind (a voice of another kind is replaced).
    fn press(&mut self, note: u8, velocity: f32, p: &MonoParams, seed: u32) {
        if !self.fits(p) {
            *self = PolyVoice::new(seed, Some(p));
        }
        match self {
            PolyVoice::Mono(v) => v.press(note, velocity, p),
            PolyVoice::La(v) => v.press(note, velocity),
            PolyVoice::Fm(v) => v.press(note, velocity, p),
            PolyVoice::Drum(v) => strike(v, note, velocity, p),
            PolyVoice::Sampler(v) => v.press(note, velocity),
            PolyVoice::Pad(v) => v.press(note, velocity, &p.pad_kit, p.level[0], p.sample_rate()),
        }
    }

    fn release(&mut self, note: u8, p: &MonoParams) {
        match self {
            PolyVoice::Mono(v) => v.release(note, p),
            PolyVoice::La(v) => v.release_all(),
            PolyVoice::Fm(v) => v.release_all(),
            PolyVoice::Drum(_) => {}
            PolyVoice::Sampler(v) => v.release_all(),
            PolyVoice::Pad(v) => v.release(note),
        }
    }

    fn set_trim(&mut self, trim: f32, cutoff: f32) {
        match self {
            PolyVoice::Mono(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            PolyVoice::La(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            PolyVoice::Fm(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            // The kit's pads are tuned by their knobs, not by drift.
            PolyVoice::Drum(_) => {}
            PolyVoice::Sampler(v) => {
                v.trim = trim;
                v.cutoff_trim = cutoff;
            }
            // A pad is tuned by its own knob.
            PolyVoice::Pad(_) => {}
        }
    }
}

/// Hit the pad `note` plays on `v` at `velocity`, accented from `ACCENT_VELOCITY` up.
fn strike(v: &mut PadVoice, note: u8, velocity: f32, p: &MonoParams) {
    let pad = Pad::from_gm(note);
    let knobs = p.drums.get(pad as usize).copied().unwrap_or_default();
    let gain = knobs.gain(velocity, velocity >= ACCENT_VELOCITY, p.drum_accent);
    v.set_pad(pad);
    v.trigger(&knobs, gain, p.sample_rate());
}

/// Who a voice plays for and what it plays.
#[derive(Clone, Copy, Default)]
struct Slot {
    owner: Option<Owner>,
    note: u8,
    /// The engine's note counter when it started: lower is older.
    age: u64,
    /// This voice's unison detune in cents (0 outside unison).
    cents: f32,
}

/// A number in −1..=1 that depends only on `i` and `salt`: a voice's own, repeatable offset.
fn unit(i: usize, salt: u32) -> f32 {
    let mut x = (i as u32 + 1).wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// The largest static detune of an analog voice, in cents, and of its drift; and its cutoff offset in semitones.
const ANALOG_CENTS: f32 = 6.0;
const DRIFT_CENTS: f32 = 3.0;
const ANALOG_CUTOFF: f32 = 0.5;

/// The seed of an owner's noise, as it was when each owner had a voice of its own.
fn owner_seed(owner: Owner) -> u32 {
    let index = match owner {
        Owner::Live(s) => usize::from(s),
        Owner::Channel(ch) => SYNTHS + usize::from(ch),
        Owner::Track(t) => SYNTHS + 16 + usize::from(t),
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
    /// Each voice's slow drift in cents, and the generator that moves it.
    drift: [f32; MAX_VOICES],
    rng: u32,
    /// The keys a unison synth holds, oldest first, to fall back on when the sounding one ends.
    keys: [(Option<Owner>, u8, f32); MAX_VOICES],
    nkeys: usize,
}

impl Pool {
    pub fn new(index: usize) -> Pool {
        let seed = |i: usize| ((index * MAX_VOICES + i) as u32 + 1).wrapping_mul(0x9E37_79B1);
        Pool {
            voices: std::array::from_fn(|i| PolyVoice::new(seed(i), None)),
            slots: [Slot::default(); MAX_VOICES],
            last: MAX_VOICES - 1,
            lfo: Lfo::default(),
            noise: Noise::new(seed(MAX_VOICES) | 1),
            shared: SharedMod::new(),
            drift: [0.0; MAX_VOICES],
            rng: seed(MAX_VOICES + 1) | 1,
            keys: [(None, 0, 0.0); MAX_VOICES],
            nkeys: 0,
        }
    }

    /// Voices sounding now.
    pub fn active(&self) -> usize {
        self.voices.iter().filter(|v| v.active()).count()
    }

    /// The notes held on the voices, lowest first (for tests).
    #[cfg(test)]
    pub fn held_notes(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self
            .slots
            .iter()
            .zip(&self.voices)
            .filter(|(_, v)| v.gated())
            .map(|(s, _)| s.note)
            .collect();
        v.sort_unstable();
        v
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
            PolyVoice::La(_)
            | PolyVoice::Fm(_)
            | PolyVoice::Drum(_)
            | PolyVoice::Sampler(_)
            | PolyVoice::Pad(_) => None,
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
            *v = PolyVoice::new(owner_seed_of_slot(i, s.owner), None);
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
        if p.model.uses_drums() {
            return self.playing(Pad::from_gm(note)).is_none();
        }
        if p.model.uses_pads() {
            return pad_of(note).is_some_and(|pad| self.playing_pad(pad).is_none());
        }
        let limit = p.voices();
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
        let limit = p.voices();
        if p.model.uses_drums() {
            self.hit(owner, note, velocity, p, clock);
        } else if p.model.uses_pads() {
            self.hit_pad(owner, note, velocity, p, clock);
        } else if limit == 1 {
            self.press_mono(owner, note, velocity, p, clock);
        } else if p.unison {
            self.remember(owner, note, velocity);
            self.sound_unison(owner, note, velocity, p, clock, limit);
        } else {
            self.press_poly(owner, note, velocity, p, clock, limit);
        }
    }

    /// Hold `note` in the unison key stack (a note held again moves to the top).
    fn remember(&mut self, owner: Owner, note: u8, velocity: f32) {
        self.forget(owner, note);
        if self.nkeys == MAX_VOICES {
            self.keys.copy_within(1.., 0);
            self.nkeys -= 1;
        }
        if let Some(k) = self.keys.get_mut(self.nkeys) {
            *k = (Some(owner), note, velocity);
            self.nkeys += 1;
        }
    }

    fn forget(&mut self, owner: Owner, note: u8) {
        let held = self.keys.get(..self.nkeys).unwrap_or(&[]);
        if let Some(i) = held
            .iter()
            .position(|(o, n, _)| *o == Some(owner) && *n == note)
        {
            if let Some(tail) = self.keys.get_mut(i..self.nkeys) {
                tail.rotate_left(1);
            }
            self.nkeys -= 1;
        }
    }

    /// Every voice of the pool plays `note`, spread in pitch by the unison detune.
    fn sound_unison(
        &mut self,
        owner: Owner,
        note: u8,
        velocity: f32,
        p: &MonoParams,
        clock: u64,
        limit: usize,
    ) {
        for i in 0..limit {
            let cents = if limit > 1 {
                let mid = (limit - 1) as f32 / 2.0;
                p.unison_cents * (i as f32 - mid) / mid
            } else {
                0.0
            };
            if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
                v.release_all();
                v.press(note, velocity, p, owner_seed(owner));
                *s = Slot {
                    owner: Some(owner),
                    note,
                    age: clock,
                    cents,
                };
            }
        }
        self.last = limit - 1;
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
                    *v = PolyVoice::new(owner_seed(owner), Some(p));
                }
                i
            }
        };
        if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
            v.press(note, velocity, p, owner_seed(owner));
            *s = Slot {
                owner: Some(owner),
                note,
                age: clock,
                cents: 0.0,
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
        if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
            v.press(note, velocity, p, owner_seed(owner));
            *s = Slot {
                owner: Some(owner),
                note,
                age: clock,
                cents: 0.0,
            };
        }
        self.last = i;
    }

    /// The voice sounding sampled pad `pad`, if one is.
    fn playing_pad(&self, pad: usize) -> Option<usize> {
        self.voices
            .iter()
            .position(|v| matches!(v, PolyVoice::Pad(d) if d.active() && d.pad() == pad))
    }

    /// Hit the sampled pad `note` plays: again on the voice already playing it, else on
    /// a new one; the other pads of its choke group fall silent.
    fn hit_pad(&mut self, owner: Owner, note: u8, velocity: f32, p: &MonoParams, clock: u64) {
        let Some(pad) = pad_of(note) else {
            return;
        };
        let group = p.pad_kit.pad(pad).map_or(0, |c| c.choke);
        if group != 0 {
            for v in self.voices.iter_mut() {
                if let PolyVoice::Pad(d) = v {
                    if d.pad() != pad && d.choke_group() == group {
                        d.choke();
                    }
                }
            }
        }
        let i = match self.playing_pad(pad) {
            Some(i) => i,
            None => self.allocate(MAX_VOICES),
        };
        if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
            v.press(note, velocity, p, owner_seed(owner));
            *s = Slot {
                owner: Some(owner),
                note,
                age: clock,
                cents: 0.0,
            };
        }
        self.last = i;
    }

    /// Add every sounding pad into `left` and `right` (the drum/pad sampler's stereo bus).
    pub fn render_pads(&mut self, samples: &SampleStore, left: &mut [f32], right: &mut [f32]) {
        for v in self.voices.iter_mut() {
            if let PolyVoice::Pad(d) = v {
                if d.active() {
                    d.render(samples, left, right);
                }
            }
        }
    }

    /// The voice sounding `pad`, if one is.
    fn playing(&self, pad: Pad) -> Option<usize> {
        self.voices
            .iter()
            .position(|v| matches!(v, PolyVoice::Drum(d) if d.active() && d.pad() == pad))
    }

    /// Hit the pad `note` plays: again on the voice already playing it (a pad
    /// retriggers, as on the 808), else on a new one. A closed hat chokes the
    /// open hat.
    fn hit(&mut self, owner: Owner, note: u8, velocity: f32, p: &MonoParams, clock: u64) {
        let pad = Pad::from_gm(note);
        if pad == Pad::Ch {
            for v in self.voices.iter_mut() {
                if let PolyVoice::Drum(d) = v {
                    if d.pad() == Pad::Oh {
                        d.choke();
                    }
                }
            }
        }
        let i = match self.playing(pad) {
            Some(i) => i,
            None => self.allocate(MAX_VOICES),
        };
        if let (Some(v), Some(s)) = (self.voices.get_mut(i), self.slots.get_mut(i)) {
            v.press(note, velocity, p, owner_seed(owner));
            *s = Slot {
                owner: Some(owner),
                note,
                age: clock,
                cents: 0.0,
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
        if p.model.uses_drums() {
            return;
        }
        if p.model.uses_pads() {
            for v in self.voices.iter_mut() {
                v.release(note, p);
            }
            return;
        }
        if p.voices() > 1 && p.unison {
            self.forget(owner, note);
            let sounding = self
                .slots
                .iter()
                .zip(&self.voices)
                .any(|(s, v)| s.owner == Some(owner) && s.note == note && v.gated());
            if !sounding {
                return;
            }
            // The last key still held takes over; with none, the voices release.
            let next = self
                .keys
                .get(..self.nkeys)
                .unwrap_or(&[])
                .iter()
                .rev()
                .find(|(o, _, _)| *o == Some(owner))
                .copied();
            match next {
                Some((_, n, vel)) => {
                    let age = self.slots.iter().map(|s| s.age).max().unwrap_or(0);
                    self.sound_unison(owner, n, vel, p, age, p.voices());
                }
                None => self.release_owner(owner),
            }
            return;
        }
        if p.voices() <= 1 {
            if let Some(i) = self.slots.iter().position(|s| s.owner == Some(owner)) {
                if let Some(v) = self.voices.get_mut(i) {
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
        if let Some(v) = held.and_then(|i| self.voices.get_mut(i)) {
            v.release(note, p);
        }
    }

    /// Release every voice `owner` holds.
    pub fn release_owner(&mut self, owner: Owner) {
        while self
            .keys
            .get(..self.nkeys)
            .is_some_and(|k| k.iter().any(|(o, _, _)| *o == Some(owner)))
        {
            let n = self.keys.get(..self.nkeys).and_then(|k| {
                k.iter()
                    .find(|(o, _, _)| *o == Some(owner))
                    .map(|(_, n, _)| *n)
            });
            match n {
                Some(n) => self.forget(owner, n),
                None => break,
            }
        }
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
        self.nkeys = 0;
        for v in self.voices.iter_mut() {
            v.release_all();
        }
    }

    /// Each voice's pitch and cutoff trim for the next block: its unison detune and,
    /// with `Analog`, its own static detune, a slow drift and a cutoff offset.
    fn retrim(&mut self, p: &MonoParams, poly: bool) {
        for (i, (v, s)) in self.voices.iter_mut().zip(self.slots.iter()).enumerate() {
            if !poly {
                v.set_trim(0.0, 0.0);
                continue;
            }
            let mut trim = 0.0;
            // xorshift: a step of drift in −1..=1.
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 17;
            self.rng ^= self.rng << 5;
            let step = (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            if let Some(d) = self.drift.get_mut(i) {
                *d = (*d * 0.998 + step * 0.12).clamp(-DRIFT_CENTS, DRIFT_CENTS);
                let own = unit(i, 1) * ANALOG_CENTS + *d;
                trim = (s.cents + own * p.analog) / 100.0;
            }
            v.set_trim(trim, unit(i, 2) * ANALOG_CUTOFF * p.analog);
        }
    }

    /// Add every sounding voice into `out`.
    pub fn render(&mut self, p: &MonoParams, tools: Tools, out: &mut [f32]) {
        let poly = p.voices() > 1 && !p.model.uses_drums();
        self.retrim(p, poly);
        if poly {
            self.shared
                .fill(&mut self.lfo, &mut self.noise, p, tools.sine, out.len());
        }
        let ctx = MonoCtx {
            params: p,
            sine: tools.sine,
            blep: tools.blep,
            ladder: tools.ladder,
            pitch: tools.pitch,
            shared: if poly { Some(&self.shared) } else { None },
            tables: tools.tables,
        };
        for v in self.voices.iter_mut().filter(|v| v.active()) {
            match v {
                PolyVoice::Mono(m) => m.render(&ctx, out),
                PolyVoice::La(l) => l.render(&ctx, out),
                PolyVoice::Fm(f) => f.render(&ctx, out),
                PolyVoice::Drum(d) => d.render(ctx.sine, ctx.blep, out),
                PolyVoice::Sampler(v) => v.render(&ctx, tools.samples, tools.zones, out),
                // Pads write both sides: see `render_pads`.
                PolyVoice::Pad(_) => {}
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
    use crate::mono::ladder::LadderTables;
    use crate::mono::osc::Blep;
    use crate::mono::voice::PitchTable;
    use crate::sample::SampleStore;
    use crate::sampler::ZoneMap;
    use crate::table::Tables;
    use crate::voice::sine_table;

    const SR: f32 = 48_000.0;

    struct Rig {
        pool: Pool,
        params: MonoParams,
        sine: Vec<f32>,
        blep: Blep,
        ladder: LadderTables,
        pitch: PitchTable,
        tables: &'static Tables,
        samples: SampleStore,
        zones: ZoneMap,
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
                tables: Tables::shared(SR),
                samples: SampleStore::new(),
                zones: ZoneMap::new(),
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
                let tools = Tools {
                    sine: &self.sine,
                    blep: &self.blep,
                    ladder: &self.ladder,
                    pitch: &self.pitch,
                    tables: self.tables,
                    samples: &self.samples,
                    zones: &self.zones,
                };
                self.pool.render(&self.params, tools, &mut out);
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
                PolyVoice::La(_)
                | PolyVoice::Fm(_)
                | PolyVoice::Drum(_)
                | PolyVoice::Sampler(_)
                | PolyVoice::Pad(_) => 0.0,
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

    fn trims(r: &Rig) -> Vec<f32> {
        r.pool
            .voices
            .iter()
            .zip(&r.pool.slots)
            .filter(|(v, _)| v.active())
            .map(|(v, _)| match v {
                PolyVoice::Mono(m) => m.trim,
                PolyVoice::La(l) => l.trim,
                PolyVoice::Fm(f) => f.trim,
                PolyVoice::Drum(_) => 1.0,
                PolyVoice::Sampler(v) => v.trim,
                PolyVoice::Pad(_) => 1.0,
            })
            .collect()
    }

    /// Spec 006 Req 3: unison presses the note on every voice, spread evenly.
    #[test]
    fn unison_spreads_every_voice_around_the_note() {
        use crate::params::Param;
        let mut r = Rig::new(6.0);
        for (p, v) in [(Param::Assign, 1.0), (Param::UnisonDetune, 0.4)] {
            r.params.set(p, p.clamp(v));
        }
        r.on(LIVE, 60);
        let out = r.run(40);
        assert_eq!(r.pool.active(), 6, "all six voices play the one note");
        assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 6.0));
        let mut t = trims(&r);
        t.sort_by(f32::total_cmp);
        assert_eq!(t.len(), 6);
        // 0.4 of 50 cents: ±20 cents, evenly spaced, six distinct pitches.
        assert!(
            (t[0] + 0.2).abs() < 1.0e-5 && (t[5] - 0.2).abs() < 1.0e-5,
            "{t:?}"
        );
        for w in t.windows(2) {
            assert!((w[1] - w[0] - 0.08).abs() < 1.0e-5, "even steps: {t:?}");
        }
    }

    #[test]
    fn unison_falls_back_to_the_last_key_held() {
        use crate::params::Param;
        let mut r = Rig::new(4.0);
        r.params.set(Param::Assign, 1.0);
        r.on(LIVE, 60);
        r.on(LIVE, 64);
        assert_eq!(r.held(), vec![64, 64, 64, 64]);
        r.off(LIVE, 64);
        assert_eq!(r.held(), vec![60, 60, 60, 60], "back to the key still down");
        r.off(LIVE, 60);
        assert!(r.held().is_empty());
        // A key let go while another sounds changes nothing.
        r.on(LIVE, 60);
        r.on(LIVE, 64);
        r.off(LIVE, 60);
        assert_eq!(r.held(), vec![64, 64, 64, 64]);
    }

    /// Spec 006 Req 3: `Analog` gives each voice its own bounded offsets, the same
    /// every time, and 0 is exact.
    #[test]
    fn analog_variance_is_bounded_repeatable_and_off_at_zero() {
        use crate::params::Param;
        let chord = |analog: f32| {
            let mut r = Rig::new(6.0);
            r.params.set(Param::Analog, analog);
            for n in [60, 64, 67, 71] {
                r.on(LIVE, n);
            }
            let out = r.run(300);
            (trims(&r), out)
        };
        let (exact, out0) = chord(0.0);
        assert!(exact.iter().all(|t| *t == 0.0), "{exact:?}");
        let (loose, out1) = chord(1.0);
        let (again, out2) = chord(1.0);
        assert_eq!(out1, out2, "the same seed gives the same sound");
        assert_ne!(out0, out1, "and variance is audible");
        assert_eq!(loose, again);
        assert!(loose.iter().any(|t| *t != 0.0));
        // At most 6 cents of static detune and 3 of drift.
        assert!(loose.iter().all(|t| t.abs() <= 0.0901), "{loose:?}");
        let distinct = loose
            .iter()
            .map(|t| t.to_bits())
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert_eq!(distinct, loose.len(), "every voice has its own");
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
