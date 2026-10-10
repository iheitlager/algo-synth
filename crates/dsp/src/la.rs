//! The D-50's LA voice (spec 006 Req 12): two partials, each a synthesised
//! oscillator (saw, pulse, with pulse-width modulation) or one of the generated PCM
//! attacks, each through its own resonant low-pass and its own amplifier, with its
//! own filter and amplifier envelopes. The pair is added, synced (partial 2 to 1,
//! when both are synthesised) or ring-modulated.
//!
//! A transient sample over a sustained synthesised body is the sound this voice is
//! for: a chiff on a flute, a pluck under a pad. The samples are our own
//! (`table::Tables`), not any ROM's. Keys are not stacked: a pool gives each note a
//! voice, so `press` and `release_all` are all a voice needs. Dual tones are not built.

use crate::mono::env::{Env, Stage};
use crate::mono::ladder::{Ladder, MAX_K};
use crate::mono::osc::Osc;
use crate::mono::voice::MonoCtx;
use crate::table::SampleOsc;

/// The partials' gain into the mix: two at full level sum below the limiter's knee.
const GAIN: f32 = 0.5;
/// Output level, as the Mono voice's.
const OUT_GAIN: f32 = 0.7;

#[derive(Clone, Copy, Default)]
pub struct LaVoice {
    note: f32,
    velocity: f32,
    gate: bool,
    retrigger: bool,
    /// Pitch trim in semitones from the pool (unison, analog variance).
    pub trim: f32,
    pub cutoff_trim: f32,
    osc: [Osc; 2],
    pcm: [SampleOsc; 2],
    lp: [Ladder; 2],
    /// Each partial's amplifier and filter envelope.
    tva: [Env; 2],
    tvf: [Env; 2],
}

impl LaVoice {
    pub fn new() -> LaVoice {
        LaVoice {
            lp: [Ladder::new(), Ladder::new()],
            ..LaVoice::default()
        }
    }

    pub fn active(&self) -> bool {
        self.retrigger || self.tva.iter().any(|e| e.stage != Stage::Idle)
    }

    pub fn gated(&self) -> bool {
        self.gate
    }

    pub fn press(&mut self, note: u8, velocity: f32) {
        if !self.active() {
            self.lp = [Ladder::new(), Ladder::new()];
        }
        self.note = f32::from(note);
        self.velocity = velocity.clamp(0.0, 1.0);
        self.gate = true;
        self.retrigger = true;
        for p in self.pcm.iter_mut() {
            p.start();
        }
    }

    pub fn release_all(&mut self) {
        self.gate = false;
    }

    /// Add this voice into `out`, advancing its state.
    pub fn render(&mut self, ctx: &MonoCtx, out: &mut [f32]) {
        let p = ctx.params;
        let adsr = [&p.adsr, &p.p2_adsr];
        let fenv = [&p.fadsr, &p.p2_fadsr];
        let (retrigger, gate) = (self.retrigger, self.gate);
        for (env, t) in self
            .tva
            .iter_mut()
            .zip(adsr)
            .chain(self.tvf.iter_mut().zip(fenv))
        {
            if retrigger || (gate && !env.gated()) {
                env.gate_on(t);
            } else if !gate && env.gated() {
                env.gate_off(t);
            }
        }
        self.retrigger = false;
        for (e, t) in self.tva.iter_mut().zip(adsr) {
            e.set_sustain(t.sustain);
        }
        for (e, t) in self.tvf.iter_mut().zip(fenv) {
            e.set_sustain(t.sustain);
        }
        for (o, w) in self.osc.iter_mut().zip(p.wave) {
            o.wave = w;
        }
        let tables = ctx.tables;
        // What each partial is, read once for the block.
        let parts = [
            Part {
                pcm: p.pcm[0],
                tune: p.tune[0],
                cutoff: p.cutoff,
                env_amount: p.normals.env_cutoff,
                k: p.k,
                level: p.level[0],
            },
            Part {
                pcm: p.pcm[1],
                tune: p.tune[1],
                cutoff: p.p2_cutoff,
                env_amount: p.p2_env_cutoff,
                k: p.p2_k,
                level: p.level[1],
            },
        ];
        let synced = p.structure == 1 && parts.iter().all(|q| q.pcm == 0);
        let ring = p.structure == 2;
        let pw = p.pulse_width;
        for (i, sample) in out.iter_mut().enumerate() {
            let a = [self.tva[0].step(), self.tva[1].step()];
            let f = [self.tvf[0].step(), self.tvf[1].step()];
            if !self.active() {
                return;
            }
            let lfo = ctx
                .shared
                .map_or(0.0, |s| s.lfo.get(i).copied().unwrap_or(0.0));
            let vibrato = lfo * p.mod_wheel * p.normals.vibrato;
            let key = self.note - 60.0;
            let mut raw = [0.0_f32; 2];
            let mut wrap = None;
            for (n, (part, y)) in parts.iter().zip(raw.iter_mut()).enumerate() {
                let inc = ctx
                    .pitch
                    .at(self.note + self.trim + p.bend + part.tune + vibrato);
                let pcm = part.pcm.checked_sub(1).and_then(|k| tables.sample(k));
                *y = match (pcm, self.pcm.get_mut(n), self.osc.get_mut(n)) {
                    (Some(sample), Some(player), _) => player.step(sample, inc),
                    (None, _, Some(o)) => {
                        let width = (pw + lfo * p.normals.lfo_pw).clamp(0.05, 0.95);
                        o.set_increment(inc);
                        let sync = if n == 1 && synced { wrap } else { None };
                        let (v, w) = o.step(ctx.blep, ctx.sine, width, sync);
                        if n == 0 {
                            wrap = w;
                        }
                        v
                    }
                    _ => 0.0,
                };
            }
            let mut y = [0.0_f32; 2];
            for (n, ((part, raw), out)) in parts.iter().zip(raw).zip(y.iter_mut()).enumerate() {
                let (Some(lp), Some(env), Some(amp)) = (self.lp.get_mut(n), f.get(n), a.get(n))
                else {
                    continue;
                };
                let cutoff = part.cutoff
                    + part.env_amount * env
                    + key * p.normals.key_track
                    + lfo * p.normals.lfo_cutoff
                    + self.cutoff_trim;
                let k = part.k.clamp(0.0, MAX_K);
                *out = lp.process(ctx.ladder, raw * 0.5, cutoff, k, 1.0) * amp * part.level;
            }
            let [y1, y2] = y;
            let mix = if ring { 2.0 * y1 * y2 } else { y1 + y2 };
            *sample += mix * GAIN * OUT_GAIN * self.velocity;
        }
    }
}

/// One partial's settings for a block.
struct Part {
    /// 0 is a synthesised oscillator, 1.. a PCM attack.
    pcm: usize,
    tune: f32,
    /// Its filter's cutoff (a MIDI note), envelope amount, feedback and its level.
    cutoff: f32,
    env_amount: f32,
    k: f32,
    level: f32,
}

#[cfg(test)]
mod tests {
    // The voice is tested through the engine and the pool: see `engine::tests` and
    // `mono::voice::tests` for the D-50 scenarios of spec 006 Req 12.
    #[test]
    fn a_voice_is_idle_until_pressed() {
        let v = super::LaVoice::new();
        assert!(!v.active() && !v.gated());
    }
}
