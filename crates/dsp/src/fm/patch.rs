//! A DX7 voice as the engine holds it (spec 006 Req 13): six operators and the
//! global settings, in the order of the DX7's unpacked 156-byte voice, so a SysEx
//! voice maps onto it without a table. Operator 0 is the DX7's operator 6, as the
//! packed voice lists them.

/// One operator's settings, each 0..=99 unless noted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpPatch {
    /// The envelope's four rates and four levels.
    pub rates: [u8; 4],
    pub levels: [u8; 4],
    /// Level scaling: the break point (a note, 0 is A-1), the depth either side,
    /// and the curve either side (0..=3: −lin, −exp, +exp, +lin).
    pub break_point: u8,
    pub left_depth: u8,
    pub right_depth: u8,
    pub left_curve: u8,
    pub right_curve: u8,
    /// Rate scaling, 0..=7.
    pub rate_scale: u8,
    /// Amplitude modulation sensitivity, 0..=3 (not applied, as in the reference).
    pub amp_sens: u8,
    /// Velocity sensitivity, 0..=7.
    pub vel_sens: u8,
    pub level: u8,
    /// 0 is a ratio of the note's frequency, 1 a fixed frequency.
    pub mode: u8,
    /// Coarse, 0..=31; fine, 0..=99; detune, 0..=14 with 7 at the centre.
    pub coarse: u8,
    pub fine: u8,
    pub detune: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FmPatch {
    pub ops: [OpPatch; 6],
    pub pitch_rates: [u8; 4],
    pub pitch_levels: [u8; 4],
    /// 0..=31 (algorithm 1 is 0).
    pub algorithm: u8,
    /// Operator 6's feedback, 0..=7.
    pub feedback: u8,
    pub osc_sync: bool,
    pub lfo_speed: u8,
    pub lfo_delay: u8,
    pub lfo_pitch_depth: u8,
    pub lfo_amp_depth: u8,
    pub lfo_sync: bool,
    /// Triangle, saw down, saw up, square, sine, sample and hold.
    pub lfo_shape: u8,
    pub pitch_sens: u8,
    /// 0..=48; 24 is no transposition.
    pub transpose: u8,
}

impl FmPatch {
    /// Set field `k` (in the unpacked voice's order: four rates, four levels, the
    /// scaling, output level and frequency) of operator `op` from a clamped value.
    pub fn set_op(&mut self, op: usize, k: usize, v: f32) {
        let Some(o) = self.ops.get_mut(op) else {
            return;
        };
        let x = v.round().clamp(0.0, 99.0) as u8;
        match k {
            0..=3 => {
                if let Some(r) = o.rates.get_mut(k) {
                    *r = x;
                }
            }
            4..=7 => {
                if let Some(l) = o.levels.get_mut(k - 4) {
                    *l = x;
                }
            }
            8 => o.break_point = x,
            9 => o.left_depth = x,
            10 => o.right_depth = x,
            11 => o.left_curve = x.min(3),
            12 => o.right_curve = x.min(3),
            13 => o.rate_scale = x.min(7),
            14 => o.amp_sens = x.min(3),
            15 => o.vel_sens = x.min(7),
            16 => o.level = x,
            17 => o.mode = x.min(1),
            18 => o.coarse = x.min(31),
            19 => o.fine = x,
            20 => o.detune = x.min(14),
            _ => {}
        }
    }

    /// Set global field `k`: the pitch envelope's rates and levels, then the
    /// algorithm, feedback, sync, the LFO and the transposition.
    pub fn set_global(&mut self, k: usize, v: f32) {
        let x = v.round().clamp(0.0, 99.0) as u8;
        match k {
            0..=3 => {
                if let Some(r) = self.pitch_rates.get_mut(k) {
                    *r = x;
                }
            }
            4..=7 => {
                if let Some(l) = self.pitch_levels.get_mut(k - 4) {
                    *l = x;
                }
            }
            8 => self.algorithm = x.min(31),
            9 => self.feedback = x.min(7),
            10 => self.osc_sync = x >= 1,
            11 => self.lfo_speed = x,
            12 => self.lfo_delay = x,
            13 => self.lfo_pitch_depth = x,
            14 => self.lfo_amp_depth = x,
            15 => self.lfo_sync = x >= 1,
            16 => self.lfo_shape = x.min(5),
            17 => self.pitch_sens = x.min(7),
            18 => self.transpose = x.min(48),
            _ => {}
        }
    }

    /// Field `k` of operator `op`, as `set_op` takes it.
    pub fn op_field(&self, op: usize, k: usize) -> u8 {
        let Some(o) = self.ops.get(op) else {
            return 0;
        };
        match k {
            0..=3 => o.rates.get(k).copied().unwrap_or(0),
            4..=7 => o.levels.get(k - 4).copied().unwrap_or(0),
            8 => o.break_point,
            9 => o.left_depth,
            10 => o.right_depth,
            11 => o.left_curve,
            12 => o.right_curve,
            13 => o.rate_scale,
            14 => o.amp_sens,
            15 => o.vel_sens,
            16 => o.level,
            17 => o.mode,
            18 => o.coarse,
            19 => o.fine,
            20 => o.detune,
            _ => 0,
        }
    }

    /// Global field `k`, as `set_global` takes it.
    pub fn global_field(&self, k: usize) -> u8 {
        match k {
            0..=3 => self.pitch_rates.get(k).copied().unwrap_or(0),
            4..=7 => self.pitch_levels.get(k - 4).copied().unwrap_or(0),
            8 => self.algorithm,
            9 => self.feedback,
            10 => u8::from(self.osc_sync),
            11 => self.lfo_speed,
            12 => self.lfo_delay,
            13 => self.lfo_pitch_depth,
            14 => self.lfo_amp_depth,
            15 => u8::from(self.lfo_sync),
            16 => self.lfo_shape,
            17 => self.pitch_sens,
            18 => self.transpose,
            _ => 0,
        }
    }
}
