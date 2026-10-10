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
        // The two-555 dub siren: the key is the pitch; mode picks the LFO
        // (square, triangle, saw up, saw down, random step, random smooth),
        // amount sets the base in semitones from the key, rate and depth the
        // LFO's speed and swing above it, and on release the pitch glides two
        // octaves down or up (dir) over sweep seconds as it fades.
        PresetDef {
            code: Some(
                "SynthDef(\\dubsiren, { |freq = 440, gate = 1, mode = 0, amount = -12, rate = 6, depth = 19, sweep = 1.5, dir = 0|\n    var lfo = Select.kr(mode, [LFPulse.kr(rate).range(-1, 1), LFTri.kr(rate), LFSaw.kr(rate), LFSaw.kr(rate).neg, LFNoise0.kr(rate), LFNoise1.kr(rate)]);\n    var env = EnvGen.kr(Env.asr(releaseTime: sweep), gate);\n    var glide = env.range(Select.kr(dir, [-24, 24]), 0);\n    var sig = LPF.ar(Pulse.ar(freq * (lfo.range(amount, amount + depth) + glide).midiratio, 0.6), 3500);\n    sig * env\n}).add;\n",
            ),
            ..PresetDef::of(Preset::ModularDubSiren, &[(Polyphony, 1.0)])
        },
    ],
    ..ModelDef::MONO
};
