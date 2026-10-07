use super::*;
use crate::mono::model::Model;
use crate::mono::osc::LATENCY;
use crate::poly::VOICE_BUDGET;
use crate::smf::tests::file;

/// Whether `owner`'s voice has a key held.
fn gated(e: &Engine, owner: Owner) -> bool {
    e.voice(owner).is_some_and(MonoVoice::gated)
}

fn peak(e: &Engine) -> f32 {
    e.output().iter().fold(0.0_f32, |m, s| m.max(s.abs()))
}

/// Spec 006 Req 14: a SysEx voice reaches the DX7 parameters, operator 6 first.
#[test]
fn a_sysex_voice_sets_the_dx7_parameters() {
    let mut p = crate::fm::patch::FmPatch::default();
    p.set_op(0, 16, 77.0); // operator 6's output level
    p.set_op(5, 16, 55.0); // operator 1's
    p.set_global(8, 12.0); // algorithm
    let data = sysex::to_voice_bytes(&p, "TESTVOICE");
    let mut file = vec![0xf0, 0x43, 0x00, 0x00, 0x01, 0x1b];
    file.extend_from_slice(&data);
    file.extend_from_slice(&[sysex::checksum(&data), 0xf7]);
    let mut e = Engine::new(48_000.0);
    e.sysex_buffer(file.len())
        .expect("fits")
        .copy_from_slice(&file);
    assert_eq!(e.load_sysex(), Ok(1));
    assert_eq!(e.sysex_name(0), "TESTVOICE");
    assert!(e.apply_sysex(2, 0));
    assert!(!e.apply_sysex(2, 1));
    let get = |e: &Engine, id: u32| e.param_value(2, Param::from_id(id).expect("id"));
    assert_eq!(get(&e, Param::Op6R1 as u32 + 16), 77.0);
    assert_eq!(get(&e, Param::Op1R1 as u32 + 16), 55.0);
    assert_eq!(get(&e, Param::Algorithm as u32), 12.0);
    assert_eq!(e.load_sysex_of(b"junk"), Err(sysex::Error::Unsupported));
    assert!(e.sysex_buffer(MAX_SYSEX + 1).is_none());
}

/// Every note a synth starts in `frames` frames, as (frame, synth, note),
/// rendered `step` frames at a time (so a frame is known to within `step`).
fn note_starts(e: &mut Engine, frames: u64, step: usize) -> Vec<(u64, usize, u8)> {
    let mut held: Vec<Vec<u8>> = vec![Vec::new(); SYNTHS];
    let mut out = Vec::new();
    for f in (0..frames).step_by(step) {
        e.render(step);
        for (s, was) in held.iter_mut().enumerate() {
            let now = e.pools.get(s).map(|p| p.held_notes()).unwrap_or_default();
            for n in &now {
                if !was.contains(n) {
                    out.push((f, s, *n));
                }
            }
            *was = now;
        }
    }
    out
}

/// #173, ADR-0022: the demo Canon imported as a song starts every note of
/// the file on the synth of its channel (in channel order, from 0), at its
/// time in the file (to the eight frames the test renders at once, far finer
/// than a tick).
#[test]
fn the_demo_imported_plays_the_file() {
    let bytes = include_bytes!("../../../../web/public/demo.mid");
    let mut e = Engine::new(48_000.0);
    for s in 0..SYNTHS {
        e.set_param(s, Param::Polyphony, 8.0);
    }
    assert_eq!(import(&mut e, bytes), Ok(4));
    assert!(e.song().arrange.len() > 1, "in sections");
    e.song_play();
    let frames = 48_000 * 24;
    let want = file_starts(bytes, 48_000.0, frames);
    let mut got = note_starts(&mut e, frames, 8);
    got.sort_unstable();
    assert!(want.len() > 40, "the demo plays: {}", want.len());
    assert_eq!(want.len(), got.len());
    for (x, y) in want.iter().zip(&got) {
        assert_eq!((x.1, x.2), (y.1, y.2), "{x:?} {y:?}");
        assert!(x.0.abs_diff(y.0) <= 8, "{x:?} {y:?}");
    }
}

/// Every note-on of a one-tempo file before `frames`, as (frame, synth,
/// note): its channels on synths 0, 1, 2… in order, ordered as `note_starts`.
fn file_starts(bytes: &[u8], rate: f64, frames: u64) -> Vec<(u64, usize, u8)> {
    let smf = smf::parse(bytes).expect("parses");
    let events = || smf.tracks.iter().flat_map(|t| t.events.iter());
    let us = events()
        .find_map(|e| match e.kind {
            smf::Kind::Tempo(us) => Some(f64::from(us)),
            _ => None,
        })
        .unwrap_or(500_000.0);
    let per_tick = us / 1.0e6 / f64::from(smf.division) * rate;
    let mut channels: Vec<u8> = events()
        .filter_map(|e| match e.kind {
            smf::Kind::NoteOn { channel, .. } => Some(channel),
            _ => None,
        })
        .collect();
    channels.sort_unstable();
    channels.dedup();
    let mut out: Vec<(u64, usize, u8)> = events()
        .filter_map(|e| match e.kind {
            smf::Kind::NoteOn {
                channel,
                note,
                velocity,
            } if velocity > 0 => {
                let synth = channels.iter().position(|c| *c == channel)?;
                Some(((e.tick as f64 * per_tick).round() as u64, synth, note))
            }
            _ => None,
        })
        .filter(|(f, _, _)| *f < frames)
        .collect();
    out.sort_unstable();
    out
}

/// Import a MIDI file as the song (#173): its tracks, or the error code.
fn import(e: &mut Engine, bytes: &[u8]) -> Result<usize, i32> {
    e.midi_buffer(bytes.len())
        .expect("fits")
        .copy_from_slice(bytes);
    e.import_midi()
}

/// Play song track `t` on synth `s`, as a loaded song routes it.
fn route_track(e: &mut Engine, t: usize, s: Option<usize>) {
    if let Some(r) = e.song_route.get_mut(t) {
        *r = s;
    }
}

/// One note on channel `ch` at tick 480 (0.5 s at 120 BPM), 480 ticks long.
fn one_note(ch: u8) -> Vec<u8> {
    let t = vec![
        0x83,
        0x60,
        0x90 | ch,
        60,
        100,
        0x83,
        0x60,
        0x80 | ch,
        60,
        0,
        0x00,
        0xFF,
        0x2F,
        0,
    ];
    file(0, 480, &[t])
}

#[test]
fn silent_without_notes() {
    let mut e = Engine::new(48_000.0);
    e.render(BLOCK);
    assert_eq!(peak(&e), 0.0);
}

/// Mono's VCA is its ADSR: it sounds, then releases to silence.
#[test]
fn mono_follows_its_adsr() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::AdsrRelease, 0.01);
    e.note_on(0, 57, 1.0);
    for _ in 0..40 {
        e.render(BLOCK);
    }
    assert!(peak(&e) > 0.01);
    e.note_off(0, 57);
    // 0.01 s is 3.75 blocks.
    for _ in 0..5 {
        e.render(BLOCK);
    }
    assert_eq!(e.active_voices(), 0);
    for _ in 0..2 {
        e.render(BLOCK);
    }
    assert_eq!(peak(&e), 0.0);
}

/// A note on and off before the next block still sounds, then ends.
#[test]
fn a_mono_tap_shorter_than_a_block_sounds() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::AdsrAttack, 0.001);
    e.set_param(0, Param::AdsrRelease, 0.01);
    e.note_on(0, 69, 1.0);
    e.note_off(0, 69);
    let mut heard = 0.0_f32;
    for _ in 0..10 {
        e.render(BLOCK);
        heard = heard.max(peak(&e));
    }
    assert!(heard > 0.01, "peak {heard}");
    assert_eq!(e.active_voices(), 0);
}

/// Mono's sustain slider moves a held note: half the sustain is half the
/// level through a linear VCA, and on the ARP 2600, whose ADSR drives an
/// exponential VCA over 60 dB, 21 dB down (#341).
#[test]
fn mono_sustain_moves_a_held_note() {
    let level = |model: Model, sustain: f32| {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Model, model as u32 as f32);
        e.set_param(0, Param::AdsrDecay, 0.01);
        e.note_on(0, 57, 1.0);
        for _ in 0..40 {
            e.render(BLOCK);
        }
        e.set_param(0, Param::AdsrSustain, sustain);
        let mut sum = 0.0;
        for _ in 0..40 {
            e.render(BLOCK);
            sum += e.output().iter().map(|s| s * s).sum::<f32>();
        }
        sum.sqrt()
    };
    let ratio = level(Model::Ms20, 0.35) / level(Model::Ms20, 0.7);
    assert!(
        (ratio - 0.5).abs() < 0.05,
        "half the sustain, half the level: {ratio}"
    );
    let db = 20.0 * (level(Model::Arp2600, 0.35) / level(Model::Arp2600, 0.7)).log10();
    assert!((db + 21.0).abs() < 1.5, "exponential: {db} dB");
}

/// Spec 004 Req 6: live input and each song track have their own Mono
/// voice, and a note off on one leaves the others gated.
#[test]
fn mono_owners_are_independent() {
    let mut e = Engine::new(48_000.0);
    route_track(&mut e, 2, Some(0));
    route_track(&mut e, 3, Some(0));
    e.start_voice(Owner::Track(2), 60, 1.0);
    e.start_voice(Owner::Track(3), 64, 1.0);
    e.note_on(0, 67, 1.0);
    e.render(BLOCK);
    assert_eq!(e.active_voices(), 3);
    e.stop_note(Owner::Track(2), 60);
    assert!(!gated(&e, Owner::Track(2)));
    assert!(gated(&e, Owner::Track(3)) && gated(&e, Owner::Live(0)));
    // Live Mono is monophonic: a second key moves the same voice.
    e.note_on(0, 69, 1.0);
    e.render(BLOCK);
    assert_eq!(e.voice(Owner::Live(0)).map(MonoVoice::note), Some(69));
    assert_eq!(e.active_voices(), 3);
}

/// 16 Mono voices at full resonance, drive and level still stay in ±1.
#[test]
fn loud_patches_are_limited_to_full_scale() {
    let mut e = Engine::new(48_000.0);
    for (p, v) in [
        (Param::MasterGain, 1.0),
        (Param::Vco2Level, 1.0),
        (Param::Vco3Level, 1.0),
        (Param::NoiseLevel, 1.0),
        (Param::Resonance, 1.0),
        (Param::Drive, 1.0),
        (Param::AdsrSustain, 1.0),
    ] {
        e.set_param(0, p, v);
    }
    // 16 Mono voices: one per song track.
    for t in 0..16 {
        route_track(&mut e, usize::from(t), Some(0));
        e.start_voice(Owner::Track(t), 36 + 3 * t, 1.0);
    }
    let mut peak_seen = 0.0_f32;
    for _ in 0..200 {
        e.render(BLOCK);
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        peak_seen = peak_seen.max(peak(&e));
    }
    assert!(peak_seen > 0.9, "the limiter is reached, peak {peak_seen}");
}

/// The same events give the same output, bit for bit: noise, the LFO and
/// the sample-and-hold are seeded, not random.
#[test]
fn the_same_events_render_bit_identical_output() {
    let play = || {
        let mut e = Engine::new(48_000.0);
        e.preset(0, Preset::Lead);
        e.set_param(0, Param::NoiseLevel, 0.5);
        e.preset(1, Preset::BowedString);
        let mut out = Vec::new();
        for block in 0..400 {
            match block {
                10 => e.note_on(0, 60, 0.9),
                30 => e.note_on(1, 48, 0.7),
                100 => e.set_param(0, Param::Cutoff, 500.0),
                200 => e.note_off(0, 60),
                300 => e.note_off(1, 48),
                _ => {}
            }
            e.render(BLOCK);
            out.extend_from_slice(e.output());
        }
        out
    };
    let (a, b) = (play(), play());
    assert!(a.iter().any(|s| *s != 0.0));
    assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()));
}

/// A Mono voice playing only noise has no DC.
#[test]
fn a_noise_only_voice_has_no_dc() {
    let mut e = Engine::new(48_000.0);
    silence(&mut e, 0);
    for (p, v) in [
        (Param::MasterGain, 1.0),
        (Param::NoiseLevel, 1.0),
        (Param::Cutoff, 20_000.0),
        (Param::AdsrSustain, 1.0),
    ] {
        e.set_param(0, p, v);
    }
    e.note_on(0, 60, 1.0);
    let mut out = Vec::new();
    for _ in 0..(48_000 * 4 / BLOCK) {
        e.render(BLOCK);
        out.extend(e.output().iter().map(|s| f64::from(*s)));
    }
    let mean = out.iter().sum::<f64>() / out.len() as f64;
    let rms = (out.iter().map(|s| s * s).sum::<f64>() / out.len() as f64).sqrt();
    assert!(rms > 0.05, "noise is heard: {rms}");
    assert!(mean.abs() < 0.01 * rms, "DC {mean} at rms {rms}");
}

/// Mute `synth` at its mixer: every VCO and the noise at level 0.
fn silence(e: &mut Engine, synth: usize) {
    for p in [
        Param::Vco1Level,
        Param::Vco2Level,
        Param::Vco3Level,
        Param::NoiseLevel,
    ] {
        e.set_param(synth, p, 0.0);
    }
}

/// Peak over `blocks` blocks.
fn heard(e: &mut Engine, blocks: usize) -> f32 {
    (0..blocks).fold(0.0_f32, |m, _| {
        e.render(BLOCK);
        m.max(peak(e))
    })
}

/// plan.md MVP 5: each synth has its own patch.
#[test]
fn synths_have_their_own_parameters() {
    let mut e = Engine::new(48_000.0);
    silence(&mut e, 1);
    assert_eq!(e.param_value(1, Param::Vco1Level), 0.0);
    assert!(e.param_value(0, Param::Vco1Level) > 0.0);
    e.note_on(1, 57, 1.0);
    // Below −80 dB: the ladder leaves a residue of about −107 dB.
    assert!(heard(&mut e, 20) < 1.0e-4, "synth 1 is silenced");
    e.note_on(0, 57, 1.0);
    assert!(heard(&mut e, 20) > 0.05, "synth 0 still sounds");
    assert_eq!(e.active_voices(), 2);
}

/// Spec 005 Req 1: the model is a parameter of its own synth.
#[test]
fn models_are_per_synth() {
    let mut e = Engine::new(48_000.0);
    e.set_param(1, Param::Model, 1.0);
    e.set_param(1, Param::Model, 99.0);
    assert_eq!(
        e.param_value(1, Param::Model),
        (Model::ALL.len() - 1) as f32,
        "clamped into range"
    );
    e.set_param(1, Param::Model, 1.0);
    assert_eq!(e.param_value(1, Param::Model), 1.0);
    assert_eq!(e.param_value(0, Param::Model), 0.0);
    assert_eq!(e.param_value(2, Param::Model), 0.0);
    for (p, v) in DEFAULTS.iter().filter(|(p, _)| *p != Param::Model) {
        assert_eq!(e.param_value(0, *p), *v, "{p:?} on synth 0");
    }
}

/// Spec 005 Req 1: two synths with the same settings and different
/// models sound different, and one's model leaves the other alone.
#[test]
fn models_sound_different() {
    let render = |model: f32| {
        let mut e = Engine::new(48_000.0);
        for (p, v) in [
            (Param::Model, model),
            (Param::Cutoff, 300.0),
            (Param::EnvCutoff, 0.6),
            (Param::FenvAttack, 0.5),
            (Param::AdsrSustain, 1.0),
        ] {
            e.set_param(1, p, v);
        }
        e.note_on(1, 45, 1.0);
        let mut out = Vec::new();
        for _ in 0..200 {
            e.render(BLOCK);
            out.extend_from_slice(e.output().get(..BLOCK).unwrap_or(&[]));
        }
        out
    };
    let (arp, mini) = (render(0.0), render(1.0));
    let diff: f32 = arp.iter().zip(&mini).map(|(a, b)| (a - b).abs()).sum();
    assert!(diff > 1.0, "the models differ: {diff}");
    assert_eq!(arp, render(0.0), "and each is repeatable");
}

#[test]
fn a_preset_on_one_synth_leaves_the_others() {
    let mut e = Engine::new(48_000.0);
    e.preset(2, Preset::Bass);
    for (p, v) in DEFAULTS {
        assert_eq!(e.param_value(0, p), v, "{p:?} on synth 0");
        assert_eq!(e.param_value(3, p), v, "{p:?} on synth 3");
    }
    e.reset(2);
    for (p, v) in DEFAULTS {
        assert_eq!(e.param_value(2, p), v, "{p:?} after reset");
    }
}

#[test]
fn master_gain_is_global() {
    let mut e = Engine::new(48_000.0);
    e.set_param(5, Param::MasterGain, 0.2);
    assert_eq!(e.param_value(0, Param::MasterGain), 0.2);
    assert_eq!(e.master_gain, 0.2);
}

/// Out-of-range synths are ignored, and a live note never reaches a
/// track's voice.
#[test]
fn unknown_synths_are_ignored() {
    let mut e = Engine::new(48_000.0);
    route_track(&mut e, 4, Some(0));
    e.start_voice(Owner::Track(4), 60, 1.0);
    e.note_on(SYNTHS, 62, 1.0);
    e.note_off(SYNTHS + 4, 60);
    e.set_param(SYNTHS, Param::Cutoff, 300.0);
    e.preset(usize::MAX, Preset::Bass);
    e.reset(SYNTHS);
    assert!(gated(&e, Owner::Track(4)));
    assert_eq!(e.param_value(SYNTHS, Param::Cutoff), 0.0);
    e.render(BLOCK);
    assert_eq!(e.active_voices(), 1);
}

#[test]
fn an_imported_track_plays_on_its_routed_synth() {
    let render = |synth: Option<usize>| {
        let mut e = Engine::new(48_000.0);
        silence(&mut e, 1);
        import(&mut e, &one_note(0)).expect("imports");
        route_track(&mut e, 0, synth);
        e.song_play();
        heard(&mut e, 300)
    };
    assert!(render(Some(0)) > 0.05);
    assert!(render(Some(1)) < 1.0e-4, "synth 1 is silenced");
    assert_eq!(render(None), 0.0, "muted");
    assert_eq!(render(Some(SYNTHS)), 0.0, "an unknown synth mutes");
}

/// Changing a synth's patch reaches the track's voice playing on it.
#[test]
fn a_track_voice_follows_its_synths_parameters() {
    let mut e = Engine::new(48_000.0);
    import(&mut e, &one_note(0)).expect("imports");
    route_track(&mut e, 0, Some(4));
    e.song_play();
    // The note starts at 0.5 s (187.5 blocks).
    assert!(heard(&mut e, 200) > 0.05);
    silence(&mut e, 4);
    heard(&mut e, 2);
    assert!(
        heard(&mut e, 10) < 1.0e-4,
        "silencing synth 4 silences the track"
    );
}

/// Spec 006 Req 1: a polyphonic synth plays a chord from live keys and a song
/// track at once, each releasing only its own notes.
#[test]
fn a_poly_synth_plays_chords_from_live_keys_and_a_track() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::Polyphony, 8.0);
    // A chord of three on channel 0 at 0.5 s, held for 0.5 s.
    let chord = vec![
        0x83, 0x60, 0x90, 60, 100, 0x00, 0x90, 64, 100, 0x00, 0x90, 67, 100, 0x83, 0x60, 0x80, 60,
        0, 0x00, 0x80, 64, 0, 0x00, 0x80, 67, 0, 0x00, 0xFF, 0x2F, 0,
    ];
    import(&mut e, &file(0, 480, &[chord])).expect("imports");
    route_track(&mut e, 0, Some(0));
    e.note_on(0, 72, 1.0);
    e.song_play();
    let mut most = 0;
    for _ in 0..260 {
        e.render(BLOCK);
        most = most.max(e.active_voices());
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
    assert_eq!(most, 4, "three chord notes and the live key");
    for _ in 0..400 {
        e.render(BLOCK);
    }
    assert!(
        gated_notes(&e, 0) == 1,
        "the live key is still held after the chord ends"
    );
}

/// Spec 006 Req 6: the Prophet-5 has five voices; a sixth note steals the oldest.
#[test]
fn the_prophet_5_has_five_voices_and_the_sixth_steals_one() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, Preset::P5Brass);
    // The model's own count limits a pool set for more.
    e.set_param(0, Param::Polyphony, 16.0);
    for n in [60, 64, 67, 71, 74] {
        e.note_on(0, n, 1.0);
    }
    e.render(BLOCK);
    assert_eq!(e.active_voices(), 5);
    e.note_on(0, 77, 1.0);
    e.render(BLOCK);
    assert_eq!(e.active_voices(), 5, "still five");
    assert_eq!(
        e.pools[0].held_notes(),
        vec![64, 67, 71, 74, 77],
        "the oldest, 60, was stolen"
    );
}

/// The Prophet bass is unison: one key, every voice.
#[test]
fn the_prophet_bass_plays_one_note_on_all_five_voices() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, Preset::P5Bass);
    e.note_on(0, 40, 1.0);
    e.render(BLOCK);
    assert_eq!(e.active_voices(), 5);
    assert_eq!(e.pools[0].held_notes(), vec![40; 5]);
}

/// Spec 006 Req 7: with the chorus off both sides of a Juno-106 are the same;
/// with it on they differ, and the sends and meter still work on the stereo strip.
#[test]
fn the_juno_chorus_makes_the_two_sides_differ() {
    let sides = |mode: f32| {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(0, Preset::JunoPad);
        e.set_param(0, Param::ChorusMode, mode);
        e.set_param(0, Param::AdsrAttack, 0.01);
        for n in [57, 61, 64] {
            e.note_on(0, n, 1.0);
        }
        let (mut l, mut r) = (Vec::new(), Vec::new());
        for _ in 0..150 {
            e.render(BLOCK);
            l.extend_from_slice(&e.output()[..BLOCK]);
            r.extend_from_slice(&e.output()[BLOCK..]);
            assert!(e.output().iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        }
        (l, r)
    };
    let (l, r) = sides(0.0);
    assert!(
        l.iter().any(|x| x.abs() > 0.01) && l == r,
        "mono without the chorus"
    );
    for mode in [1.0, 2.0, 3.0] {
        let (l, r) = sides(mode);
        let diff: f32 = l
            .iter()
            .zip(&r)
            .skip(4_800)
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(diff > 1.0, "mode {mode}: {diff}");
    }
}

/// The Juno-106 has six voices.
#[test]
fn the_juno_106_has_six_voices() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, Preset::JunoPoly);
    for n in [48, 52, 55, 59, 62, 65, 69] {
        e.note_on(0, n, 1.0);
    }
    e.render(BLOCK);
    assert_eq!(e.active_voices(), 6);
    assert_eq!(e.pools[0].held_notes(), vec![52, 55, 59, 62, 65, 69]);
}

/// The Jupiter-8 has eight voices.
#[test]
fn the_jupiter_8_has_eight_voices() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, Preset::JupiterBrass);
    for n in [48, 52, 55, 59, 62, 65, 69, 72, 76] {
        e.note_on(0, n, 1.0);
    }
    e.render(BLOCK);
    assert_eq!(e.active_voices(), 8);
    assert_eq!(
        e.pools[0].held_notes(),
        vec![52, 55, 59, 62, 65, 69, 72, 76]
    );
}

/// The Matrix-12 has twelve voices.
#[test]
fn the_matrix_12_has_twelve_voices() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, Preset::MatrixPad);
    let chord: Vec<u8> = (0..13).map(|k| 40 + 3 * k).collect();
    for n in &chord {
        e.note_on(0, *n, 1.0);
    }
    e.render(BLOCK);
    assert_eq!(e.active_voices(), 12);
    assert_eq!(e.pools[0].held_notes(), chord[1..].to_vec());
}

/// The PPG Wave has eight voices.
#[test]
fn the_ppg_wave_has_eight_voices() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, Preset::PpgSweepPad);
    for n in [48, 52, 55, 59, 62, 65, 69, 72, 76] {
        e.note_on(0, n, 1.0);
    }
    e.render(BLOCK);
    assert_eq!(e.active_voices(), 8);
    assert_eq!(
        e.pools[0].held_notes(),
        vec![52, 55, 59, 62, 65, 69, 72, 76]
    );
}

/// A D-50 voice set up with the given parameters, playing note `note`; its left
/// channel for `blocks` blocks.
fn d50_note(settings: &[(Param, f32)], note: u8, blocks: usize) -> Vec<f32> {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    for (p, v) in [
        (Param::Model, 12.0),
        (Param::Polyphony, 16.0),
        (Param::Cutoff, 20_000.0),
        (Param::P2Cutoff, 20_000.0),
    ]
    .iter()
    .chain(settings)
    {
        e.set_param(0, *p, *v);
    }
    e.note_on(0, note, 1.0);
    let mut out = Vec::new();
    for _ in 0..blocks {
        e.render(BLOCK);
        out.extend_from_slice(&e.output()[..BLOCK]);
    }
    out
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

/// Spec 006 Req 12: a PCM attack sounds in the first tens of milliseconds and the
/// synthesised body carries on after it.
#[test]
fn the_d50_attack_sounds_first_and_the_body_carries_on() {
    let with_attack = d50_note(
        &[
            (Param::Pcm1Sample, 7.0),
            (Param::Vco1Level, 1.0),
            (Param::AdsrAttack, 0.001),
            (Param::AdsrDecay, 0.1),
            (Param::AdsrSustain, 0.0),
            (Param::Vco2Level, 0.15),
            (Param::P2AdsrAttack, 0.001),
            (Param::P2AdsrSustain, 1.0),
        ],
        48,
        300,
    );
    let (early, late) = (rms(&with_attack[..2_000]), rms(&with_attack[24_000..]));
    assert!(
        early > 2.5 * late,
        "the attack stands out over the body: {early} vs {late}"
    );
    assert!(late > 0.005, "and the body carries on: {late}");
    // Without the attack the body is steady from the first moments.
    let body = d50_note(
        &[
            (Param::Vco1Level, 0.0),
            (Param::Vco2Level, 0.15),
            (Param::P2AdsrAttack, 0.001),
            (Param::P2AdsrSustain, 1.0),
        ],
        48,
        300,
    );
    let ratio = rms(&body[2_000..6_000]) / rms(&body[24_000..]);
    assert!((0.7..1.4).contains(&ratio), "steady: {ratio}");
}

/// The pair adds, rings or syncs: ring needs both partials, sync makes partial 2
/// repeat with partial 1.
#[test]
fn the_d50_partials_add_ring_and_sync() {
    let both = [
        (Param::Vco1Level, 0.8),
        (Param::Vco2Level, 0.8),
        (Param::Vco2Coarse, 7.0),
    ];
    let mut add = both.to_vec();
    add.push((Param::Structure, 0.0));
    let mut ring = both.to_vec();
    ring.push((Param::Structure, 2.0));
    let (a, r) = (d50_note(&add, 57, 60), d50_note(&ring, 57, 60));
    assert!(a != r, "ring is not add");
    // Ring with partial 2 silent is silence; add is not.
    let mut quiet = vec![
        (Param::Vco1Level, 0.8),
        (Param::Vco2Level, 0.0),
        (Param::Structure, 2.0),
    ];
    assert!(
        rms(&d50_note(&quiet, 57, 60)[4_800..]) < 1.0e-6,
        "ring of a silent partial"
    );
    quiet[2].1 = 0.0;
    assert!(
        rms(&d50_note(&quiet, 57, 60)[4_800..]) > 0.01,
        "while add keeps partial 1"
    );
    // Synced, the sound repeats with partial 1's period (220 Hz is 218.18
    // samples); added, a partial a fifth up does not.
    let repeats = |structure: f32| {
        let mut settings = both.to_vec();
        settings.push((Param::Structure, structure));
        let out = d50_note(&settings, 57, 80);
        let x = &out[6_000..];
        let norm = x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
        (216..=221)
            .map(|lag| {
                x.iter()
                    .zip(&x[lag..])
                    .map(|(a, b)| f64::from(*a) * f64::from(*b))
                    .sum::<f64>()
                    / norm
            })
            .fold(f64::MIN, f64::max)
    };
    let (synced, added) = (repeats(1.0), repeats(0.0));
    assert!(synced > 0.9, "synced repeats with partial 1: {synced}");
    assert!(added < 0.8, "added does not: {added}");
}

/// The D-50 has sixteen voices.
#[test]
fn the_d50_has_sixteen_voices() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, Preset::LaFantasia);
    let chord: Vec<u8> = (0..17).map(|k| 36 + 3 * k).collect();
    for n in &chord {
        e.note_on(0, *n, 1.0);
    }
    e.render(BLOCK);
    assert_eq!(e.active_voices(), 16);
    assert_eq!(e.pools[0].held_notes(), chord[1..].to_vec());
}

/// Changing a synth from a Mono-voice model to the D-50 plays the D-50 at the next note.
#[test]
fn a_model_change_replaces_the_voices() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.note_on(0, 57, 1.0);
    e.render(BLOCK);
    e.note_off(0, 57);
    e.preset(0, Preset::LaThumpBass);
    e.note_on(0, 45, 1.0);
    let mut heard = 0.0_f32;
    for _ in 0..40 {
        e.render(BLOCK);
        heard = heard.max(e.output().iter().fold(0.0, |m, s| m.max(s.abs())));
    }
    assert!(heard > 0.05);
    assert!(e.pools[0].held_notes().contains(&45));
}

/// The DX7 has sixteen voices, and its voices are FM voices.
#[test]
fn the_polymoog_has_sixteen_voices() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.preset(0, Preset::PolyStrings);
    for k in 0..17 {
        e.note_on(0, 36 + 3 * k, 1.0);
    }
    let mut heard = 0.0_f32;
    for _ in 0..200 {
        e.render(BLOCK);
        heard = heard.max(e.output().iter().fold(0.0, |m, s| m.max(s.abs())));
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
    assert!(heard > 0.05);
    assert_eq!(e.active_voices(), 16);
}

/// Spec 006 Req 16: the Vox Humana's resonance peak falls with the filter envelope, so
/// the strongest harmonic of a held note is higher early than late, and stands out
/// from its neighbours like a formant.
#[test]
fn the_vox_humana_resonance_peak_follows_the_filter_envelope() {
    let f0 = 440.0 * 2.0_f64.powf((48.0 - 69.0) / 12.0);
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.preset(0, Preset::VoxHumana);
    e.set_param(0, Param::Analog, 0.0);
    e.set_param(0, Param::ChorusMode, 0.0);
    e.set_param(0, Param::Vco2Level, 0.0);
    e.note_on(0, 48, 1.0);
    let mut left = Vec::new();
    for _ in 0..(48_000 * 2 / BLOCK) {
        e.render(BLOCK);
        left.extend_from_slice(&e.output()[..BLOCK]);
    }
    // The level of each harmonic of the note in a window: (the most prominent harmonic and how far it stands out, in dB).
    let peak = |from: usize, to: usize| {
        let out = &left[from..to];
        let levels: Vec<f64> = (1..=30)
            .map(|k| {
                let w = std::f64::consts::TAU * f0 * f64::from(k) / 48_000.0;
                let (mut re, mut im) = (0.0, 0.0);
                for (i, y) in out.iter().enumerate() {
                    re += f64::from(*y) * (w * i as f64).cos();
                    im += f64::from(*y) * (w * i as f64).sin();
                }
                re * re + im * im
            })
            .collect();
        // The harmonic that stands highest above the ones around it, and by how much (dB).
        let db: Vec<f64> = levels.iter().map(|l| 10.0 * l.max(1e-9).log10()).collect();
        let prominence = |k: usize| {
            let near = |j: usize| db.get(j).copied().unwrap_or(f64::MIN);
            db.get(k).copied().unwrap_or(f64::MIN)
                - 0.5 * (near(k.wrapping_sub(2)).max(-99.0) + near(k + 2).max(-99.0))
        };
        let k = (2..20).fold(2, |m, k| if prominence(k) > prominence(m) { k } else { m });
        (k + 1, prominence(k))
    };
    let (early, early_ratio) = peak(4_800, 14_400);
    let (late, _) = peak(57_600, 81_600);
    assert!(early > late, "the peak is at harmonic {early}, then {late}");
    assert!(
        early_ratio > 4.0,
        "the peak stands out by only {early_ratio}"
    );
}

#[test]
fn the_dx7_has_sixteen_voices() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.preset(0, Preset::FmElectricPiano);
    let chord: Vec<u8> = (0..17).map(|k| 40 + 3 * k).collect();
    for n in &chord {
        e.note_on(0, *n, 1.0);
    }
    let mut heard = 0.0_f32;
    for _ in 0..40 {
        e.render(BLOCK);
        heard = heard.max(e.output().iter().fold(0.0, |m, s| m.max(s.abs())));
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
    assert!(heard > 0.05);
    assert_eq!(e.active_voices(), 16);
    assert_eq!(e.pools[0].held_notes(), chord[1..].to_vec());
}

/// Notes held on a synth: its voices with a key down.
fn gated_notes(e: &Engine, synth: usize) -> usize {
    e.pools[synth].held()
}

/// Spec 006 Req 5: at most the voice budget sounds at once, across synths.
#[test]
fn the_voice_budget_caps_the_voices_across_synths() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    for synth in 0..SYNTHS {
        e.set_param(synth, Param::Polyphony, 8.0);
        for k in 0..8 {
            e.note_on(synth, 36 + 3 * k + synth as u8, 1.0);
        }
    }
    for _ in 0..100 {
        e.render(BLOCK);
        assert!(
            e.active_voices() <= VOICE_BUDGET,
            "{} voices",
            e.active_voices()
        );
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
    assert_eq!(e.active_voices(), VOICE_BUDGET, "and the cap is used");
}

/// A note past the budget takes the oldest voice in release first.
#[test]
fn a_note_at_the_budget_takes_a_released_voice_first() {
    let mut e = Engine::new(48_000.0);
    for synth in 0..SYNTHS {
        e.set_param(synth, Param::Polyphony, 4.0);
        e.set_param(synth, Param::AdsrRelease, 5.0);
    }
    // Synth 9 has room for more notes than it plays.
    e.set_param(9, Param::Polyphony, 8.0);
    for synth in 0..SYNTHS {
        for k in 0..4 {
            e.note_on(synth, 40 + 4 * k, 1.0);
        }
    }
    e.render(BLOCK);
    assert_eq!(e.active_voices(), VOICE_BUDGET);
    // Synth 5 lets a note go: it is in release, the oldest of the releasing voices.
    e.note_off(5, 40);
    e.render(BLOCK);
    e.note_on(9, 90, 1.0);
    e.render(BLOCK);
    assert_eq!(e.active_voices(), VOICE_BUDGET);
    assert_eq!(e.pools[5].held(), 3, "synth 5 kept its three held notes");
    assert_eq!(
        e.pools[5].active(),
        3,
        "and its released tail was the voice taken"
    );
    assert_eq!(e.pools[9].held(), 5, "while the new note sounds on synth 9");
}

#[test]
fn sixteen_differently_patched_synths_play_together() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    for synth in 0..SYNTHS {
        let (preset, _) = Preset::ALL[synth % Preset::ALL.len()];
        e.preset(synth, preset);
        e.note_on(synth, 36 + 3 * synth as u8, 1.0);
    }
    for _ in 0..200 {
        e.render(BLOCK);
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
    assert_eq!(e.active_voices(), SYNTHS);
}

/// Left and right peaks over `blocks` blocks of one held note.
fn side_peaks(e: &mut Engine, blocks: usize) -> (f32, f32) {
    (0..blocks).fold((0.0_f32, 0.0_f32), |(l, r), _| {
        e.render(BLOCK);
        let out = e.output();
        let side = |s: &[f32]| s.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        (l.max(side(&out[..BLOCK])), r.max(side(&out[BLOCK..])))
    })
}

#[test]
fn pan_is_equal_power() {
    // A fresh engine per pan, so each hears the same note.
    let at = |pan: f32| {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Pan, pan);
        e.note_on(0, 57, 1.0);
        side_peaks(&mut e, 40)
    };
    let (cl, cr) = at(0.0);
    assert!(cl > 0.01 && (cl - cr).abs() < 1.0e-6, "centre {cl} {cr}");
    let (l, r) = at(-1.0);
    assert!(l > 0.01 && r == 0.0, "left only: {l} {r}");
    assert!((cl / l - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.02);
    let (l, r) = at(1.0);
    assert!(l == 0.0 && r > 0.01, "right only: {l} {r}");
}

#[test]
fn fader_mute_and_solo() {
    let mut e = Engine::new(48_000.0);
    e.note_on(0, 57, 1.0);
    e.note_on(1, 64, 1.0);
    assert!(heard(&mut e, 40) > 0.01);
    e.set_param(0, Param::Level, 0.0);
    e.set_param(1, Param::Mute, 1.0);
    // A fader closing on a sounding synth ramps down across one block (#271).
    heard(&mut e, 1);
    assert_eq!(heard(&mut e, 10), 0.0, "fader 0 and a mute are silent");
    e.set_param(1, Param::Mute, 0.0);
    assert!(heard(&mut e, 10) > 0.01, "synth 1 unmuted");
    e.set_param(0, Param::Level, 1.0);
    e.set_param(0, Param::Solo, 1.0);
    e.set_param(1, Param::Level, 0.0);
    assert!(
        heard(&mut e, 10) > 0.01,
        "solo silences the others, not itself"
    );
    e.set_param(0, Param::Solo, 0.0);
    e.set_param(1, Param::Level, 1.0);
    e.set_param(1, Param::Solo, 1.0);
    e.set_param(0, Param::Level, 0.0);
    assert!(heard(&mut e, 10) > 0.01, "only the soloed synth sounds");
}

#[test]
fn sends_follow_the_fader_and_leave_the_mix_alone() {
    let mut e = Engine::new(48_000.0);
    e.note_on(0, 57, 1.0);
    e.render(BLOCK);
    let dry: Vec<f32> = e.output().to_vec();
    assert!(
        e.mixer.sends[0]
            .iter()
            .chain(&e.mixer.sends[1])
            .all(|x| *x == 0.0)
    );
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::Send1, 0.5);
    e.set_param(0, Param::Send2, 1.0);
    e.set_param(0, Param::Level, 0.5);
    e.note_on(0, 57, 1.0);
    e.render(BLOCK);
    let peak = |b: &[f32]| b.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
    let (echo, reverb) = (peak(&e.mixer.sends[0]), peak(&e.mixer.sends[1]));
    assert!(
        echo > 0.0 && (reverb / echo - 2.0).abs() < 1.0e-4,
        "{echo} {reverb}"
    );
    // Sends are taps: the dry mix only changes with the fader.
    assert!(peak(e.output()) < peak(&dry));
}

/// #144: a send taken before the fader ignores it; one switched off is
/// silent and keeps its level; mute silences both kinds.
#[test]
fn sends_before_the_fader_and_switched_off() {
    let send = |setup: &dyn Fn(&mut Engine)| {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Send1, 1.0);
        setup(&mut e);
        e.note_on(0, 57, 1.0);
        e.render(BLOCK);
        e.mixer.sends[0].iter().fold(0.0_f32, |m, x| m.max(x.abs()))
    };
    let post_full = send(&|_| {});
    let post_down = send(&|e| e.set_param(0, Param::Level, 0.0));
    let pre_down = send(&|e| {
        e.set_param(0, Param::Send1Pre, 1.0);
        e.set_param(0, Param::Level, 0.0);
    });
    assert!(
        post_full > 0.0 && post_down == 0.0,
        "post follows the fader"
    );
    assert_eq!(pre_down, post_full, "pre ignores it");
    let off = send(&|e| e.set_param(0, Param::Send1On, 0.0));
    assert_eq!(off, 0.0, "off is silent");
    let back = send(&|e| {
        e.set_param(0, Param::Send1On, 0.0);
        e.set_param(0, Param::Send1On, 1.0);
    });
    assert_eq!(back, post_full, "and keeps its level");
    let muted = send(&|e| {
        e.set_param(0, Param::Send1Pre, 1.0);
        e.set_param(0, Param::Mute, 1.0);
    });
    assert_eq!(muted, 0.0, "mute silences a pre send");
}

#[test]
fn each_send_feeds_its_own_processor_bus() {
    for (i, send) in [Param::Send1, Param::Send2, Param::Send3, Param::Send4]
        .into_iter()
        .enumerate()
    {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, send, 1.0);
        e.note_on(0, 57, 1.0);
        e.render(BLOCK);
        for (n, bus) in e.mixer.sends.iter().enumerate() {
            let peak = bus.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
            assert_eq!(peak > 0.0, n == i, "send {} on bus {n}", i + 1);
        }
    }
}

#[test]
fn strip_parameters_never_reach_the_synth() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::Level, 0.25);
    assert_eq!(e.param_value(0, Param::Level), 0.25);
    // Every strip parameter has a default, and nothing else is one.
    assert_eq!(
        Param::ALL.iter().filter(|(p, _)| p.is_strip()).count(),
        STRIP_DEFAULTS.len()
    );
    assert!(STRIP_DEFAULTS.iter().all(|(p, _)| p.is_strip()));
}

#[test]
fn sixteen_full_synths_stay_bounded_in_stereo() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    for synth in 0..SYNTHS {
        e.set_param(synth, Param::Pan, synth as f32 / 7.5 - 1.0);
        e.set_param(synth, Param::Vco2Level, 1.0);
        e.set_param(synth, Param::Vco3Level, 1.0);
        e.note_on(synth, 36 + 3 * synth as u8, 1.0);
    }
    for _ in 0..200 {
        e.render(BLOCK);
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}

#[test]
fn inserts_are_per_strip_and_in_series() {
    let play = |setup: fn(&mut Engine)| {
        let mut e = Engine::new(48_000.0);
        setup(&mut e);
        e.note_on(0, 57, 1.0);
        e.render(BLOCK);
        e.output().to_vec()
    };
    let dry = play(|_| {});
    // The same slot types in two orders sound different.
    let drive_eq = play(|e| {
        e.set_param(0, Param::I1Type, 2.0);
        e.set_param(0, Param::I1A, 0.9);
        e.set_param(0, Param::I2Type, 4.0);
        e.set_param(0, Param::I2C, 1.0);
    });
    let eq_drive = play(|e| {
        e.set_param(0, Param::I1Type, 4.0);
        e.set_param(0, Param::I1C, 1.0);
        e.set_param(0, Param::I2Type, 2.0);
        e.set_param(0, Param::I2A, 0.9);
    });
    assert!(drive_eq != dry && eq_drive != dry && drive_eq != eq_drive);
    // A neutral EQ slot changes nothing; a slot on synth 1 doesn't touch synth 0.
    assert!(play(|e| e.set_param(0, Param::I3Type, 4.0)) == dry);
    assert!(
        play(|e| {
            e.set_param(1, Param::I1Type, 3.0);
            e.set_param(1, Param::I1A, 1.0);
        }) == dry
    );
}

#[test]
fn insert_parameters_are_the_strips_own() {
    let mut e = Engine::new(48_000.0);
    e.set_param(2, Param::I2Type, 5.0);
    e.set_param(2, Param::I2B, 0.7);
    assert_eq!(e.param_value(2, Param::I2Type), 5.0);
    assert_eq!(e.param_value(0, Param::I2Type), 0.0);
    assert_eq!(e.param_value(2, Param::I2B), 0.7);
    e.reset(2);
    assert_eq!(
        e.param_value(2, Param::I2Type),
        0.0,
        "reset puts the slots back"
    );
    assert_eq!(e.param_value(2, Param::I2E), 0.0);
    assert_eq!(e.param_value(2, Param::I2A), 0.5);
}

#[test]
fn drive_shapes_the_synth_bus_only() {
    let play = |mode: f32| {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::I1Type, mode);
        e.set_param(0, Param::I1A, 1.0);
        e.set_param(1, Param::Level, 0.0);
        e.note_on(0, 57, 1.0);
        e.note_on(1, 57, 1.0);
        e.render(BLOCK);
        e.output().to_vec()
    };
    assert!(play(0.0) != play(2.0), "drive changes the sound");
    assert_eq!(play(0.0), play(0.0));
}

/// The first block of a held note on synth 0, after `setup`.
fn first_block(setup: impl FnOnce(&mut Engine)) -> Vec<f32> {
    let mut e = Engine::new(48_000.0);
    setup(&mut e);
    e.note_on(0, 57, 1.0);
    let mut all = Vec::new();
    for _ in 0..80 {
        e.render(BLOCK);
        all.extend_from_slice(e.output());
    }
    all
}

#[test]
fn the_effects_only_sound_through_a_send_and_a_return() {
    let dry = first_block(|_| {});
    // Sends up, returns at 0: nothing changes.
    let muted = first_block(|e| {
        e.set_param(0, Param::Send1, 1.0);
        e.set_param(0, Param::Send2, 1.0);
    });
    assert!(dry == muted, "a send alone is silent");
    // A return up, sends at 0: nothing changes either.
    let no_send = first_block(|e| {
        e.set_param(0, Param::P1Return, 1.0);
        e.set_param(0, Param::P2Return, 1.0);
    });
    assert!(dry == no_send, "a return alone is silent");
    let wet = first_block(|e| {
        e.set_param(0, Param::Send1, 1.0);
        e.set_param(0, Param::P1Return, 1.0);
        e.set_param(0, Param::P1A, 0.39);
        e.set_param(0, Param::Send2, 1.0);
        e.set_param(0, Param::P2Return, 1.0);
    });
    assert!(dry != wet, "both together sound");
}

#[test]
fn effect_parameters_are_global() {
    let mut e = Engine::new(48_000.0);
    e.set_param(3, Param::P1Return, 0.4);
    e.set_param(5, Param::P2A, 0.9);
    for synth in [0, 3, 15] {
        assert_eq!(e.param_value(synth, Param::P1Return), 0.4);
        assert_eq!(e.param_value(synth, Param::P2A), 0.9);
    }
    e.reset(2);
    assert_eq!(e.param_value(0, Param::P1Return), 0.4, "reset leaves them");
}

#[test]
fn sixteen_synths_through_both_effects_stay_bounded() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.set_param(0, Param::P1Return, 1.0);
    e.set_param(0, Param::P1B, 1.0);
    e.set_param(0, Param::P1D, 1.0);
    e.set_param(0, Param::P2Return, 1.0);
    e.set_param(0, Param::P2A, 1.0);
    for synth in 0..SYNTHS {
        e.set_param(synth, Param::Send1, 1.0);
        e.set_param(synth, Param::Send2, 1.0);
        e.set_param(synth, Param::I1Type, 3.0);
        e.set_param(synth, Param::I1A, 1.0);
        e.set_param(synth, Param::I2Type, 4.0);
        e.set_param(synth, Param::I2A, 0.9);
        e.set_param(synth, Param::I3Type, 5.0);
        e.set_param(synth, Param::I3A, 0.5);
        e.set_param(synth, Param::I3B, 0.6);
        e.note_on(synth, 36 + 3 * synth as u8, 1.0);
    }
    for _ in 0..600 {
        e.render(BLOCK);
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}

/// Spec 005 Req 9: 16 synths cycling through the models, each on
/// that model's first preset, play together within ±1.
#[test]
fn sixteen_synths_of_every_model_play_together() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    for synth in 0..SYNTHS {
        let (model, _) = Model::ALL[synth % Model::ALL.len()];
        let (preset, _) = Preset::ALL
            .iter()
            .find(|(p, _)| p.model() == model)
            .copied()
            .expect("every model has a preset");
        e.preset(synth, preset);
        assert_eq!(e.param_value(synth, Param::Model), model as u32 as f32);
        e.note_on(synth, 36 + 3 * synth as u8, 1.0);
    }
    for _ in 0..400 {
        e.render(BLOCK);
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
    // Every held note sounds; a drum kit's hit is a one-shot and has rung out.
    let held = (0..SYNTHS)
        .filter(|s| !Model::ALL[s % Model::ALL.len()].0.uses_drums())
        .count();
    assert_eq!(e.active_voices(), held);
}

/// Peak of a held loud note on synth 0 after `setup`.
fn loud_peak(setup: impl FnOnce(&mut Engine)) -> f32 {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.set_param(0, Param::Vco2Level, 1.0);
    e.set_param(0, Param::AdsrSustain, 1.0);
    setup(&mut e);
    e.note_on(0, 57, 1.0);
    let mut peak = 0.0_f32;
    for i in 0..400 {
        e.render(BLOCK);
        if i > 300 {
            peak = peak.max(side_peaks(&mut e, 1).0);
        }
    }
    peak
}

#[test]
fn the_master_compressor_turns_a_loud_mix_down() {
    let plain = loud_peak(|_| {});
    let squeezed = loud_peak(|e| {
        e.set_param(0, Param::CompThreshold, -30.0);
        e.set_param(0, Param::CompRatio, 8.0);
    });
    assert!(squeezed < 0.7 * plain, "{squeezed} vs {plain}");
    let mut e = Engine::new(48_000.0);
    assert_eq!(e.gain_reduction_db(), 0.0);
    e.set_param(0, Param::CompThreshold, -40.0);
    e.set_param(0, Param::CompRatio, 20.0);
    e.note_on(0, 57, 1.0);
    for _ in 0..100 {
        e.render(BLOCK);
    }
    assert!(e.gain_reduction_db() > 3.0);
}

#[test]
fn the_master_eq_shapes_the_mix_and_flat_leaves_it() {
    let flat = first_block(|_| {});
    let same = first_block(|e| e.set_param(0, Param::EqMid1Freq, 800.0));
    assert!(flat == same, "a band at 0 dB changes nothing");
    let boosted = first_block(|e| {
        e.set_param(0, Param::EqMid1Freq, 220.0);
        e.set_param(0, Param::EqMid1Gain, 12.0);
    });
    assert!(flat != boosted);
    let mut e = Engine::new(48_000.0);
    e.set_param(3, Param::EqHighGain, -6.0);
    assert_eq!(e.param_value(0, Param::EqHighGain), -6.0);
}

#[test]
fn meters_read_the_post_fader_peaks_and_start_over() {
    let mut e = Engine::new(48_000.0);
    assert!(e.meters().iter().all(|m| *m == 0.0));
    e.note_on(0, 57, 1.0);
    e.set_param(0, Param::Send1, 1.0);
    e.set_param(0, Param::P1Return, 1.0);
    e.set_param(0, Param::P1A, 0.2);
    for _ in 0..80 {
        e.render(BLOCK);
    }
    let m = *e.meters();
    assert!(m[0] > 0.05 && m[0] <= 1.0, "strip 0: {}", m[0]);
    assert_eq!(m[1], 0.0, "an idle strip reads zero");
    assert!(
        m[STRIPS] > 0.0 && m[STRIPS + 1] > 0.0,
        "master left and right"
    );
    assert!(m[STRIPS + 2] > 0.0, "the echo return has something");
    assert_eq!(m[STRIPS + 3], 0.0, "and the reverb does not");
    // The fader scales the strip's reading.
    e.clear_meters();
    assert!(e.meters().iter().all(|m| *m == 0.0));
    e.set_param(0, Param::Level, 0.25);
    e.render(BLOCK);
    assert!(e.meters()[0] < 0.5 * m[0]);
}

#[test]
fn muted_and_unsoloed_strips_read_zero() {
    let mut e = Engine::new(48_000.0);
    e.note_on(0, 57, 1.0);
    e.note_on(1, 64, 1.0);
    e.set_param(0, Param::Mute, 1.0);
    for _ in 0..40 {
        e.render(BLOCK);
    }
    assert_eq!(e.meters()[0], 0.0);
    assert!(e.meters()[1] > 0.0);
    e.set_param(0, Param::Mute, 0.0);
    e.set_param(0, Param::Solo, 1.0);
    e.clear_meters();
    for _ in 0..40 {
        e.render(BLOCK);
    }
    assert!(e.meters()[0] > 0.0);
    assert_eq!(e.meters()[1], 0.0);
}

/// Render `blocks` blocks and return the output.
fn render_out(e: &mut Engine, blocks: usize) -> Vec<f32> {
    let mut all = Vec::new();
    for _ in 0..blocks {
        e.render(BLOCK);
        all.extend_from_slice(e.output());
    }
    all
}

const GROUPS_N: usize = crate::mixer::GROUPS;
const G1: usize = SYNTHS;
const G2: usize = SYNTHS + 1;

#[test]
fn a_strip_routed_to_a_group_is_heard_through_the_group() {
    let play = |setup: fn(&mut Engine)| {
        let mut e = Engine::new(48_000.0);
        setup(&mut e);
        e.note_on(0, 57, 1.0);
        render_out(&mut e, 40)
    };
    let direct = play(|_| {});
    let via = play(|e| e.set_param(0, Param::Out, 1.0));
    assert!(direct == via, "a group at unity changes nothing");
    let quiet = play(|e| {
        e.set_param(0, Param::Out, 1.0);
        e.set_param(G1, Param::Level, 0.25);
    });
    assert!(
        quiet.iter().fold(0.0_f32, |m, x| m.max(x.abs()))
            < 0.4 * direct.iter().fold(0.0_f32, |m, x| m.max(x.abs()))
    );
    let muted = play(|e| {
        e.set_param(0, Param::Out, 1.0);
        e.set_param(G1, Param::Mute, 1.0);
    });
    assert!(
        muted.iter().all(|x| *x == 0.0),
        "a muted group silences what it carries"
    );
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::Out, 1.0);
    e.set_param(G1, Param::Pan, -1.0);
    e.note_on(0, 57, 1.0);
    let out = render_out(&mut e, 40);
    let side = |from: usize| {
        (0..40)
            .flat_map(|b| out[b * 2 * BLOCK + from..b * 2 * BLOCK + from + BLOCK].to_vec())
            .fold(0.0_f32, |m, x| m.max(x.abs()))
    };
    assert!(
        side(0) > 0.01 && side(BLOCK) == 0.0,
        "balance −1 keeps only the left"
    );
}

#[test]
fn routes_cannot_loop() {
    let mut e = Engine::new(48_000.0);
    e.set_param(G1, Param::Out, 1.0);
    assert_eq!(
        e.param_value(G1, Param::Out),
        0.0,
        "a group can't feed itself"
    );
    e.set_param(G2, Param::Out, 1.0);
    assert_eq!(e.param_value(G2, Param::Out), 0.0, "nor a lower group");
    e.set_param(G1, Param::Out, 2.0);
    assert_eq!(e.param_value(G1, Param::Out), 2.0, "but a higher one");
    e.set_param(0, Param::Out, 8.0);
    assert_eq!(e.param_value(0, Param::Out), 8.0, "any group for a synth");
    e.set_param(0, Param::Out, 99.0);
    assert_eq!(
        e.param_value(0, Param::Out),
        9.0,
        "clamped to nowhere (#161), not wrapped"
    );
}

#[test]
fn a_chain_of_groups_reaches_the_master() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::Out, 1.0);
    e.set_param(G1, Param::Out, 2.0);
    e.set_param(G2, Param::Level, 0.0);
    e.note_on(0, 57, 1.0);
    assert!(
        render_out(&mut e, 40).iter().all(|x| *x == 0.0),
        "the second group's fader closes it"
    );
    e.set_param(G2, Param::Level, 1.0);
    assert!(render_out(&mut e, 40).iter().any(|x| *x != 0.0));
    let m = *e.meters();
    assert!(m[G1] > 0.0 && m[G2] > 0.0, "both groups meter");
}

#[test]
fn solo_keeps_the_groups_in_a_soloed_path_heard() {
    let heard = |solo: usize| {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Out, 1.0);
        e.note_on(0, 57, 1.0);
        e.note_on(1, 64, 1.0);
        e.set_param(solo, Param::Solo, 1.0);
        for _ in 0..40 {
            e.render(BLOCK);
        }
        let m = *e.meters();
        (m[0] > 0.0, m[1] > 0.0, m[G1] > 0.0)
    };
    assert_eq!(
        heard(0),
        (true, false, true),
        "a soloed strip is heard through its group"
    );
    assert_eq!(
        heard(1),
        (false, true, false),
        "and the group it doesn't pass is not"
    );
    assert_eq!(
        heard(G1),
        (true, false, true),
        "a soloed group is heard with what feeds it"
    );
}

#[test]
fn a_group_has_inserts_for_the_mix_it_carries() {
    let play = |setup: fn(&mut Engine)| {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::Out, 1.0);
        e.set_param(1, Param::Out, 1.0);
        setup(&mut e);
        e.note_on(0, 57, 1.0);
        e.note_on(1, 64, 1.0);
        render_out(&mut e, 60)
    };
    let dry = play(|_| {});
    assert!(
        play(|e| e.set_param(G1, Param::I1Type, 4.0)) == dry,
        "a flat EQ on a group"
    );
    let boosted = play(|e| {
        e.set_param(G1, Param::I1Type, 4.0);
        e.set_param(G1, Param::I1C, 1.0);
        e.set_param(G1, Param::I1B, 0.4);
    });
    assert!(boosted != dry);
    let peak = |x: &[f32]| x.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    let squeezed = play(|e| {
        e.set_param(G1, Param::I1Type, 5.0);
        e.set_param(G1, Param::I1A, 0.1);
        e.set_param(G1, Param::I1B, 0.9);
    });
    assert!(
        peak(&squeezed) < 0.6 * peak(&dry),
        "{} vs {}",
        peak(&squeezed),
        peak(&dry)
    );
    // The inserts are the group's own: strips going straight to the master keep theirs.
    let strip_only = play(|e| {
        e.set_param(0, Param::I1Type, 3.0);
        e.set_param(0, Param::I1A, 1.0);
    });
    assert!(strip_only != dry);
}

#[test]
fn groups_have_sends_of_their_own() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::Out, 1.0);
    e.set_param(G1, Param::Send3, 1.0);
    e.note_on(0, 57, 1.0);
    e.render(BLOCK);
    let peak = |b: &[f32]| b.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
    assert!(peak(&e.mixer.sends[2]) > 0.0);
    assert_eq!(peak(&e.mixer.sends[0]), 0.0);
}

#[test]
fn sixteen_strips_through_eight_groups_stay_bounded() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    for synth in 0..SYNTHS {
        e.set_param(synth, Param::Out, 1.0 + (synth % GROUPS_N) as f32);
        e.set_param(synth, Param::Vco2Level, 1.0);
        e.note_on(synth, 36 + 3 * synth as u8, 1.0);
    }
    for g in 0..GROUPS_N {
        // Chain the groups: each into the next, the last into the master.
        if g + 1 < GROUPS_N {
            e.set_param(SYNTHS + g, Param::Out, (g + 2) as f32);
        }
    }
    for _ in 0..200 {
        e.render(BLOCK);
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}

#[test]
fn processors_can_be_chained_through_the_engine() {
    let play = |chained: bool| {
        let mut e = Engine::new(48_000.0);
        // P1 an echo with no return, P2 a reverb fed only by P1; the strip sends to P1 only.
        e.set_param(0, Param::Send1, 1.0);
        e.set_param(0, Param::P1Return, 0.0);
        e.set_param(0, Param::P2Return, 1.0);
        e.set_param(0, Param::P2In, if chained { 1.0 } else { 0.0 });
        // The echo's first repeat comes after 300 ms.
        e.note_on(0, 57, 1.0);
        render_out(&mut e, 400)
    };
    let alone = play(false);
    let chained = play(true);
    assert!(alone != chained, "the chain changes the mix");
    assert_eq!(Engine::new(48_000.0).param_value(0, Param::P2In), 0.0);
}

#[test]
fn chain_parameters_are_global_and_p1_has_none() {
    let mut e = Engine::new(48_000.0);
    e.set_param(3, Param::P3In, 1.0);
    assert_eq!(e.param_value(0, Param::P3In), 1.0);
    e.reset(2);
    assert_eq!(e.param_value(0, Param::P3In), 1.0, "reset leaves it");
    e.set_param(0, Param::P4In, 7.0);
    assert_eq!(e.param_value(0, Param::P4In), 1.0, "clamped");
    assert!(!Param::ALL.iter().any(|(_, n)| *n == "P1In"));
}

#[test]
fn four_chained_processors_stay_bounded() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    for (i, ty) in [(0, 4.0), (1, 3.0), (2, 1.0), (3, 2.0)] {
        let p = [Param::P1Type, Param::P2Type, Param::P3Type, Param::P4Type][i];
        let r = [
            Param::P1Return,
            Param::P2Return,
            Param::P3Return,
            Param::P4Return,
        ][i];
        e.set_param(0, p, ty);
        e.set_param(0, r, 1.0);
    }
    for p in [Param::P2In, Param::P3In, Param::P4In] {
        e.set_param(0, p, 1.0);
    }
    for synth in 0..SYNTHS {
        e.set_param(synth, Param::Send1, 1.0);
        e.note_on(synth, 36 + 3 * synth as u8, 1.0);
    }
    for _ in 0..400 {
        e.render(BLOCK);
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}

#[test]
fn compressor_parameters_are_global() {
    let mut e = Engine::new(48_000.0);
    e.set_param(4, Param::CompRatio, 6.0);
    assert_eq!(e.param_value(0, Param::CompRatio), 6.0);
    e.reset(1);
    assert_eq!(e.param_value(0, Param::CompRatio), 6.0);
}

#[test]
fn short_blocks_leave_the_tail_silent() {
    let mut e = Engine::new(48_000.0);
    e.note_on(0, 69, 1.0);
    e.render(BLOCK);
    e.render(64);
    assert!(
        e.output()
            .iter()
            .skip(64)
            .take(BLOCK - 64)
            .all(|s| *s == 0.0)
    );
}

#[test]
fn bad_sample_rate_falls_back() {
    let e = Engine::new(f32::NAN);
    assert_eq!(e.sample_rate, 48_000.0);
}

#[test]
fn an_imported_note_starts_on_its_exact_sample() {
    let mut e = Engine::new(48_000.0);
    assert_eq!(import(&mut e, &one_note(0)), Ok(1));
    e.song_play();
    // 24_000 = 187 blocks + 64 frames; Mono's VCOs lag by LATENCY.
    for _ in 0..187 {
        e.render(BLOCK);
        assert_eq!(peak(&e), 0.0);
    }
    e.render(BLOCK);
    let left = &e.output()[..BLOCK];
    assert!(left[..64].iter().all(|s| *s == 0.0));
    assert!(
        left[64..64 + LATENCY + 2].iter().any(|s| *s != 0.0),
        "the note should start at frame 64"
    );
}

#[test]
fn clock_steps_land_on_their_samples_through_render() {
    let mut e = Engine::new(48_000.0);
    e.song_play();
    let mut onsets = Vec::new();
    let mut last = None;
    // One frame at a time, so the step a frame fires is visible.
    for s in 0..48_000u64 {
        e.render(1);
        if e.clock().step() != last {
            last = e.clock().step();
            onsets.push(s);
        }
    }
    let want: Vec<u64> = (0..8).map(|k| k * 6000).collect();
    assert_eq!(onsets, want);
}

/// #295: an imported note held past the last start sounds until its end, on
/// through the empty sections after it.
#[test]
fn an_imported_held_note_rings_to_its_end() {
    // At 120 BPM a bar is 2 s: a note from 0.5 s held 12 bars.
    let held = vec![
        0x83, 0x60, 0x90, 60, 100, 0x81, 0xB4, 0x00, 0x80, 60, 0, 0x00, 0xFF, 0x2F, 0,
    ];
    let mut e = Engine::new(48_000.0);
    import(&mut e, &file(0, 480, &[held])).expect("imports");
    e.song_play();
    // Ten bars in: 20 s.
    for _ in 0..(48_000 * 20 / BLOCK) {
        e.render(BLOCK);
    }
    assert!(e.clock().playing(), "the song is still playing");
    assert!(gated(&e, Owner::Track(0)), "the note still sounds");
}

/// ADR-0022: one transport. Pause holds the place and play goes on from it;
/// stop goes back to the top.
#[test]
fn pause_holds_the_place_and_stop_goes_to_the_top() {
    let mut e = Engine::new(48_000.0);
    e.set_tempo(60.0);
    e.song_play();
    for _ in 0..100 {
        e.render(BLOCK);
    }
    // 12_800 samples at 12_000 per step: steps 0 and 1 have fired.
    assert_eq!(e.clock().step(), Some(1));
    e.song_pause();
    let at = e.clock().position();
    e.render(BLOCK);
    assert!(!e.clock().playing());
    assert_eq!((e.clock().position(), e.clock().step()), (at, Some(1)));
    e.song_play();
    for _ in 0..90 {
        e.render(BLOCK);
    }
    assert_eq!(e.clock().step(), Some(2), "on from where it paused");
    e.song_stop();
    assert_eq!((e.clock().position(), e.clock().step()), (0, None));
    e.song_play();
    e.render(BLOCK);
    assert_eq!(e.clock().step(), Some(0), "from the top again");
}

/// Pausing lets the song's notes go and leaves the keyboard's held.
#[test]
fn pause_releases_song_voices_but_not_live_ones() {
    let mut e = Engine::new(48_000.0);
    import(&mut e, &one_note(0)).expect("imports");
    e.note_on(0, 72, 1.0);
    e.song_play();
    for _ in 0..200 {
        e.render(BLOCK);
    }
    assert!(gated(&e, Owner::Track(0)), "the song's note sounds");
    e.song_pause();
    assert!(
        gated(&e, Owner::Live(0)),
        "the live Mono voice is still held"
    );
    assert!(
        !gated(&e, Owner::Track(0)),
        "the song's Mono voice is released"
    );
}

#[test]
fn bad_files_are_rejected_and_keep_the_old_song() {
    let mut e = Engine::new(48_000.0);
    import(&mut e, &one_note(0)).expect("imports");
    assert_eq!(import(&mut e, b"not midi"), Err(smf::Error::NotMidi.code()));
    assert_eq!(e.song().tracks.len(), 1);
    assert!(e.midi_buffer(MAX_MIDI + 1).is_none());
}

/// The shipped demo (tools/make_demo_mid.py) imports as four tracks, one per
/// channel, on synths 0–3.
#[test]
fn demo_file_imports() {
    let mut e = Engine::new(48_000.0);
    let tracks = import(&mut e, include_bytes!("../../../../web/public/demo.mid"));
    assert_eq!(tracks, Ok(4));
    let synths: Vec<Option<usize>> = (0..4).map(|t| e.song_routed(t)).collect();
    assert_eq!(
        synths,
        vec![Some(0), Some(1), Some(2), Some(3)],
        "one synth per track"
    );
}

/// A kit on synth `s`, master at full, and the peak of `blocks` rendered.
fn kit(s: usize) -> Engine {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.preset(s, Preset::Kit808);
    e
}

fn run(e: &mut Engine, blocks: usize) -> f32 {
    let mut heard = 0.0_f32;
    for _ in 0..blocks {
        e.render(BLOCK);
        assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        heard = heard.max(peak(e));
    }
    heard
}

/// #114: a synth slot holding the TR-808 plays its pads, one voice each,
/// and they ring out.
#[test]
fn a_kit_slot_plays_its_pads() {
    let mut e = kit(0);
    for note in [36, 38, 42] {
        e.note_on(0, note, 0.8);
    }
    assert_eq!(e.active_voices(), 3);
    assert!(run(&mut e, 20) > 0.05);
    // One-shots: a note-off changes nothing.
    e.note_off(0, 36);
    assert_eq!(e.active_voices(), 3);
    run(&mut e, 48_000 * 3 / BLOCK);
    assert_eq!(e.active_voices(), 0, "every pad has rung out");
}

#[test]
fn a_pad_hit_again_retriggers_its_own_voice() {
    let mut e = kit(0);
    for _ in 0..10 {
        e.note_on(0, 42, 0.8);
        run(&mut e, 2);
        assert_eq!(e.active_voices(), 1);
    }
    // Any other key with the closed hat's place plays it too.
    e.note_on(0, 44, 0.8);
    assert_eq!(e.active_voices(), 1);
}

#[test]
fn the_closed_hat_chokes_the_open_hat() {
    let mut e = kit(0);
    e.note_on(0, 46, 0.8);
    run(&mut e, 10);
    e.note_on(0, 42, 0.8);
    run(&mut e, 1);
    assert_eq!(e.active_voices(), 1, "only the closed hat is left");
    run(&mut e, 48_000 / 5 / BLOCK);
    assert_eq!(e.active_voices(), 0);
    // Alone, the open hat is still ringing then.
    let mut open = kit(0);
    open.note_on(0, 46, 0.8);
    run(&mut open, 11 + 48_000 / 5 / BLOCK);
    assert_eq!(open.active_voices(), 1);
}

#[test]
fn a_hard_hit_is_accented_and_the_knobs_reach_the_pads() {
    let hit = |velocity: f32, level: Option<f32>| {
        let mut e = kit(0);
        if let Some(l) = level {
            e.set_param(0, Param::BdLevel, l);
        }
        e.note_on(0, 36, velocity);
        run(&mut e, 40)
    };
    // Accent 0.5 at velocity 1, none at 0.8: 1.5 / 0.8 louder.
    let ratio = hit(1.0, None) / hit(0.8, None);
    assert!((ratio - 1.875).abs() < 1.0e-3, "{ratio}");
    assert_eq!(hit(1.0, Some(0.0)), 0.0, "a pad at level 0 is silent");
}

/// #264: the Heavy kits are their machine with a kick tuned down, driven and
/// louder in its tail.
#[test]
fn the_heavy_kits_have_a_deeper_louder_kick() {
    // The tail's loudness, tune and drive; the engine stays in here, as one
    // is large for a test thread's stack.
    let tail = |preset: Preset| {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(0, preset);
        e.note_on(0, 36, 0.8);
        run(&mut e, 48_000 / 5 / BLOCK);
        let loud = run(&mut e, 48_000 / 5 / BLOCK);
        (
            loud,
            e.param_value(0, Param::BdTune),
            e.param_value(0, Param::BdDrive),
        )
    };
    for (stock, heavy) in [
        (Preset::Kit808, Preset::Heavy808),
        (Preset::Kit909, Preset::Heavy909),
    ] {
        assert_eq!(stock.model(), heavy.model());
        let (quiet, _, _) = tail(stock);
        let (loud, tune, drive) = tail(heavy);
        assert!(loud > 1.5 * quiet, "{heavy:?}: tail {loud} vs {quiet}");
        assert!(tune < 0.0, "{heavy:?} is tuned down");
        assert!(drive > 0.0, "{heavy:?} is driven");
    }
}

/// #114: a MIDI file's channel 10, imported, plays on the slot its track is
/// routed to, when that slot holds the kit.
#[test]
fn channel_ten_plays_on_a_kit_slot() {
    let mut e = kit(2);
    assert_eq!(import(&mut e, &one_note(9)), Ok(1));
    route_track(&mut e, 0, Some(2));
    e.song_play();
    let heard = run(&mut e, 48_000 * 3 / 4 / BLOCK);
    assert!(heard > 0.05, "the kick at 0.5 s");
    assert_eq!(e.pools[0].active(), 0, "nothing on synth 0");
}

fn load_text(e: &mut Engine, text: &str) -> Result<(), SongError> {
    e.song_buffer(text.len())
        .expect("fits")
        .copy_from_slice(text.as_bytes());
    e.load_song()
}

const FOUR: &str = "tempo 120\ntrack kit drums\nfrag b = kit /16\n  bd x...x...x...x...\n";

/// #100: `bd x...x...x...x...` at 120 BPM and 48 kHz hits on 0, 24000,
/// 48000 and 72000, rendered a frame at a time.
#[test]
fn a_drum_lane_hits_on_its_exact_samples() {
    let mut e = kit(0);
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    assert_eq!(e.song_routed(0), Some(0), "the first kit plays the track");
    e.song_play();
    let mut hits = Vec::new();
    let mut count = e.note_count;
    for s in 0..96_000u64 {
        e.render(1);
        if e.note_count != count {
            count = e.note_count;
            hits.push(s);
        }
    }
    assert_eq!(hits, vec![0, 24_000, 48_000, 72_000]);
}

/// The arranger's first "+ Section" on a looping beat keeps it playing: the
/// section holds every frag and the clock keeps its place in the bar, even
/// when it was already past the new end.
#[test]
fn the_first_section_keeps_the_beat_playing() {
    let mut e = kit(0);
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    e.song_play();
    assert_eq!(
        hit_steps(&mut e, 40),
        vec![0, 4, 8, 12, 16, 20, 24, 28, 32, 36]
    );
    assert!(e.arrange_edit(1, 1, 0, 0), "a 1-bar section");
    assert_eq!(e.song().sections[0].frags, vec![0]);
    // Step 40 is step 8 of the 1-bar section: hits on 8 and 12, then the
    // arrangement (without a loop) ends.
    assert_eq!(hit_steps(&mut e, 16), vec![0, 4]);
}

/// The clock steps (at 120 BPM and 48 kHz, 6000 samples each) that start
/// a note within `steps` steps, rendered a frame at a time.
fn hit_steps(e: &mut Engine, steps: u64) -> Vec<u64> {
    let mut hits = Vec::new();
    let mut count = e.note_count;
    for s in 0..steps * 6000 {
        e.render(1);
        if e.note_count != count {
            for _ in count..e.note_count {
                hits.push(s / 6000);
            }
            count = e.note_count;
        }
    }
    hits
}

/// Spec 002 Req 4: each section plays its own fragments from its first
/// bar, on the exact sample, and the song stops after the last bar.
#[test]
fn sections_play_in_order_and_the_song_ends() {
    let mut e = kit(0);
    let text = "tempo 120\ntrack kit drums\nfrag a = kit\n  bd x...\nfrag b = kit\n  sn x.\n\
                section one 1: a\nsection two 1: b\narrange one two\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    let hits = hit_steps(&mut e, 40);
    assert_eq!(hits, vec![0, 4, 8, 12, 16, 18, 20, 22, 24, 26, 28, 30]);
    assert!(!e.clock().playing(), "the song stops after its last bar");
    assert_eq!(e.clock().position(), 0, "and goes back to the top");
}

/// A fragment starts again at each section's first bar: a long one is cut,
/// a short one loops inside it.
#[test]
fn a_fragment_restarts_with_its_section() {
    let mut e = kit(0);
    let long = format!("x{}x{}", ".".repeat(19), ".".repeat(11));
    let text = format!(
        "track kit drums\nfrag l = kit\n  bd {long}\nfrag s = kit\n  sn x..\nsection a 1: l s\narrange a a\n"
    );
    assert_eq!(load_text(&mut e, &text), Ok(()));
    e.song_play();
    let hits = hit_steps(&mut e, 32);
    // l: step 0 of each bar (its 20th step is cut); s: 0, 3, 6, 9, 12, 15 of each bar.
    let mut want = Vec::new();
    for bar in [0, 16] {
        want.extend([bar, bar, bar + 3, bar + 6, bar + 9, bar + 12, bar + 15]);
    }
    assert_eq!(hits, want);
}

/// The loop region repeats its bars; seek lands on a bar.
#[test]
fn the_loop_region_repeats_and_seek_lands_on_a_bar() {
    let mut e = kit(0);
    let text = "track kit drums\nfrag a = kit\n  bd x...............\nfrag b = kit\n  sn x...............\n\
                section one 1: a\nsection two 1: b\narrange one two one\nloop 2 2\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    hit_steps(&mut e, 16);
    assert_eq!(e.song_place(), Some((0, 15)));
    hit_steps(&mut e, 48);
    assert_eq!(e.song_place(), Some((1, 15)), "bar 2 three times over");
    assert!(e.clock().playing());
    e.song_seek_bar(2);
    let before = e.note_count;
    e.render(1);
    assert_eq!(e.note_count, before + 1, "bar 3 starts at once");
    assert_eq!(
        e.song_place(),
        Some((1, 0)),
        "inside the loop: bar 3 wraps to bar 2"
    );
}

/// ADR-0015: a scene sets its values on the first sample of its section.
#[test]
fn a_scene_lands_on_its_sections_first_sample() {
    let mut e = kit(0);
    let text = "track kit drums\nfrag b = kit\n  bd x...\nscene s: strip1.Send2 0.25, master.P2Return 0.6\n\
                section one 1: b\nsection two 1: b [s]\narrange one two\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    let before = e.param_value(0, Param::Send2);
    let mut changed = None;
    for s in 0..120_000u64 {
        e.render(1);
        if changed.is_none() && e.param_value(0, Param::Send2) != before {
            changed = Some(s);
        }
    }
    assert_eq!(changed, Some(96_000), "bar 2 begins at 16 steps of 6000");
    assert_eq!(e.param_value(0, Param::Send2), 0.25);
    assert_eq!(
        e.param_value(5, Param::P2Return),
        0.6,
        "a global reaches every row"
    );
    assert_eq!(e.take_touched() & 1, 1, "the view is told");
    assert_eq!(e.take_touched(), 0, "once");
}

/// #225: a scene's solo still reworks who is heard, now that only a
/// solo or a route does; synth 1's strip falls silent at bar 2.
#[test]
fn a_scene_solo_silences_the_other_strips() {
    let mut e = kit(0);
    e.note_on(1, 64, 1.0);
    let text = "track kit drums\nfrag b = kit\n  bd x...\nscene s: strip1.Solo 1\n\
                section one 1: b\nsection two 1: b [s]\narrange one two\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    assert_eq!(e.song_routed(0), Some(0), "the kit plays the track");
    e.song_play();
    run(&mut e, 90_000 / BLOCK);
    assert!(e.meters()[1] > 0.0, "synth 1 is heard before the scene");
    run(&mut e, 10_000 / BLOCK);
    e.clear_meters();
    run(&mut e, 4_000 / BLOCK);
    assert_eq!(e.meters()[1], 0.0, "the scene soloed strip 1 only");
}

/// A ramp climbs over its bars and reaches its end value at its end, a
/// block at a time.
#[test]
fn a_ramp_reaches_its_end_value() {
    let mut e = kit(0);
    let text = "track kit drums\nfrag b = kit\n  bd x\nauto r = strip1.Send1 ramp 0 1 /1\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    let mut last = -1.0;
    for _ in 0..(96_000 / BLOCK) {
        e.render(BLOCK);
        let v = e.param_value(0, Param::Send1);
        assert!(v >= last, "{v} after {last}");
        last = v;
    }
    assert!(last > 0.99, "{last}");
    e.render(BLOCK);
    assert!(e.param_value(0, Param::Send1) < 0.01, "and loops");
}

/// Automating a mixer level is the same as setting it by hand at that block.
#[test]
fn automation_is_bit_identical_to_a_hand_set_value() {
    let song = |auto: bool| {
        format!(
            "track kit drums\nfrag b = kit\n  bd x...\n{}",
            if auto {
                "auto l = strip1.Level 0.3 /1\n"
            } else {
                ""
            }
        )
    };
    let mut a = kit(0);
    let mut b = kit(0);
    assert_eq!(load_text(&mut a, &song(true)), Ok(()));
    assert_eq!(load_text(&mut b, &song(false)), Ok(()));
    b.set_param(0, Param::Level, 0.3);
    a.song_play();
    b.song_play();
    for _ in 0..200 {
        a.render(BLOCK);
        b.render(BLOCK);
        assert_eq!(a.output(), b.output());
    }
    assert!(a.output().iter().any(|s| *s != 0.0) || a.note_count > 0);
}

/// ADR-0019: a modulation writes its signal once per block, at the song's
/// position in bars (a bar is 96000 samples at 120 BPM).
#[test]
fn a_mod_follows_its_signal() {
    let mut e = kit(0);
    let text = "track kit drums\nfrag b = kit\n  bd x...\nmod strip1.send1 = saw.range(0, 0.5)\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.render(BLOCK);
    assert_eq!(e.param_value(0, Param::Send1), 0.0, "stopped, nothing runs");
    e.song_play();
    for k in 0..(96_000 / BLOCK) {
        e.render(BLOCK);
        let want = 0.5 * (k * BLOCK) as f32 / 96_000.0;
        let v = e.param_value(0, Param::Send1);
        assert!((v - want).abs() < 1e-4, "block {k}: {v} for {want}");
    }
    assert!(e.take_touched() & 1 == 1, "the view hears of it");
}

/// A constant modulation is the same as setting the value by hand.
#[test]
fn a_constant_mod_is_bit_identical_to_a_hand_set_value() {
    let song = |m: &str| format!("track kit drums\nfrag b = kit\n  bd x...\n{m}");
    let mut a = kit(0);
    let mut b = kit(0);
    assert_eq!(load_text(&mut a, &song("mod strip1.level = 0.3\n")), Ok(()));
    assert_eq!(load_text(&mut b, &song("")), Ok(()));
    b.set_param(0, Param::Level, 0.3);
    a.song_play();
    b.song_play();
    for _ in 0..200 {
        a.render(BLOCK);
        b.render(BLOCK);
        assert_eq!(a.output(), b.output());
    }
    assert!(a.output().iter().any(|s| *s != 0.0) || a.note_count > 0);
}

/// ADR-0019: scenes, then lanes, then modulations; the last write wins, so a
/// modulation holds its parameter against a lane and a scene on it.
#[test]
fn a_mod_writes_after_a_lane_and_a_scene() {
    let mut e = kit(0);
    let text = "track kit drums\nfrag b = kit\n  bd x...\n\
        auto l = strip1.Level 0.9 0.8 /1\nscene s: strip1.Level 1\n\
        mod strip1.level = 0.3\nsection a 1: b l [s]\narrange a a\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    for k in 0..(2 * 96_000 / BLOCK) {
        e.render(BLOCK);
        // A scene lands inside a block; the modulation takes over at the next.
        if k % (96_000 / BLOCK) != 0 {
            assert_eq!(e.param_value(0, Param::Level), 0.3, "block {k}");
        }
    }
}

/// #208's acceptance: the SuperCollider example on a fixed synth, a filter
/// swept by an LFO, renders the same twice and stays bounded.
#[test]
fn a_swept_filter_renders_deterministically() {
    let text = "tempo 120\ntrack lead synth Minimoog\nfrag r = lead\n  \"c3 eb3 g3 c4\"\n\
        mod lead.cutoff = lfo(1).exprange(100, 2000) + lfo(3).range(0, 300)\n";
    let render = || {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        assert_eq!(load_text(&mut e, text), Ok(()));
        e.song_play();
        let mut out = Vec::new();
        let mut cutoffs = Vec::new();
        let strip = e.song_routed(0).expect("routed");
        for _ in 0..(96_000 / BLOCK) {
            e.render(BLOCK);
            assert!(e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            out.extend_from_slice(e.output());
            cutoffs.push(e.param_value(strip, Param::Cutoff));
        }
        (out, cutoffs)
    };
    let (a, cut) = render();
    assert_eq!(a, render().0);
    assert!(a.iter().any(|s| *s != 0.0));
    let (lo, hi) = cut
        .iter()
        .fold((f32::MAX, 0.0_f32), |(l, h), c| (l.min(*c), h.max(*c)));
    assert!(lo < 250.0 && hi > 1900.0, "swept from {lo} to {hi}");
}

/// #208: a song loaded while the song plays takes over at the next bar;
/// until then the old one plays on. Loaded while stopped, it is there at once.
#[test]
fn a_song_loaded_while_playing_takes_over_at_the_next_bar() {
    let mut e = kit(0);
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    e.song_play();
    // The kick of FOUR on 0 and 24000; then a song with one kick a bar.
    run(&mut e, 30_000 / BLOCK);
    assert_eq!(e.note_count, 2);
    let one = FOUR.replace("x...x...x...x...", "x...............");
    assert_eq!(load_text(&mut e, &one), Ok(()));
    assert!(
        e.song_text().contains("x..............."),
        "the text is the new one"
    );
    // FOUR plays on to the bar line (48000, 72000), then the new song (96000).
    run(&mut e, (96_000 - 30_000) / BLOCK);
    assert_eq!(e.note_count, 4, "the old song to the bar");
    assert!(!e.take_taken());
    run(&mut e, 30_000 / BLOCK);
    assert_eq!(e.note_count, 5, "the new song from the bar");
    assert!(e.take_taken(), "the view hears of it");
    assert!(!e.take_taken());
    e.song_stop();
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    assert_eq!(
        e.song().frags[0].lanes[0].steps[4],
        Step::Hit,
        "stopped: at once"
    );
}

/// An edit while a song waits for the bar line edits the waiting song.
#[test]
fn an_edit_before_the_bar_line_edits_the_new_song() {
    let mut e = kit(0);
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    e.song_play();
    run(&mut e, 4);
    let two = FOUR.replace("frag b", "frag c");
    assert_eq!(load_text(&mut e, &two), Ok(()));
    assert!(e.set_step(0, 0, 1, 1));
    assert_eq!(e.song().frags[0].name, "c");
    assert!(
        e.song_text()
            .contains("frag c = kit /16\n  bd xx..x...x...x..."),
        "{}",
        e.song_text()
    );
}

/// #204: a fragment's method writes while the fragment plays, and the value
/// it found comes back when the fragment stops (ADR-0019).
#[test]
fn a_fragment_method_writes_while_its_fragment_plays() {
    let mut e = kit(0);
    let text = "track kit drums\nfrag b = kit /16 .send1(0.5)\n  bd x...\n\
        frag q = kit\n  sn x...\nsection one 1: b\nsection two 1: q\narrange one two one\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    let bar = 96_000 / BLOCK;
    run(&mut e, bar - 1);
    assert_eq!(e.param_value(0, Param::Send1), 0.5, "in its section");
    run(&mut e, 2);
    assert_eq!(e.param_value(0, Param::Send1), 0.0, "the value it found");
    run(&mut e, bar);
    assert_eq!(e.param_value(0, Param::Send1), 0.5, "and again");
}

/// A modulation puts back the value it found when the song stops, and one a
/// reload keeps keeps that value through the takeover.
#[test]
fn a_mod_puts_back_the_value_it_found() {
    let mut e = kit(0);
    e.set_param(0, Param::Send1, 0.1);
    let song =
        |v: f32| format!("track kit drums\nfrag b = kit\n  bd x...\nmod strip1.send1 = {v}\n");
    assert_eq!(load_text(&mut e, &song(0.4)), Ok(()));
    e.song_play();
    run(&mut e, 10);
    assert_eq!(e.param_value(0, Param::Send1), 0.4);
    assert_eq!(load_text(&mut e, &song(0.3)), Ok(()));
    run(&mut e, 96_000 / BLOCK);
    assert_eq!(e.param_value(0, Param::Send1), 0.3, "the new song's");
    e.song_stop();
    assert_eq!(
        e.param_value(0, Param::Send1),
        0.1,
        "not 0.4: the value first found"
    );
    // A reload that drops the modulation puts the value back on the bar line.
    e.song_play();
    run(&mut e, 10);
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    run(&mut e, 96_000 / BLOCK);
    assert_eq!(e.param_value(0, Param::Send1), 0.1);
}

/// #215: a frag plays the notes its pattern methods make, not its line.
#[test]
fn a_patterned_frag_plays_its_transformed_notes() {
    let count = |methods: &str| {
        let mut e = Engine::new(48_000.0);
        let text = format!("tempo 120\ntrack lead synth\nfrag r = lead{methods}\n  \"c4 ~\"\n");
        assert_eq!(load_text(&mut e, &text), Ok(()));
        e.song_play();
        run(&mut e, 2 * 96_000 / BLOCK);
        e.note_count
    };
    assert_eq!(count(""), 2);
    assert_eq!(count(" .fast(2)"), 4);
    assert_eq!(count(" .ply(3) .off(1/8, add(7))"), 12);
}

/// #208 stage 5: the engine names the parameters its modulations write, a
/// fragment's only while it plays, so the view can mark their knobs.
#[test]
fn the_engine_names_what_its_modulations_write() {
    let mut e = kit(0);
    let text = "track kit drums\nfrag b = kit /16 .send1(0.5)\n  bd x...\n\
        frag q = kit\n  sn x...\nmod master.p2return = 0.3\n\
        section one 1: b\nsection two 1: q\narrange one two\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    assert_eq!(e.modulated(0), None, "nothing while stopped");
    e.song_play();
    run(&mut e, 2);
    let all = |e: &Engine| (0..).map_while(|i| e.modulated(i)).collect::<Vec<_>>();
    assert_eq!(all(&e), vec![(0, Param::Send1), (0, Param::P2Return)]);
    run(&mut e, 96_000 / BLOCK);
    assert_eq!(
        all(&e),
        vec![(0, Param::P2Return)],
        "the frag's section is over"
    );
    e.song_stop();
    assert_eq!(all(&e), vec![]);
}

#[test]
fn each_lane_loops_on_its_own_length() {
    let mut e = kit(0);
    let text = "track kit drums\nfrag p = kit\n  bd x..\n  sn x...\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    // Twelve steps at 6000 samples: the kick on 0, 3, 6, 9; the snare on 0, 4, 8.
    run(&mut e, 72_000 / BLOCK);
    assert_eq!(e.note_count, 7);
}

#[test]
fn a_bad_text_is_reported_and_the_song_plays_on() {
    let mut e = kit(0);
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    let good = e.song().clone();
    let bad = "tempo 120\ntrack kit drums\nfrag b = kit\n  bd x..z\n";
    let err = load_text(&mut e, bad).expect_err("z is not a step");
    assert_eq!((err.line, err.col), (4, 9));
    assert_eq!(e.song_error(), Some(err));
    assert_eq!(e.song(), &good);
    e.song_play();
    assert!(run(&mut e, 40) > 0.05, "the old beat still plays");
    // Not UTF-8: reported where the bad byte is.
    e.song_buffer(4).expect("fits").copy_from_slice(b"\n\nab");
    e.song_buf[3] = 0xff;
    let err = e.load_song().expect_err("not UTF-8");
    assert_eq!((err.line, err.col), (3, 2));
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    assert_eq!(e.song_error(), None);
}

/// #210: each track gets a synth of its own with its patch on it; a
/// reload leaves the synth alone until the text changes the patch.
/// A drum track whose kit was picked plays the kit in the rack, a 909 or a
/// pad sampler, and does not turn the lead on synth 0 into an 808.
#[test]
fn a_picked_drum_track_plays_the_kit_in_the_rack() {
    for (kit, name) in [
        (Preset::Kit909, "Tr909"),
        (Preset::PadsLoud, "PadSampler"),
        (Preset::Kit808, "Tr808"),
    ] {
        let mut e = Engine::new(48_000.0);
        e.preset(1, kit);
        let lead = e.param_value(0, Param::Model);
        assert_eq!(load_text(&mut e, FOUR), Ok(()));
        assert_eq!(e.song_routed(0), Some(1), "{kit:?}");
        assert_eq!(
            e.param_value(0, Param::Model),
            lead,
            "{kit:?}: synth 0 is untouched"
        );
        assert_eq!(e.param_value(1, Param::Model), kit.model() as u32 as f32);
        assert!(
            e.song_text().contains(&format!("track kit drums {name} ")),
            "{}",
            e.song_text()
        );
    }
    // A kit written in the text is made, as before.
    let mut e = Engine::new(48_000.0);
    e.preset(1, Preset::Kit909);
    let text = FOUR.replace("track kit drums", "track kit drums Tr808 Kit808");
    assert_eq!(load_text(&mut e, &text), Ok(()));
    assert_eq!(e.song_routed(0), Some(0));
    assert_eq!(e.param_value(0, Param::Model), Model::Tr808 as u32 as f32);
}

#[test]
fn each_track_gets_its_own_synth_and_patch() {
    let mut e = Engine::new(48_000.0);
    let text = "setting nile = Minimoog MiniLead: Cutoff 1200\n\
                track kit drums\ntrack lead synth nile\ntrack bass synth\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    let routes: Vec<_> = (0..3).map(|t| e.song_routed(t)).collect();
    assert_eq!(routes, vec![Some(0), Some(1), Some(2)]);
    let model = |e: &Engine, s| e.param_value(s, Param::Model);
    assert_eq!(model(&e, 0), Model::Tr808 as u32 as f32);
    assert_eq!(model(&e, 1), Model::Minimoog as u32 as f32);
    assert_eq!(model(&e, 2), Model::Minimoog as u32 as f32, "MiniBass");
    assert_eq!(
        e.param_value(1, Param::Cutoff),
        1200.0,
        "the setting's change"
    );
    e.set_param(1, Param::Cutoff, 500.0);
    assert_eq!(load_text(&mut e, text), Ok(()));
    assert_eq!(
        e.param_value(1, Param::Cutoff),
        500.0,
        "an unchanged patch keeps the knob"
    );
    let changed = text.replace("Cutoff 1200", "Cutoff 2000");
    assert_eq!(load_text(&mut e, &changed), Ok(()));
    assert_eq!(
        e.param_value(1, Param::Cutoff),
        2000.0,
        "a changed patch is set again"
    );
    let swapped = changed.replace("track bass synth", "track bass synth Sh101 AcidBass");
    assert_eq!(load_text(&mut e, &swapped), Ok(()));
    assert_eq!(e.song_routed(2), Some(2), "the track keeps its synth");
    assert_eq!(model(&e, 2), Model::Sh101 as u32 as f32);
}

#[test]
fn the_song_sets_the_clock_and_tracks_find_a_kit() {
    let mut e = Engine::new(48_000.0);
    assert_eq!(
        load_text(&mut e, "tempo 90\nswing 60\ntrack kit drums\n"),
        Ok(())
    );
    assert_eq!((e.clock().tempo(), e.clock().swing()), (90.0, 60.0));
    assert_eq!(
        e.song_routed(0),
        Some(0),
        "no kit, so the first synth becomes one"
    );
    assert_eq!(e.param_value(0, Param::Model), Model::Tr808 as u32 as f32);
    e.preset(3, Preset::Kit808);
    e.preset(0, Preset::Lead);
    e.song_route(0, None);
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    assert_eq!(
        e.song_routed(0),
        Some(3),
        "a synth on the kit's model takes it"
    );
    e.song_route(0, Some(5));
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    assert_eq!(e.song_routed(0), Some(5), "a reload keeps the route");
    e.song_route(0, Some(99));
    assert_eq!(e.song_routed(0), None, "an unknown synth mutes");
}

#[test]
fn set_step_edits_the_playing_song_and_its_text() {
    let mut e = kit(0);
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    assert!(e.set_step(0, 0, 2, 2));
    assert!(e.song_text().contains("  bd x.X.x...x...x...\n"));
    assert!(!e.set_step(0, 0, 2, 9), "no level 9");
    assert!(!e.set_step(0, 1, 0, 1), "no second lane");
    e.song_play();
    run(&mut e, 12_001 / BLOCK + 1);
    assert_eq!(e.note_count, 2, "the new step at 12000 plays");
}

/// #199: the text the view gets back after a load, a grid click and a tempo change still has its comments.
#[test]
fn the_song_text_keeps_its_comments_through_edits() {
    let mut e = kit(0);
    let text = "# my beat\ntempo 120 # steady\ntrack kit drums\nfrag b = kit /16\n  # the kick\n  bd x...x...x...x...\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    assert!(e.song_text().starts_with("# my beat\ntempo 120 # steady\n"));
    assert!(e.set_step(0, 0, 2, 2));
    e.set_song_tempo(90.0);
    let out = e.song_text();
    assert!(out.starts_with("# my beat\ntempo 90 # steady\n"), "{out}");
    assert!(
        out.contains("  # the kick\n  bd x.X.x...x...x...\n"),
        "{out}"
    );
}

#[test]
fn tempo_and_swing_change_the_song_and_the_clock() {
    let mut e = kit(0);
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    e.set_song_tempo(90.5);
    e.set_song_swing(400.0);
    assert_eq!((e.clock().tempo(), e.clock().swing()), (90.5, 75.0));
    assert!(e.song_text().starts_with("tempo 90.5\nswing 75\n"));
    e.set_song_tempo(f32::NAN);
    assert_eq!(e.song().tempo, 90.5, "NaN is ignored");
}

/// A poly synth on slot 0 with the song loaded, playing; the sample at
/// which the held notes change, with what they are, over `frames`.
fn held_changes(text: &str, frames: u64) -> Vec<(u64, Vec<u8>)> {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    assert_eq!(load_text(&mut e, text), Ok(()));
    assert_eq!(e.song_routed(0), Some(0), "a synth track finds a synth");
    // After the load: the track's preset sets the synth's polyphony.
    e.set_param(0, Param::Polyphony, 8.0);
    e.song_play();
    let mut now: Vec<u8> = Vec::new();
    let mut out = Vec::new();
    for s in 0..frames {
        e.render(1);
        let held = e.pools[0].held_notes();
        if held != now {
            out.push((s, held.clone()));
            now = held;
        }
    }
    out
}

/// #163: `"c4 e4 g4 c5"` at 120 BPM and 48 kHz starts a note every beat
/// (24000 samples), each ending as the next begins.
#[test]
fn note_fragments_sound_at_their_samples_and_pitches() {
    let got = held_changes(
        "tempo 120\ntrack lead synth\nfrag r = lead\n  \"c4 e4 g4 c5\"\n",
        48_000 * 4,
    );
    assert_eq!(
        got,
        vec![
            (0, vec![60]),
            (24_000, vec![64]),
            (48_000, vec![67]),
            (72_000, vec![72]),
            (96_000, vec![60]),
            (120_000, vec![64]),
            (144_000, vec![67]),
            (168_000, vec![72]),
        ]
    );
}

/// ADR-0015 with ADR-0016: a note fragment plays only in its sections,
/// from each section's first bar. Two bars of `c4 e4` (one bar long) in
/// `b`, after a silent bar `a`: notes from 96000, starting over at
/// 192000 rather than running on from where the line would be.
#[test]
fn a_note_fragment_plays_in_its_section_from_its_start() {
    let got = held_changes(
        "tempo 120\ntrack lead synth\nfrag r = lead\n  c4:2 e4:4 g4:4\nsection a 1:\nsection b 1: r\narrange a b b\n",
        300_000,
    );
    let starts: Vec<(u64, Vec<u8>)> = got.into_iter().filter(|(_, n)| !n.is_empty()).collect();
    assert_eq!(
        starts,
        vec![
            (96_000, vec![60]),
            (144_000, vec![64]),
            (168_000, vec![67]),
            (192_000, vec![60]),
            (240_000, vec![64]),
            (264_000, vec![67]),
        ]
    );
}

/// #241: a slid note overlaps the next, so a legato glide patch slides to it
/// on one gate; without the `&` the pitch jumps.
#[test]
fn a_slide_glides_into_the_next_note() {
    let run = |line: &str| {
        let mut e = Engine::new(48_000.0);
        let text = format!("tempo 120\ntrack b synth Sh101 AcidBass\nfrag r = b\n  {line}\n");
        assert_eq!(load_text(&mut e, &text), Ok(()));
        e.song_play();
        let (mut rises, mut gate, mut between) = (0, false, false);
        for _ in 0..30_000 {
            e.render(1);
            let v = e.voice(Owner::Track(0));
            let g = v.is_some_and(|v| v.gated());
            rises += usize::from(g && !gate);
            gate = g;
            // E1 is 28, G1 is 31: a pitch between them is a glide.
            between |= v.is_some_and(|v| (28.5..30.5).contains(&v.pitch()));
        }
        (rises, between)
    };
    assert_eq!(
        run("e1:8& g1:8 r:4 r:2"),
        (1, true),
        "slid: one gate, a glide"
    );
    assert!(!run("e1:8 g1:8 r:4 r:2").1, "plain: the pitch jumps");
}

#[test]
fn a_note_lasts_its_written_length() {
    // a quarter note, then a rest: held 0 to 24000.
    let got = held_changes(
        "tempo 120\ntrack lead synth\nfrag r = lead\n  c4:4 r:4 r:2\n",
        60_000,
    );
    assert_eq!(got, vec![(0, vec![60]), (24_000, vec![])]);
}

#[test]
fn a_triplet_lands_between_the_sixteenths() {
    // three notes in a bar: ticks 0, 16 and 32, at 2000 samples a tick.
    let got = held_changes(
        "tempo 120\ntrack lead synth\nfrag r = lead\n  \"c4 d4 e4\"\n",
        96_000,
    );
    let starts: Vec<u64> = got.iter().map(|(s, _)| *s).collect();
    assert_eq!(starts, vec![0, 32_000, 64_000]);
}

#[test]
fn a_chord_uses_the_voice_pool() {
    let got = held_changes(
        "tempo 120\ntrack lead synth\nfrag r = lead\n  \"[c4,e4,g4] ~\"\n",
        60_000,
    );
    assert!(got.iter().any(|(s, n)| *s == 0 && n == &vec![60, 64, 67]));
    assert_eq!(
        got.last().map(|(s, n)| (*s, n.clone())),
        Some((48_000, vec![]))
    );
}

#[test]
fn stopping_the_song_ends_its_notes() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::Polyphony, 8.0);
    let text = "track lead synth\nfrag r = lead\n  c4:1\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    for _ in 0..4 {
        e.render(BLOCK);
    }
    assert_eq!(e.pools[0].held(), 1);
    e.song_stop();
    assert_eq!(e.pools[0].held(), 0);
}

#[test]
fn a_song_load_with_a_bad_note_keeps_the_old_one_playing() {
    let mut e = Engine::new(48_000.0);
    let good = "track lead synth\nfrag r = lead\n  c4:4\n";
    assert_eq!(load_text(&mut e, good), Ok(()));
    let bad = "track lead synth\nfrag r = lead\n  c4:4 x4:4\n";
    let err = load_text(&mut e, bad).unwrap_err();
    assert_eq!((err.line, err.col), (3, 8));
    assert_eq!(e.song_text(), Song::parse(good).unwrap().print());
}

/// #124: a song's drum track finds a pad sampler as it finds the 808, and its lanes
/// hit the pads their General MIDI notes name on the clock's steps (bd is note 36).
#[test]
fn a_song_track_plays_a_pad_sampler_on_the_clock() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.preset(2, Preset::PadsLoud);
    let tone: Vec<f32> = (0..24_000)
        .map(|i| (i as f32 / 100.0 * std::f32::consts::TAU).sin() * 0.9)
        .collect();
    let wav = crate::sample::test_wav(48_000, &tone, None);
    e.sample_buffer(wav.len())
        .expect("fits")
        .copy_from_slice(&wav);
    e.load_sample(0).expect("loads");
    e.set_pad(2, 0, crate::padsampler::PadField::Sample, 0.0);
    let text = FOUR.replace("track kit drums", "track kit drums PadSampler PadsLoud");
    assert_eq!(load_text(&mut e, &text), Ok(()));
    assert_eq!(e.song_routed(0), Some(2), "the pad sampler takes the track");
    e.song_play();
    let heard = run(&mut e, 48_000 / 2 / BLOCK);
    assert!(heard > 0.05, "the kick lane hits pad 1");
    assert_eq!(e.pools[0].active(), 0, "nothing on synth 0");
}

/// #164: a `sampler` track with lanes goes to the pad sampler and hits its pad.
#[test]
fn a_sampler_track_with_lanes_plays_the_pad_sampler() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.preset(2, Preset::PadsLoud);
    let tone: Vec<f32> = (0..24_000)
        .map(|i| (i as f32 / 100.0 * std::f32::consts::TAU).sin() * 0.9)
        .collect();
    let wav = crate::sample::test_wav(48_000, &tone, None);
    e.sample_buffer(wav.len())
        .expect("fits")
        .copy_from_slice(&wav);
    e.load_sample(0).expect("loads");
    e.set_pad(2, 0, crate::padsampler::PadField::Sample, 0.0);
    let text = FOUR.replace("track kit drums", "track kit sampler");
    assert_eq!(load_text(&mut e, &text), Ok(()));
    assert_eq!(e.song_routed(0), Some(2));
    e.song_play();
    assert!(
        run(&mut e, 48_000 / 2 / BLOCK) > 0.05,
        "the kick lane hits pad 1"
    );
}

/// #164: a `sampler` track with notes goes to the multisampler, at pitch.
#[test]
fn a_sampler_track_with_notes_plays_the_multisampler_at_pitch() {
    let mut e = Engine::new(48_000.0);
    e.preset(3, Preset::SamplerKeys);
    let text = "track keys sampler\nfrag r = keys\n  c4:1\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    assert_eq!(e.song_routed(0), Some(3));
    e.song_play();
    e.render(1);
    assert_eq!(e.pools[3].held_notes(), vec![60]);
}

/// #165: `bd euclid(3,8)` hits on steps 0, 3 and 6 of each eight, so at 120
/// BPM and 48 kHz on samples 0, 18000, 36000, then again from 48000.
#[test]
fn a_euclid_lane_plays_like_a_written_one() {
    let mut e = kit(0);
    let text = "tempo 120\ntrack kit drums\nfrag b = kit\n  bd euclid(3,8)\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    let mut hits = Vec::new();
    let mut count = e.note_count;
    for s in 0..60_000u64 {
        e.render(1);
        if e.note_count != count {
            count = e.note_count;
            hits.push(s);
        }
    }
    assert_eq!(hits, vec![0, 18_000, 36_000, 48_000]);
}

/// #165: a euclid note line plays the same events on every run.
#[test]
fn a_euclid_note_line_walks_the_scale_deterministically() {
    let text =
        "tempo 120\nscale c minor\ntrack lead synth\nfrag r = lead\n  euclid(4,8) scale c4\n";
    let a = held_changes(text, 96_000);
    let b = held_changes(text, 96_000);
    assert_eq!(a, b);
    let notes: Vec<Vec<u8>> = a
        .into_iter()
        .map(|(_, n)| n)
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(notes, vec![vec![60], vec![62], vec![63], vec![65]]);
}

const LIVE: &str =
    "tempo 120\nscale c minor\ntrack lead synth\nfrag w = lead live\n  walk(c4,8,1)\n";

/// The notes the song starts, in order, over `frames`: each new entry of
/// the note-off table is one start (a repeated note ends at another tick).
fn started(e: &mut Engine, frames: u64) -> Vec<u8> {
    let mut out = Vec::new();
    let mut seen: Vec<(u64, u8, u8)> = e.note_offs.iter().flatten().copied().collect();
    for _ in 0..frames {
        e.render(1);
        for entry in e.note_offs.iter().flatten() {
            if !seen.contains(entry) {
                seen.push(*entry);
                out.push(entry.2);
            }
        }
        seen.retain(|s| e.note_offs.iter().flatten().any(|x| x == s));
    }
    out
}

fn poly() -> Engine {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::Polyphony, 8.0);
    e
}

/// #167: a live fragment plays a new walk each bar, the same ones every run.
#[test]
fn a_live_fragment_changes_each_cycle_and_repeats_each_run() {
    let mut e = poly();
    assert_eq!(load_text(&mut e, LIVE), Ok(()));
    e.song_play();
    let played = started(&mut e, 96_000 * 3);
    let Some(Seq::Generated(call)) = e.song().frags[0].notes.as_ref().map(|n| n.seq.clone()) else {
        panic!("a generated frag");
    };
    let scale = e.song().scale;
    let mut want = Vec::new();
    for cycle in 0..3 {
        let evs = call.events(cycle_seed(call.seed(), cycle), scale.as_ref());
        want.extend(evs.iter().map(|ev| ev.note));
    }
    assert_eq!(played.len(), 24);
    assert_eq!(played, want);
    assert_ne!(played[..8], played[8..16], "the bars differ");
    let mut again = poly();
    assert_eq!(load_text(&mut again, LIVE), Ok(()));
    again.song_play();
    assert_eq!(started(&mut again, 96_000 * 3), played);
}

/// #167: nothing grows in `render`: the buffers keep the room reserved at load.
#[test]
fn a_live_fragment_does_not_grow_its_buffers() {
    let mut e = poly();
    assert_eq!(load_text(&mut e, LIVE), Ok(()));
    let room = (e.live[0].cur.capacity(), e.live[0].nxt.capacity());
    assert!(room.0 >= 8 && room.1 >= 8);
    e.song_play();
    for _ in 0..(96_000 * 6 / BLOCK) {
        e.render(BLOCK);
    }
    assert_eq!((e.live[0].cur.capacity(), e.live[0].nxt.capacity()), room);
    assert_eq!(e.live[0].cycle, Some(5));
}

/// #167: freezing prints the bar it was playing; parsing that plays the same.
#[test]
fn freezing_keeps_the_bar_that_was_playing() {
    let mut e = poly();
    assert_eq!(load_text(&mut e, LIVE), Ok(()));
    e.song_play();
    let first = started(&mut e, 96_000);
    let second = started(&mut e, 48_000); // half way into bar 2
    assert_eq!(first.len() + second.len(), 12);
    assert!(e.freeze(0));
    assert!(!e.song().frags[0].live);
    assert!(!e.song_text().contains("live") && !e.song_text().contains("walk("));
    let frozen: Vec<u8> = e.song().frags[0]
        .notes
        .as_ref()
        .unwrap()
        .events
        .iter()
        .map(|ev| ev.note)
        .collect();
    assert_eq!(&frozen[..4], &second[..], "the notes it had played so far");
    // Parsing the printed text plays the same bar, every bar.
    let text = e.song_text().to_string();
    let mut other = poly();
    assert_eq!(load_text(&mut other, &text), Ok(()));
    other.song_play();
    let again = started(&mut other, 96_000 * 2);
    assert_eq!([frozen.clone(), frozen].concat(), again);
    assert!(!e.freeze(0), "a frozen frag has no call to freeze");
}

/// #124: channel 10 plays on a drum/pad sampler slot too: pad 2 answers note 38.
#[test]
fn channel_ten_plays_on_a_pad_sampler_slot() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.preset(2, Preset::PadsLoud);
    let tone: Vec<f32> = (0..24_000)
        .map(|i| (i as f32 / 100.0 * std::f32::consts::TAU).sin() * 0.9)
        .collect();
    let wav = crate::sample::test_wav(48_000, &tone, None);
    e.sample_buffer(wav.len())
        .expect("fits")
        .copy_from_slice(&wav);
    e.load_sample(0).expect("loads");
    e.set_pad(2, 2, crate::padsampler::PadField::Sample, 0.0);
    let mut file_bytes = one_note(9);
    // The note in the file is 60; make it the snare's 38.
    let at = file_bytes
        .windows(3)
        .position(|w| w == [0x99, 60, 100])
        .expect("note on");
    file_bytes[at + 1] = 38;
    let off = file_bytes
        .windows(3)
        .position(|w| w == [0x89, 60, 0])
        .expect("note off");
    file_bytes[off + 1] = 38;
    assert_eq!(import(&mut e, &file_bytes), Ok(1));
    route_track(&mut e, 0, Some(2));
    e.song_play();
    let heard = run(&mut e, 48_000 * 3 / 4 / BLOCK);
    assert!(heard > 0.05, "the snare pad at 0.5 s");
    assert_eq!(e.pools[0].active(), 0, "nothing on synth 0");
}

/// One clap on a kit at synth 0, the meters read after `blocks`.
fn clap_meters(setup: &dyn Fn(&mut Engine)) -> (Vec<f32>, Vec<f32>) {
    let mut e = kit(0);
    setup(&mut e);
    e.clear_meters();
    e.note_on(0, 39, 0.8);
    let mut out = Vec::new();
    for _ in 0..20 {
        e.render(BLOCK);
        out.extend_from_slice(e.output());
    }
    (e.meters().to_vec(), out)
}

const GROUP_3: usize = SYNTHS + 2;

/// #162: a pad on a group is heard only through that group, and its
/// controls apply; a pad on Main sounds exactly as before.
#[test]
fn a_pad_on_a_group_goes_only_through_that_group() {
    let (main_meters, main_out) = clap_meters(&|_| {});
    assert!(main_meters[0] > 0.0 && main_meters[GROUP_3] == 0.0);
    let (meters, out) = clap_meters(&|e| e.set_param(0, Param::CpOut, 3.0));
    assert_eq!(meters[0], 0.0, "not on the kit's strip");
    assert!(meters[GROUP_3] > 0.0, "on group 3");
    assert!(out.iter().any(|x| *x != 0.0) && out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    // The group's fader and mute apply; the kit's own mute does not.
    let (_, down) = clap_meters(&|e| {
        e.set_param(0, Param::CpOut, 3.0);
        e.set_param(GROUP_3, Param::Level, 0.0);
    });
    assert!(down.iter().all(|x| *x == 0.0));
    let (_, kit_muted) = clap_meters(&|e| {
        e.set_param(0, Param::CpOut, 3.0);
        e.set_param(0, Param::Mute, 1.0);
    });
    assert_eq!(kit_muted, out, "an individual out bypasses the kit's strip");
    // Back on Main, bit for bit as a fresh kit.
    let (_, back) = clap_meters(&|e| {
        e.set_param(0, Param::CpOut, 3.0);
        e.set_param(0, Param::CpOut, 0.0);
    });
    assert_eq!(back, main_out);
}

#[test]
fn a_pad_is_panned_into_its_group() {
    let (_, hard_left) = clap_meters(&|e| {
        e.set_param(0, Param::CpOut, 3.0);
        e.set_param(0, Param::CpPan, -1.0);
    });
    let right: Vec<f32> = hard_left
        .chunks(BLOCK)
        .skip(1)
        .step_by(2)
        .flatten()
        .copied()
        .collect();
    let left: Vec<f32> = hard_left
        .chunks(BLOCK)
        .step_by(2)
        .flatten()
        .copied()
        .collect();
    assert!(left.iter().any(|x| *x != 0.0));
    assert!(
        right.iter().all(|x| x.abs() < 1.0e-6),
        "nothing on the right"
    );
}

/// #220: the groups a pad sampler's pads go to are what the solos follow; clearing the pads clears them.
#[test]
fn a_pad_samplers_outs_are_reported_for_the_solos() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, crate::mono::preset::Preset::PadsLoud);
    e.set_pad(0, 0, PadField::Out, 3.0);
    e.set_pad(0, 4, PadField::Out, 1.0);
    assert_eq!(e.synths[0].pad_groups(), 0b101);
    e.clear_pads(0);
    assert_eq!(e.synths[0].pad_groups(), 0);
}

#[test]
fn solos_follow_a_kits_individual_outs() {
    let heard = |setup: &dyn Fn(&mut Engine)| clap_meters(setup).1.iter().any(|x| *x != 0.0);
    assert!(
        heard(&|e| {
            e.set_param(0, Param::CpOut, 3.0);
            e.set_param(0, Param::Solo, 1.0);
        }),
        "soloing the kit keeps the group its clap goes to"
    );
    assert!(
        !heard(&|e| {
            e.set_param(0, Param::CpOut, 3.0);
            e.set_param(1, Param::Solo, 1.0);
        }),
        "soloing another synth silences it"
    );
    assert!(
        heard(&|e| {
            e.set_param(0, Param::CpOut, 3.0);
            e.set_param(GROUP_3, Param::Solo, 1.0);
        }),
        "soloing the group plays it"
    );
}

/// A carrier on synth 0 with a vocoder keyed to synth 1, both playing.
fn vocoded(setup: &dyn Fn(&mut Engine)) -> Vec<f32> {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.set_param(0, Param::I1Type, 6.0);
    e.set_param(0, Param::Key, 2.0);
    setup(&mut e);
    e.note_on(0, 48, 1.0);
    e.note_on(1, 60, 1.0);
    let mut out = Vec::new();
    for _ in 0..40 {
        e.render(BLOCK);
        assert!(e.output().iter().all(|x| x.is_finite()));
        out.extend_from_slice(e.output());
    }
    out
}

fn loud(x: &[f32]) -> f32 {
    x.iter().fold(0.0_f32, |m, v| m.max(v.abs()))
}

/// #161: the vocoder follows its key's raw signal, which a muted or
/// unrouted key strip still gives; without a key, or keyed to itself, it is silent.
#[test]
fn a_vocoder_follows_its_key_even_muted_or_routed_nowhere() {
    let both = vocoded(&|_| {});
    assert!(loud(&both) > 0.01);
    let muted_key = vocoded(&|e| e.set_param(1, Param::Mute, 1.0));
    assert!(loud(&muted_key) > 0.01, "the key is read before its mute");
    let hidden_key = vocoded(&|e| e.set_param(1, Param::Out, 9.0));
    assert!(loud(&hidden_key) > 0.01, "and before its Out");
    // With the key muted only the vocoded carrier is heard.
    let silent_key = vocoded(&|e| {
        e.set_param(1, Param::Mute, 1.0);
        e.set_param(0, Param::Key, 0.0);
    });
    assert!(silent_key.iter().all(|x| *x == 0.0), "no key, no sound");
    let own = vocoded(&|e| {
        e.set_param(1, Param::Mute, 1.0);
        e.set_param(0, Param::Key, 1.0);
    });
    assert!(own.iter().all(|x| *x == 0.0), "a strip cannot key itself");
}

/// #161: Out None takes a strip out of the mix, but not out of its sends.
#[test]
fn a_strip_routed_nowhere_leaves_no_trace_but_feeds_its_sends() {
    let render = |setup: &dyn Fn(&mut Engine)| {
        let mut e = Engine::new(48_000.0);
        setup(&mut e);
        e.note_on(0, 57, 1.0);
        let mut out = Vec::new();
        let mut sent = 0.0_f32;
        for _ in 0..20 {
            e.render(BLOCK);
            out.extend_from_slice(e.output());
            sent = sent.max(loud(&e.mixer.sends[0]));
        }
        (out, sent, e.meters()[0])
    };
    let (silent, _, _) = render(&|_| {});
    let (out, sent, meter) = render(&|e| {
        e.set_param(0, Param::Out, 9.0);
        e.set_param(0, Param::Send1, 1.0);
        e.set_param(0, Param::P1Type, 0.0);
    });
    assert!(loud(&silent) > 0.0);
    assert!(out.iter().all(|x| *x == 0.0), "nothing reaches the master");
    assert!(sent > 0.0, "but the send is fed");
    assert!(meter > 0.0, "and the strip's meter shows it");
}

/// #148: a beat written for the 808, pads the 909 lacks included, plays on a
/// TR-909 slot; every hit is heard and nothing passes full scale.
#[test]
fn an_808_beat_plays_on_a_909() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    e.preset(0, Preset::Kit909);
    let beat = "tempo 120\ntrack kit drums\nfrag b = kit /16\n  bd x...\n  sn .x..\n  cl ..x.\n  cb ...x\n  ma x...\n  lc .x..\n  cy ..x.\n  cr ...X\n";
    assert_eq!(load_text(&mut e, beat), Ok(()));
    assert_eq!(
        e.song_routed(0),
        Some(0),
        "the first kit, a 909, plays the track"
    );
    e.song_play();
    let heard = run(&mut e, 48_000 / BLOCK);
    assert!(heard > 0.05);
    // One second at 120 BPM is eight sixteenths: each four-step lane twice.
    assert_eq!(e.note_count, 8 * 2, "eight lanes, one hit each per pass");
}

/// Press C-E-G on synth 0 with its arp on, then render one frame at a
/// time and return (sample, note) for every gate rising and (sample) for
/// every fall.
fn arp_run(e: &mut Engine, frames: u64) -> (Vec<(u64, u8)>, Vec<u64>) {
    let (mut ons, mut offs) = (Vec::new(), Vec::new());
    let mut gate = false;
    for s in 0..frames {
        e.render(1);
        let v = e.voice(Owner::Live(0));
        let g = v.is_some_and(|v| v.gated());
        if g && !gate {
            ons.push((s, v.map_or(0, |v| v.note())));
        }
        if !g && gate {
            offs.push(s);
        }
        gate = g;
    }
    (ons, offs)
}

fn arp_on(e: &mut Engine) {
    e.set_param(0, Param::ArpOn, 1.0);
    for n in [60, 64, 67] {
        e.note_on(0, n, 1.0);
    }
}

#[test]
fn the_arp_steps_on_the_clock_from_the_next_step() {
    let mut e = Engine::new(48_000.0);
    e.song_play();
    e.render(1); // step 0 has fired; the next is at 6000
    arp_on(&mut e);
    let (ons, _) = arp_run(&mut e, 29_000);
    let at: Vec<u64> = ons.iter().map(|(s, _)| s + 1).collect();
    assert_eq!(at, [6000, 12_000, 18_000, 24_000]);
    let notes: Vec<u8> = ons.iter().map(|(_, n)| *n).collect();
    assert_eq!(notes, [60, 64, 67, 60]);
}

#[test]
fn the_gate_is_a_fraction_of_the_step() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::ArpRate, 0.0); // 1/8: 12000 samples
    e.set_param(0, Param::ArpGate, 0.5);
    e.song_play();
    e.render(1);
    arp_on(&mut e);
    let (ons, offs) = arp_run(&mut e, 14_000);
    assert_eq!(ons.first().map(|(s, _)| s + 1), Some(12_000));
    assert_eq!(offs.len(), 0, "the first note is still on at 14000");
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::ArpRate, 0.0);
    e.set_param(0, Param::ArpGate, 0.5);
    e.song_play();
    e.render(1);
    arp_on(&mut e);
    let (_, offs) = arp_run(&mut e, 24_000);
    // On at 12000 for half a step: the gate ends 6000 samples later (the
    // voice's release shows as the gate dropping).
    assert_eq!(offs.first().map(|s| s + 1), Some(18_000));
}

#[test]
fn latch_keeps_playing_after_the_keys_go() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::ArpLatch, 1.0);
    e.song_play();
    e.render(1);
    arp_on(&mut e);
    for n in [60, 64, 67] {
        e.note_off(0, n);
    }
    let (ons, _) = arp_run(&mut e, 20_000);
    assert_eq!(ons.len(), 3, "{ons:?}");
    // A new chord replaces it.
    e.note_on(0, 72, 1.0);
    let (ons, _) = arp_run(&mut e, 20_000);
    // The first entry is the old chord's note still gated when the run begins.
    assert!(ons.iter().skip(1).all(|(_, n)| *n == 72), "{ons:?}");
}

#[test]
fn a_stopped_clock_silences_the_arp_unless_it_runs_free() {
    let mut e = Engine::new(48_000.0);
    arp_on(&mut e);
    let (ons, _) = arp_run(&mut e, 30_000);
    assert!(ons.is_empty());
    e.set_param(0, Param::ArpFree, 1.0);
    let (ons, _) = arp_run(&mut e, 30_000);
    let gaps: Vec<u64> = ons.windows(2).map(|w| w[1].0 - w[0].0).collect();
    assert!(ons.len() >= 4);
    assert!(gaps.iter().all(|g| *g == 6000), "{gaps:?}");
}

#[test]
fn turning_the_arp_off_lets_go_and_live_input_plays_again() {
    let mut e = Engine::new(48_000.0);
    e.song_play();
    arp_on(&mut e);
    for _ in 0..200 {
        e.render(BLOCK);
    }
    e.set_param(0, Param::ArpOn, 0.0);
    e.render(BLOCK);
    assert!(!e.voice(Owner::Live(0)).is_some_and(|v| v.gated()));
    e.note_on(0, 60, 1.0);
    e.render(BLOCK);
    assert!(e.voice(Owner::Live(0)).is_some_and(|v| v.gated()));
}

#[test]
fn the_arp_does_not_grow_its_buffers() {
    let mut e = Engine::new(48_000.0);
    e.song_play();
    arp_on(&mut e);
    let before = e.arps.len();
    for _ in 0..500 {
        e.render(BLOCK);
    }
    assert_eq!(e.arps.len(), before);
}

#[test]
fn every_arp_step_retriggers_even_with_legato_and_a_full_gate() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::Legato, 1.0);
    e.set_param(0, Param::AdsrAttack, 0.001);
    e.set_param(0, Param::AdsrDecay, 0.01);
    e.set_param(0, Param::AdsrSustain, 0.0);
    e.set_param(0, Param::ArpGate, 1.0);
    e.song_play();
    e.render(1);
    arp_on(&mut e);
    // The envelope has died away just before each onset and is back just after.
    let mut at = 1usize;
    for onset in [6000usize, 12_000, 18_000] {
        let mut before = 0.0_f32;
        let mut after = 0.0_f32;
        while at < onset + 200 {
            e.render(1);
            at += 1;
            let l = peak(&e);
            if at == onset - 1 {
                before = l;
            }
            after = after.max(if at > onset { l } else { 0.0 });
        }
        assert!(before < 0.001, "onset {onset}: still sounding ({before})");
        assert!(after > 0.01, "onset {onset}: not retriggered ({after})");
    }
}

/// #213: the composer picks a track's preset; the synth takes it at once and
/// the text names it.
#[test]
fn a_track_takes_a_preset_from_the_composer() {
    let mut e = Engine::new(48_000.0);
    assert_eq!(load_text(&mut e, "track lead synth\n"), Ok(()));
    let s = e.song_routed(0).expect("routed");
    assert!(e.track_edit(0, 0, Preset::MiniBass as u32));
    assert_eq!(
        e.param_value(s, Param::Model),
        Model::Minimoog as u32 as f32
    );
    assert!(
        e.song_text().contains("track lead synth Minimoog MiniBass"),
        "{}",
        e.song_text()
    );
    assert!(
        !e.track_edit(0, 0, Preset::Kit808 as u32),
        "an 808 does not play a synth track"
    );
    assert!(
        !e.track_edit(0, 3, Preset::MiniBass as u32),
        "no such track"
    );
    assert!(!e.track_edit(9, 0, 0), "no such edit");
    // The text loads back as the same song, and keeps the synth as it is.
    let text = e.song_text().to_string();
    e.set_param(s, Param::Cutoff, 777.0);
    assert_eq!(load_text(&mut e, &text), Ok(()));
    assert_eq!(e.param_value(s, Param::Cutoff), 777.0);
}

/// #213: Save as setting writes the synth's changes into the song, and a
/// track can then play that setting.
#[test]
fn a_tracks_sound_saves_as_a_setting() {
    let mut e = Engine::new(48_000.0);
    assert_eq!(
        load_text(
            &mut e,
            "track bass synth Minimoog MiniBass\ntrack lead synth\n"
        ),
        Ok(())
    );
    let s = e.song_routed(0).expect("routed");
    e.set_param(s, Param::Cutoff, 1234.0);
    e.set_param(s, Param::Level, 0.3); // a strip parameter stays out of the setting
    assert!(e.track_edit(2, 0, 0));
    let st = &e.song().settings[0];
    assert_eq!((st.name.as_str(), st.preset), ("bass", Preset::MiniBass));
    assert_eq!(st.sets, vec![(Param::Cutoff, 1234.0)]);
    assert!(
        e.song_text()
            .contains("setting bass = Minimoog MiniBass: Cutoff 1234")
    );
    assert!(e.song_text().contains("track bass synth bass"));
    assert_eq!(
        Song::parse(e.song_text()).map(|x| x.print()),
        Ok(e.song_text().to_string())
    );
    // Saved again: a second setting, named apart.
    assert!(e.track_edit(2, 0, 0));
    assert_eq!(e.song().settings[1].name, "bass2");
    // The lead plays the first setting: a Minimoog with that cutoff.
    let lead = e.song_routed(1).expect("routed");
    assert!(e.track_edit(1, 1, 0));
    assert_eq!(
        e.param_value(lead, Param::Model),
        Model::Minimoog as u32 as f32
    );
    assert_eq!(e.param_value(lead, Param::Cutoff), 1234.0);
    assert!(e.song_text().contains("track lead synth bass"));
    assert!(!e.track_edit(1, 1, 7), "no such setting");
}

/// A setting holds at most 32 changes; a synth changed further does not save.
#[test]
fn a_setting_past_its_room_is_refused() {
    let mut e = Engine::new(48_000.0);
    assert_eq!(
        load_text(&mut e, "track lead synth Minimoog MiniLead\n"),
        Ok(())
    );
    let s = e.song_routed(0).expect("routed");
    let sound: Vec<Param> = DEFAULTS
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| *p != Param::Model && !p.is_strip() && !p.is_global())
        .collect();
    let mut moved = 0;
    for p in sound {
        let before = e.param_value(s, p);
        e.set_param(s, p, before + 0.37);
        if e.param_value(s, p) != before {
            moved += 1;
        }
        if moved > crate::song::MAX_SETS {
            break;
        }
    }
    assert!(moved > crate::song::MAX_SETS);
    assert!(!e.track_edit(2, 0, 0));
    assert!(e.song().settings.is_empty());
}

/// ADR-0018: mixer lines set the strips, groups and master on load; a value
/// the text keeps holds a hand-moved fader, a changed one is set again.
#[test]
fn mixer_lines_set_the_mix_and_hold_a_hand() {
    let mut e = Engine::new(48_000.0);
    let text = "track bass synth\n\
                strip bass: Level 0.8, I1Type Overdrive, Out group1\n\
                group 1 drums: Level 0.7\n\
                master: MasterGain 0.6, P3Type Chorus\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    let s = e.song_routed(0).expect("routed");
    assert_eq!(e.param_value(s, Param::Level), 0.8);
    assert_eq!(e.param_value(s, Param::I1Type), 1.0);
    assert_eq!(e.param_value(s, Param::Out), 1.0);
    assert_eq!(e.param_value(SYNTHS, Param::Level), 0.7, "group 1");
    assert_eq!(e.param_value(0, Param::MasterGain), 0.6);
    assert_eq!(e.param_value(0, Param::P3Type), 3.0);
    // A fader moved by hand holds through an Apply that keeps its value...
    e.set_param(s, Param::Level, 0.3);
    assert_eq!(
        load_text(&mut e, &text.replace("MasterGain 0.6", "MasterGain 0.5")),
        Ok(())
    );
    assert_eq!(e.param_value(s, Param::Level), 0.3);
    assert_eq!(
        e.param_value(0, Param::MasterGain),
        0.5,
        "the changed value is set"
    );
    // ...and is set again when its own value changes.
    assert_eq!(
        load_text(&mut e, &text.replace("Level 0.8", "Level 0.9")),
        Ok(())
    );
    assert_eq!(e.param_value(s, Param::Level), 0.9);
}

/// Write mixer to song prints the mixer as lines that load back to the same mix.
#[test]
fn the_mixer_writes_itself_into_the_song() {
    let mut e = Engine::new(48_000.0);
    assert_eq!(
        load_text(
            &mut e,
            "track bass synth\ntrack kit drums\ngroup 2 verb: Level 1\n"
        ),
        Ok(())
    );
    let bass = e.song_routed(0).expect("routed");
    e.set_param(bass, Param::Pan, -0.25);
    e.set_param(bass, Param::I2Type, 5.0);
    e.set_param(9, Param::Mute, 1.0);
    e.set_param(SYNTHS + 1, Param::Out, 3.0);
    e.set_param(0, Param::P2Return, 0.4);
    e.write_mixer();
    let text = e.song_text().to_string();
    for line in [
        "strip bass: Pan -0.25, I2Type Comp",
        "strip strip10: Mute 1",
        "group 2 verb: Out group3",
        "master: P2Return 0.4",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
    assert!(
        !text.contains("strip kit"),
        "an untouched strip has no line"
    );
    // A fresh engine loads it to the same mix.
    let mut f = Engine::new(48_000.0);
    assert_eq!(load_text(&mut f, &text), Ok(()));
    let fb = f.song_routed(0).expect("routed");
    assert_eq!(f.param_value(fb, Param::Pan), -0.25);
    assert_eq!(f.param_value(fb, Param::I2Type), 5.0);
    assert_eq!(f.param_value(9, Param::Mute), 1.0);
    assert_eq!(f.param_value(SYNTHS + 1, Param::Out), 3.0);
    assert_eq!(f.param_value(0, Param::P2Return), 0.4);
}

/// ADR-0020: every Modular preset sounds, stays bounded, renders the same
/// twice, and ends after its key is let go.
#[test]
fn a_modular_voice_sounds_bounded_deterministic_and_ends() {
    use crate::mono::preset::Preset;
    for preset in [Preset::ModularBasic, Preset::ModularHoover] {
        let play = || {
            let mut e = Engine::new(48_000.0);
            e.preset(0, preset);
            e.note_on(0, 57, 1.0);
            e.note_on(0, 64, 0.8);
            let out = render_out(&mut e, 200);
            e.note_off(0, 57);
            e.note_off(0, 64);
            render_out(&mut e, 400);
            (out, e.active_voices())
        };
        let (out, left) = play();
        assert!(
            out.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
            "{preset:?}"
        );
        let peak = out.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.05, "{preset:?} is heard: {peak}");
        assert_eq!(out, play().0, "{preset:?} renders the same");
        assert_eq!(left, 0, "{preset:?} ends");
    }
}

/// `SinOsc.ar(freq)` plays the note's pitch: note 69 crosses zero upward
/// 440 times a second.
#[test]
fn a_modular_sine_plays_its_pitch() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, crate::mono::preset::Preset::ModularBasic);
    assert_eq!(e.set_code(0, &synthdef("SinOsc.ar(freq)")), Ok(()));
    e.note_on(0, 69, 1.0);
    render_out(&mut e, 40);
    let mut left = Vec::new();
    for _ in 0..(48_000 / BLOCK) {
        e.render(BLOCK);
        left.extend_from_slice(&e.output()[..BLOCK]);
    }
    let ups = left
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    let want = 440.0 * left.len() as f32 / 48_000.0;
    assert!(
        (ups as f32 - want).abs() <= 2.0,
        "{ups} crossings for {want}"
    );
}

/// A SynthDef with its own envelope ends by it, a held key or not.
#[test]
fn a_modular_voice_with_a_percussive_env_ends_while_held() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, crate::mono::preset::Preset::ModularBasic);
    assert_eq!(
        e.set_code(
            0,
            &synthdef("LFTri.ar(freq) * EnvGen.kr(Env.perc(0.002, 0.3))")
        ),
        Ok(())
    );
    e.note_on(0, 60, 1.0);
    let early = render_out(&mut e, 20);
    assert!(early.iter().any(|s| s.abs() > 0.01));
    render_out(&mut e, 48_000 / BLOCK);
    assert_eq!(e.active_voices(), 0);
}

/// ADR-0024: a Modular setting's code plays on the synth its track is
/// routed to.
#[test]
fn a_setting_code_plays_on_its_track() {
    let text = "tempo 120\nsetting beep = Modular ModularBasic\n  SynthDef(\\beep, { |freq = 440|\n    SinOsc.ar(freq) * EnvGen.kr(Env.perc(0.01, 0.3))\n  }).add;\n\
        track lead synth beep\nfrag r = lead\n  \"a4 ~ ~ ~\"\n";
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::MasterGain, 1.0);
    assert_eq!(load_text(&mut e, text), Ok(()));
    let s = e.song_routed(0).expect("routed");
    assert_eq!(e.param_value(s, Param::Model), 19.0);
    assert!(e.code(s).is_some_and(|c| c.contains("SinOsc.ar(freq)")));
    e.song_play();
    let out = render_out(&mut e, 48_000 / BLOCK);
    let left: Vec<f32> = out
        .chunks(2 * BLOCK)
        .flat_map(|b| b[..BLOCK].to_vec())
        .collect();
    let ups = left[..24_000]
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    // A perc on a4 sounds for a while and dies: crossings near 440 a second
    // while it rings, none at the end.
    assert!(ups > 50, "{ups}");
    assert!(left[44_000..].iter().all(|s| s.abs() < 1e-3));
}

/// ADR-0024: a setting's knobs start at its code's numbers when the code is
/// new or changed, and a hand on a knob holds through an unchanged reload.
#[test]
fn a_setting_knob_starts_and_holds() {
    let song = |v: &str| {
        format!(
            "setting s = Modular ModularBasic\n  SynthDef(\\s, {{ |freq = 440| SinOsc.ar(freq, 0, {v}) }}).add;\ntrack l synth s\nfrag r = l\n  \"a4\"\n"
        )
    };
    let mut e = Engine::new(48_000.0);
    assert_eq!(load_text(&mut e, &song("0.5")), Ok(()));
    let s = e.song_routed(0).expect("routed");
    let mul = e
        .patch(s)
        .and_then(|p| p.knobs.iter().find(|k| k.name == "mul"))
        .and_then(|k| Param::ctl_param(k.ctl))
        .expect("a knob for mul");
    assert_eq!(e.param_value(s, mul), 0.5);
    e.set_param(s, mul, 0.2);
    assert_eq!(load_text(&mut e, &song("0.5")), Ok(()));
    assert_eq!(e.param_value(s, mul), 0.2, "a hand holds");
    assert_eq!(load_text(&mut e, &song("0.9")), Ok(()));
    assert_eq!(e.param_value(s, mul), 0.9, "changed code starts again");
}

/// A SynthDef of `body`, with `freq` and `gate` arguments.
fn synthdef(body: &str) -> String {
    format!("SynthDef(\\t, {{ |freq = 440, gate = 1| {body} }}).add;")
}

/// A Modular synth playing a SynthDef of `body`, note `note` held for
/// `secs`: the left channel.
fn graph_out(body: &str, note: u8, secs: f32) -> Vec<f32> {
    let mut e = Engine::new(48_000.0);
    e.preset(0, crate::mono::preset::Preset::ModularBasic);
    assert_eq!(e.set_code(0, &synthdef(body)), Ok(()), "{body}");
    e.note_on(0, note, 1.0);
    left_of(&mut e, secs)
}

fn left_of(e: &mut Engine, secs: f32) -> Vec<f32> {
    let mut left = Vec::new();
    for _ in 0..(secs * 48_000.0 / BLOCK as f32) as usize {
        e.render(BLOCK);
        left.extend_from_slice(&e.output()[..BLOCK]);
    }
    left
}

/// The frequency of `x` in Hz from its first and last rising zero
/// crossings, each placed between samples: precise to a fraction of a cent.
fn pitch_of(x: &[f32]) -> f64 {
    let ups: Vec<f64> = x
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
        .map(|(i, w)| i as f64 + f64::from(w[0] / (w[0] - w[1])))
        .collect();
    match (ups.first(), ups.last()) {
        (Some(a), Some(b)) if ups.len() > 2 => (ups.len() - 1) as f64 * 48_000.0 / (b - a),
        _ => 0.0,
    }
}

/// #339: with `Analog` up, a VCO monosynth plays the same key a little
/// off each time as its oscillators drift; the Juno-106's DCOs play it in
/// tune every time, on every voice. `Analog` 0 holds every model in tune.
#[test]
fn vcos_drift_between_notes_and_dcos_do_not() {
    let spread = |model: Model, analog: f32| {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.set_param(0, Param::Model, model as u32 as f32);
        e.set_param(0, Param::Analog, analog);
        e.set_param(0, Param::Cutoff, 400.0);
        let mut hz = Vec::new();
        for _ in 0..8 {
            e.note_on(0, 57, 0.8);
            hz.push(pitch_of(&left_of(&mut e, 0.25)[2400..]));
            e.note_off(0, 57);
            left_of(&mut e, 0.6);
        }
        assert!(
            hz.iter().all(|h| (h - 220.0).abs() < 5.0),
            "{model:?}: {hz:?}"
        );
        let cents = |h: f64| 1200.0 * (h / 220.0).log2();
        let (lo, hi) = hz.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &h| {
            (lo.min(cents(h)), hi.max(cents(h)))
        });
        hi - lo
    };
    for vco in [Model::Minimoog, Model::Sh101] {
        let s = spread(vco, 1.0);
        assert!(s > 0.5, "{vco:?} drifts: {s} cents");
        assert!(spread(vco, 0.0) < 0.05, "{vco:?} at Analog 0");
    }
    assert!(
        spread(Model::Minimoog, 1.0) > spread(Model::Sh101, 1.0),
        "discrete VCOs drift more than a CEM3340"
    );
    let juno = spread(Model::Juno106, 1.0);
    assert!(
        juno < 0.05,
        "the Juno-106's DCOs stay in tune: {juno} cents"
    );
}

fn ups(x: &[f32]) -> usize {
    x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count()
}

fn level(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// `PMOsc` at index 0 is its carrier; `softclip` squares a driven sine;
/// `MoogFF` darkens a saw; `CombN` combs a pulse and stays bounded.
#[test]
fn the_modular_units_do_what_they_say() {
    let fm = graph_out("PMOsc.ar(freq, freq * 2, 0)", 69, 1.0);
    assert!(
        (ups(&fm[4800..]) as f32 - 396.0).abs() <= 2.0,
        "{}",
        ups(&fm[4800..])
    );
    let wide = graph_out("PMOsc.ar(freq, freq * 2, 3)", 69, 1.0);
    assert!(
        ups(&wide[4800..]) > 420,
        "fm adds partials: {}",
        ups(&wide[4800..])
    );
    let sine = graph_out("SinOsc.ar(freq)", 69, 0.5);
    let driven = graph_out("(SinOsc.ar(freq) * 20).softclip", 69, 0.5);
    let crest = |x: &[f32]| x.iter().fold(0.0_f32, |m, v| m.max(v.abs())) / level(x);
    assert!(crest(&sine[4800..]) > 1.35 && crest(&driven[4800..]) < 1.15);
    let saw = graph_out("Saw.ar(freq)", 69, 0.5);
    let dark = graph_out("MoogFF.ar(Saw.ar(freq), 300, 0)", 69, 0.5);
    assert!(level(&dark[4800..]) < 0.6 * level(&saw[4800..]));
    let pulse = graph_out("Pulse.ar(freq)", 57, 0.5);
    let comb = graph_out(
        "Pulse.ar(freq) + CombN.ar(Pulse.ar(freq), 0.01, 0.002, 0.04)",
        57,
        0.5,
    );
    assert_ne!(pulse, comb);
    assert!(comb.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
}

/// #216's acceptance: the gabber kick falls in pitch, is driven square, ends
/// by itself and renders the same twice.
#[test]
fn the_gabber_kick_sounds_right() {
    let play = || {
        let mut e = Engine::new(48_000.0);
        e.preset(0, crate::mono::preset::Preset::ModularKick);
        e.note_on(0, 36, 1.0);
        let out = left_of(&mut e, 0.8);
        (out, e.active_voices())
    };
    let (out, left) = play();
    assert_eq!(out, play().0, "deterministic");
    assert_eq!(left, 0, "it ends with its key held");
    let early = ups(&out[..1200]) as f32 / 1200.0;
    let late = ups(&out[4800..9600]) as f32 / 4800.0;
    assert!(early > 3.0 * late, "the pitch falls: {early} then {late}");
    let body = &out[1440..2880];
    let peak = body.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    assert!(
        peak > 0.1 && peak / level(body) < 1.25,
        "driven: {peak} {} {}",
        level(body),
        peak / level(body)
    );
    assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
}

/// #216's acceptance: the hoover's detuned pulses beat under a held chord,
/// bounded and the same twice.
#[test]
fn the_hoover_sounds_right() {
    let play = || {
        let mut e = Engine::new(48_000.0);
        e.preset(0, crate::mono::preset::Preset::ModularHoover);
        for n in [48, 55, 60] {
            e.note_on(0, n, 1.0);
        }
        left_of(&mut e, 1.0)
    };
    let out = play();
    assert_eq!(out, play(), "deterministic");
    assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    let windows: Vec<f32> = out[9600..].chunks(480).map(level).collect();
    let mean = windows.iter().sum::<f32>() / windows.len() as f32;
    let var = windows.iter().map(|w| (w - mean).powi(2)).sum::<f32>() / windows.len() as f32;
    assert!(mean > 0.02, "heard: {mean}");
    assert!(var.sqrt() / mean > 0.05, "it beats: {}", var.sqrt() / mean);
}

/// ADR-0024: saving a Modular track's sound as a setting keeps its code,
/// the knobs' values in it, and the song plays the same code back.
#[test]
fn a_saved_setting_keeps_the_code() {
    let mut e = Engine::new(48_000.0);
    let song = "track l synth Modular ModularBasic\nfrag r = l\n  \"c3\"\n";
    assert_eq!(load_text(&mut e, song), Ok(()));
    let s = e.song_routed(0).expect("routed");
    let code = "SynthDef(\\b, { |freq = 440| RLPF.ar(Saw.ar(freq), 900, 0.5) }).add;";
    assert_eq!(e.set_code(s, code), Ok(()));
    let cutoff = e
        .patch(s)
        .and_then(|p| p.knobs.iter().find(|k| k.default == 900.0))
        .and_then(|k| Param::ctl_param(k.ctl))
        .expect("a knob for the cutoff");
    e.set_param(s, cutoff, 1500.0);
    assert!(e.track_edit(2, 0, 0));
    let text = e.song_text().to_string();
    assert!(
        text.contains("setting l = Modular ModularBasic\n  SynthDef(\\b, { |freq = 440| RLPF.ar(Saw.ar(freq), 1500, 0.5) }).add;\n"),
        "{text}"
    );
    assert!(!text.contains("Ctl"), "the code holds the knobs: {text}");
    assert_eq!(load_text(&mut e, &text), Ok(()));
    assert_eq!(e.song_text(), text);
    // Another model has no code to show or save.
    e.set_param(s, Param::Model, Model::Minimoog as u32 as f32);
    assert_eq!((e.code(s), e.patch(s).is_some()), (None, false));
}

/// ADR-0024: a SynthDef set on a synth plays, its numbers are live knobs on
/// a held note, the code shows their values, and a bad edit changes nothing.
#[test]
fn a_synthdef_plays_with_live_knobs() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, crate::mono::preset::Preset::ModularBasic);
    let code = "SynthDef(\\s, { |freq = 440, gate = 1|\n    SinOsc.ar(freq, 0, 0.5)\n}).add;\n";
    assert_eq!(e.set_code(0, code), Ok(()));
    assert_eq!(e.code(0).as_deref(), Some(code));
    let mul = Param::Ctl1;
    assert_eq!(e.param_value(0, mul), 0.5);
    e.note_on(0, 69, 1.0);
    let loud = left_of(&mut e, 0.3);
    e.set_param(0, mul, 0.1);
    let soft = left_of(&mut e, 0.3);
    let peak = |x: &[f32]| x[x.len() / 2..].iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    assert!(
        peak(&soft) < 0.3 * peak(&loud),
        "live on the held note: {} {}",
        peak(&soft),
        peak(&loud)
    );
    assert_eq!(
        e.code(0).as_deref(),
        Some(code.replace("0.5)", "0.1)").as_str())
    );
    let err = e.set_code(0, "{ Saw.ar(440 }").expect_err("bad");
    assert_eq!((err.line, err.col), (1, 14));
    assert!(
        e.code(0).is_some_and(|c| c.contains("0.1)")),
        "the synth plays on as it was"
    );
    // Another model's preset drops the code.
    e.preset(0, crate::mono::preset::Preset::MiniLead);
    assert_eq!(e.code(0), None);
}

/// The cutoffs in hertz the sounding voices of the track's synth hold (ADR-0023).
fn voice_cutoffs(e: &Engine, track: usize) -> Vec<f32> {
    let s = e.song_routed(track).expect("routed");
    (0..MAX_VOICES)
        .filter_map(|i| e.pools[s].voice_value(i, Param::Cutoff))
        .map(|n| 440.0 * ((n - 69.0) / 12.0).exp2())
        .collect()
}

/// #273's acceptance: `env(perc)` on a Poly synth's cutoff restarts with each
/// note, each voice its own, while the synth's own cutoff is left alone.
#[test]
fn an_envelope_on_the_cutoff_restarts_with_each_note_of_a_poly_synth() {
    let text = "tempo 120\ntrack lead synth Juno106 JunoPad\nfrag r = lead\n  \"c3 ~ e3 ~\"\n\
        mod lead.cutoff = env(perc).exprange(200, 4000)\n";
    let mut e = Engine::new(48_000.0);
    assert_eq!(load_text(&mut e, text), Ok(()));
    let s = e.song_routed(0).expect("routed");
    let own = e.param_value(s, Param::Cutoff);
    e.song_play();
    // Up to `secs` into the song, then the brightest voice.
    let mut at = 0.0;
    let mut brightest = |e: &mut Engine, secs: f32| {
        while at < secs {
            e.render(BLOCK);
            at += BLOCK as f32 / 48_000.0;
        }
        voice_cutoffs(e, 0).into_iter().fold(0.0_f32, f32::max)
    };
    let first = brightest(&mut e, 0.01);
    assert!(first > 2500.0, "c3 starts bright: {first}");
    let late = brightest(&mut e, 0.9);
    assert!(late < 300.0, "and falls: {late}");
    let second = brightest(&mut e, 1.01);
    assert!(second > 2500.0, "e3 starts bright again: {second}");
    assert_eq!(
        e.param_value(s, Param::Cutoff),
        own,
        "the synth's cutoff is its own"
    );
    e.song_stop();
    assert!(
        voice_cutoffs(&e, 0).is_empty(),
        "stopped, the voices follow the synth"
    );
}

/// #273's acceptance: `lfo([1, 3])` runs each voice of a chord at its own rate.
#[test]
fn a_list_gives_two_held_voices_their_own_values() {
    let text = "tempo 120\ntrack lead synth Juno106 JunoPad\nfrag r = lead\n  \"[c3,e3]\"\n\
        mod lead.cutoff = lfo([1, 3]).exprange(200, 4000)\n";
    let mut e = Engine::new(48_000.0);
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    run(&mut e, 48_000 / 5 / BLOCK);
    let cut = voice_cutoffs(&e, 0);
    assert_eq!(cut.len(), 2, "two voices: {cut:?}");
    assert!((cut[0] / cut[1] - 1.0).abs() > 0.1, "each its own: {cut:?}");
}

/// ADR-0024: `Env(levels, times)` through `.midiratio` glides the pitch an
/// octave up, and `Rand` draws its number afresh per note, the same way
/// every time.
#[test]
fn a_breakpoint_envelope_and_a_random_number_per_note() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, crate::mono::preset::Preset::ModularBasic);
    let glide =
        "{ SinOsc.ar(440 * Env([-12, 0], [0.5]).kr.midiratio) * Env.asr(0.001, 1, 0.1).kr }";
    assert_eq!(e.set_code(0, glide), Ok(()));
    e.note_on(0, 69, 1.0);
    let out = left_of(&mut e, 1.0);
    let early = ups(&out[..2400]) as f32 / 0.05;
    let late = ups(&out[33_600..43_200]) as f32 / 0.2;
    assert!(
        (early - 225.0).abs() < 25.0,
        "starts an octave down: {early}"
    );
    assert!((late - 440.0).abs() < 12.0, "ends on the note: {late}");
    let pick = "{ SinOsc.ar(Rand(200, 800)) * Env.perc(0.001, 0.3).kr }";
    let notes = || {
        let mut e = Engine::new(48_000.0);
        e.preset(0, crate::mono::preset::Preset::ModularBasic);
        assert_eq!(e.set_code(0, pick), Ok(()));
        (0..3)
            .map(|_| {
                e.note_on(0, 60, 1.0);
                let o = left_of(&mut e, 0.1);
                e.note_off(0, 60);
                left_of(&mut e, 0.5);
                ups(&o)
            })
            .collect::<Vec<_>>()
    };
    let a = notes();
    assert_eq!(a, notes(), "the same way every time");
    assert!(a[0] != a[1] || a[1] != a[2], "a new number per note: {a:?}");
}

/// #216's target, A2: the SuperCollider hoover (its Splay as a Mix, no
/// reverb yet) plays a held note bounded and the same twice, sounds while
/// held, ends after its four-second release, and is capped to a few voices.
#[test]
fn the_supercollider_hoover_plays_mono() {
    let play = || {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(0, crate::mono::preset::Preset::ModularBasic);
        assert_eq!(
            e.set_code(0, crate::modular::sc::hoover::MONO_HOOVER),
            Ok(())
        );
        e.note_on(0, 57, 1.0);
        let held = left_of(&mut e, 1.0);
        e.note_off(0, 57);
        left_of(&mut e, 5.0);
        (held, e.active_voices())
    };
    let (held, after) = play();
    assert_eq!(held, play().0, "deterministic");
    assert!(held.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    let level = held[24_000..].iter().map(|v| v * v).sum::<f32>() / 24_000.0;
    assert!(level.sqrt() > 0.005, "heard: {}", level.sqrt());
    assert_eq!(after, 0, "it ends after its release");
    let cap = crate::modular::sc::compile(crate::modular::sc::hoover::MONO_HOOVER)
        .expect("builds")
        .program
        .voice_cap();
    assert!((2..=6).contains(&cap), "a few voices: {cap}");
}

/// #216's target: the SuperCollider hoover exactly as pasted, Splay and
/// FreeVerb2 included, plays a held chord on two different sides, bounded
/// and the same twice, and ends after its release (ADR-0024).
#[test]
fn the_supercollider_hoover_plays_as_pasted() {
    let play = || {
        let mut e = Engine::new(48_000.0);
        e.set_param(0, Param::MasterGain, 1.0);
        e.preset(0, crate::mono::preset::Preset::ModularBasic);
        assert_eq!(e.set_code(0, crate::modular::sc::hoover::HOOVER), Ok(()));
        for n in [57, 64] {
            e.note_on(0, n, 1.0);
        }
        let mut sides = (Vec::new(), Vec::new());
        for _ in 0..(48_000 / BLOCK) {
            e.render(BLOCK);
            sides.0.extend_from_slice(&e.output()[..BLOCK]);
            sides.1.extend_from_slice(&e.output()[BLOCK..]);
        }
        for n in [57, 64] {
            e.note_off(0, n);
        }
        left_of(&mut e, 6.0);
        (sides, e.active_voices())
    };
    let ((l, r), after) = play();
    assert_eq!((l.clone(), r.clone()), play().0, "deterministic");
    assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() <= 1.0));
    let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
    assert!(
        rms(&l[24_000..]) > 0.003 && rms(&r[24_000..]) > 0.003,
        "both sides: {} {}",
        rms(&l[24_000..]),
        rms(&r[24_000..])
    );
    let diff: f32 = l.iter().zip(&r).map(|(a, b)| (a - b).abs()).sum::<f32>() / l.len() as f32;
    assert!(diff > 1e-4, "wide, not mono: {diff}");
    assert_eq!(after, 0, "it ends after its release");
}

/// #308: A-440 sounds a 440 Hz tone with no key held, and stops when off.
#[test]
fn a440_sounds_with_no_key_held() {
    let mut e = Engine::new(48_000.0);
    e.set_param(0, Param::Model, Model::Minimoog as u32 as f32);
    e.set_param(0, Param::A440, 1.0);
    let mut left: Vec<f32> = Vec::new();
    while left.len() < 48_000 {
        e.render(BLOCK);
        left.extend_from_slice(&e.output()[..BLOCK]);
    }
    let rising = left
        .windows(2)
        .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
        .count();
    assert!((438..=442).contains(&rising), "{rising} cycles in a second");
    e.set_param(0, Param::A440, 0.0);
    for _ in 0..8 {
        e.render(BLOCK);
    }
    assert!(peak(&e) < 1e-4, "off is silent: {}", peak(&e));
}

/// #325: New starts over: an empty stopped song, defaults everywhere, silence,
/// and one Modular synth on synth 0.
#[test]
fn clear_starts_over_with_one_modular_synth() {
    let mut e = Engine::new(48_000.0);
    load_text(&mut e, FOUR).expect("parses");
    e.song_play();
    e.set_param(3, Param::Cutoff, 300.0);
    e.set_param(5, Param::Level, 0.1);
    e.set_param(0, Param::MasterGain, 0.9);
    e.preset(2, crate::mono::preset::Preset::ModularHoover);
    e.note_on(1, 60, 1.0);
    left_of(&mut e, 0.5);
    e.clear();
    assert!(!e.clock().playing(), "stopped");
    assert_eq!(e.song_text(), Song::default().print(), "empty song");
    assert_eq!(e.param_value(0, Param::Model), Model::Modular as u32 as f32);
    assert_eq!(e.param_value(2, Param::Model), Model::Arp2600 as u32 as f32);
    let fresh = Engine::new(48_000.0);
    for (s, p) in [
        (3, Param::Cutoff),
        (5, Param::Level),
        (0, Param::MasterGain),
    ] {
        assert_eq!(e.param_value(s, p), fresh.param_value(s, p), "{p:?} on {s}");
    }
    left_of(&mut e, 1.0);
    assert_eq!(e.active_voices(), 0, "every voice let go");
    left_of(&mut e, 0.1);
    assert!(peak(&e) < 1e-4, "silent: {}", peak(&e));
    e.note_on(0, 60, 1.0);
    assert!(
        left_of(&mut e, 0.3).iter().any(|v| v.abs() > 0.01),
        "synth 0 plays"
    );
}

/// #327: an imported MIDI file's parts play on the synths its text names:
/// the demo's bass on a Minimoog, its violins on Pro-Ones, synths 0 to 3.
#[test]
fn a_midi_import_sets_the_synths_its_text_names() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/demo.mid");
    let bytes = std::fs::read(path).expect("the demo MIDI file");
    let mut e = Engine::new(48_000.0);
    // Synth 2 already a Minimoog: the bass still goes to synth 0, in channel order.
    e.preset(2, crate::mono::preset::Preset::MiniLead);
    e.midi_buffer(bytes.len())
        .expect("fits")
        .copy_from_slice(&bytes);
    assert_eq!(e.import_midi(), Ok(4));
    let model = |s: usize| e.param_value(s, Param::Model);
    assert_eq!(model(0), Model::Minimoog as u32 as f32);
    for s in 1..4 {
        assert_eq!(model(s), Model::ProOne as u32 as f32, "synth {s}");
    }
    assert_eq!(
        (0..4).map(|t| e.song_routed(t)).collect::<Vec<_>>(),
        [Some(0), Some(1), Some(2), Some(3)]
    );
}

/// #329: a Modular synth hands out a knob per number of its SynthDef, with
/// its UGen, range and control; another model has none.
#[test]
fn a_modular_synth_lists_its_knobs() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, crate::mono::preset::Preset::ModularBasic);
    e.set_code(
        0,
        "SynthDef(\\a, { |freq = 440, gate = 1| RLPF.ar(Saw.ar(freq), 800, 0.3) }).add;",
    )
    .expect("builds");
    let list = e.knob_list(0).to_string();
    let rows: Vec<Vec<&str>> = list.lines().map(|l| l.split('\t').collect()).collect();
    let rlpf: Vec<&Vec<&str>> = rows.iter().filter(|r| r.get(1) == Some(&"RLPF")).collect();
    assert_eq!(rlpf.len(), 2, "{list}");
    assert!(rows.iter().all(|r| r.len() == 8), "{list}");
    let freq = rlpf
        .iter()
        .find(|r| r.get(2) == Some(&"freq"))
        .expect("a cutoff knob");
    assert_eq!(freq.get(7), Some(&"800"));
    assert_eq!(freq.get(6), Some(&"1"), "a frequency turns exponentially");
    e.preset(1, crate::mono::preset::Preset::Bass);
    assert_eq!(e.knob_list(1), "");
}

/// A Prophet-5 on synth 0, from its first preset.
fn prophet() -> Engine {
    let mut e = Engine::new(48_000.0);
    e.preset(0, Preset::P5Brass);
    e
}

/// The Prophet-5's VCO, filter and envelope switches, and its Revision.
fn revision_of(e: &Engine) -> [f32; 4] {
    [
        Param::VcoRev,
        Param::FilterRev,
        Param::EnvRev,
        Param::Revision,
    ]
    .map(|p| e.param_value(0, p))
}

/// #343: a revision sets the parts it had, SSM for Rev 1 and 2, Curtis for
/// Rev 3 and 4, and its drift: Rev 1 the most, the Rev 4 held stable.
#[test]
fn a_revision_sets_its_parts_and_drift() {
    let mut e = prophet();
    assert_eq!(revision_of(&e), [3.0, 3.0, 3.0, 3.0], "a preset is Rev 3");
    let mut drift = Vec::new();
    for (rev, part) in [(1.0, 1.0), (2.0, 1.0), (3.0, 3.0), (4.0, 3.0)] {
        e.set_param(0, Param::Revision, rev);
        assert_eq!(revision_of(&e), [part, part, part, rev], "Rev {rev}");
        drift.push(e.param_value(0, Param::Analog));
    }
    assert!(drift.windows(2).all(|d| d[1] < d[0]), "{drift:?}");
    assert_eq!(
        e.take_touched() & 1,
        1,
        "the view fetches the parts it moved"
    );
}

/// #343: turning a part away from its revision makes the Revision Custom;
/// the Vintage knob does not.
#[test]
fn a_part_turned_away_makes_it_custom() {
    let mut e = prophet();
    e.set_param(0, Param::Revision, 2.0);
    e.set_param(0, Param::Analog, 0.2);
    assert_eq!(e.param_value(0, Param::Revision), 2.0, "Vintage is free");
    e.set_param(0, Param::FilterRev, 2.0);
    assert_eq!(
        e.param_value(0, Param::Revision),
        2.0,
        "1 and 2 are one chip"
    );
    e.set_param(0, Param::VcoRev, 3.0);
    assert_eq!(revision_of(&e), [3.0, 2.0, 1.0, 0.0], "Custom");
    e.set_param(0, Param::Revision, 0.0);
    assert_eq!(revision_of(&e), [3.0, 2.0, 1.0, 0.0], "Custom sets nothing");
}

/// #343: a setup's values load the same in any order, and an old setup
/// that only has `FilterRev` keeps its sound: SSM filter, Curtis VCOs and
/// envelopes, now shown as Custom.
#[test]
fn revisions_load_in_any_order_and_old_setups_keep_their_sound() {
    let saved = [
        (Param::Revision, 0.0),
        (Param::VcoRev, 1.0),
        (Param::FilterRev, 3.0),
        (Param::EnvRev, 1.0),
    ];
    for order in [[0, 1, 2, 3], [3, 2, 1, 0], [1, 0, 3, 2]] {
        let mut e = prophet();
        for i in order {
            let (p, v) = saved[i];
            e.set_param(0, p, v);
        }
        assert_eq!(revision_of(&e), [1.0, 3.0, 1.0, 0.0], "{order:?}");
    }
    let mut e = prophet();
    e.set_param(0, Param::FilterRev, 1.0);
    assert_eq!(revision_of(&e), [3.0, 1.0, 3.0, 0.0]);
}

/// #343: on a model without the Revision switch nothing is linked: the
/// Odyssey's filter switch leaves the Revision, the Revision its drift.
#[test]
fn only_a_model_with_revisions_links_them() {
    let mut e = Engine::new(48_000.0);
    e.preset(0, Preset::CurrieLead);
    let analog = e.param_value(0, Param::Analog);
    e.set_param(0, Param::FilterRev, 1.0);
    e.set_param(0, Param::Revision, 1.0);
    assert_eq!(e.param_value(0, Param::FilterRev), 1.0);
    assert_eq!(e.param_value(0, Param::Analog), analog);
}

/// #343: the SSM2030 of Rev 1/2 lacks the CEM3340's temperature
/// compensation: with the same Vintage it plays a key further off.
#[test]
fn the_prophets_ssm_vcos_drift_more_than_its_curtis_ones() {
    let spread = |vco: f32| {
        let mut e = prophet();
        e.set_param(0, Param::MasterGain, 1.0);
        e.set_param(0, Param::VcoRev, vco);
        e.set_param(0, Param::Analog, 1.0);
        e.set_param(0, Param::Vco2Level, 0.0);
        e.set_param(0, Param::Cutoff, 400.0);
        e.set_param(0, Param::EnvCutoff, 0.0);
        let cents: Vec<f64> = (0..8)
            .map(|_| {
                e.note_on(0, 57, 0.8);
                let hz = pitch_of(&left_of(&mut e, 0.25)[2400..]);
                e.note_off(0, 57);
                left_of(&mut e, 0.6);
                1200.0 * (hz / 220.0).log2()
            })
            .collect();
        let (lo, hi) = cents
            .iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), &c| (lo.min(c), hi.max(c)));
        hi - lo
    };
    let (ssm, cem) = (spread(1.0), spread(3.0));
    assert!(ssm > 1.3 * cem && cem > 0.1, "SSM {ssm} cents, CEM {cem}");
}

/// #343: the SSM2050's attack is almost straight, the CEM3310's an RC
/// curve: a quarter into a long attack the Curtis envelope is further up.
#[test]
fn the_prophets_ssm_attack_is_straighter_than_its_curtis_one() {
    let quarter = |env: f32| {
        let mut e = prophet();
        e.set_param(0, Param::MasterGain, 1.0);
        e.set_param(0, Param::EnvRev, env);
        e.set_param(0, Param::AdsrAttack, 0.4);
        e.set_param(0, Param::AdsrSustain, 1.0);
        e.set_param(0, Param::Polyphony, 1.0);
        e.note_on(0, 57, 1.0);
        let out = left_of(&mut e, 0.4);
        let rms = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
        let n = out.len();
        rms(&out[n / 4 - 960..n / 4 + 960]) / rms(&out[n - 1920..])
    };
    let (ssm, cem) = (quarter(1.0), quarter(3.0));
    assert!(cem > ssm + 0.05, "a quarter in: SSM {ssm}, CEM {cem}");
}

/// The loudest sample of a one-bar `sn` lane on the kit, a step at a time.
fn snare_peak(lane: &str) -> f32 {
    let mut e = kit(0);
    e.set_param(0, Param::MasterGain, 1.0);
    let text = format!("tempo 120\ntrack kit drums\n\nfrag a = kit /16\n  sn {lane}\n");
    assert_eq!(load_text(&mut e, &text), Ok(()));
    e.song_play();
    let mut peak = 0.0_f32;
    for _ in 0..(12_000 / BLOCK) {
        e.render(BLOCK);
        peak = e.output().iter().fold(peak, |m, s| m.max(s.abs()));
    }
    peak
}

/// #353: a ghost note `o` plays well under a hit, as its velocity has it.
#[test]
fn a_ghost_note_is_softer_than_a_hit() {
    let (ghost, hit) = (
        snare_peak("o..............."),
        snare_peak("x..............."),
    );
    assert!(ghost > 0.0 && ghost < 0.7 * hit, "ghost {ghost}, hit {hit}");
}

/// The samples a song's notes start on within `frames`, a frame at a time.
fn starts_within(e: &mut Engine, frames: u64) -> Vec<u64> {
    let mut hits = Vec::new();
    let mut count = e.note_count;
    for s in 0..frames {
        e.render(1);
        for _ in count..e.note_count {
            hits.push(s);
        }
        count = e.note_count;
    }
    hits
}

/// #353: a lane of 12, 24, 32 or 48 steps a bar hits every bar / grid, on
/// its exact sample: at 120 BPM and 48 kHz a bar is 96 000 samples.
#[test]
fn lanes_hit_on_their_grid() {
    for grid in [12u64, 16, 24, 32, 48] {
        let mut e = kit(0);
        let lane = "x".repeat(grid as usize);
        let text = format!("tempo 120\ntrack kit drums\nfrag a = kit /{grid}\n  bd {lane}\n");
        assert_eq!(load_text(&mut e, &text), Ok(()));
        e.song_play();
        let want: Vec<u64> = (0..grid).map(|n| n * 96_000 / grid).collect();
        assert_eq!(starts_within(&mut e, 96_000), want, "/{grid}");
    }
}

/// #353: lanes on different grids keep time together, and with a note frag:
/// a triplet lane meets the 16ths on every beat, and swing moves a hit
/// between steps with its step.
#[test]
fn mixed_grids_keep_time_and_follow_swing() {
    let mut e = kit(0);
    let text = "tempo 120\ntrack kit drums\nfrag a = kit /16\n  bd x...x...x...x...\nfrag b = kit /12\n  ch x..x..x..x..\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    let starts = starts_within(&mut e, 96_000);
    assert_eq!(
        starts,
        vec![0, 0, 24_000, 24_000, 48_000, 48_000, 72_000, 72_000]
    );
    // At swing 66 the second 16th lands 2/3 of the way to the third: a
    // /32 hit halfway into the first 16th sits halfway to the swung one.
    let mut e = kit(0);
    let text = "tempo 120\nswing 66\ntrack kit drums\nfrag a = kit /32\n  bd .x.x\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    let swung = e.clock.step_sample(1);
    assert_eq!(
        starts_within(&mut e, 12_000),
        vec![swung / 2, swung + (12_000 - swung) / 2]
    );
}

/// #353: a grid prints back as written; anything else says where.
#[test]
fn a_grid_prints_back_and_a_bad_one_is_refused() {
    for grid in [12, 16, 24, 32, 48] {
        let text = format!("tempo 120\ntrack kit drums\n\nfrag a = kit /{grid}\n  bd x..x\n");
        let song = Song::parse(&text).expect("parses");
        assert_eq!(song.frags[0].grid, grid);
        assert!(song.print().contains(&format!("frag a = kit /{grid}\n")));
    }
    let err = Song::parse("track kit drums\nfrag a = kit /20\n  bd x\n").expect_err("/20");
    assert_eq!((err.line, err.col), (2, 14));
}
