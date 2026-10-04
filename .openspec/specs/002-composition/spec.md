# 002: Composition

Tracks, sources, effects, fragments, the arrangement, generators, the song as text, score import and the clock. Decisions: ADR-0005, ADR-0012 (the song is text). Req 1 and 3-7 are planned (plan.md MVP 3-10); paths name where the code will land.

### Requirement 1: A track owns one source [MUST]

A track SHALL be a synth slot (strip 0–15) and own exactly one source: a Mono or Poly model (ADR-0009), the Drums kit (MVP 3) or, later, the Sampler (MVP 6), with its own fixed voice pool and parameters. Parameters SHALL be addressed as (track, parameter).

**Implementation:** `crates/dsp/src/track.rs::Track` *(planned, MVP 3)*; the Drums kit's pads `crates/dsp/src/drums.rs::Kit` (bd sn lt mt ht lc mc hc rs cl cp ma cb cy oh ch, the TR-808's sixteen voices: the cowbell two band-limited squares through a band-pass near 850 Hz, the rimshot and claves struck resonators; one voice per pad, the closed hat choking the open hat, per-pad tune, decay, tone, level and an accent); in a synth slot as the TR-808 model, `crates/dsp/src/poly.rs::Pool` (`hit`, `PolyVoice::Drum`): a key plays the pad General MIDI puts there (any other key by its place in the octave from 36), on the voice already playing that pad or a new one; a hit has no note-off; the closed hat chokes the open hat; a hit at velocity 0.9 or more is accented. Per-pad tune, decay, tone, level and the accent are synth parameters (`BdTune` … `CbLevel`, `DrumAccent`)

#### Scenario: a kit slot

- GIVEN a synth slot with the TR-808 model
- WHEN keys 36, 38 and 42 are pressed, the closed hat ten times, or an open hat then a closed hat
- THEN three pads sound on three voices and ring out with no note-off, the closed hat keeps one voice, and the open hat is choked

#### Scenario: the loudest hit

- GIVEN any pad at full level, accented by the most, at any extreme of tune, decay and tone
- WHEN it is rendered offline
- THEN every sample is finite and within ±1, there is no DC, and it is silent within four seconds

**Tests:** `crates/dsp/src/track.rs::tests` *(planned)*, `crates/dsp/src/drums/tests.rs::every_pad_at_every_extreme_is_finite_bounded_without_dc_and_ends`, `crates/dsp/src/drums/tests.rs::the_kick_falls_to_its_tune`, `crates/dsp/src/drums/tests.rs::tune_moves_the_pitch_by_semitones`, `crates/dsp/src/drums/tests.rs::the_hats_sit_high`, `crates/dsp/src/drums/tests.rs::the_closed_hat_chokes_the_open_hat`, `crates/dsp/src/drums/tests.rs::the_clap_comes_in_bursts`, `crates/dsp/src/drums/tests.rs::an_accent_is_louder_by_the_amount`, `crates/dsp/src/drums/tests.rs::the_same_hit_gives_the_same_samples`, `crates/dsp/src/drums/tests.rs::every_key_plays_a_pad_and_general_midi_its_own`, `crates/dsp/src/drums/tests.rs::the_cowbell_sits_in_its_band_and_falls_in_two_stages`, `crates/dsp/src/drums/tests.rs::the_claves_ring_at_their_tune_and_stop_short`, `crates/dsp/src/drums/tests.rs::the_congas_sit_above_the_toms`, `crates/dsp/src/engine.rs::tests::a_kit_slot_plays_its_pads`, `crates/dsp/src/engine.rs::tests::a_pad_hit_again_retriggers_its_own_voice`, `crates/dsp/src/engine.rs::tests::the_closed_hat_chokes_the_open_hat`, `crates/dsp/src/engine.rs::tests::a_hard_hit_is_accented_and_the_knobs_reach_the_pads`, `crates/dsp/src/engine.rs::tests::channel_ten_plays_on_a_kit_slot`

### Requirement 2: Fixed mixer [MUST]

The mixer SHALL own every strip parameter (`Level`, `Pan`, `Send1`–`Send4`, `Mute`, `Solo`; `Param::is_strip`). Each synth's strip SHALL pass through its drive insert, a fader and pan, and four post-fader sends (P1–P4) into a master chain of equalizer, compressor and limiter. The equalizer SHALL have a low shelf, two parametric bands (frequency, gain, Q) and a high shelf, with coefficients computed when a parameter changes; a band at 0 dB SHALL be skipped, so a flat equalizer is bit-exact. The insert order MAY change; nothing in the mixer SHALL allocate after init.

The mixer SHALL also hold eight group buses and three insert slots on every strip and group (ADR-0010, epic #62). Strip indices 0–15 are the synths and 16–23 the groups; a strip parameter is the same id on all of them. Each strip and group SHALL have an `Out` (0 is the master, 1–8 a group); a group SHALL feed only the master or a higher-numbered group, and an invalid route SHALL be ignored. An insert slot SHALL have a type (off, overdrive, distortion, fuzz, EQ, compressor) and five knobs A–E in 0..=1 whose meaning depends on the type; every slot SHALL hold every type, allocated at init, and an Off slot SHALL pass the signal bit for bit. A solo SHALL keep the groups a soloed strip passes through audible; a mute on a group SHALL silence what it carries.

**Implementation:** `crates/dsp/src/fx/insert.rs::Insert` (insert slots, #58), `crates/dsp/src/mixer.rs::Mixer` (strips, group buses, routing and sends), `crates/dsp/src/fx/processor.rs::Processor` (P1–P4), `crates/dsp/src/fx/echo.rs::Echo`, `crates/dsp/src/fx/reverb.rs::Reverb`, `crates/dsp/src/fx/chorus.rs::Chorus`, `crates/dsp/src/fx/flanger.rs::Flanger`, `crates/dsp/src/fx/drive.rs::Drive` (the insert, spec 004 Req 11)`crates/dsp/src/fx/compressor.rs::Compressor`, `crates/dsp/src/fx/limiter.rs::Limiter``crates/dsp/src/fx/eq.rs::Equalizer`

The four sends SHALL feed four effect processors, P1–P4. Each SHALL have a type (off, echo, reverb, chorus, flanger), a return level and five knobs A–E in 0..=1 whose meaning depends on the type (echo: time, feedback, tone, ping-pong; reverb: size, damping, pre-delay; chorus: rate, depth, delay, spread, tone; flanger: rate, depth, manual, feedback of either sign, tone). A processor P2–P4 SHALL be able to take its input from the processor before it (`P2In`–`P4In`) as well as from its sends, so effects combine in series and a chain cannot loop; P1 has no input setting. A chained processor SHALL hear the previous one's output before its return level, so the previous one's return may be 0, and a processor that is Off SHALL pass nothing on. Chorus and flanger SHALL take the stereo output, echo and reverb its mono sum. The effects SHALL make a wet signal at full level and the slot SHALL scale it by the return. The chorus and flanger SHALL be modulated delays with stereo in and out whose modulator uses no transcendental function per sample, and the flanger's feedback SHALL be clamped within ±0.95. All types of a slot SHALL be allocated at init; switching type SHALL hand the input to the new type and let the old one ring out, without a click or an allocation. The echo SHALL be a stereo delay of up to 2 s set in milliseconds, with feedback clamped below 1, a low-pass in the loop and ping-pong. The reverb SHALL be an 8-line feedback delay network with a Hadamard matrix. With every return at 0 the processors SHALL be silent. The master compressor SHALL be feed-forward and stereo-linked, with threshold, ratio (1 is off), attack, release and make-up, and report its gain reduction; it SHALL work out its gain at control rate, not per sample. The limiter SHALL be the last stage: it never passes ±1 and turns NaN and infinity into silence.

#### Scenario: strips

- GIVEN a synth playing
- WHEN its pan is −1, 0 or +1, its level 0, or another synth soloed
- THEN it sounds left only, 3 dB down on both sides, right only, or not at all

#### Scenario: returns

- GIVEN an impulse on the echo send
- WHEN the time is 100 ms and the feedback 0.5
- THEN echoes land every 100 ms, each half the one before

The engine SHALL report peak meters for the view: each synth strip after its fader (0 when muted or silenced by a solo), master left and right after the limiter, and each processor's return, as the highest linear level since the view last read and cleared them (`meters_ptr`, `meters_len`, `meters_clear`).

#### Scenario: meters

- GIVEN a synth playing through its strip
- WHEN the view reads the meters and then clears them
- THEN the strip, the master and any fed return show their peaks, an idle or muted strip shows 0, and the fader scales the strip's reading

**Tests:** `crates/dsp/src/engine.rs::tests::meters_read_the_post_fader_peaks_and_start_over`, `crates/dsp/src/engine.rs::tests::muted_and_unsoloed_strips_read_zero`, `crates/dsp/src/fx/eq.rs::tests::flat_is_bit_exact`, `crates/dsp/src/fx/eq.rs::tests::each_band_reaches_its_gain`, `crates/dsp/src/fx/eq.rs::tests::q_narrows_a_band`, `crates/dsp/src/fx/compressor.rs::tests::above_the_threshold_the_rise_is_divided_by_the_ratio`, `crates/dsp/src/fx/compressor.rs::tests::attack_and_release_follow_their_times`, `crates/dsp/src/fx/limiter.rs::tests::nothing_passes_full_scale_and_it_recovers`, `crates/dsp/src/engine.rs::tests::the_master_compressor_turns_a_loud_mix_down`, `crates/dsp/src/fx/processor.rs::tests::switching_type_lets_the_old_tail_ring_out`, `crates/dsp/src/fx/processor.rs::tests::the_echo_knobs_set_time_and_feedback`, `crates/dsp/src/fx/processor.rs::tests::the_reverb_knob_sets_the_size`, `crates/dsp/src/fx/echo.rs::tests::echoes_land_at_the_set_time_and_decay_by_the_feedback`, `crates/dsp/src/fx/echo.rs::tests::feedback_is_clamped_and_the_loop_stays_bounded`, `crates/dsp/src/fx/reverb.rs::tests::the_tail_falls_60_db_in_about_the_size_setting`, `crates/dsp/src/fx/reverb.rs::tests::no_dc_and_always_bounded`, `crates/dsp/src/engine.rs::tests::the_effects_only_sound_through_a_send_and_a_return`, `crates/dsp/src/engine.rs::tests::sixteen_synths_through_both_effects_stay_bounded`

**Tests:** `crates/dsp/src/engine.rs::tests::pan_is_equal_power`, `crates/dsp/src/engine.rs::tests::fader_mute_and_solo`, `crates/dsp/src/engine.rs::tests::sends_follow_the_fader_and_leave_the_mix_alone`, `crates/dsp/src/engine.rs::tests::sixteen_full_synths_stay_bounded_in_stereo`

### Requirement 3: Fragments [MUST]

A fragment SHALL be a loop of events (note or pad, velocity, start, length, probability) on a beat grid, of any length, playing on one track. A drum fragment SHALL be one lane per pad, one step per character: `x` a hit, `X` an accented hit, `.` a rest. A pitched fragment SHALL be written in mini-notation (a quoted sequence divides one cycle; `[ ]` subdivides, `~` rests, `*n` repeats, `<a b>` alternates per cycle, `?` plays with a probability) or as classic notes with durations (`c4:4`, `e4:8.`), laid out one after another; mixing the two in one sequence SHALL be a parse error. A `sampler` track SHALL take lanes of pad names, as a drum track does, for a pad sampler, or note fragments for a multisampler, never both in one fragment; an unknown pad name SHALL be a parse error.

**Implementation:** drum fragments `crates/dsp/src/song.rs::Fragment` (lanes of up to 64 steps, each lane looping on its own length; `/16` steps for now), played on the clock's steps by `crates/dsp/src/engine.rs::Engine::play_step` (a hit at velocity 0.75, an accent at 1.0); pitched fragments `crates/dsp/src/notes.rs::Notes` (mini-notation and classic durations parsed, printed and compiled to events on 48 ticks to the bar, ADR-0016), held by `crates/dsp/src/song.rs::Fragment` on a `synth` track and played by `crates/dsp/src/engine.rs::Engine::play_tick` on the clock's ticks (`crates/dsp/src/clock.rs::Clock::due_sub`), with a fixed note-off table

#### Scenario: a drum lane

- GIVEN `bd x...x...x...x...` at 120 BPM and 48 kHz
- WHEN one bar is rendered
- THEN the kick starts on samples 0, 24000, 48000 and 72000

#### Scenario: classic durations

- GIVEN `"c4:4 e4:8 g4:8 c5:2"`
- WHEN it is compiled
- THEN the notes start on beats 0, 1, 1.5 and 2, and the fragment is one bar long

**Tests:** `crates/dsp/src/notes/tests.rs::a_quoted_sequence_divides_the_bar`, `crates/dsp/src/notes/tests.rs::brackets_subdivide_and_three_is_a_triplet`, `crates/dsp/src/notes/tests.rs::classic_durations_lay_notes_one_after_another`, `crates/dsp/src/notes/tests.rs::print_then_parse_is_identity`, `crates/dsp/src/notes/tests.rs::every_error_says_where`, `crates/dsp/src/notes/tests.rs::it_never_panics_on_garbage`, `crates/dsp/src/song/tests.rs::note_fragments_parse_and_print_back`, `crates/dsp/src/song/tests.rs::note_errors_say_line_and_column`, `crates/dsp/src/engine.rs::tests::note_fragments_sound_at_their_samples_and_pitches`, `crates/dsp/src/engine.rs::tests::a_note_lasts_its_written_length`, `crates/dsp/src/engine.rs::tests::a_chord_uses_the_voice_pool`, `crates/dsp/src/engine.rs::tests::a_triplet_lands_between_the_sixteenths`, `crates/dsp/src/song/tests.rs::sampler_tracks_hold_lanes_or_notes`, `crates/dsp/src/song/tests.rs::sampler_errors_say_where`, `crates/dsp/src/engine.rs::tests::a_sampler_track_with_lanes_plays_the_pad_sampler`, `crates/dsp/src/engine.rs::tests::a_sampler_track_with_notes_plays_the_multisampler_at_pitch`

### Requirement 4: The arrangement [MUST]

An arrangement SHALL be a list of sections, each a number of bars and the fragments that play in it; a fragment SHALL loop inside its section. A section MAY repeat an earlier one by name. A loop region SHALL repeat a range of bars.

**Implementation:** `crates/dsp/src/arrangement.rs` *(planned, MVP 4)*

### Requirement 5: Sample-accurate clock in the engine [MUST]

The engine SHALL run the transport (tempo, swing, play, stop, position) inside `render` and fire events on their exact sample. The UI SHALL NOT schedule notes. The song and the MIDI file SHALL each have their own transport: the clock plays the song (`song_play`, `song_stop`, which goes back to the top) and the MIDI file plays on its own (`play`, `stop`, `seek`); starting or stopping one SHALL leave the other as it is, and both MAY play at once. The clock SHALL count sixteenth steps, compute each step's sample from its index (no drift), take a tempo change from the next step on without moving that step's place on the grid, and delay odd steps by swing (50% straight to 75%). Tempo and swing belong to the song (ADR-0012), not to the parameter registry.

**Implementation:** `crates/dsp/src/clock.rs::Clock`, `crates/dsp/src/engine.rs::Engine::render` (blocks split at clock steps), `crates/dsp/src/ffi.rs` (`tempo`, `swing`, `clock_step`, `song_play`, `song_stop`, `song_playing`)

#### Scenario: tempo accuracy

- GIVEN 120 BPM at 48 kHz and a pattern of sixteenth notes
- WHEN 4 bars are rendered offline
- THEN note onsets fall exactly 6000 samples apart

**Tests:** `crates/dsp/src/clock.rs::tests::sixteenths_at_120_bpm`, `crates/dsp/src/clock.rs::tests::swing_delays_the_off_beats`, `crates/dsp/src/clock.rs::tests::a_tempo_change_keeps_the_next_step`, `crates/dsp/src/clock.rs::tests::a_swing_change_moves_the_next_off_beat_not_the_grid`, `crates/dsp/src/clock.rs::tests::no_drift_over_a_thousand_bars`, `crates/dsp/src/clock.rs::tests::seek_lands_on_the_next_step`, `crates/dsp/src/engine.rs::tests::clock_steps_land_on_their_samples_through_render`, `crates/dsp/src/engine.rs::tests::the_song_and_the_file_have_their_own_transports`

### Requirement 6: The song is text [MUST]

The view SHALL send the song to the engine as text (ADR-0012). The engine SHALL parse and compile it outside `render`; `render` SHALL only read the compiled song. A text that does not parse SHALL be rejected with a line, a column and a message, and the song that is playing SHALL keep playing; a new song SHALL play from the next clock step, each lane keeping its place against the clock (taking over at a bar line would free the old song inside `render`, against ADR-0002). The parser SHALL never panic. The engine SHALL print a song canonically, and parsing a printed song SHALL give the same song. Edits from a view (`set_step`) SHALL change the song in the engine, which returns the printed text. A track SHALL be routed to a synth as a MIDI channel is: on load a track without a synth goes to the first drum kit, and the view MAY route it elsewhere or mute it.

**Implementation:** `crates/dsp/src/song.rs::Song` (`parse`, `print`, `set_step`), `crates/dsp/src/engine.rs::Engine::load_song`, `crates/dsp/src/ffi.rs` (`song_buf`, `song_load`, `song_error_*`, `song_text_*`, `set_step`, `song_tracks`, `song_route`, `song_routed`, `song_frags`, `frag_*`, `lane_*`, `step_level`), `web/public/worklet.js` (`loadSong`, `sendSong`)

#### Scenario: round trip

- GIVEN any song the parser accepts
- WHEN it is printed and parsed again
- THEN the result equals the first parse

#### Scenario: a bad edit

- GIVEN a song playing
- WHEN a text with an error on line 7 is sent
- THEN the engine reports line 7 and the song plays on unchanged

**Tests:** `crates/dsp/src/song/tests.rs::print_then_parse_is_identity`, `crates/dsp/src/song/tests.rs::never_panics_on_garbage`, `crates/dsp/src/song/tests.rs::every_error_says_where`, `crates/dsp/src/song/tests.rs::the_print_is_canonical_and_parses_back`, `crates/dsp/src/song/tests.rs::set_step_changes_one_step`, `crates/dsp/src/engine.rs::tests::a_drum_lane_hits_on_its_exact_samples`, `crates/dsp/src/engine.rs::tests::each_lane_loops_on_its_own_length`, `crates/dsp/src/engine.rs::tests::a_bad_text_is_reported_and_the_song_plays_on`, `crates/dsp/src/engine.rs::tests::the_song_sets_the_clock_and_tracks_find_a_kit`, `crates/dsp/src/engine.rs::tests::set_step_edits_the_playing_song_and_its_text`, `crates/dsp/src/ffi.rs::tests::song_round_trip_through_the_abi`

### Requirement 7: Deterministic generators [MUST]

A generator SHALL be a function in the notation (`euclid`, `walk`, `arp`, `markov`, `mutate`) that produces a fragment's events from its parameters, a scale and an explicit seed. With the same seed and parameters it SHALL produce the same events. A live fragment SHALL regenerate every cycle; freezing SHALL replace the call with the events it produced, in the same notation.

**Implementation:** `crates/dsp/src/algo.rs` (`Rng`, `Euclid`, `Scale`; `euclid(k,n,rot)` for a drum lane and for notes, `scale <root> <mode>`, `scale <note>` walks); `walk`, `arp`, `markov`, `mutate` and live regeneration *(planned)*

#### Scenario: Euclid

- GIVEN Euclid(k=3, n=8, rotation=0)
- WHEN it generates
- THEN the hits are on steps 0, 3 and 6

**Tests:** `crates/dsp/src/algo.rs::tests::euclid_3_8`, `crates/dsp/src/algo.rs::tests::the_published_euclidean_rhythms`, `crates/dsp/src/algo.rs::tests::rotation_moves_the_pattern_left`, `crates/dsp/src/algo.rs::tests::a_scale_walk_climbs_the_scale`, `crates/dsp/src/song/tests.rs::generators_and_scales_parse_and_print_back`, `crates/dsp/src/song/tests.rs::generator_errors_say_where`, `crates/dsp/src/engine.rs::tests::a_euclid_lane_plays_like_a_written_one`, `crates/dsp/src/engine.rs::tests::a_euclid_note_line_walks_the_scale_deterministically`

### Requirement 8: Score import [SHOULD]

The engine SHALL parse Standard MIDI Files (types 0 and 1) without panicking on malformed input, and MAY convert a file into the notation (ADR-0012): each channel a track, its notes a fragment of classic notes, its tempo changes the song's tempo.

**Implementation:** `crates/dsp/src/smf.rs::parse` (the parser); the conversion into the notation *(planned, MVP 11)*

#### Scenario: the ensemble

- GIVEN a six-part MIDI score
- WHEN it is converted
- THEN the song has six tracks, each with a fragment that plays its part

#### Scenario: malformed files

- GIVEN truncated, oversized or random bytes
- WHEN they are parsed
- THEN the parser returns an error and never panics

**Tests:** `crates/dsp/src/smf.rs::tests::never_panics_on_garbage`, `crates/dsp/src/smf.rs::tests::rejects_what_it_cannot_play`, `crates/dsp/src/smf.rs::tests::running_status_and_zero_velocity_off`, `crates/dsp/src/smf.rs::tests::tempo_and_name`

### Requirement 9: MIDI file playback [SHOULD]

Separately from the song (ADR-0012), the engine SHALL play a loaded MIDI file directly: ticks SHALL become samples once, at load, through the tempo map, and each note SHALL start on its exact sample in `render`. Each MIDI channel SHALL play on one Mono synth or be muted; loading puts the parts on synths 0, 1, 2… in order (spec 004 Req 10). A file that fails to parse SHALL leave the loaded song untouched. Stop SHALL release the player's voices and leave live ones sounding.

**Implementation:** `crates/dsp/src/player.rs::Sequence`, `crates/dsp/src/engine.rs::Engine::load_midi`, `crates/dsp/src/ffi.rs` (`midi_buf`, `midi_load`, `part_*`, `event_*`, `song_length`, `song_bar`, `play`, `stop`, `seek`, `position`, `playing`, `route`, `routed`)

#### Scenario: sample-accurate start

- GIVEN a file with a note at a known tick
- WHEN it is played
- THEN the note starts on the sample the tempo map gives

**Tests:** `crates/dsp/src/player.rs::tests::ticks_become_samples_through_the_tempo_map`, `crates/dsp/src/engine.rs::tests::player_note_starts_on_its_exact_sample`, `crates/dsp/src/engine.rs::tests::every_channel_plays_and_mute_silences`, `crates/dsp/src/engine.rs::tests::bad_files_are_rejected_and_keep_the_old_song`, `crates/dsp/src/engine.rs::tests::stop_releases_player_voices_but_not_live_ones`, `crates/dsp/src/ffi.rs::tests::midi_round_trip_through_the_abi`
