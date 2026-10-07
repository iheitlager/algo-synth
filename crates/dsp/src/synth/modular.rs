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
                &[
                    (Model, 19.0),
                    (Polyphony, 8.0),
                    (AdsrAttack, 0.005),
                    (AdsrRelease, 0.25),
                ],
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
                    (Model, 19.0),
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
            ..PresetDef::of(Preset::ModularKick, &[(Model, 19.0), (Polyphony, 4.0)])
        },
    ],
    ..ModelDef::MONO
};
