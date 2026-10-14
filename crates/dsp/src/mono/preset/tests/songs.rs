//! The preset sweeps: each renders seconds of audio for every preset.
//! Ignored by `cargo test`, so neither CI nor coverage runs them;
//! `make test-songs` does (#535).

use super::*;

#[test]
#[ignore = "a long render: make test-songs"]
fn every_preset_is_bounded() {
    for_every_preset(|preset, name| {
        // A sampler with nothing loaded is silent; `sampler::tests` and `padsampler::tests` play them.
        if preset.model().uses_sampler() || preset.model().uses_pads() {
            return;
        }
        for note in [24, 48, 72, 96, 108] {
            let mut e = Engine::new(48_000.0);
            e.set_param(0, Param::MasterGain, 1.0);
            e.preset(0, preset);
            e.note_on(0, note, 1.0);
            let mut heard = 0.0_f32;
            for i in 0..(48_000 * 5 / 2 / BLOCK) {
                if i == 48_000 / BLOCK {
                    e.note_off(0, note);
                }
                e.render(BLOCK);
                for s in e.output() {
                    assert!(s.is_finite() && s.abs() <= 1.0, "{name} {note}: {s}");
                    heard = heard.max(s.abs());
                }
            }
            assert!(heard > 0.02, "{name} {note} is silent");
            assert_eq!(e.active_voices(), 0, "{name} {note} still sounds");
        }
    });
}

/// Spec 006 Req 15: every polyphonic preset plays a full chord, bounded,
/// audible, and silent once released.
#[test]
#[ignore = "a long render: make test-songs"]
fn every_poly_preset_plays_a_full_chord() {
    for_every_preset(|preset, name| {
        let mut probe = Engine::new(48_000.0);
        probe.preset(0, preset);
        let voices = (probe.param_value(0, Param::Polyphony) as usize).min(preset.model().voices());
        if voices < 2 || preset.model().uses_sampler() || preset.model().uses_pads() {
            return;
        }
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(0, preset);
        let chord: Vec<u8> = (0..voices).map(|k| (40 + 5 * k).min(100) as u8).collect();
        for n in &chord {
            e.note_on(0, *n, 1.0);
        }
        let mut heard = 0.0_f32;
        for i in 0..(48_000 * 3 / BLOCK) {
            if i == 24_000 / BLOCK {
                for n in &chord {
                    e.note_off(0, *n);
                }
            }
            e.render(BLOCK);
            for s in e.output() {
                assert!(s.is_finite() && s.abs() <= 1.0, "{name}: {s}");
                heard = heard.max(s.abs());
            }
        }
        assert!(heard > 0.05, "{name} is silent");
        assert_eq!(e.active_voices(), 0, "{name} still sounds");
    });
}

/// No leftovers: a preset after another sounds like it does from fresh.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_preset_after_another_renders_like_a_fresh_one() {
    let render = |presets: &[Preset]| {
        let mut e = Engine::new(48_000.0);
        for p in presets {
            e.preset(0, *p);
        }
        e.note_on(0, 60, 1.0);
        let mut out = Vec::new();
        for _ in 0..50 {
            e.render(BLOCK);
            out.extend_from_slice(e.output());
        }
        out
    };
    // After the first, the middle and the last preset of the list: one of
    // each kind of leftover, without rendering every pair.
    let n = Preset::ALL.len();
    let befores = [Preset::ALL[0].0, Preset::ALL[n / 2].0, Preset::ALL[n - 1].0];
    for (preset, name) in Preset::ALL {
        let fresh = render(&[preset]);
        for before in befores {
            assert!(
                render(&[before, preset]) == fresh,
                "{name} after {before:?}"
            );
        }
    }
}

/// Run `check` on every preset, spread over the machine's threads: each check
/// renders seconds of audio, and a failure in any thread fails the test.
fn for_every_preset(check: impl Fn(Preset, &str) + Sync) {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let per = Preset::ALL.len().div_ceil(threads);
    std::thread::scope(|scope| {
        for chunk in Preset::ALL.chunks(per) {
            let check = &check;
            scope.spawn(move || {
                for (preset, name) in chunk {
                    check(*preset, name);
                }
            });
        }
    });
}
