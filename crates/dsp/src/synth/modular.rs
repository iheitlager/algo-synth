//! The Modular synth: what the model is and its presets (#330).

use crate::mono::preset::Preset;
use crate::params::Param::*;
use crate::synth::{Engine, ModelDef, PresetDef};

pub const DEF: ModelDef = ModelDef {
    engine: Engine::Graph,
    presets: &[
        // A Modular synth's sound is its SynthDef (`code`); the ADSR shapes
        // a voice without an envelope.
        PresetDef {
            code: Some(
                "SynthDef(\\basic, { |freq = 440, gate = 1|\n    RLPF.ar(Saw.ar(freq), 1800, 0.7)\n}).add;\n",
            ),
            ..PresetDef::of(
                Preset::ModularBasic,
                &[(Polyphony, 8.0), (AdsrAttack, 0.005), (AdsrRelease, 0.25)],
            )
        },
        // Two detuned pulses whose widths an LFO each moves, and a saw an
        // octave down: the rave hoover's buzz.
        PresetDef {
            code: Some(
                "SynthDef(\\hoover, { |freq = 440, gate = 1|\n    var a = Pulse.ar(freq * 0.995, SinOsc.kr(5).range(0.1, 0.4));\n    var b = Pulse.ar(freq * 1.005, SinOsc.kr(4.3).range(0.2, 0.5));\n    var sub = Saw.ar(freq * 0.5);\n    RLPF.ar(a + b + sub, 2500, 0.7)\n}).add;\n",
            ),
            ..PresetDef::of(
                Preset::ModularHoover,
                &[
                    (Polyphony, 8.0),
                    (AdsrAttack, 0.02),
                    (AdsrSustain, 0.9),
                    (AdsrRelease, 0.4),
                ],
            )
        },
        // A gabber kick: a sine swept from 900 Hz down to 48 in a tenth of
        // a second, under a short decay, driven hard into a square-ish boom.
        PresetDef {
            code: Some(
                "SynthDef(\\kick, { |amp = 0.5|\n    var pitch = Env.perc(0.001, 0.12).kr.exprange(48, 900);\n    var body = SinOsc.ar(pitch) * Env.perc(0.001, 0.45).kr;\n    (body * 12).tanh * amp\n}).add;\n",
            ),
            ..PresetDef::of(Preset::ModularKick, &[(Polyphony, 4.0)])
        },
        // The SID's parts in a graph (#112): a pulse whose 12-bit width an LFO
        // sweeps, through the 6581's resonant low-pass, under the chip's envelope.
        PresetDef {
            code: Some(
                "SynthDef(\\sid, { |freq = 440, gate = 1|\n    var osc = SidOsc.ar(freq, 4, SinOsc.kr(0.7).range(600, 3400));\n    SidFilter.ar(osc, 900, 9, 1) * SidEnv.kr(gate, 0, 8, 11, 8)\n}).add;\n",
            ),
            ..PresetDef::of(
                Preset::ModularSid,
                &[
                    (Polyphony, 4.0),
                    (AdsrAttack, 0.001),
                    (AdsrSustain, 1.0),
                    (AdsrRelease, 1.0),
                ],
            )
        },
        // The two-555 dub siren: the key is the pitch; mode picks the LFO
        // (square, triangle, saw up, saw down, random step, random smooth),
        // amount sets the base in semitones from the key, rate and depth the
        // LFO's speed and swing above it, and on release the pitch glides two
        // octaves down or up (dir) over sweep seconds as it fades.
        // The NJD clone's ways (#777): rise is the press's climb (the cap
        // charging), tone the low-pass, and tail 1 its release: the swing
        // falls back to the base at full level while one half of the square
        // follows the cap, so the tone slows into ticks and stops.
        PresetDef {
            code: Some(
                "SynthDef(\\dubsiren, { |freq = 440, gate = 1, mode = 0, amount = -12, rate = 6, depth = 19, sweep = 1.25, dir = 0, rise = 0.01, tone = 3500, tail = 0|\n    var lfo = Select.kr(mode, [LFPulse.kr(rate).range(-1, 1), LFTri.kr(rate), LFSaw.kr(rate), LFSaw.kr(rate).neg, LFNoise0.kr(rate), LFNoise1.kr(rate)]);\n    var env = EnvGen.kr(Env.asr(rise, 1, sweep), gate);\n    var glide = Select.kr(tail, [env.range(Select.kr(dir, [-24, 24]), 0), 0]);\n    var swing = lfo.range(0, depth) * Select.kr(tail, [1, env]);\n    var cap = Select.kr(tail, [1, env.max(0.001)]);\n    var pitch = freq * (amount + swing + glide).midiratio * 2 * cap / (1 + cap);\n    var sig = LPF.ar(Pulse.ar(pitch, Select.kr(tail, [0.6, cap / (1 + cap)])), tone);\n    Select.ar(tail, [sig * env, LeakDC.ar(sig) * (env * 1000).min(1)])\n}).add;\n",
            ),
            ..PresetDef::of(Preset::ModularDubSiren, &[(Polyphony, 1.0)])
        },
    ],
    ..ModelDef::MONO
};
