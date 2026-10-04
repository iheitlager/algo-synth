//! The multisampler (plan.md samplers epic, #123): a synth model whose voices
//! play zones of the sample store (`sample`).
//!
//! A *zone* maps a key range and a velocity range to a sample, with its root
//! key, tune, level, a loop mode with loop points, a round-robin position and
//! a release flag. Each synth owns a `ZoneMap`. A voice picks its zone when it
//! starts: the first zone covering the note and velocity (round-robin zones
//! take turns per key), plays it with an interpolated read at the pitch ratio
//! of note against root, through the synth's own filter and amplifier
//! envelope. A *release* zone sounds once when the key comes up. Voices come
//! from the pool, so polyphony, stealing and the MIDI player work as for every
//! other model (ADR-0011).
//!
//! Real-time rules (ADR-0002): zones and voices are plain values allocated up
//! front; a block costs one `exp2` per voice, a sample a few multiplies.
//! Stereo samples are mixed to mono here, as the mixer's strips are mono
//! until the pan (ADR-0010).

use std::cell::Cell;

use crate::mono::env::{Env, Stage};
use crate::mono::ladder::{Ladder, MAX_K};
use crate::mono::voice::MonoCtx;
use crate::sample::{SLOTS, Sample, SampleStore};

/// Zones a synth holds.
pub const ZONES: usize = 64;

/// Longest loop crossfade, in frames.
const CROSSFADE: usize = 256;

/// Output level, as the other voices'.
const OUT_GAIN: f32 = 0.7;

/// A zone's loop: how the sample behaves past its loop end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum LoopMode {
    /// Plays through once; the key coming up releases the envelope.
    #[default]
    Off = 0,
    /// Loops until the voice ends, release included.
    Loop = 1,
    /// Loops while the key is held, then plays on through the tail.
    Sustain = 2,
}

/// A zone parameter id for `ZoneMap::set`; mirrored in `web/src/audio/params.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ZoneField {
    /// Sample slot, or −1 to empty the zone.
    Sample = 0,
    KeyLo = 1,
    KeyHi = 2,
    VelLo = 3,
    VelHi = 4,
    /// Root key; −1 takes the sample's own.
    Root = 5,
    /// Fine tune in cents.
    Tune = 6,
    Level = 7,
    /// A `LoopMode`.
    Loop = 8,
    /// Loop points in frames; both 0 take the sample's own.
    LoopStart = 9,
    LoopEnd = 10,
    /// Round-robin: the zone is the `SeqPos`th of `SeqLen` that take turns.
    SeqLen = 11,
    SeqPos = 12,
    /// Sounds when the key comes up instead of when it goes down.
    Release = 13,
}

impl ZoneField {
    /// Every field with the name the TypeScript mirror uses.
    pub const ALL: [(ZoneField, &'static str); 14] = [
        (ZoneField::Sample, "Sample"),
        (ZoneField::KeyLo, "KeyLo"),
        (ZoneField::KeyHi, "KeyHi"),
        (ZoneField::VelLo, "VelLo"),
        (ZoneField::VelHi, "VelHi"),
        (ZoneField::Root, "Root"),
        (ZoneField::Tune, "Tune"),
        (ZoneField::Level, "Level"),
        (ZoneField::Loop, "Loop"),
        (ZoneField::LoopStart, "LoopStart"),
        (ZoneField::LoopEnd, "LoopEnd"),
        (ZoneField::SeqLen, "SeqLen"),
        (ZoneField::SeqPos, "SeqPos"),
        (ZoneField::Release, "Release"),
    ];

    pub fn from_id(id: u32) -> Option<ZoneField> {
        Self::ALL
            .iter()
            .find(|(f, _)| *f as u32 == id)
            .map(|(f, _)| *f)
    }
}

/// One zone. An empty one (`sample` `None`) never sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zone {
    pub sample: Option<usize>,
    pub key_lo: u8,
    pub key_hi: u8,
    pub vel_lo: u8,
    pub vel_hi: u8,
    /// `None` takes the sample's root.
    pub root: Option<u8>,
    pub tune: f32,
    pub level: f32,
    pub loop_mode: LoopMode,
    pub loop_start: usize,
    pub loop_end: usize,
    pub seq_len: u8,
    pub seq_pos: u8,
    pub release: bool,
}

impl Default for Zone {
    fn default() -> Zone {
        Zone {
            sample: None,
            key_lo: 0,
            key_hi: 127,
            vel_lo: 1,
            vel_hi: 127,
            root: None,
            tune: 0.0,
            level: 1.0,
            loop_mode: LoopMode::Off,
            loop_start: 0,
            loop_end: 0,
            seq_len: 1,
            seq_pos: 1,
            release: false,
        }
    }
}

impl Zone {
    fn covers(&self, note: u8, vel: u8, release: bool) -> bool {
        self.sample.is_some()
            && self.release == release
            && (self.key_lo..=self.key_hi).contains(&note)
            && (self.vel_lo..=self.vel_hi).contains(&vel)
    }

    /// Set a field from a view value, clamped into its range.
    pub fn set(&mut self, field: ZoneField, v: f32) {
        let v = if v.is_finite() { v } else { 0.0 };
        let byte = |lo: f32| v.round().clamp(lo, 127.0) as u8;
        match field {
            ZoneField::Sample => {
                let slot = v.round();
                self.sample = (slot >= 0.0 && (slot as usize) < SLOTS).then_some(slot as usize);
            }
            ZoneField::KeyLo => self.key_lo = byte(0.0),
            ZoneField::KeyHi => self.key_hi = byte(0.0),
            ZoneField::VelLo => self.vel_lo = byte(1.0),
            ZoneField::VelHi => self.vel_hi = byte(1.0),
            ZoneField::Root => self.root = (v >= 0.0).then(|| byte(0.0)),
            ZoneField::Tune => self.tune = v.clamp(-1200.0, 1200.0),
            ZoneField::Level => self.level = v.clamp(0.0, 2.0),
            ZoneField::Loop => {
                self.loop_mode = match v.round() as i32 {
                    1 => LoopMode::Loop,
                    2 => LoopMode::Sustain,
                    _ => LoopMode::Off,
                }
            }
            ZoneField::LoopStart => self.loop_start = v.max(0.0) as usize,
            ZoneField::LoopEnd => self.loop_end = v.max(0.0) as usize,
            ZoneField::SeqLen => self.seq_len = v.round().clamp(1.0, 16.0) as u8,
            ZoneField::SeqPos => self.seq_pos = v.round().clamp(1.0, 16.0) as u8,
            ZoneField::Release => self.release = v >= 0.5,
        }
    }
}

/// A synth's zones and the round-robin turn of each key.
pub struct ZoneMap {
    zones: [Zone; ZONES],
    /// Per key: the next round-robin turn, for note-on zones and release zones.
    turn: [[Cell<u8>; 128]; 2],
}

impl Default for ZoneMap {
    fn default() -> ZoneMap {
        ZoneMap::new()
    }
}

impl ZoneMap {
    pub fn new() -> ZoneMap {
        ZoneMap {
            zones: [Zone::default(); ZONES],
            turn: std::array::from_fn(|_| std::array::from_fn(|_| Cell::new(0))),
        }
    }

    pub fn set(&mut self, zone: usize, field: ZoneField, v: f32) {
        if let Some(z) = self.zones.get_mut(zone) {
            z.set(field, v);
        }
    }

    pub fn get(&self, zone: usize) -> Option<&Zone> {
        self.zones.get(zone)
    }

    /// Empty every zone.
    pub fn clear(&mut self) {
        self.zones = [Zone::default(); ZONES];
    }

    /// The zone that sounds for `note` at `vel` (1..=127), or for its release.
    /// Of several covering zones the first wins; round-robin zones take turns
    /// per key, in `seq_pos` order.
    pub fn pick(&self, note: u8, vel: u8, release: bool) -> Option<Zone> {
        let turn = self
            .turn
            .get(usize::from(release))
            .and_then(|t| t.get(usize::from(note)))?;
        let chosen = self.zones.iter().find(|z| {
            z.covers(note, vel, release)
                && (z.seq_len <= 1 || turn.get() % z.seq_len + 1 == z.seq_pos)
        })?;
        if chosen.seq_len > 1 {
            turn.set((turn.get() + 1) % chosen.seq_len);
        }
        Some(*chosen)
    }
}

/// A zone being played: the position in the sample and what to read.
#[derive(Clone, Copy, Default)]
struct Player {
    zone: Zone,
    pos: f64,
    on: bool,
}

/// The loop in frames if the zone loops and the points make sense.
fn loop_points(zone: &Zone, s: &Sample, gated: bool) -> Option<(usize, usize)> {
    let live = match zone.loop_mode {
        LoopMode::Off => false,
        LoopMode::Loop => true,
        LoopMode::Sustain => gated,
    };
    if !live {
        return None;
    }
    let (start, end) = if zone.loop_start == 0 && zone.loop_end == 0 {
        s.loop_range?
    } else {
        (zone.loop_start, zone.loop_end)
    };
    let end = end.min(s.frames());
    (start + 2 <= end).then_some((start, end))
}

/// Four-point Catmull-Rom read at `pos`; frames outside the sample are
/// the nearest edge.
fn read(s: &Sample, pos: f64) -> f32 {
    let last = s.frames().saturating_sub(1) as isize;
    let i = pos.floor();
    let t = (pos - i) as f32;
    let at = |k: isize| {
        let (l, r) = s.frame((i as isize + k).clamp(0, last) as usize);
        0.5 * (l + r)
    };
    let (p0, p1, p2, p3) = (at(-1), at(0), at(1), at(2));
    p1 + 0.5
        * t
        * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
}

impl Player {
    fn start(zone: Zone) -> Player {
        Player {
            zone,
            pos: 0.0,
            on: true,
        }
    }

    /// The next sample at `inc` frames per output frame; ends the player at
    /// the sample's end.
    fn step(&mut self, store: &SampleStore, inc: f64, gated: bool) -> f32 {
        let Some(s) = self.zone.sample.and_then(|slot| store.get(slot)) else {
            self.on = false;
            return 0.0;
        };
        let len = s.frames();
        if self.pos >= len as f64 {
            self.on = false;
            return 0.0;
        }
        let mut y = read(s, self.pos);
        if let Some((start, end)) = loop_points(&self.zone, s, gated) {
            // Blend the last frames before the loop end with the audio that
            // precedes the loop start, so the wrap is seamless.
            let xf = CROSSFADE.min(start).min((end - start) / 2);
            let from = (end - xf) as f64;
            if xf > 0 && self.pos >= from && self.pos < end as f64 {
                let t = ((self.pos - from) / xf as f64) as f32;
                y = y * (1.0 - t) + read(s, self.pos - (end - start) as f64) * t;
            }
            self.pos += inc;
            if self.pos >= end as f64 {
                self.pos -= (end - start) as f64;
            }
        } else {
            self.pos += inc;
        }
        y
    }
}

/// One voice of a sampler synth's pool.
#[derive(Clone, Copy, Default)]
pub struct SamplerVoice {
    note: u8,
    velocity: f32,
    gate: bool,
    /// A note-on waiting for the next block to pick its zone.
    retrigger: bool,
    /// A note-off waiting for the next block to start its release zone.
    release: bool,
    /// Pitch trim in semitones from the pool (unison, analog variance).
    pub trim: f32,
    pub cutoff_trim: f32,
    main: Player,
    tail: Player,
    tva: Env,
    tvf: Env,
    lp: Ladder,
}

impl SamplerVoice {
    pub fn new() -> SamplerVoice {
        SamplerVoice {
            lp: Ladder::new(),
            ..SamplerVoice::default()
        }
    }

    pub fn active(&self) -> bool {
        self.retrigger || self.release || self.tail.on || self.tva.stage != Stage::Idle
    }

    pub fn gated(&self) -> bool {
        self.gate
    }

    pub fn press(&mut self, note: u8, velocity: f32) {
        if !self.active() {
            self.lp = Ladder::new();
        }
        self.note = note;
        self.velocity = velocity.clamp(0.0, 1.0);
        self.gate = true;
        self.retrigger = true;
        self.release = false;
        self.tail.on = false;
    }

    pub fn release_all(&mut self) {
        if self.gate {
            self.gate = false;
            self.release = true;
        }
    }

    fn vel7(&self) -> u8 {
        (self.velocity * 127.0).round().clamp(1.0, 127.0) as u8
    }

    /// Add this voice into `out`, advancing its state.
    pub fn render(&mut self, ctx: &MonoCtx, store: &SampleStore, zones: &ZoneMap, out: &mut [f32]) {
        let p = ctx.params;
        if self.retrigger {
            self.main = zones
                .pick(self.note, self.vel7(), false)
                .map_or_else(Player::default, Player::start);
            self.tva.gate_on(&p.adsr);
            self.tvf.gate_on(&p.fadsr);
            self.retrigger = false;
        } else if !self.gate && self.tva.gated() {
            self.tva.gate_off(&p.adsr);
            self.tvf.gate_off(&p.fadsr);
        }
        if self.release {
            self.release = false;
            if let Some(z) = zones.pick(self.note, self.vel7(), true) {
                self.tail = Player::start(z);
            }
        }
        self.tva.set_sustain(p.adsr.sustain);
        self.tvf.set_sustain(p.fadsr.sustain);

        // Frames of sample per output frame, from the key against the root.
        let ratio = |z: &Zone, store: &SampleStore| {
            let root = z
                .root
                .or_else(|| z.sample.and_then(|s| store.get(s)).map(|s| s.root))
                .unwrap_or(60);
            let semis =
                f32::from(self.note) - f32::from(root) + self.trim + p.tune[0] + z.tune / 100.0;
            f64::from((semis / 12.0).exp2())
        };
        let main_inc = ratio(&self.main.zone, store);
        let tail_inc = ratio(&self.tail.zone, store);
        let key = f32::from(self.note) - 60.0;
        let gain = self.velocity * p.level[0] * OUT_GAIN;
        let gate = self.gate;

        for sample in out.iter_mut() {
            let a = self.tva.step();
            let f = self.tvf.step();
            let mut y = 0.0;
            if self.main.on {
                let raw = self.main.step(store, main_inc, gate);
                let cutoff = p.cutoff
                    + p.normals.env_cutoff * f
                    + key * p.normals.key_track
                    + self.cutoff_trim;
                let k = p.k.clamp(0.0, MAX_K);
                y = self.lp.process(ctx.ladder, raw * 0.5, cutoff, k, 1.0)
                    * 2.0
                    * a
                    * self.main.zone.level;
            }
            *sample += y * gain;
            if self.tail.on {
                *sample += self.tail.step(store, tail_inc, false) * self.tail.zone.level * gain;
            }
            if self.tva.stage == Stage::Idle && !self.tail.on {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(f: &[(ZoneField, f32)]) -> Zone {
        let mut z = Zone::default();
        for (field, v) in f {
            z.set(*field, *v);
        }
        z
    }

    #[test]
    fn fields_clamp_and_ignore_nan() {
        let z = z(&[
            (ZoneField::Sample, 3.0),
            (ZoneField::KeyLo, 300.0),
            (ZoneField::VelLo, 0.0),
            (ZoneField::Tune, f32::NAN),
            (ZoneField::Level, 9.0),
            (ZoneField::Root, -1.0),
        ]);
        assert_eq!(z.sample, Some(3));
        assert_eq!(z.key_lo, 127);
        assert_eq!(z.vel_lo, 1);
        assert_eq!(z.tune, 0.0);
        assert_eq!(z.level, 2.0);
        assert_eq!(z.root, None);
        let mut z = z;
        z.set(ZoneField::Sample, 9999.0);
        assert_eq!(z.sample, None, "a slot past the store empties the zone");
    }

    #[test]
    fn pick_follows_key_and_velocity() {
        let mut m = ZoneMap::new();
        for (i, (lo, hi, vlo, vhi, slot)) in [
            (0, 59, 1, 127, 0.0),
            (60, 127, 1, 63, 1.0),
            (60, 127, 64, 127, 2.0),
        ]
        .into_iter()
        .enumerate()
        {
            m.set(i, ZoneField::KeyLo, lo as f32);
            m.set(i, ZoneField::KeyHi, hi as f32);
            m.set(i, ZoneField::VelLo, vlo as f32);
            m.set(i, ZoneField::VelHi, vhi as f32);
            m.set(i, ZoneField::Sample, slot);
        }
        assert_eq!(m.pick(40, 100, false).unwrap().sample, Some(0));
        assert_eq!(m.pick(72, 30, false).unwrap().sample, Some(1));
        assert_eq!(m.pick(72, 100, false).unwrap().sample, Some(2));
        assert!(m.pick(72, 100, true).is_none(), "no release zone");
    }

    #[test]
    fn round_robin_takes_turns_per_key() {
        let mut m = ZoneMap::new();
        for n in 0..3 {
            m.set(n, ZoneField::Sample, n as f32);
            m.set(n, ZoneField::SeqLen, 3.0);
            m.set(n, ZoneField::SeqPos, n as f32 + 1.0);
        }
        let seq: Vec<_> = (0..6)
            .map(|_| m.pick(60, 100, false).unwrap().sample.unwrap())
            .collect();
        assert_eq!(seq, vec![0, 1, 2, 0, 1, 2]);
        assert_eq!(
            m.pick(61, 100, false).unwrap().sample,
            Some(0),
            "another key starts over"
        );
    }

    #[test]
    fn release_zones_are_separate() {
        let mut m = ZoneMap::new();
        m.set(0, ZoneField::Sample, 0.0);
        m.set(1, ZoneField::Sample, 1.0);
        m.set(1, ZoneField::Release, 1.0);
        assert_eq!(m.pick(60, 100, false).unwrap().sample, Some(0));
        assert_eq!(m.pick(60, 100, true).unwrap().sample, Some(1));
    }

    // --- through the engine ------------------------------------------------

    use crate::engine::{BLOCK, Engine};
    use crate::mono::preset::Preset;
    use crate::sample::test_wav;

    const SR: f32 = 48_000.0;

    /// A sample's values and its loop points.
    type Wave = (Vec<f32>, Option<(u32, u32)>);

    /// `frames` of a sine with `period` frames to the cycle.
    fn sine(frames: usize, period: f32, amp: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| (i as f32 / period * std::f32::consts::TAU).sin() * amp)
            .collect()
    }

    /// A sampler on synth 0 with `waves` loaded into slots 0.., root 60.
    fn rig(waves: &[Wave]) -> Engine {
        let mut e = Engine::new(SR);
        e.preset(0, Preset::SamplerKeys);
        for (slot, (data, loop_range)) in waves.iter().enumerate() {
            let file = test_wav(
                48_000,
                data,
                Some((
                    60,
                    loop_range.map_or(0, |l| l.0),
                    loop_range.map_or(0, |l| l.1),
                )),
            );
            e.sample_buffer(file.len())
                .expect("fits")
                .copy_from_slice(&file);
            e.load_sample(slot).expect("loads");
        }
        e
    }

    /// The left channel of the next `blocks` blocks.
    fn left(e: &mut Engine, blocks: usize) -> Vec<f32> {
        let mut all = Vec::new();
        for _ in 0..blocks {
            e.render(BLOCK);
            all.extend_from_slice(e.output().get(..BLOCK).unwrap_or(&[]));
        }
        all
    }

    fn crossings(x: &[f32]) -> usize {
        x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0, |m, v| m.max(v.abs()))
    }

    /// A one-second sine at 480 Hz, 100 frames to the cycle.
    fn one_second() -> Wave {
        (sine(48_000, 100.0, 0.9), None)
    }

    #[test]
    fn plays_at_its_root_and_an_octave_up_at_double() {
        let mut e = rig(&[one_second()]);
        e.set_zone(0, 0, ZoneField::Sample, 0.0);
        e.note_on(0, 60, 1.0);
        let root = left(&mut e, 100);
        assert!(root.iter().all(|v| v.is_finite()) && peak(&root) < 1.0);
        assert!(peak(&root) > 0.1, "it sounds");
        // 100 blocks of 128 frames at 480 Hz.
        let n = crossings(&root);
        assert!((126..=130).contains(&n), "root key: {n} cycles");
        e.all_off();
        left(&mut e, 400);
        e.note_on(0, 72, 1.0);
        let up = left(&mut e, 100);
        let n = crossings(&up);
        assert!((252..=260).contains(&n), "an octave up: {n} cycles");
    }

    #[test]
    fn silent_without_a_zone_for_the_key() {
        let mut e = rig(&[one_second()]);
        e.set_zone(0, 0, ZoneField::Sample, 0.0);
        e.set_zone(0, 0, ZoneField::KeyHi, 59.0);
        e.note_on(0, 60, 1.0);
        assert_eq!(peak(&left(&mut e, 20)), 0.0);
        e.note_on(0, 50, 1.0);
        assert!(peak(&left(&mut e, 20)) > 0.1);
    }

    #[test]
    fn velocity_layers_pick_their_sample() {
        let mut e = rig(&[
            (sine(48_000, 100.0, 0.9), None),
            (sine(48_000, 50.0, 0.9), None),
        ]);
        e.set_zone(0, 0, ZoneField::Sample, 0.0);
        e.set_zone(0, 0, ZoneField::VelHi, 63.0);
        e.set_zone(0, 1, ZoneField::Sample, 1.0);
        e.set_zone(0, 1, ZoneField::VelLo, 64.0);
        e.note_on(0, 60, 0.3);
        let soft = crossings(&left(&mut e, 50));
        e.all_off();
        left(&mut e, 400);
        e.note_on(0, 60, 1.0);
        let hard = crossings(&left(&mut e, 50));
        assert!((62..=66).contains(&soft), "soft layer: {soft}");
        assert!((126..=130).contains(&hard), "hard layer: {hard}");
    }

    #[test]
    fn a_sustain_loop_holds_the_note_and_the_wrap_is_seamless() {
        // 100 frames to the cycle, so a loop of ten cycles joins without a step.
        let mut e = rig(&[(sine(2000, 100.0, 0.9), Some((500, 1499)))]);
        e.set_zone(0, 0, ZoneField::Sample, 0.0);
        e.set_zone(0, 0, ZoneField::Loop, 2.0);
        e.note_on(0, 60, 1.0);
        let held = left(&mut e, 800);
        // Two seconds of a 2000-frame (42 ms) sample still sounds.
        let tail = held.get(held.len() - 4000..).unwrap_or(&[]);
        assert!(peak(tail) > 0.1, "still sounding after two seconds");
        let widest = held
            .windows(2)
            .skip(4 * BLOCK)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(widest < 0.08, "no click at the loop point: {widest}");
        // Released, it plays on through the tail and ends.
        e.note_off(0, 60);
        left(&mut e, 400);
        assert_eq!(e.active_voices(), 0);
    }

    #[test]
    fn a_release_zone_sounds_when_the_key_comes_up() {
        let mut e = rig(&[(vec![0.0; 48_000], None), (sine(2000, 50.0, 0.9), None)]);
        e.set_zone(0, 0, ZoneField::Sample, 0.0);
        e.set_zone(0, 1, ZoneField::Sample, 1.0);
        e.set_zone(0, 1, ZoneField::Release, 1.0);
        e.note_on(0, 60, 1.0);
        assert!(
            peak(&left(&mut e, 20)) < 1.0e-4,
            "nothing before the key comes up"
        );
        e.note_off(0, 60);
        assert!(peak(&left(&mut e, 4)) > 0.1, "the release sample sounds");
    }

    #[test]
    fn many_notes_steal_voices_and_stay_bounded() {
        let mut e = rig(&[one_second()]);
        e.set_zone(0, 0, ZoneField::Sample, 0.0);
        for n in 0..40 {
            e.note_on(0, 36 + n, 1.0);
            left(&mut e, 2);
        }
        assert!(e.active_voices() <= crate::poly::MAX_VOICES);
        let y = left(&mut e, 50);
        assert!(y.iter().all(|v| v.is_finite()));
        assert!(peak(&y) <= 1.0, "the limiter holds");
    }

    #[test]
    fn a_missing_sample_is_silence_not_a_panic() {
        let mut e = Engine::new(SR);
        e.preset(0, Preset::SamplerKeys);
        e.set_zone(0, 0, ZoneField::Sample, 5.0);
        e.note_on(0, 60, 1.0);
        assert!(peak(&left(&mut e, 10)) < 1.0e-4);
    }

    #[test]
    fn a_voice_is_idle_until_pressed() {
        let v = SamplerVoice::new();
        assert!(!v.active() && !v.gated());
    }
}
