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
        // chip picks the oscillator: 0 a 555, hard edges and its duty
        // stretched by the control pin, wide low and narrowing as it climbs;
        // 1 the NJD's transistor pair, a 50% square whose rising edges the
        // collector's 4k7 into 10nF (47 µs) rounds into a shark fin.
        PresetDef {
            code: Some(
                "SynthDef(\\dubsiren, { |freq = 440, gate = 1, mode = 0, amount = -12, rate = 6, depth = 19, sweep = 1.25, dir = 0, rise = 0.01, tone = 3500, tail = 0, chip = 0|\n    var lfo = Select.kr(mode, [LFPulse.kr(rate).range(-1, 1), LFTri.kr(rate), LFSaw.kr(rate), LFSaw.kr(rate).neg, LFNoise0.kr(rate), LFNoise1.kr(rate)]);\n    var env = EnvGen.kr(Env.asr(rise, 1, sweep), gate);\n    var glide = Select.kr(tail, [env.range(Select.kr(dir, [-24, 24]), 0), 0]);\n    var swing = lfo.range(0, depth) * Select.kr(tail, [1, env]);\n    var cap = Select.kr(tail, [1, env.max(0.001)]);\n    var pitch = freq * (amount + swing + glide).midiratio * 2 * cap / (1 + cap);\n    var duty = Select.kr(chip, [(0.6 - ((swing + glide) * 0.01)).clip(0.5, 0.9), 0.5]);\n    var sq = Pulse.ar(pitch, Select.kr(tail, [duty, cap / (1 + cap)]));\n    var sig = LPF.ar(Select.ar(chip, [sq, sq.min(LPF.ar(sq, 3400))]), tone);\n    Select.ar(tail, [sig * env, LeakDC.ar(sig) * (env * 1000).min(1)])\n}).add;\n",
            ),
            ..PresetDef::of(Preset::ModularDubSiren, &[(Polyphony, 1.0)])
        },
        // Moritz Klein's FM drum (MKI x ES.EDU, #779): the key is VCO1, the
        // carrier; VCO2 a ratio of it, its triangle FMs VCO1 exponentially by
        // fm semitones; a pitch envelope lifts both by depth over pdecay.
        // wave 0 is the diodes rounding VCO1's triangle into a sine, driven
        // harder for the first ms of the hit; 1 VCO1's pulse ring-modulated
        // by VCO2 through the high-pass. The velocity is the accent.
        PresetDef {
            code: Some(
                "SynthDef(\\fmdrum, { |freq = 110, amp = 0.5, ratio = 1.5, fm = 18, depth = 24, pdecay = 0.08, decay = 0.5, wave = 0, drive = 3|\n    var hit = Env.perc(0.001, pdecay).kr * depth;\n    var mod = LFTri.ar(freq * ratio * hit.midiratio);\n    var car = freq * (hit + (mod * fm)).midiratio;\n    var sine = (LFTri.ar(car) * (1.5 + (Env.perc(0.0005, 0.005).kr * drive * amp * 2))).softclip;\n    var ring = HPF.ar(Pulse.ar(car) * mod, 7000) * 2;\n    Select.ar(wave, [sine, ring]) * Env.perc(0.001, decay).kr * amp\n}).add;\n",
            ),
            ..PresetDef::of(Preset::ModularFmDrum, &[(Polyphony, 4.0)])
        },
        // The Atari Punk Console (Forrest Mims' Stepped Tone Generator, a
        // 556, #781), played as a synth: the key is the astable's pot, and
        // the one-shot it triggers ignores the edges while it runs, so it
        // sounds the key divided by n = floor(w) + 1, w its pulse in the
        // key's periods: the undertones, always in tune with the key. width
        // is the one-shot's pot (a cubed taper, the mod wheel turns it on
        // to the end): track 1 makes it 8 periods of the key at most, the
        // same steps on every key; 0 the real 0..55 ms (500k into 0.1 µF),
        // so a higher key divides further. chip 0 the 556: the astable's
        // output holds the trigger low through its low half, 0.5 minus
        // f / 288 kHz with 1k over the pot into 10 nF, so the one-shot's
        // pulse is that wide at least; 1 transistors: the trigger through
        // a coupling cap fires on the edge only, so the pulse goes thin,
        // and the collector's 47 µs rounds the rising edges (#777). glide
        // is the key's lag, sliding from one legato note to the next when
        // the synth is assigned mono.
        PresetDef {
            code: Some(
                "SynthDef(\\punk, { |gate = 1, width = 0.45, track = 1, chip = 0, glide = 0.05, modwheel = 0|\n    var freq = \\freq.kr(440, glide);\n    var pot = (width + (modwheel * (1 - width))).cubed;\n    var w = Select.kr(track, [pot * 0.055 * freq, pot * 8]);\n    var n = w.floor + 1;\n    var low = Select.kr(chip, [0.5 - (freq / 288000), 0]);\n    var sq = Pulse.ar(freq / n, (w.max(low) / n).clip(0.02, 0.98));\n    LeakDC.ar(Select.ar(chip, [sq, sq.min(LPF.ar(sq, 3400))])) * 0.6\n}).add;\n",
            ),
            ..PresetDef::of(
                Preset::ModularPunk,
                &[
                    (Polyphony, 8.0),
                    (AdsrAttack, 0.002),
                    (AdsrSustain, 1.0),
                    (AdsrRelease, 0.15),
                ],
            )
        },
        // Moritz Klein's hi-hat (MKI x ES.EDU, #783): six 40106 squares at
        // 261, 240, 219, 180, 150 and 123 Hz, which TUNE moves together
        // through the chip's supply, so the key is the top one (C4 plays the
        // circuit as built), each at a fresh phase on every hit. A multiple-
        // feedback band-pass (82k, 2 x 3.3 nF, 470 R + the 1k TONE pot) picks
        // 4.7..8 kHz at Q 5..7; the mix goes in small, as the 100k-into-680R
        // node does, under BPF's saturation. The envelope is the VCA
        // transistor's collector supply, so it gates and clips (clip2): an
        // instant attack, the 0.47 uF cap falling through 470k || DECAY.
        // Then the collector's 2.2 nF / 33k pole (a two-pole LPF standing in
        // for the one-pole) and the 3.4 kHz Q 1 Sallen-Key high-pass. One
        // envelope in the box: a hit restarts it, a closed hat chokes an open
        // one. The velocity is the accent.
        PresetDef {
            code: Some(
                "SynthDef(\\hihat, { |freq = 261.63, amp = 0.5, decay = 0.25, tone = 0.5, drive = 0.5|\n    var sq = Mix([LFPulse.ar(freq, Rand(0, 1)), LFPulse.ar(freq * 0.9195, Rand(0, 1)), LFPulse.ar(freq * 0.8391, Rand(0, 1)), LFPulse.ar(freq * 0.6897, Rand(0, 1)), LFPulse.ar(freq * 0.5747, Rand(0, 1)), LFPulse.ar(freq * 0.4713, Rand(0, 1))]);\n    var band = BPF.ar((sq - 3) * 0.02, 4700 * (1.7 ** (1 - tone)), 0.17);\n    var env = Env.perc(0.0005, decay).kr * amp;\n    var vca = (band * (4 + (drive * 60))).clip2(env);\n    RHPF.ar(LPF.ar(vca, 3600), 3400, 1) * 10\n}).add;\n",
            ),
            ..PresetDef::of(Preset::ModularHihat, &[(Polyphony, 1.0)])
        },
    ],
    ..ModelDef::MONO
};
