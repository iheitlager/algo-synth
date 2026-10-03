# 002: Composition

Tracks, sources, effects, patterns, clips, algo loops, score import and the clock. Decision: ADR-0005. Draft: every requirement here is planned (plan.md MVP 2-10); paths name where the code will land. Req 1-7 are deferred until after the ensemble, and Wave and Drums are removed for now (ADR-0008).

### Requirement 1: A track owns one source [MUST]

A track SHALL own exactly one source instance (`Mono`, `Wave`, `Drums`), with its own fixed voice pool and parameters. Parameters SHALL be addressed as (track, parameter).

**Implementation:** `crates/dsp/src/track.rs::Track` *(planned, MVP 4)*

**Tests:** `crates/dsp/src/track.rs::tests` *(planned)*

### Requirement 2: Fixed mixer [MUST]

The mixer SHALL own every strip parameter (`Level`, `Pan`, `Send1`–`Send4`, `Mute`, `Solo`; `Param::is_strip`). Each synth's strip SHALL pass through its drive insert, a fader and pan, and four post-fader sends (P1–P4) into a master chain of equalizer, compressor and limiter. The insert order MAY change; nothing in the mixer SHALL allocate after init.

**Implementation:** `crates/dsp/src/mixer.rs::Mixer` (strips, buses and sends), `crates/dsp/src/fx/processor.rs::Processor` (P1–P4), `crates/dsp/src/fx/echo.rs::Echo`, `crates/dsp/src/fx/reverb.rs::Reverb`, `crates/dsp/src/fx/drive.rs::Drive` (the insert, spec 004 Req 11)`crates/dsp/src/fx/compressor.rs::Compressor`, `crates/dsp/src/fx/limiter.rs::Limiter`; the master equalizer *(planned)*

The four sends SHALL feed four effect processors, P1–P4. Each SHALL have a type (off, echo, reverb), a return level and five knobs A–E in 0..=1 whose meaning depends on the type (echo: time, feedback, tone, ping-pong; reverb: size, damping, pre-delay). All types of a slot SHALL be allocated at init; switching type SHALL hand the input to the new type and let the old one ring out, without a click or an allocation. The echo SHALL be a stereo delay of up to 2 s set in milliseconds, with feedback clamped below 1, a low-pass in the loop and ping-pong. The reverb SHALL be an 8-line feedback delay network with a Hadamard matrix. With every return at 0 the processors SHALL be silent. The master compressor SHALL be feed-forward and stereo-linked, with threshold, ratio (1 is off), attack, release and make-up, and report its gain reduction; it SHALL work out its gain at control rate, not per sample. The limiter SHALL be the last stage: it never passes ±1 and turns NaN and infinity into silence.

#### Scenario: strips

- GIVEN a synth playing
- WHEN its pan is −1, 0 or +1, its level 0, or another synth soloed
- THEN it sounds left only, 3 dB down on both sides, right only, or not at all

#### Scenario: returns

- GIVEN an impulse on the echo send
- WHEN the time is 100 ms and the feedback 0.5
- THEN echoes land every 100 ms, each half the one before

**Tests:** `crates/dsp/src/fx/compressor.rs::tests::above_the_threshold_the_rise_is_divided_by_the_ratio`, `crates/dsp/src/fx/compressor.rs::tests::attack_and_release_follow_their_times`, `crates/dsp/src/fx/limiter.rs::tests::nothing_passes_full_scale_and_it_recovers`, `crates/dsp/src/engine.rs::tests::the_master_compressor_turns_a_loud_mix_down`, `crates/dsp/src/fx/processor.rs::tests::switching_type_lets_the_old_tail_ring_out`, `crates/dsp/src/fx/processor.rs::tests::the_echo_knobs_set_time_and_feedback`, `crates/dsp/src/fx/processor.rs::tests::the_reverb_knob_sets_the_size`, `crates/dsp/src/fx/echo.rs::tests::echoes_land_at_the_set_time_and_decay_by_the_feedback`, `crates/dsp/src/fx/echo.rs::tests::feedback_is_clamped_and_the_loop_stays_bounded`, `crates/dsp/src/fx/reverb.rs::tests::the_tail_falls_60_db_in_about_the_size_setting`, `crates/dsp/src/fx/reverb.rs::tests::no_dc_and_always_bounded`, `crates/dsp/src/engine.rs::tests::the_effects_only_sound_through_a_send_and_a_return`, `crates/dsp/src/engine.rs::tests::sixteen_synths_through_both_effects_stay_bounded`

**Tests:** `crates/dsp/src/engine.rs::tests::pan_is_equal_power`, `crates/dsp/src/engine.rs::tests::fader_mute_and_solo`, `crates/dsp/src/engine.rs::tests::sends_follow_the_fader_and_leave_the_mix_alone`, `crates/dsp/src/engine.rs::tests::sixteen_full_synths_stay_bounded_in_stereo`

### Requirement 3: Patterns [MUST]

A pattern SHALL be a list of steps with note, velocity, length, probability and optional parameter locks, of any length in steps.

**Implementation:** `crates/dsp/src/pattern.rs::Pattern` *(planned, MVP 3)*

### Requirement 4: Clips and arrangement [MUST]

A clip SHALL place a pattern on a track from a bar for a number of bars, looping the pattern inside its span, and record its origin (`hand`, `algo`, `score`). The origin SHALL NOT change playback.

**Implementation:** `crates/dsp/src/arrangement.rs` *(planned, MVP 4)*

### Requirement 5: Sample-accurate clock in the engine [MUST]

The engine SHALL run the transport (tempo, swing, play, stop, position) inside `render` and fire events on their exact sample. The UI SHALL NOT schedule notes.

**Implementation:** `crates/dsp/src/clock.rs` *(planned, MVP 3)*

#### Scenario: tempo accuracy

- GIVEN 120 BPM at 48 kHz and a pattern of sixteenth notes
- WHEN 4 bars are rendered offline
- THEN note onsets fall exactly 6000 samples apart

**Tests:** `crates/dsp/src/clock.rs::tests::sixteenths_at_120_bpm` *(planned)*

### Requirement 6: Song as data [MUST]

The UI SHALL send the song to the engine in a versioned binary format written into a buffer the engine allocated at init; `render` SHALL only read it, and a malformed song SHALL be rejected without affecting playback.

**Implementation:** `crates/dsp/src/song.rs` *(planned, MVP 3)*

### Requirement 7: Deterministic generators [MUST]

An algo loop SHALL combine a generator (Euclid, walk, arp, Markov, mutate), a scale and a target track. With the same seed and parameters it SHALL produce the same pattern. A live loop SHALL regenerate every cycle; freezing SHALL commit the current pattern as a clip.

**Implementation:** `crates/dsp/src/algo.rs` *(planned, MVP 9-10)*

#### Scenario: Euclid

- GIVEN Euclid(k=3, n=8, rotation=0)
- WHEN it generates
- THEN the hits are on steps 0, 3 and 6

**Tests:** `crates/dsp/src/algo.rs::tests::euclid_3_8` *(planned)*

### Requirement 8: Score import [SHOULD]

The engine SHALL parse Standard MIDI Files (types 0 and 1) without panicking on malformed input, and import each track as a track with a Mono source, its notes as `score` clips, and its tempo changes as the song's tempo map.

**Implementation:** `crates/dsp/src/smf.rs::parse` (the parser); the import as tracks and clips *(planned, MVP 5)*

#### Scenario: the ensemble

- GIVEN a six-part MIDI score
- WHEN it is imported
- THEN six Mono tracks exist, each with its own patch, and the arrangement shows their clips

#### Scenario: malformed files

- GIVEN truncated, oversized or random bytes
- WHEN they are parsed
- THEN the parser returns an error and never panics

**Tests:** `crates/dsp/src/smf.rs::tests::never_panics_on_garbage`, `crates/dsp/src/smf.rs::tests::rejects_what_it_cannot_play`, `crates/dsp/src/smf.rs::tests::running_status_and_zero_velocity_off`, `crates/dsp/src/smf.rs::tests::tempo_and_name`

### Requirement 9: MIDI file playback [SHOULD]

Until tracks and clips exist (MVP 4-5), the engine SHALL play a loaded MIDI file directly: ticks SHALL become samples once, at load, through the tempo map, and each note SHALL start on its exact sample in `render`. Each MIDI channel SHALL play on one Mono synth or be muted; loading puts the parts on synths 0, 1, 2… in order (spec 004 Req 10). A file that fails to parse SHALL leave the loaded song untouched. Stop SHALL release the player's voices and leave live ones sounding.

**Implementation:** `crates/dsp/src/player.rs::Sequence`, `crates/dsp/src/engine.rs::Engine::load_midi`, `crates/dsp/src/ffi.rs` (`midi_buf`, `midi_load`, `part_*`, `event_*`, `song_length`, `song_bar`, `play`, `stop`, `seek`, `position`, `playing`, `route`, `routed`)

#### Scenario: sample-accurate start

- GIVEN a file with a note at a known tick
- WHEN it is played
- THEN the note starts on the sample the tempo map gives

**Tests:** `crates/dsp/src/player.rs::tests::ticks_become_samples_through_the_tempo_map`, `crates/dsp/src/engine.rs::tests::player_note_starts_on_its_exact_sample`, `crates/dsp/src/engine.rs::tests::every_channel_plays_and_mute_silences`, `crates/dsp/src/engine.rs::tests::bad_files_are_rejected_and_keep_the_old_song`, `crates/dsp/src/engine.rs::tests::stop_releases_player_voices_but_not_live_ones`, `crates/dsp/src/ffi.rs::tests::midi_round_trip_through_the_abi`
