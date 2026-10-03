//! The mixer (ADR-0005, ADR-0010, spec 002 Req 2): one strip per synth and
//! eight group buses, each with insert slots, a fader, a pan and four
//! post-fader sends (P1–P4), routed by `Out` to the stereo master or to a
//! higher-numbered group. It owns every strip parameter (`Param::is_strip`).
//! Everything is allocated in `Mixer::new` (ADR-0002); gains and the solo
//! paths are worked out when a parameter changes, never per sample.

use crate::engine::{BLOCK, SYNTHS};
use crate::fx::insert::{Insert, InsertType};
use crate::params::{INSERT_SLOTS, InsertField, Param};

/// The processors the sends feed (P1–P4).
pub const SENDS: usize = 4;
/// Group buses, strips 16–23.
pub const GROUPS: usize = 8;
/// Every strip: the synths, then the groups.
pub const STRIPS: usize = SYNTHS + GROUPS;

/// Where a strip starts: full fader, centred, no sends, not muted or soloed.
pub const STRIP_DEFAULTS: [(Param, f32); 27] = [
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
];

/// One strip's or group's fader, pan, sends and routing, as set from the view.
#[derive(Clone, Copy)]
struct Strip {
    level: f32,
    /// Left and right gains: an equal-power pan for a synth, a balance for a group.
    pan: [f32; 2],
    send: [f32; SENDS],
    mute: bool,
    solo: bool,
    /// 0 is the master, 1–8 a group.
    out: usize,
    group: bool,
}

impl Strip {
    fn new(group: bool) -> Strip {
        let mut s = Strip {
            level: 0.0,
            pan: [0.0; 2],
            send: [0.0; SENDS],
            mute: false,
            solo: false,
            out: 0,
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
            Param::Mute => self.mute = v >= 0.5,
            Param::Solo => self.solo = v >= 0.5,
            Param::Out => self.out = v.round() as usize,
            _ => {}
        }
    }
}

pub struct Mixer {
    /// Each strip's insert slots, between its voices and its fader.
    inserts: Vec<[Insert; INSERT_SLOTS]>,
    /// Each synth's mono bus, filled by its voices.
    bus: Box<[[f32; BLOCK]; SYNTHS]>,
    /// Each group's stereo bus, left then right.
    groups: Box<[[[f32; BLOCK]; 2]; GROUPS]>,
    strips: [Strip; STRIPS],
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
            groups: Box::new([[[0.0; BLOCK]; 2]; GROUPS]),
            strips: std::array::from_fn(|i| Strip::new(i >= SYNTHS)),
            heard: [true; STRIPS],
            sends: [[0.0; BLOCK]; SENDS],
            peaks: [0.0; STRIPS],
        }
    }

    /// Whether `strip` may go to `out`: the master, or a group; a group only to
    /// a higher-numbered one, so the routes can't loop.
    pub fn route_ok(strip: usize, out: usize) -> bool {
        match out {
            0 => true,
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
        self.update_solo();
        true
    }

    /// The groups a strip's signal passes through, nearest first.
    fn chain(&self, strip: usize) -> impl Iterator<Item = usize> + '_ {
        let mut out = self.strips.get(strip).map_or(0, |s| s.out);
        std::iter::from_fn(move || {
            if out == 0 {
                return None;
            }
            let g = SYNTHS + out - 1;
            out = self.strips.get(g).map_or(0, |s| s.out);
            Some(g)
        })
        .take(GROUPS)
    }

    fn update_solo(&mut self) {
        let soloed: Vec<usize> = (0..STRIPS)
            .filter(|i| self.strips.get(*i).is_some_and(|s| s.solo))
            .collect();
        let mut heard = [soloed.is_empty(); STRIPS];
        for &j in &soloed {
            if let Some(h) = heard.get_mut(j) {
                *h = true;
            }
            // A soloed strip is heard through every group it passes, and a
            // group it feeds stays heard.
            for g in self.chain(j) {
                if let Some(h) = heard.get_mut(g) {
                    *h = true;
                }
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

    /// Silence the buses at the start of a block.
    pub fn clear(&mut self, frames: usize) {
        for bus in self.bus.iter_mut() {
            if let Some(b) = bus.get_mut(..frames) {
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
        for g in self.groups.iter_mut() {
            for side in g.iter_mut() {
                if let Some(s) = side.get_mut(..n) {
                    s.fill(0.0);
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
            for insert in inserts.iter_mut() {
                insert.process_mono(bus);
            }
            if let Some(p) = self.peaks.get_mut(i) {
                *p = bus.iter().fold(0.0_f32, |m, x| m.max(x.abs())) * s.level;
            }
            // Into the master, or a group's stereo bus.
            let (dl, dr): (&mut [f32], &mut [f32]) = match s.out {
                0 => (&mut *left, &mut *right),
                o => {
                    let Some([gl, gr]) = self.groups.get_mut(o - 1) else {
                        continue;
                    };
                    let (Some(gl), Some(gr)) = (gl.get_mut(..n), gr.get_mut(..n)) else {
                        continue;
                    };
                    (gl, gr)
                }
            };
            let [pl, pr] = s.pan;
            for ((x, l), r) in bus.iter().zip(dl.iter_mut()).zip(dr.iter_mut()) {
                *l += x * s.level * pl;
                *r += x * s.level * pr;
            }
            for (send, amount) in self.sends.iter_mut().zip(s.send) {
                let gain = s.level * amount;
                if gain == 0.0 {
                    continue;
                }
                for (acc, x) in send.iter_mut().zip(bus.iter()) {
                    *acc += x * gain;
                }
            }
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
                for insert in inserts.iter_mut() {
                    insert.process_stereo(gl, gr);
                }
            }
            if let Some(p) = self.peaks.get_mut(idx) {
                let peak = gl
                    .iter()
                    .chain(gr.iter())
                    .fold(0.0_f32, |m, x| m.max(x.abs()));
                *p = peak * s.level;
            }
            let (dl, dr): (&mut [f32], &mut [f32]) = match s.out {
                0 => (&mut *left, &mut *right),
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
            let [bl, br] = s.pan;
            for (((l, r), ol), or) in gl
                .iter()
                .zip(gr.iter())
                .zip(dl.iter_mut())
                .zip(dr.iter_mut())
            {
                *ol += l * s.level * bl;
                *or += r * s.level * br;
            }
            for (send, amount) in self.sends.iter_mut().zip(s.send) {
                let gain = 0.5 * s.level * amount;
                if gain == 0.0 {
                    continue;
                }
                for ((acc, l), r) in send.iter_mut().zip(gl.iter()).zip(gr.iter()) {
                    *acc += (l + r) * gain;
                }
            }
        }
    }
}
