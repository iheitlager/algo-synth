//! The tests that render minutes of audio: whole songs, launches and scenes
//! over many bars, VCO drift. Ignored by `cargo test`, so neither CI nor
//! coverage runs them; `make test-songs` does (#535).

use super::*;

/// A clip starts again at each scene's first bar: a long one is cut,
/// a short one loops inside it.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_clip_restarts_with_its_scene() {
    let mut e = kit(0);
    let long = format!("x{}x{}", ".".repeat(19), ".".repeat(11));
    let text = format!(
        "track kit drums\nclip l = kit\n  bd {long}\nclip s = kit\n  sn x..\nscene a 1: l s\narrange a a\n"
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

/// #165: a euclid note line plays the same events on every run.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_euclid_note_line_walks_the_scale_deterministically() {
    let text =
        "tempo 120\nscale c minor\ntrack lead synth\nclip r = lead\n  euclid(4,8) scale c4\n";
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

/// #353: a flam on the first step of play has nothing before it to sound
/// its grace in; from the second time round it has.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_flam_on_the_first_step_graces_from_the_second_bar() {
    assert_eq!(
        lane_starts(120, 16, "f...............", 192_000),
        vec![0, 96_000 - 960, 96_000, 192_000 - 960]
    );
}

/// #487: stopped, a launch starts the song with the scene; End waits for
/// the scene to finish, not the bar.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_launch_at_the_end_waits_for_the_scene() {
    let mut e = launch_song();
    e.song_launch(1, Quantize::Bar);
    assert!(e.clock().playing(), "a launch starts the song");
    assert_eq!(hits_at(&mut e, 0, 5), vec![0, 2, 4]);
    e.song_launch(2, Quantize::End);
    assert_eq!(
        e.song_launch_in(),
        Some(27),
        "two's two bars end on step 32"
    );
    let hits = hits_at(&mut e, 5, 27 + 24);
    let want: Vec<u64> = (6..32).step_by(2).chain([32, 40, 48]).collect();
    assert_eq!(hits, want);
    assert_eq!(e.song_launched(), Some(2));
}

/// #487: Now starts the scene at once, as far into its first bar as the
/// clock is into its own; its snapshot is set at once.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_launch_now_keeps_the_phase() {
    let mut e = launch_song();
    e.song_play();
    hit_steps(&mut e, 21);
    e.song_launch(1, Quantize::Now);
    assert_eq!(e.song_launched(), Some(1));
    assert_eq!(e.param_value(0, Param::Send2), 0.25);
    // two from bar 1: step 21 is its step 5, so it hits on 22, 24, … (even).
    assert_eq!(hits_at(&mut e, 21, 11), vec![22, 24, 26, 28, 30]);
}

/// #487: Phrase waits for the next eight-bar line.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_launch_on_the_phrase_waits_eight_bars() {
    let mut e = launch_song();
    e.song_play();
    hit_steps(&mut e, 5);
    e.song_launch(2, Quantize::Phrase);
    assert_eq!(e.song_launch_in(), Some(123));
    let hits = hits_at(&mut e, 5, 123 + 16);
    assert!(hits.iter().filter(|k| **k < 128).all(|k| k % 4 == 0));
    assert_eq!(
        hits.iter()
            .filter(|k| **k >= 128)
            .copied()
            .collect::<Vec<_>>(),
        vec![128, 136]
    );
}

/// #487: a song that takes over finds the launched scene by its name.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_launched_scene_is_found_again_by_name() {
    let mut e = launch_song();
    e.song_launch(2, Quantize::Bar);
    hit_steps(&mut e, 4);
    let moved = LAUNCH
        .replace("scene one 1: a\n", "")
        .replace("arrange one", "scene one 1: a\narrange one");
    assert_eq!(load_text(&mut e, &moved), Ok(()));
    hit_steps(&mut e, 14);
    assert_eq!(e.song_launched(), Some(1), "three moved up a place");
    let gone = moved.replace("scene three 1: c\n", "");
    assert_eq!(load_text(&mut e, &gone), Ok(()));
    hit_steps(&mut e, 16);
    assert_eq!(e.song_launched(), None, "gone, the arrangement plays");
}

/// #487: a scene launched mid-bar lands on the next bar line, from its
/// first step, its snapshot set, and loops until the next launch.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_launched_scene_lands_on_the_next_bar_and_loops() {
    let mut e = launch_song();
    e.song_play();
    assert_eq!(hits_at(&mut e, 0, 5), vec![0, 4]);
    e.song_launch(1, Quantize::Bar);
    assert_eq!(
        e.song_queued(),
        Some((LaunchTarget::Scene(1), Quantize::Bar))
    );
    assert_eq!(
        e.song_launch_in(),
        Some(11),
        "steps 5 to 15 are left of bar 1"
    );
    let before = e.param_value(0, Param::Send2);
    let hits = hits_at(&mut e, 5, 11 + 48);
    let want: Vec<u64> = [8, 12].into_iter().chain((16..64).step_by(2)).collect();
    assert_eq!(
        hits, want,
        "one to the bar line, then two, again after two bars"
    );
    assert_eq!(e.song_launched(), Some(1));
    assert_eq!(e.song_queued(), None);
    assert_ne!(e.param_value(0, Param::Send2), before);
    assert_eq!(e.param_value(0, Param::Send2), 0.25, "its snapshot is set");
    assert_eq!(
        e.song_place(),
        Some((usize::MAX, 15)),
        "step 63: 47 into a 32-step loop, no entry"
    );
}

/// #167: a live clip plays a new walk each bar, the same ones every run.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_live_clip_changes_each_cycle_and_repeats_each_run() {
    let mut e = poly();
    assert_eq!(load_text(&mut e, LIVE), Ok(()));
    e.song_play();
    let played = started(&mut e, 96_000 * 3);
    let Some(Seq::Generated(call)) = e.song().clips[0].notes.as_ref().map(|n| n.seq.clone()) else {
        panic!("a generated clip");
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

/// ADR-0015 with ADR-0016: a note clip plays only in its scenes,
/// from each scene's first bar. Two bars of `c4 e4` (one bar long) in
/// `b`, after a silent bar `a`: notes from 96000, starting over at
/// 192000 rather than running on from where the line would be.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_note_clip_plays_in_its_scene_from_its_start() {
    let got = held_changes(
        "tempo 120\ntrack lead synth\nclip r = lead\n  c4:2 e4:4 g4:4\nscene a 1:\nscene b 1: r\narrange a b b\n",
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

/// #488: the next scene's own snapshot still lands; a button landing on
/// the same bar line comes after it.
#[test]
#[ignore = "a long render: make test-songs"]
fn a_scenes_own_snapshot_still_lands_and_the_button_wins() {
    let mut e = kit(0);
    assert_eq!(load_text(&mut e, SNAPS), Ok(()));
    e.song_play();
    e.song_snapshot(1, Quantize::Now);
    assert_eq!(
        frame_when(&mut e, 0.1, 200_000),
        Some(192_000),
        "scene two's [dry] on bar 3"
    );
    e.song_seek_bar(1);
    hit_steps(&mut e, 1);
    e.song_snapshot(1, Quantize::Bar);
    hit_steps(&mut e, 16);
    assert_eq!(
        e.param_value(0, Param::Send2),
        0.7,
        "on bar 3's line too, after [dry]"
    );
    e.song_snapshot(0, Quantize::Bar);
    e.song_stop();
    assert_eq!(e.song_snapshot_queued(), None, "stop forgets it");
}

#[test]
#[ignore = "a long render: make test-songs"]
fn autumn_rv293_plays_start_to_finish() {
    plays_start_to_finish(
        "rv293-autumn",
        include_bytes!("../../../../../examples/scores/rv293-autumn.mid"),
    );
}

/// #487: back to the arrangement on the next bar, where the clock is.
#[test]
#[ignore = "a long render: make test-songs"]
fn back_to_the_arrangement_goes_on_where_the_clock_is() {
    let mut e = launch_song();
    e.song_launch(2, Quantize::Bar);
    hit_steps(&mut e, 20);
    e.song_resume_arrangement(Quantize::Bar);
    assert_eq!(
        e.song_queued(),
        Some((LaunchTarget::Arrangement, Quantize::Bar))
    );
    assert_eq!(hits_at(&mut e, 20, 20), vec![24, 32, 36]);
    assert_eq!(e.song_launched(), None);
    assert_eq!(e.song_place(), Some((2, 7)), "bar 3 of the arrangement");
    e.song_launch(1, Quantize::Bar);
    e.song_launch_cancel();
    assert_eq!(e.song_queued(), None);
    e.song_stop();
    assert_eq!(
        (e.song_launched(), e.song_queued()),
        (None, None),
        "stop forgets the launch"
    );
}

/// #167: freezing prints the bar it was playing; parsing that plays the same.
#[test]
#[ignore = "a long render: make test-songs"]
fn freezing_keeps_the_bar_that_was_playing() {
    let mut e = poly();
    assert_eq!(load_text(&mut e, LIVE), Ok(()));
    e.song_play();
    let first = started(&mut e, 96_000);
    let second = started(&mut e, 48_000); // half way into bar 2
    assert_eq!(first.len() + second.len(), 12);
    assert!(e.freeze(0));
    assert!(!e.song().clips[0].live);
    assert!(!e.song_text().contains("live") && !e.song_text().contains("walk("));
    let frozen: Vec<u8> = e.song().clips[0]
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
    assert!(!e.freeze(0), "a frozen clip has no call to freeze");
}

/// #353: a lane of 12, 24, 32 or 48 steps a bar hits every bar / grid, on
/// its exact sample: at 120 BPM and 48 kHz a bar is 96 000 samples.
#[test]
#[ignore = "a long render: make test-songs"]
fn lanes_hit_on_their_grid() {
    for grid in [12u64, 16, 24, 32, 48] {
        let mut e = kit(0);
        let lane = "x".repeat(grid as usize);
        let text = format!("tempo 120\ntrack kit drums\nclip a = kit /{grid}\n  bd {lane}\n");
        assert_eq!(load_text(&mut e, &text), Ok(()));
        e.song_play();
        let want: Vec<u64> = (0..grid).map(|n| n * 96_000 / grid).collect();
        assert_eq!(starts_within(&mut e, 96_000), want, "/{grid}");
    }
}

/// #163: `"c4 e4 g4 c5"` at 120 BPM and 48 kHz starts a note every beat
/// (24000 samples), each ending as the next begins.
#[test]
#[ignore = "a long render: make test-songs"]
fn note_clips_sound_at_their_samples_and_pitches() {
    let got = held_changes(
        "tempo 120\ntrack lead synth\nclip r = lead\n  \"c4 e4 g4 c5\"\n",
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

/// Spec 002 Req 4: each scene plays its own clips from its first
/// bar, on the exact sample, and the song stops after the last bar.
#[test]
#[ignore = "a long render: make test-songs"]
fn scenes_play_in_order_and_the_song_ends() {
    let mut e = kit(0);
    let text = "tempo 120\ntrack kit drums\nclip a = kit\n  bd x...\nclip b = kit\n  sn x.\n\
                scene one 1: a\nscene two 1: b\narrange one two\n";
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    let hits = hit_steps(&mut e, 40);
    assert_eq!(hits, vec![0, 4, 8, 12, 16, 18, 20, 22, 24, 26, 28, 30]);
    assert!(!e.clock().playing(), "the song stops after its last bar");
    assert_eq!(e.clock().position(), 0, "and goes back to the top");
}

/// #547's recursive modulator as a song: its one track is heard, finite and
/// bounded, from the first block.
#[test]
fn composed_drone_sounds_as_its_song() {
    let mut e = Engine::new(48_000.0);
    let text = include_str!("../../../../../examples/sounds/composed-drone.song");
    assert_eq!(load_text(&mut e, text), Ok(()));
    e.song_play();
    let mut heard = 0.0_f32;
    for _ in 0..(3 * 48_000 / BLOCK) {
        e.clear_meters();
        e.render(BLOCK);
        assert!(
            e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0),
            "finite and bounded"
        );
        heard = heard.max(e.meters()[e.song_routed(0).expect("routed")]);
    }
    assert!(heard > 0.001, "the drone is heard");
}

/// The algorave example, nearly all of it generated: it plays to its end,
/// finite and bounded, and every one of its fourteen tracks is heard.
#[test]
#[ignore = "a long render: make test-songs"]
fn seed_rave_plays_start_to_finish() {
    let mut e = Engine::new(48_000.0);
    let text = include_str!("../../../../../examples/songs/seed-rave.song");
    assert_eq!(load_text(&mut e, text), Ok(()));
    let tracks = e.song().tracks.len();
    assert_eq!(tracks, 14);
    let bars = e.song().bars();
    let frames = (bars as f32 * 4.0 * 60.0 / e.song().tempo * 48_000.0) as u64;
    e.song_play();
    let mut heard = vec![0.0_f32; tracks];
    for _ in (0..frames).step_by(BLOCK) {
        e.clear_meters();
        e.render(BLOCK);
        assert!(
            e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0),
            "finite and bounded"
        );
        for (t, h) in heard.iter_mut().enumerate() {
            let strip = e.song_routed(t).expect("routed");
            *h = h.max(e.meters()[strip]);
        }
    }
    for (t, h) in heard.iter().enumerate() {
        assert!(*h > 0.001, "track {t} is heard");
    }
}

/// #355: while a track is soloed only soloed tracks play; solo outranks mute.
#[test]
#[ignore = "a long render: make test-songs"]
fn solo_plays_only_the_soloed_tracks() {
    let mut e = two_tracks(" mute solo");
    e.song_play();
    assert_eq!(hit_steps(&mut e, 16), vec![0, 4, 8, 12]);
    assert!(e.set_track_flags(0, false, false));
    assert_eq!(hit_steps(&mut e, 16), vec![0, 2, 4, 6, 8, 10, 12, 14]);
}

#[test]
#[ignore = "a long render: make test-songs"]
fn spring_rv269_plays_start_to_finish() {
    plays_start_to_finish(
        "rv269-spring",
        include_bytes!("../../../../../examples/scores/rv269-spring.mid"),
    );
}

#[test]
#[ignore = "a long render: make test-songs"]
fn summer_rv315_plays_start_to_finish() {
    plays_start_to_finish(
        "rv315-summer",
        include_bytes!("../../../../../examples/scores/rv315-summer.mid"),
    );
}

#[test]
#[ignore = "a long render: make test-songs"]
fn sync_pulls_a_deck_onto_the_masters_bar() {
    let mut master = Engine::new(48_000.0);
    let mut deck = Engine::new(48_000.0);
    master.set_tempo(130.0);
    deck.set_tempo(130.0);
    master.song_play();
    for _ in 0..300 {
        master.render(BLOCK);
    }
    // Started 700 samples after the master's bar: behind.
    let f = master.cue_frames(16, 0).expect("the master plays") as usize;
    deck.song_play_in(f + 700);
    let both = |m: &mut Engine, d: &mut Engine, blocks: usize| {
        for _ in 0..blocks {
            m.render(BLOCK);
            d.render(BLOCK);
        }
    };
    both(&mut master, &mut deck, 2000);
    let phase = |m: &Engine, d: &Engine| {
        (m.clock().step_position() - d.clock().step_position()).rem_euclid(16.0)
    };
    let off = phase(&master, &deck);
    assert!(
        (off - 700.0 / (48_000.0 * 60.0 / 130.0 / 4.0)).abs() < 1e-3,
        "behind by 700 samples: {off} steps"
    );
    let f = master.cue_frames(16, 0).expect("the master plays") as usize;
    deck.sync_bar_in(f);
    both(&mut master, &mut deck, 2000);
    assert!(
        (deck.sync_error() + 700).abs() <= 2,
        "it was 700 samples behind: {}",
        deck.sync_error()
    );
    let off = phase(&master, &deck);
    assert!(
        off.min(16.0 - off) < 1e-3,
        "on the master's bar now: {off} steps off"
    );
    // Synced again, it is on the bar and stays put.
    let f = master.cue_frames(16, 0).expect("the master plays") as usize;
    deck.sync_bar_in(f);
    both(&mut master, &mut deck, 2000);
    assert!(
        deck.sync_error().abs() <= 2,
        "already on the bar: {}",
        deck.sync_error()
    );
}

/// #173, ADR-0022: the demo Canon imported as a song starts every note of
/// the file on the synth of its channel (in channel order, from 0), at its
/// time in the file (to the eight frames the test renders at once, far finer
/// than a tick).
#[test]
#[ignore = "a long render: make test-songs"]
fn the_demo_imported_plays_the_file() {
    let bytes = include_bytes!("../../../../../examples/scores/canon.mid");
    let mut e = Engine::new(48_000.0);
    for s in 0..SYNTHS {
        e.set_param(s, Param::Polyphony, 8.0);
    }
    assert_eq!(import(&mut e, bytes), Ok(4));
    assert!(e.song().arrange.len() > 1, "in scenes");
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

/// The arranger's first "+ Scene" on a looping beat keeps it playing: the
/// scene holds every clip and the clock keeps its place in the bar, even
/// when it was already past the new end.
#[test]
#[ignore = "a long render: make test-songs"]
fn the_first_scene_keeps_the_beat_playing() {
    let mut e = kit(0);
    assert_eq!(load_text(&mut e, FOUR), Ok(()));
    e.song_play();
    assert_eq!(
        hit_steps(&mut e, 40),
        vec![0, 4, 8, 12, 16, 20, 24, 28, 32, 36]
    );
    assert!(e.arrange_edit(1, 1, 0, 0), "a 1-bar scene");
    assert_eq!(e.song().scenes[0].clips, vec![0]);
    // Step 40 is step 8 of the 1-bar scene: hits on 8 and 12, then the
    // arrangement (without a loop) ends.
    assert_eq!(hit_steps(&mut e, 16), vec![0, 4]);
}

/// The loop region repeats its bars; seek lands on a bar.
#[test]
#[ignore = "a long render: make test-songs"]
fn the_loop_region_repeats_and_seek_lands_on_a_bar() {
    let mut e = kit(0);
    let text = "track kit drums\nclip a = kit\n  bd x...............\nclip b = kit\n  sn x...............\n\
                scene one 1: a\nscene two 1: b\narrange one two one\nloop 2 2\n";
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

/// #343: the SSM2030 of Rev 1/2 lacks the CEM3340's temperature
/// compensation: with the same Vintage it plays a key further off.
#[test]
#[ignore = "a long render: make test-songs"]
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

/// #216's target: the SuperCollider hoover exactly as pasted, Splay and
/// FreeVerb2 included, plays a held chord on two different sides, bounded
/// and the same twice, and ends after its release (ADR-0024).
#[test]
#[ignore = "a long render: make test-songs"]
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

/// #339: with `Analog` up, a VCO monosynth plays the same key a little
/// off each time as its oscillators drift; the Juno-106's DCOs play it in
/// tune every time, on every voice. `Analog` 0 holds every model in tune.
#[test]
#[ignore = "a long render: make test-songs"]
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

#[test]
#[ignore = "a long render: make test-songs"]
fn winter_rv297_plays_start_to_finish() {
    plays_start_to_finish(
        "rv297-winter",
        include_bytes!("../../../../../examples/scores/rv297-winter.mid"),
    );
}

/// The chip and toy-box band: it plays to its end, finite and bounded, and
/// every track but the CR-78 is heard (its samples are the app's to load).
#[test]
#[ignore = "a long render: make test-songs"]
fn pocket_orchestra_plays_start_to_finish() {
    let mut e = Engine::new(48_000.0);
    let text = include_str!("../../../../../examples/songs/pocket-orchestra.song");
    assert_eq!(load_text(&mut e, text), Ok(()));
    let tracks = e.song().tracks.len();
    assert_eq!(tracks, 16);
    let bars = e.song().bars();
    let frames = (bars as f32 * 4.0 * 60.0 / e.song().tempo * 48_000.0) as u64;
    e.song_play();
    let mut heard = vec![0.0_f32; tracks];
    for _ in (0..frames).step_by(BLOCK) {
        e.clear_meters();
        e.render(BLOCK);
        assert!(
            e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0),
            "finite and bounded"
        );
        for (t, h) in heard.iter_mut().enumerate() {
            let strip = e.song_routed(t).expect("routed");
            *h = h.max(e.meters()[strip]);
        }
    }
    for (t, h) in heard.iter().enumerate().skip(1) {
        assert!(*h > 0.001, "track {t} is heard");
    }
}

/// Electro on the 808 samples, accent across: it plays to its end, finite
/// and bounded, and the bass, lead and stab are heard (the 808's samples
/// are the app's to load, #789).
#[test]
#[ignore = "a long render: make test-songs"]
fn accent_across_plays_start_to_finish() {
    let mut e = Engine::new(48_000.0);
    let text = include_str!("../../../../../examples/songs/accent-across.song");
    assert_eq!(load_text(&mut e, text), Ok(()));
    let tracks = e.song().tracks.len();
    assert_eq!(tracks, 4);
    let bars = e.song().bars();
    assert_eq!(bars, 44, "the arrangement's scenes");
    let frames = (bars as f32 * 4.0 * 60.0 / e.song().tempo * 48_000.0) as u64;
    e.song_play();
    let mut heard = vec![0.0_f32; tracks];
    for _ in (0..frames).step_by(BLOCK) {
        e.clear_meters();
        e.render(BLOCK);
        assert!(
            e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0),
            "finite and bounded"
        );
        for (t, h) in heard.iter_mut().enumerate() {
            let strip = e.song_routed(t).expect("routed");
            *h = h.max(e.meters()[strip]);
        }
    }
    for (t, h) in heard.iter().enumerate().skip(1) {
        assert!(*h > 0.001, "track {t} is heard");
    }
}

/// The rhythm boxes' percussion: it plays to its end, finite and bounded,
/// and every track but the CR-78 and the 808 is heard (their samples are
/// the app's to load, #787).
#[test]
#[ignore = "a long render: make test-songs"]
fn rhythm_box_plays_start_to_finish() {
    let mut e = Engine::new(48_000.0);
    let text = include_str!("../../../../../examples/songs/rhythm-box.song");
    assert_eq!(load_text(&mut e, text), Ok(()));
    let tracks = e.song().tracks.len();
    assert_eq!(tracks, 8);
    let bars = e.song().bars();
    assert_eq!(bars, 46, "the arrangement's scenes");
    let frames = (bars as f32 * 4.0 * 60.0 / e.song().tempo * 48_000.0) as u64;
    e.song_play();
    let mut heard = vec![0.0_f32; tracks];
    for _ in (0..frames).step_by(BLOCK) {
        e.clear_meters();
        e.render(BLOCK);
        assert!(
            e.output().iter().all(|s| s.is_finite() && s.abs() <= 1.0),
            "finite and bounded"
        );
        for (t, h) in heard.iter_mut().enumerate() {
            let strip = e.song_routed(t).expect("routed");
            *h = h.max(e.meters()[strip]);
        }
    }
    for (t, h) in heard.iter().enumerate().skip(2) {
        assert!(*h > 0.001, "track {t} is heard");
    }
}
