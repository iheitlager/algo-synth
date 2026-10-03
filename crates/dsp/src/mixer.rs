//! The mixer (ADR-0005, spec 002 Req 2): one bus per synth, each with a
//! fader, an equal-power pan and two post-fader sends, summed into a stereo
//! master. Everything is allocated in `Mixer::new` (ADR-0002); gains are
//! worked out when a parameter changes, never per sample.

use crate::engine::{BLOCK, SYNTHS};
use crate::params::Param;

/// Where a strip starts: full fader, centred, no sends, not muted or soloed.
pub const STRIP_DEFAULTS: [(Param, f32); 6] = [
    (Param::Level, 1.0),
    (Param::Pan, 0.0),
    (Param::EchoSend, 0.0),
    (Param::ReverbSend, 0.0),
    (Param::Mute, 0.0),
    (Param::Solo, 0.0),
];

/// One synth's fader, pan and sends, as set from the view.
#[derive(Clone, Copy)]
struct Strip {
    level: f32,
    /// Left and right gains of the pan law, for pan −1..=1.
    pan: [f32; 2],
    echo: f32,
    reverb: f32,
    mute: bool,
    solo: bool,
}

impl Strip {
    fn new() -> Strip {
        let mut s = Strip {
            level: 0.0,
            pan: [0.0; 2],
            echo: 0.0,
            reverb: 0.0,
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
            Param::EchoSend => self.echo = v,
            Param::ReverbSend => self.reverb = v,
            Param::Mute => self.mute = v >= 0.5,
            Param::Solo => self.solo = v >= 0.5,
            _ => {}
        }
    }
}

pub struct Mixer {
    /// Each synth's mono bus, filled by its voices.
    bus: Box<[[f32; BLOCK]; SYNTHS]>,
    strips: [Strip; SYNTHS],
    any_solo: bool,
    /// The summed post-fader sends, for the echo and the reverb.
    pub echo_send: [f32; BLOCK],
    pub reverb_send: [f32; BLOCK],
}

impl Default for Mixer {
    fn default() -> Mixer {
        Mixer::new()
    }
}

impl Mixer {
    pub fn new() -> Mixer {
        Mixer {
            bus: Box::new([[0.0; BLOCK]; SYNTHS]),
            strips: [Strip::new(); SYNTHS],
            any_solo: false,
            echo_send: [0.0; BLOCK],
            reverb_send: [0.0; BLOCK],
        }
    }

    /// Apply an already clamped strip parameter; other ids are ignored.
    pub fn set(&mut self, synth: usize, param: Param, v: f32) {
        if let Some(s) = self.strips.get_mut(synth) {
            s.set(param, v);
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
    /// sends into `echo_send` and `reverb_send`. The first `frames` of each
    /// are overwritten.
    pub fn mix(&mut self, frames: usize, left: &mut [f32], right: &mut [f32]) {
        let n = frames.min(BLOCK).min(left.len()).min(right.len());
        let (Some(left), Some(right)) = (left.get_mut(..n), right.get_mut(..n)) else {
            return;
        };
        let (Some(echo), Some(reverb)) =
            (self.echo_send.get_mut(..n), self.reverb_send.get_mut(..n))
        else {
            return;
        };
        for out in [&mut *left, &mut *right, &mut *echo, &mut *reverb] {
            out.fill(0.0);
        }
        for (bus, s) in self.bus.iter().zip(self.strips.iter()) {
            if s.mute || (self.any_solo && !s.solo) {
                continue;
            }
            let [gl, gr] = s.pan;
            let (ge, gv) = (s.level * s.echo, s.level * s.reverb);
            let frames = bus
                .iter()
                .zip(left.iter_mut().zip(right.iter_mut()))
                .zip(echo.iter_mut().zip(reverb.iter_mut()));
            for ((x, (l, r)), (e, v)) in frames {
                *l += x * s.level * gl;
                *r += x * s.level * gr;
                *e += x * ge;
                *v += x * gv;
            }
        }
    }
}
