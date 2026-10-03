//! The mixer (ADR-0005, spec 002 Req 2): one strip per synth, each with a
//! drive insert, a fader, an equal-power pan and four post-fader sends
//! (P1–P4), summed into a stereo master. It owns every strip parameter
//! (`Param::is_strip`). Everything is allocated in `Mixer::new` (ADR-0002); gains are
//! worked out when a parameter changes, never per sample.

use crate::engine::{BLOCK, SYNTHS};
use crate::fx::insert::{Insert, InsertType};
use crate::params::{INSERT_SLOTS, InsertField, Param};

/// The processors the sends feed (P1–P4).
pub const SENDS: usize = 4;

/// Where a strip starts: full fader, centred, no sends, not muted or soloed.
pub const STRIP_DEFAULTS: [(Param, f32); 26] = [
    (Param::Level, 1.0),
    (Param::Pan, 0.0),
    (Param::Send1, 0.0),
    (Param::Send2, 0.0),
    (Param::Send3, 0.0),
    (Param::Send4, 0.0),
    (Param::Mute, 0.0),
    (Param::Solo, 0.0),
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

/// One synth's fader, pan and sends, as set from the view.
#[derive(Clone, Copy)]
struct Strip {
    level: f32,
    /// Left and right gains of the pan law, for pan −1..=1.
    pan: [f32; 2],
    send: [f32; SENDS],
    mute: bool,
    solo: bool,
}

impl Strip {
    fn new() -> Strip {
        let mut s = Strip {
            level: 0.0,
            pan: [0.0; 2],
            send: [0.0; SENDS],
            mute: false,
            solo: false,
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
                // Equal power: −1 is all left, +1 all right, 0 is −3 dB each.
                let angle = (v + 1.0) * std::f32::consts::FRAC_PI_4;
                self.pan = match v {
                    v if v <= -1.0 => [1.0, 0.0],
                    v if v >= 1.0 => [0.0, 1.0],
                    _ => [angle.cos(), angle.sin()],
                };
            }
            Param::Send1 => self.send[0] = v,
            Param::Send2 => self.send[1] = v,
            Param::Send3 => self.send[2] = v,
            Param::Send4 => self.send[3] = v,
            Param::Mute => self.mute = v >= 0.5,
            Param::Solo => self.solo = v >= 0.5,
            _ => {}
        }
    }
}

pub struct Mixer {
    /// Each strip's insert slots, between its voices and its fader.
    inserts: Vec<[Insert; INSERT_SLOTS]>,
    /// Each synth's mono bus, filled by its voices.
    bus: Box<[[f32; BLOCK]; SYNTHS]>,
    strips: [Strip; SYNTHS],
    any_solo: bool,
    /// The summed post-fader sends, one buffer per processor.
    pub sends: [[f32; BLOCK]; SENDS],
    /// Each strip's post-fader peak in the last block; 0 when it is silenced.
    pub peaks: [f32; SYNTHS],
}

impl Mixer {
    pub fn new(sample_rate: f32) -> Mixer {
        Mixer {
            inserts: (0..SYNTHS)
                .map(|_| std::array::from_fn(|_| Insert::new(sample_rate)))
                .collect(),
            bus: Box::new([[0.0; BLOCK]; SYNTHS]),
            strips: [Strip::new(); SYNTHS],
            any_solo: false,
            sends: [[0.0; BLOCK]; SENDS],
            peaks: [0.0; SYNTHS],
        }
    }

    /// Apply an already clamped strip parameter; other ids are ignored.
    pub fn set(&mut self, synth: usize, param: Param, v: f32) {
        if let Some(s) = self.strips.get_mut(synth) {
            s.set(param, v);
        }
        if let Some((slot, field)) = param.insert() {
            if let Some(ins) = self.inserts.get_mut(synth).and_then(|i| i.get_mut(slot)) {
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
        self.any_solo = self.strips.iter().any(|s| s.solo);
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

    /// Sum the buses through their strips into `left` and `right`, and the
    /// sends into `sends`. The first `frames` of each are overwritten.
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
        for (((bus, s), inserts), peak) in self
            .bus
            .iter_mut()
            .zip(self.strips.iter())
            .zip(self.inserts.iter_mut())
            .zip(self.peaks.iter_mut())
        {
            if s.mute || (self.any_solo && !s.solo) {
                continue;
            }
            let Some(bus) = bus.get_mut(..n) else {
                continue;
            };
            for insert in inserts.iter_mut() {
                insert.process_mono(bus);
            }
            *peak = bus.iter().fold(0.0_f32, |m, x| m.max(x.abs())) * s.level;
            let [gl, gr] = s.pan;
            for ((x, l), r) in bus.iter().zip(left.iter_mut()).zip(right.iter_mut()) {
                *l += x * s.level * gl;
                *r += x * s.level * gr;
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
    }
}
