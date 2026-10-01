//! ADSR and AR envelopes for the Mono voice (spec 004 Req 4).
//!
//! Each segment is a one-pole curve aimed a little past its end, as an RC
//! envelope is, and takes exactly its set time from wherever it starts: an
//! attack from the middle of a release continues from that level. The
//! coefficients are computed when the gate changes (ADR-0002); a sample
//! costs one multiply-add and a compare. The AR is an ADSR with sustain 1.
//!
//! State is f64: over a 10 s segment one step moves the level by less than
//! an f32 ulp, and an f32 coefficient alone would be 45 ms off.

/// The attack aims this far past 1, for an analog-style concave rise.
const ATTACK_OVERSHOOT: f64 = 0.3;
/// Decay and release aim this far past their end: an exponential fall
/// that still arrives on time.
const FALL_OVERSHOOT: f64 = 1.0e-3;

/// Segment times in samples and the sustain level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvTimes {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy, Default)]
pub struct Env {
    pub stage: Stage,
    level: f64,
    /// The current segment: the curve's target, its multiplier, its end.
    target: f64,
    coef: f64,
    end: f64,
    /// The decay segment, worked out when the gate opens.
    decay_target: f64,
    decay_coef: f64,
    sustain: f64,
}

impl Env {
    pub fn level(&self) -> f32 {
        self.level as f32
    }

    /// True between gate on and gate off.
    pub fn gated(&self) -> bool {
        matches!(self.stage, Stage::Attack | Stage::Decay | Stage::Sustain)
    }

    /// Open the gate: attack from the current level, then decay.
    pub fn gate_on(&mut self, t: &EnvTimes) {
        self.sustain = f64::from(t.sustain.clamp(0.0, 1.0));
        self.decay_target = self.sustain - FALL_OVERSHOOT;
        self.decay_coef = coef(1.0, self.sustain, self.decay_target, t.decay);
        if self.level >= 1.0 {
            self.level = 1.0;
            self.enter_decay();
        } else {
            let target = 1.0 + ATTACK_OVERSHOOT;
            self.set_segment(
                Stage::Attack,
                target,
                coef(self.level, 1.0, target, t.attack),
                1.0,
            );
        }
    }

    /// Follow a new sustain level while gated: in Sustain the level moves
    /// there, in Decay the segment aims there instead.
    pub fn set_sustain(&mut self, sustain: f32) {
        let s = f64::from(sustain.clamp(0.0, 1.0));
        if s == self.sustain || !self.gated() {
            return;
        }
        self.sustain = s;
        self.decay_target = s - FALL_OVERSHOOT;
        if self.stage == Stage::Decay {
            self.target = self.decay_target;
            self.end = s;
        }
    }

    /// Close the gate: release from the current level.
    pub fn gate_off(&mut self, t: &EnvTimes) {
        if self.stage == Stage::Idle {
            return;
        }
        if self.level <= 0.0 {
            self.stage = Stage::Idle;
            self.level = 0.0;
            return;
        }
        let target = -FALL_OVERSHOOT;
        self.set_segment(
            Stage::Release,
            target,
            coef(self.level, 0.0, target, t.release),
            0.0,
        );
    }

    /// Advance one sample and return the level.
    pub fn step(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Sustain => self.level = self.sustain,
            Stage::Attack | Stage::Decay | Stage::Release => {
                self.level = self.target + (self.level - self.target) * self.coef;
                let arrived = if self.stage == Stage::Attack {
                    self.level >= self.end
                } else {
                    self.level <= self.end
                };
                if arrived || !self.level.is_finite() {
                    self.level = self.end;
                    match self.stage {
                        Stage::Attack => self.enter_decay(),
                        Stage::Decay => self.stage = Stage::Sustain,
                        _ => self.stage = Stage::Idle,
                    }
                }
            }
        }
        self.level as f32
    }

    fn enter_decay(&mut self) {
        if self.sustain >= 1.0 {
            self.stage = Stage::Sustain;
        } else {
            self.set_segment(
                Stage::Decay,
                self.decay_target,
                self.decay_coef,
                self.sustain,
            );
        }
    }

    fn set_segment(&mut self, stage: Stage, target: f64, coef: f64, end: f64) {
        self.stage = stage;
        self.target = target;
        self.coef = coef;
        self.end = end;
    }
}

/// The multiplier that takes a one-pole aimed at `target` from `from` to
/// `to` in `samples`. Per gate change, so `ln` and `exp` are fine.
fn coef(from: f64, to: f64, target: f64, samples: f32) -> f64 {
    let ratio = (to - target) / (from - target);
    if !(ratio > 0.0 && ratio < 1.0) {
        return 0.0;
    }
    (ratio.ln() / f64::from(samples.max(1.0))).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn times(a: f32, d: f32, s: f32, r: f32) -> EnvTimes {
        EnvTimes {
            attack: a * SR,
            decay: d * SR,
            sustain: s,
            release: r * SR,
        }
    }

    /// Samples until the envelope leaves `stage`.
    fn time_in(env: &mut Env, stage: Stage) -> usize {
        let mut n = 0;
        while env.stage == stage && n < 20 * SR as usize {
            env.step();
            n += 1;
        }
        n
    }

    #[test]
    fn segment_times() {
        let ms = SR as usize / 1000;
        for (a, d, r) in [(0.001, 0.001, 0.001), (0.01, 0.2, 0.5), (2.0, 10.0, 10.0)] {
            let t = times(a, d, 0.4, r);
            let mut env = Env::default();
            env.gate_on(&t);
            let attack = time_in(&mut env, Stage::Attack);
            let decay = time_in(&mut env, Stage::Decay);
            assert_eq!(env.stage, Stage::Sustain);
            assert!((env.level() - 0.4).abs() < 1.0e-6);
            for _ in 0..100 {
                env.step();
            }
            env.gate_off(&t);
            let release = time_in(&mut env, Stage::Release);
            assert_eq!((env.stage, env.level()), (Stage::Idle, 0.0));
            for (got, want) in [(attack, a), (decay, d), (release, r)] {
                let want = (want * SR) as usize;
                assert!(got.abs_diff(want) <= ms, "{got} samples, wanted {want}");
            }
        }
    }

    #[test]
    fn retrigger_does_not_jump() {
        let t = times(0.05, 0.1, 0.6, 0.5);
        let mut env = Env::default();
        env.gate_on(&t);
        for _ in 0..(0.2 * SR) as usize {
            env.step();
        }
        env.gate_off(&t);
        for _ in 0..(0.1 * SR) as usize {
            env.step();
        }
        let before = env.level();
        assert!(before > 0.1 && before < 0.6);
        env.gate_on(&t);
        let after = env.step();
        assert!(
            after > before && after - before < 0.01,
            "{before} -> {after}"
        );
        // And the attack still takes its set time from there.
        let attack = time_in(&mut env, Stage::Attack) + 1;
        assert!(attack.abs_diff((0.05 * SR) as usize) <= 48, "{attack}");
    }

    #[test]
    fn ar_holds_at_full_level() {
        let t = times(0.01, 0.0, 1.0, 0.1);
        let mut env = Env::default();
        env.gate_on(&t);
        for _ in 0..SR as usize {
            env.step();
        }
        assert_eq!((env.stage, env.level()), (Stage::Sustain, 1.0));
    }

    #[test]
    fn segments_are_curved_and_bounded() {
        let t = times(0.1, 0.1, 0.0, 0.1);
        let mut env = Env::default();
        env.gate_on(&t);
        let rise: Vec<f32> = (0..(0.1 * SR) as usize).map(|_| env.step()).collect();
        // Concave: past half way at half time.
        assert!(rise[rise.len() / 2] > 0.55);
        let fall: Vec<f32> = (0..(0.1 * SR) as usize).map(|_| env.step()).collect();
        // Exponential: well below half way at half time.
        assert!(fall[fall.len() / 2] < 0.1);
        assert!(rise.iter().chain(&fall).all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn sustain_follows_while_held() {
        let t = times(0.001, 0.01, 0.5, 0.1);
        let mut env = Env::default();
        env.gate_on(&t);
        for _ in 0..4_800 {
            env.step();
        }
        assert_eq!(env.stage, Stage::Sustain);
        env.set_sustain(0.8);
        assert!((env.step() - 0.8).abs() < 1.0e-6);
        // During the decay the segment re-aims and still lands on it.
        let mut env = Env::default();
        env.gate_on(&t);
        for _ in 0..200 {
            env.step();
        }
        assert_eq!(env.stage, Stage::Decay);
        env.set_sustain(0.2);
        time_in(&mut env, Stage::Decay);
        assert!((env.level() - 0.2).abs() < 1.0e-6);
        // Released, it no longer listens.
        env.gate_off(&t);
        env.set_sustain(0.9);
        assert!(env.step() < 0.2);
    }

    #[test]
    fn gate_off_when_idle_stays_idle() {
        let mut env = Env::default();
        env.gate_off(&times(0.01, 0.01, 0.5, 0.01));
        assert_eq!((env.stage, env.step()), (Stage::Idle, 0.0));
    }
}
