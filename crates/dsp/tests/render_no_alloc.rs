//! `render` never allocates (ADR-0002, #233), checked by counting every
//! allocation while a busy song plays: drum lanes, chords, a live arp and a
//! walk, strip and synth automation, scenes that solo, mute and send,
//! modulations, a Modular voice, a new song taking over on a bar line, and
//! a SuperCollider hoover.
//!
//! The counter is the whole process's, so this file is its own test binary
//! with one test: nothing else runs while it counts.

use algo_dsp::engine::Engine;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
use std::alloc::System;

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

const SONG: &str = "\
tempo 180
scale e phrygian
voice buzz = { (pulse(freq, lfo(3).range(0.2, 0.6)) + saw(freq * 0.5) + noise() * 0.05) |> svf(lp, lfo(0.2).exprange(300, bright), 0.4) * env(0.005, 0.2, 0.6, 0.3) }
  ctl bright = 3000 [300 6000 exp]
voice metal = { mix(fm(freq, freq * [1.5, 2], env(perc).range(0, 4)), sin(freq) * 0.2) |> ladder(2000, 0.4) |> delay(0.002, 0.5) |> drive(2) }
track kit drums
track lead synth
track pad synth
track bass synth
track buzzer synth Modular buzz
track bell synth Modular metal

frag beat = kit /16
  bd x..x..x...x..x..
  sn ....x.......X...
  cl euclid(7,16,2)
frag hold = pad .cutoff(saw.exprange(200, 2000))
  \"[e3,g#3,b3] [f3,a3,c4]\"
frag sand = lead live
  arp([e4,g#4,b4,d5],random,16,3)
frag roam = bass live
  walk(e2,16,5)
frag ring = bell
  \"[e4,b4] ~ g#4 ~\"
frag zap = buzzer
  \"e3 [g3 b3] ~ <e4 d4>\"

auto fade = strip2.Level ramp 1 0.2 /2
auto pan = strip3.Pan -1 0 1 0 /1
auto sweep = lead.Cutoff ramp 300 4000 /2
scene solo: strip1.Solo 1, strip2.Mute 1, strip3.Send1 0.5
scene open: strip1.Solo 0, strip2.Mute 0, master.P2Return 0.4
mod lead.resonance = lfo(0.5, tri).range(0.1, 0.6).lag(0.05)
mod strip3.send2 = rand.segment(8) * 0.3 + perlin.slow(2) * 0.2
mod buzzer.bright = sine.slow(2).exprange(800, 5000)
mod lead.cutoff = env(perc).exprange(300, 4000)
mod pad.vco1level = [1, 0.6, 0.8]

section a 2: beat hold sand roam zap ring fade pan sweep [open]
section b 1: beat sand roam zap [solo]
arrange a b a b
";

#[test]
fn a_busy_song_renders_without_allocating() {
    let mut e = Engine::new(48_000.0);
    let text = SONG.as_bytes();
    e.song_buffer(text.len())
        .expect("the song fits")
        .copy_from_slice(text);
    e.load_song().expect("the song parses");
    e.song_play();
    // Six bars at 180 BPM, past every section change, scene and live cycle.
    let blocks = 6 * 4 * 48_000 * 60 / 180 / 128;
    let region = Region::new(GLOBAL);
    for _ in 0..blocks {
        e.render(128);
    }
    let change = region.change();
    assert_eq!(
        (
            change.allocations,
            change.reallocations,
            change.deallocations
        ),
        (0, 0, 0),
        "render allocated: {change:?}"
    );
    assert!(e.meters().iter().any(|m| *m > 0.0), "the song was heard");

    // A song loaded while playing takes over on the next bar line, inside
    // `render`: that too only moves what the load prepared (#208).
    let edited = SONG.replace("tempo 180", "tempo 160");
    e.song_buffer(edited.len())
        .expect("the song fits")
        .copy_from_slice(edited.as_bytes());
    e.load_song().expect("the edit parses");
    assert_eq!(e.song().tempo, 180.0, "not before the bar");
    let region = Region::new(GLOBAL);
    for _ in 0..blocks / 3 {
        e.render(128);
    }
    let change = region.change();
    assert_eq!(
        (
            change.allocations,
            change.reallocations,
            change.deallocations
        ),
        (0, 0, 0),
        "taking over allocated: {change:?}"
    );
    assert_eq!(e.song().tempo, 160.0, "taken over");

    // A SuperCollider hoover set on a free synth (ADR-0024): its state was
    // sized when the code was set, so its notes start and play without
    // allocating: 40 saws, 20 delays, numbers drawn per note, Splay and a
    // FreeVerb2 on a stereo bus.
    e.preset(15, algo_dsp::mono::preset::Preset::ModularBasic);
    e.set_code(15, HOOVER).expect("the hoover builds");
    let region = Region::new(GLOBAL);
    for n in [57, 60, 64] {
        e.note_on(15, n, 0.9);
    }
    for _ in 0..blocks / 6 {
        e.render(128);
    }
    e.note_off(15, 60);
    for _ in 0..blocks / 6 {
        e.render(128);
    }
    let change = region.change();
    assert_eq!(
        (
            change.allocations,
            change.reallocations,
            change.deallocations
        ),
        (0, 0, 0),
        "the hoover allocated: {change:?}"
    );
}

const HOOVER: &str = r"SynthDef(\hoover, {
    var snd, freq, bw, delay, decay;
    freq = \freq.kr(440);
    freq = freq * Env([-5, 6, 0], [0.1, 1.7], [\lin, -4]).kr.midiratio;
    bw = 1.035;
    snd = { DelayN.ar(Saw.ar(freq * ExpRand(bw, 1 / bw)) + Saw.ar(freq * 0.5 * ExpRand(bw, 1 / bw)), 0.01, Rand(0, 0.01)) }.dup(20);
    snd = (Splay.ar(snd) * 3).atan;
    snd = snd * Env.asr(0.01, 1.0, 1.0).kr(0, \gate.kr(1));
    snd = FreeVerb2.ar(snd[0], snd[1], 0.3, 0.9);
    snd = snd * Env.asr(0, 1.0, 4, 6).kr(2, \gate.kr(1));
    Out.ar(\out.kr(0), snd * \amp.kr(0.1));
}).add;";
