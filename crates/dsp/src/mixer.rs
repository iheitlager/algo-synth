//! The mixer (ADR-0005, ADR-0010, spec 002 Req 2): one strip per synth and
//! eight group buses, each with insert slots, a fader, a pan and four
//! sends (P1–P4), each after the fader or before it and switched on or off (#144), routed by `Out` to the stereo master or to a
//! higher-numbered group. It owns every strip parameter (`Param::is_strip`).
//! Everything is allocated in `Mixer::new` (ADR-0002); gains and the solo
//! paths are worked out when a parameter changes, never per sample, and a
//! strip's gains ramp to new values across a block so a moving fader, pan or
//! send does not zipper (#271).

use crate::engine::{BLOCK, SYNTHS};
use crate::fx::insert::{Insert, InsertType};
use crate::params::{INSERT_SLOTS, InsertField, Param};
use crate::voice::Gains;

/// The processors the sends feed (P1–P4).
pub const SENDS: usize = 4;
/// Group buses, strips 16–23.
pub const GROUPS: usize = 8;
/// Every strip: the synths, then the groups.
pub const STRIPS: usize = SYNTHS + GROUPS;
/// The `Out` that goes nowhere (#161): the strip still feeds its sends and keys.
pub const OUT_NONE: usize = GROUPS + 1;

/// Where a strip starts: full fader, centred, no sends, not muted or soloed.
pub const STRIP_DEFAULTS: [(Param, f32); 36] = [
    (Param::Level, 1.0),
    (Param::Pan, 0.0),
    (Param::Send1, 0.0),
    (Param::Send2, 0.0),
    (Param::Send3, 0.0),
    (Param::Send4, 0.0),
    (Param::Mute, 0.0),
    (Param::Solo, 0.0),
    (Param::Out, 0.0),
    (Param::I1Type, 0.0),
    (Param::I1A, 0.5),
    (Param::I1B, 0.5),
    (Param::I1C, 0.5),
    (Param::I1D, 0.5),
    (Param::I1E, 0.0),
    (Param::I2Type, 0.0),
    (Param::I2A, 0.5),
    (Param::I2B, 0.5),
    (Param::I2C, 0.5),
    (Param::I2D, 0.5),
    (Param::I2E, 0.0),
    (Param::I3Type, 0.0),
    (Param::I3A, 0.5),
    (Param::I3B, 0.5),
    (Param::I3C, 0.5),
    (Param::I3D, 0.5),
    (Param::I3E, 0.0),
    (Param::Send1Pre, 0.0),
    (Param::Send2Pre, 0.0),
    (Param::Send3Pre, 0.0),
    (Param::Send4Pre, 0.0),
    (Param::Send1On, 1.0),
    (Param::Send2On, 1.0),
    (Param::Send3On, 1.0),
    (Param::Send4On, 1.0),
    (Param::Key, 0.0),
];

/// One strip's or group's fader, pan, sends and routing, as set from the view.
#[derive(Clone, Copy)]
struct Strip {
    level: f32,
    /// Left and right gains: an equal-power pan for a synth, a balance for a group.
    pan: [f32; 2],
    send: [f32; SENDS],
    /// Per send: taken before the fader (#144), and switched on.
    pre: [bool; SENDS],
    on: [bool; SENDS],
    mute: bool,
    solo: bool,
    /// Left and right gains for a stereo strip: a balance, unity at the centre.
    balance: [f32; 2],
    /// 0 is the master, 1–8 a group, `OUT_NONE` nowhere.
    out: usize,
    /// The synth (1–16) whose raw signal a vocoder here follows; 0 none.
    key: usize,
    group: bool,
}

impl Strip {
    fn new(group: bool) -> Strip {
        let mut s = Strip {
            level: 0.0,
            pan: [0.0; 2],
            balance: [1.0; 2],
            send: [0.0; SENDS],
            pre: [false; SENDS],
            on: [true; SENDS],
            mute: false,
            solo: false,
            out: 0,
            key: 0,
            group,
        };
        for (p, v) in STRIP_DEFAULTS {
            s.set(p, v);
        }
        s
    }

    fn set(&mut self, param: Param, v: f32) {
        match param {
            Param::Level => self.level = v,
            Param::Pan => {
                self.balance = [1.0 - v.max(0.0), 1.0 + v.min(0.0)];
                self.pan = if self.group {
                    // A group is already stereo: balance, unity at the centre.
                    [1.0 - v.max(0.0), 1.0 + v.min(0.0)]
                } else {
                    // Equal power: −1 is all left, +1 all right, 0 is −3 dB each.
                    let angle = (v + 1.0) * std::f32::consts::FRAC_PI_4;
                    match v {
                        v if v <= -1.0 => [1.0, 0.0],
                        v if v >= 1.0 => [0.0, 1.0],
                        _ => [angle.cos(), angle.sin()],
                    }
                };
            }
            Param::Send1 => self.send[0] = v,
            Param::Send2 => self.send[1] = v,
            Param::Send3 => self.send[2] = v,
            Param::Send4 => self.send[3] = v,
            Param::Send1Pre => self.pre[0] = v >= 0.5,
            Param::Send2Pre => self.pre[1] = v >= 0.5,
            Param::Send3Pre => self.pre[2] = v >= 0.5,
            Param::Send4Pre => self.pre[3] = v >= 0.5,
            Param::Send1On => self.on[0] = v >= 0.5,
            Param::Send2On => self.on[1] = v >= 0.5,
            Param::Send3On => self.on[2] = v >= 0.5,
            Param::Send4On => self.on[3] = v >= 0.5,
            Param::Mute => self.mute = v >= 0.5,
            Param::Solo => self.solo = v >= 0.5,
            Param::Out => self.out = v.round() as usize,
            Param::Key => self.key = v.round() as usize,
            _ => {}
        }
    }

    /// Where this block's gains head: left and right into the strip's out,
    /// then the four sends. A stereo source feeds a send half from each side.
    fn targets(&self, stereo: bool) -> [f32; 2 + SENDS] {
        let [l, r] = if stereo && !self.group {
            self.balance
        } else {
            self.pan
        };
        let half = if stereo { 0.5 } else { 1.0 };
        let mut g = [self.level * l, self.level * r, 0.0, 0.0, 0.0, 0.0];
        for (k, v) in g.iter_mut().skip(2).enumerate() {
            *v = half * self.send_gain(k);
        }
        g
    }

    /// What send `k` takes of the strip's signal: its level, times the fader
    /// unless it is taken before it, and nothing when it is off.
    fn send_gain(&self, k: usize) -> f32 {
        let (Some(amount), Some(pre), Some(on)) =
            (self.send.get(k), self.pre.get(k), self.on.get(k))
        else {
            return 0.0;
        };
        match (on, pre) {
            (false, _) => 0.0,
            (true, true) => *amount,
            (true, false) => amount * self.level,
        }
    }
}

pub struct Mixer {
    /// Each strip's insert slots, between its voices and its fader.
    inserts: Vec<[Insert; INSERT_SLOTS]>,
    /// Each synth's mono bus, filled by its voices.
    bus: Box<[[f32; BLOCK]; SYNTHS]>,
    /// The right side of a synth that is stereo (the chorus): its bus is then the left.
    bus_r: Box<[[f32; BLOCK]; SYNTHS]>,
    wide: [bool; SYNTHS],
    /// The dry signal while a stereo effect writes both sides.
    dry: [f32; BLOCK],
    /// Each synth's raw signal this block, before its inserts: what a
    /// vocoder keyed to it follows (#161). Copied before any strip is
    /// processed, so any strip can key any other.
    raw: Box<[[f32; BLOCK]; SYNTHS]>,
    /// Where a strip routed nowhere is summed and forgotten, cleared each block.
    nowhere: Box<[[f32; BLOCK]; 2]>,
    /// Each group's stereo bus, left then right.
    groups: Box<[[[f32; BLOCK]; 2]; GROUPS]>,
    /// What voices send straight into a group, before its inserts: a drum
    /// kit's individual outs (#162). Cleared each block.
    direct: Box<[[[f32; BLOCK]; 2]; GROUPS]>,
    /// The groups each synth feeds directly, a bit per group, for the solos.
    feeds: [u8; SYNTHS],
    strips: [Strip; STRIPS],
    /// Each strip's gains as they ramp from one block's to the next (#271).
    gains: [Gains<{ 2 + SENDS }>; STRIPS],
    /// Each strip's input was silent last block: its gains then jump, as
    /// nothing is sounding that a step could click (a song's first values).
    quiet: [bool; STRIPS],
    /// Heard, given the solos: a strip is silent when something is soloed and
    /// neither it, nor a group it feeds, nor a strip feeding it is.
    heard: [bool; STRIPS],
    /// The summed post-fader sends, one buffer per processor.
    pub sends: [[f32; BLOCK]; SENDS],
    /// Each strip's post-fader peak in the last block; 0 when it is silenced.
    pub peaks: [f32; STRIPS],
}

impl Mixer {
    pub fn new(sample_rate: f32) -> Mixer {
        Mixer {
            inserts: (0..STRIPS)
                .map(|_| std::array::from_fn(|_| Insert::new(sample_rate)))
                .collect(),
            bus: Box::new([[0.0; BLOCK]; SYNTHS]),
            bus_r: Box::new([[0.0; BLOCK]; SYNTHS]),
            wide: [false; SYNTHS],
            dry: [0.0; BLOCK],
            raw: Box::new([[0.0; BLOCK]; SYNTHS]),
            nowhere: Box::new([[0.0; BLOCK]; 2]),
            groups: Box::new([[[0.0; BLOCK]; 2]; GROUPS]),
            direct: Box::new([[[0.0; BLOCK]; 2]; GROUPS]),
            feeds: [0; SYNTHS],
            strips: std::array::from_fn(|i| Strip::new(i >= SYNTHS)),
            gains: std::array::from_fn(|i| {
                Gains::new(Strip::new(i >= SYNTHS).targets(i >= SYNTHS))
            }),
            quiet: [true; STRIPS],
            heard: [true; STRIPS],
            sends: [[0.0; BLOCK]; SENDS],
            peaks: [0.0; STRIPS],
        }
    }

    /// Whether `strip` may go to `out`: the master, or a group; a group only to
    /// a higher-numbered one, so the routes can't loop.
    pub fn route_ok(strip: usize, out: usize) -> bool {
        match out {
            0 | OUT_NONE => true,
            1..=GROUPS => strip < SYNTHS || out - 1 > strip - SYNTHS,
            _ => false,
        }
    }

    /// Apply an already clamped strip parameter; other ids are ignored.
    /// Returns false when it was refused: a route that is not allowed.
    pub fn set(&mut self, strip: usize, param: Param, v: f32) -> bool {
        if param == Param::Out && !Mixer::route_ok(strip, v.round() as usize) {
            return false;
        }
        if let Some(s) = self.strips.get_mut(strip) {
            s.set(param, v);
        }
        if let Some((slot, field)) = param.insert() {
            if let Some(ins) = self.inserts.get_mut(strip).and_then(|i| i.get_mut(slot)) {
                match field {
                    InsertField::Type => {
                        if let Some(t) = InsertType::from_id(v.round() as u32) {
                            ins.set_type(t);
                        }
                    }
                    InsertField::Knob(k) => ins.set_knob(k, v),
                }
            }
        }
        // Only a solo or a route changes who is heard; automation sets the
        // others from `render`, and the solo pass is not for every block.
        if matches!(param, Param::Solo | Param::Out) {
            self.update_solo();
        }
        true
    }

    /// The groups `synth` feeds directly (a bit per group, bit 0 for group 1):
    /// its kit's individual outs. Solos follow them.
    pub fn set_feeds(&mut self, synth: usize, groups: u8) {
        if let Some(f) = self.feeds.get_mut(synth) {
            if *f != groups {
                *f = groups;
                self.update_solo();
            }
        }
    }

    /// The groups a strip feeds directly, as indices into the strips.
    fn fed(&self, strip: usize) -> impl Iterator<Item = usize> + '_ {
        let mask = self.feeds.get(strip).copied().unwrap_or(0);
        (0..GROUPS)
            .filter(move |g| mask & (1 << g) != 0)
            .map(|g| SYNTHS + g)
    }

    /// The groups a strip's signal passes through, nearest first.
    fn chain(&self, strip: usize) -> impl Iterator<Item = usize> + '_ {
        let mut out = self.strips.get(strip).map_or(0, |s| s.out);
        std::iter::from_fn(move || {
            if out == 0 || out > GROUPS {
                return None;
            }
            let g = SYNTHS + out - 1;
            out = self.strips.get(g).map_or(0, |s| s.out);
            Some(g)
        })
        .take(GROUPS)
    }

    /// Who is heard under the solos; fixed-size, so it never allocates (ADR-0002).
    fn update_solo(&mut self) {
        let soloed = |i: usize| self.strips.get(i).is_some_and(|s| s.solo);
        let mut heard = [!(0..STRIPS).any(soloed); STRIPS];
        let mut hear = |g: usize| {
            if let Some(h) = heard.get_mut(g) {
                *h = true;
            }
        };
        for j in (0..STRIPS).filter(|j| soloed(*j)) {
            hear(j);
            // A soloed strip is heard through every group it passes, and a
            // group it feeds stays heard: by its Out, or by its individual outs.
            self.chain(j).for_each(&mut hear);
            for d in self.fed(j) {
                hear(d);
                self.chain(d).for_each(&mut hear);
            }
        }
        // Whatever feeds a soloed group is heard too.
        for i in 0..STRIPS {
            if self
                .chain(i)
                .any(|g| self.strips.get(g).is_some_and(|s| s.solo))
            {
                if let Some(h) = heard.get_mut(i) {
                    *h = true;
                }
            }
        }
        self.heard = heard;
    }

    fn silenced(&self, strip: usize) -> bool {
        self.strips.get(strip).is_none_or(|s| s.mute)
            || !self.heard.get(strip).copied().unwrap_or(true)
    }

    /// The first `frames` samples of `synth`'s bus, to add voices into.
    pub fn bus(&mut self, synth: usize, range: std::ops::Range<usize>) -> Option<&mut [f32]> {
        self.bus.get_mut(synth)?.get_mut(range)
    }

    /// The left and right buses of `synth` for voices that write both sides (the
    /// drum/pad sampler's pans); the synth is stereo from here on, so its strip balances.
    pub fn stereo_bus(
        &mut self,
        synth: usize,
        range: std::ops::Range<usize>,
    ) -> Option<(&mut [f32], &mut [f32])> {
        *self.wide.get_mut(synth)? = true;
        let l = self.bus.get_mut(synth)?.get_mut(range.clone())?;
        let r = self.bus_r.get_mut(synth)?.get_mut(range)?;
        Some((l, r))
    }

    /// The left and right buses of `synth` and every group's direct input from `range.start`,
    /// for a pad sampler or a drum kit whose pads go to its strip or straight to a group
    /// (#162, #220, #364), panned either way; stereo as `stereo_bus` is.
    #[allow(clippy::type_complexity)]
    pub fn pad_outs(
        &mut self,
        synth: usize,
        range: std::ops::Range<usize>,
    ) -> Option<(&mut [f32], &mut [f32], &mut [[[f32; BLOCK]; 2]; GROUPS])> {
        *self.wide.get_mut(synth)? = true;
        let l = self.bus.get_mut(synth)?.get_mut(range.clone())?;
        let r = self.bus_r.get_mut(synth)?.get_mut(range)?;
        Some((l, r, &mut self.direct))
    }

    /// Whether `synth` is stereo this block.
    pub fn is_wide(&self, synth: usize) -> bool {
        self.wide.get(synth).copied().unwrap_or(false)
    }

    /// Make `synth` stereo for this block: `effect` reads its mono bus and writes
    /// the left and right sides, and the strip then balances them instead of panning.
    pub fn widen(
        &mut self,
        synth: usize,
        frames: usize,
        effect: impl FnOnce(&[f32], &mut [f32], &mut [f32]),
    ) {
        let n = frames.min(BLOCK);
        let (Some(l), Some(r), Some(wide)) = (
            self.bus.get_mut(synth),
            self.bus_r.get_mut(synth),
            self.wide.get_mut(synth),
        ) else {
            return;
        };
        let (Some(l), Some(r), Some(dry)) = (l.get_mut(..n), r.get_mut(..n), self.dry.get_mut(..n))
        else {
            return;
        };
        dry.copy_from_slice(l);
        effect(dry, l, r);
        *wide = true;
    }

    /// Silence the buses at the start of a block.
    pub fn clear(&mut self, frames: usize) {
        self.wide = [false; SYNTHS];
        for bus in self.bus.iter_mut().chain(self.bus_r.iter_mut()) {
            if let Some(b) = bus.get_mut(..frames) {
                b.fill(0.0);
            }
        }
        for side in self
            .direct
            .iter_mut()
            .flatten()
            .chain(self.nowhere.iter_mut())
        {
            if let Some(b) = side.get_mut(..frames) {
                b.fill(0.0);
            }
        }
    }

    /// Sum the strips and groups into `left` and `right`, and the sends into
    /// `sends`. The first `frames` of each are overwritten.
    pub fn mix(&mut self, frames: usize, left: &mut [f32], right: &mut [f32]) {
        let n = frames.min(BLOCK).min(left.len()).min(right.len());
        let (Some(left), Some(right)) = (left.get_mut(..n), right.get_mut(..n)) else {
            return;
        };
        left.fill(0.0);
        right.fill(0.0);
        self.peaks.fill(0.0);
        for send in self.sends.iter_mut() {
            if let Some(s) = send.get_mut(..n) {
                s.fill(0.0);
            }
        }
        // The keys: each synth's raw signal, a stereo one as its mid.
        for (i, raw) in self.raw.iter_mut().enumerate() {
            let (Some(r), Some(bus)) = (raw.get_mut(..n), self.bus.get(i).and_then(|b| b.get(..n)))
            else {
                continue;
            };
            r.copy_from_slice(bus);
            if self.wide.get(i).copied().unwrap_or(false) {
                if let Some(right) = self.bus_r.get(i).and_then(|b| b.get(..n)) {
                    for (x, y) in r.iter_mut().zip(right) {
                        *x = 0.5 * (*x + y);
                    }
                }
            }
        }
        // A group starts from what was sent to it directly, else silence.
        for (g, d) in self.groups.iter_mut().zip(self.direct.iter()) {
            for (side, from) in g.iter_mut().zip(d.iter()) {
                if let (Some(s), Some(f)) = (side.get_mut(..n), from.get(..n)) {
                    s.copy_from_slice(f);
                }
            }
        }
        for i in 0..SYNTHS {
            if self.silenced(i) {
                continue;
            }
            let (Some(s), Some(bus), Some(inserts)) = (
                self.strips.get(i).copied(),
                self.bus.get_mut(i),
                self.inserts.get_mut(i),
            ) else {
                continue;
            };
            let Some(bus) = bus.get_mut(..n) else {
                continue;
            };
            let mut wide_r = if self.wide.get(i).copied().unwrap_or(false) {
                self.bus_r.get_mut(i).and_then(|r| r.get_mut(..n))
            } else {
                None
            };
            let key = key_of(&self.raw, i, s.key, n);
            for insert in inserts.iter_mut() {
                match wide_r.as_deref_mut() {
                    Some(r) => insert.process_stereo(bus, r, key),
                    None => insert.process_mono(bus, key),
                }
            }
            if let Some(p) = self.peaks.get_mut(i) {
                let mut peak = bus.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
                if let Some(r) = wide_r.as_deref() {
                    peak = r.iter().fold(peak, |m, x| m.max(x.abs()));
                }
                *p = peak * s.level;
            }
            let was_quiet = self.quiet.get(i).copied().unwrap_or(true);
            if let Some(q) = self.quiet.get_mut(i) {
                *q = bus
                    .iter()
                    .chain(wide_r.iter().flat_map(|r| r.iter()))
                    .all(|x| *x == 0.0);
            }
            // Into the master, a group's stereo bus, or nowhere: a strip routed
            // nowhere still feeds its sends below.
            let (dl, dr): (&mut [f32], &mut [f32]) = match s.out {
                0 => (&mut *left, &mut *right),
                o if o <= GROUPS => {
                    let Some([gl, gr]) = self.groups.get_mut(o - 1) else {
                        continue;
                    };
                    let (Some(gl), Some(gr)) = (gl.get_mut(..n), gr.get_mut(..n)) else {
                        continue;
                    };
                    (gl, gr)
                }
                _ => {
                    let [nl, nr] = &mut *self.nowhere;
                    let (Some(nl), Some(nr)) = (nl.get_mut(..n), nr.get_mut(..n)) else {
                        continue;
                    };
                    (nl, nr)
                }
            };
            let Some(gains) = self.gains.get_mut(i) else {
                continue;
            };
            let right = wide_r.as_deref();
            let to = s.targets(right.is_some());
            if was_quiet {
                gains.jump(to);
            }
            send_out(gains, to, bus, right, dl, dr, &mut self.sends);
        }
        // Groups in ascending order: one that feeds another is done first.
        for gi in 0..GROUPS {
            let idx = SYNTHS + gi;
            if self.silenced(idx) {
                continue;
            }
            let Some(s) = self.strips.get(idx).copied() else {
                continue;
            };
            let (lo, hi) = self.groups.split_at_mut(gi + 1);
            let Some([gl, gr]) = lo.get_mut(gi) else {
                continue;
            };
            let (Some(gl), Some(gr)) = (gl.get_mut(..n), gr.get_mut(..n)) else {
                continue;
            };
            if let Some(inserts) = self.inserts.get_mut(idx) {
                let key = key_of(&self.raw, idx, s.key, n);
                for insert in inserts.iter_mut() {
                    insert.process_stereo(gl, gr, key);
                }
            }
            if let Some(p) = self.peaks.get_mut(idx) {
                let peak = gl
                    .iter()
                    .chain(gr.iter())
                    .fold(0.0_f32, |m, x| m.max(x.abs()));
                *p = peak * s.level;
            }
            let was_quiet = self.quiet.get(idx).copied().unwrap_or(true);
            if let Some(q) = self.quiet.get_mut(idx) {
                *q = gl.iter().chain(gr.iter()).all(|x| *x == 0.0);
            }
            let (dl, dr): (&mut [f32], &mut [f32]) = match s.out {
                0 => (&mut *left, &mut *right),
                o if o > GROUPS => {
                    let [nl, nr] = &mut *self.nowhere;
                    let (Some(nl), Some(nr)) = (nl.get_mut(..n), nr.get_mut(..n)) else {
                        continue;
                    };
                    (nl, nr)
                }
                o => {
                    // `route_ok` keeps the destination above this group.
                    let Some([ol, or]) = o.checked_sub(gi + 2).and_then(|k| hi.get_mut(k)) else {
                        continue;
                    };
                    let (Some(ol), Some(or)) = (ol.get_mut(..n), or.get_mut(..n)) else {
                        continue;
                    };
                    (ol, or)
                }
            };
            let Some(gains) = self.gains.get_mut(idx) else {
                continue;
            };
            let to = s.targets(true);
            if was_quiet {
                gains.jump(to);
            }
            send_out(gains, to, gl, Some(gr), dl, dr, &mut self.sends);
        }
    }
}

/// Add a strip's signal, mono (`right` none) or stereo, into its out and its
/// sends at gains that ramp to `to` across the block (#271); a send at 0
/// before and after is skipped.
fn send_out(
    gains: &mut Gains<{ 2 + SENDS }>,
    to: [f32; 2 + SENDS],
    left: &[f32],
    right: Option<&[f32]>,
    dl: &mut [f32],
    dr: &mut [f32],
    sends: &mut [[f32; BLOCK]; SENDS],
) {
    let n = left.len().min(dl.len()).min(dr.len());
    let from = gains.now();
    let moving = gains.aim(to, n);
    let mut live = [false; SENDS];
    for (k, l) in live.iter_mut().enumerate() {
        let at = |g: &[f32; 2 + SENDS]| g.get(2 + k).copied().unwrap_or(0.0);
        *l = at(&from) != 0.0 || at(&to) != 0.0;
    }
    for j in 0..n {
        let g = if moving { gains.tick() } else { to };
        let x = left.get(j).copied().unwrap_or(0.0);
        let y = right.and_then(|r| r.get(j)).copied().unwrap_or(x);
        if let (Some(l), Some(r)) = (dl.get_mut(j), dr.get_mut(j)) {
            *l += x * g[0];
            *r += y * g[1];
        }
        let mid = if right.is_some() { x + y } else { x };
        for (k, send) in sends.iter_mut().enumerate() {
            if live.get(k).copied().unwrap_or(false) {
                if let (Some(acc), Some(gain)) = (send.get_mut(j), g.get(2 + k)) {
                    *acc += mid * gain;
                }
            }
        }
    }
    gains.settle();
}

/// The raw signal a strip's vocoder follows: synth `key` (1–16), never the
/// strip itself; `None` when it has no key.
fn key_of(raw: &[[f32; BLOCK]; SYNTHS], strip: usize, key: usize, n: usize) -> Option<&[f32]> {
    let synth = key.checked_sub(1)?;
    if synth == strip {
        return None;
    }
    raw.get(synth)?.get(..n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The largest jump between neighbouring samples, across blocks.
    fn largest_step(x: &[f32]) -> f32 {
        x.windows(2).fold(0.0, |m, w| m.max((w[1] - w[0]).abs()))
    }

    /// #271: a fader, pan and send written every block, as a `mod` line or a
    /// lane writes them, ramp across the block instead of stepping at its edge.
    #[test]
    fn a_fader_pan_and_send_moved_every_block_do_not_step() {
        let mut m = Mixer::new(48_000.0);
        let (mut left, mut right, mut send) = (Vec::new(), Vec::new(), Vec::new());
        for block in 0..16 {
            let up = block % 2 == 0;
            m.set(0, Param::Level, if up { 1.0 } else { 0.2 });
            m.set(0, Param::Pan, if up { -1.0 } else { 1.0 });
            m.set(0, Param::Send1, if up { 1.0 } else { 0.0 });
            // A steady input: any step in a gain shows up whole.
            m.bus(0, 0..BLOCK).expect("bus").fill(0.5);
            let (mut l, mut r) = ([0.0; BLOCK], [0.0; BLOCK]);
            m.mix(BLOCK, &mut l, &mut r);
            left.extend_from_slice(&l);
            right.extend_from_slice(&r);
            send.extend_from_slice(&m.sends[0]);
        }
        // Stepped, a side jumps by half the input at a block's edge; ramped,
        // by that over a block.
        let ramp = 0.5 / BLOCK as f32 * 1.01;
        for (side, x) in [("left", &left), ("right", &right), ("send", &send)] {
            let step = largest_step(&x[BLOCK..]);
            assert!(
                step <= ramp,
                "{side}: a step of {step}, a ramp moves {ramp}"
            );
        }
        assert!(
            left[BLOCK - 1] > 0.49 && right[2 * BLOCK - 1] > 0.09,
            "each block ends on its gains"
        );
    }

    /// #225: automation sets strip parameters from `render`; only a solo or a
    /// route reworks who is heard, so a fader move leaves it alone.
    #[test]
    fn only_solo_and_out_rework_who_is_heard() {
        let mut m = Mixer::new(48_000.0);
        assert!(m.set(1, Param::Solo, 1.0));
        assert!(m.silenced(0) && !m.silenced(1), "solo hears only itself");
        let before = m.heard;
        for p in [Param::Level, Param::Pan, Param::Send1, Param::Mute] {
            m.set(0, p, 0.5);
            m.set(1, p, 0.0);
        }
        assert_eq!(
            m.heard, before,
            "faders, pans, sends and mutes keep the solo"
        );
        let group = SYNTHS;
        assert!(m.set(1, Param::Out, 1.0));
        assert!(!m.silenced(group), "the soloed strip's group is heard");
        assert!(m.set(1, Param::Solo, 0.0));
        assert_eq!(m.heard, [true; STRIPS], "no solo hears everyone");
    }
}
