# 002: Composition

Tracks, sources, effects, fragments, the arrangement, generators, the song as text, score import and the clock. Decisions: ADR-0005, ADR-0012 (the song is text). Every requirement is built.

### Requirement 1: A track owns one source [MUST]

A track SHALL play on one synth slot (0–15, strip 0–15), routed by the song (Req 6, Req 11). Each slot SHALL hold exactly one source, chosen by its model: a Mono or Poly model (ADR-0009, specs 005 and 006), a drum machine (the TR-808 or TR-909, below) or a sampler (spec 007), with its own fixed voice pool (ADR-0011) and parameters. Parameters SHALL be addressed as (synth, parameter).

**Implementation:** `crates/dsp/src/song.rs::Track` (a track in the text), `crates/dsp/src/engine.rs::Engine::song_route` (its synth), `crates/dsp/src/poly.rs::Pool` (each synth's voices); the drum machines' pads `crates/dsp/src/drums.rs::Kit` (eighteen pads: bd sn lt mt ht lc mc hc rs cl cp ma cb cy oh ch, the TR-808's sixteen voices, and cr rd, the TR-909's crash and ride; the cowbell two band-limited squares through a band-pass near 850 Hz, the rimshot and claves struck resonators; one voice per pad, the closed hat choking the open hat, per-pad tune, decay, tone, level and an accent); in a synth slot as the TR-808 or the TR-909 model (#148: the same pads, each machine's own sounds; a pad a machine lacks plays its nearest voice with that voice's knobs, so a beat for one plays on the other; `cr` and `rd`, the 909's crash and ride, play the 808's cymbal), `crates/dsp/src/poly.rs::Pool` (`hit`, `PolyVoice::Drum`): a key plays the pad General MIDI puts there (any other key by its place in the octave from 36), on the voice already playing that pad or a new one; a hit has no note-off; the closed hat chokes the open hat; a hit at velocity 0.9 or more is accented. Per-pad tune, decay, tone, level and the accent are synth parameters (`BdTune` … `CbLevel`, `DrumAccent`). Each pad SHALL have an individual out, as the 808's back panel does (#162): `Out` 0 plays it on the kit's strip, panned there by its `Pan` (equal power) into the kit's stereo bus, whose strip `Pan` then balances (#364), 1–8 sends it straight into that group bus, before the group's inserts and panned by its `Pan` (equal power), bypassing the kit's strip (its inserts, fader and mute); soloing the kit SHALL keep the groups its pads go to heard

#### Scenario: a kit slot

- GIVEN a synth slot with the TR-808 model
- WHEN keys 36, 38 and 42 are pressed, the closed hat ten times, or an open hat then a closed hat
- THEN three pads sound on three voices and ring out with no note-off, the closed hat keeps one voice, and the open hat is choked

#### Scenario: the loudest hit

- GIVEN any pad at full level, accented by the most, at any extreme of tune, decay and tone
- WHEN it is rendered offline
- THEN every sample is finite and within ±1, there is no DC, and it is silent within four seconds

**Tests:** `crates/dsp/src/engine/tests.rs::synths_have_their_own_parameters`, `crates/dsp/src/engine/tests.rs::the_song_sets_the_clock_and_tracks_find_a_kit`, `crates/dsp/src/drums/tests.rs::every_pad_at_every_extreme_is_finite_bounded_without_dc_and_ends`, `crates/dsp/src/drums/tests.rs::the_kick_falls_to_its_tune`, `crates/dsp/src/drums/tests.rs::tune_moves_the_pitch_by_semitones`, `crates/dsp/src/drums/tests.rs::the_hats_sit_high`, `crates/dsp/src/drums/tests.rs::the_closed_hat_chokes_the_open_hat`, `crates/dsp/src/drums/tests.rs::the_clap_comes_in_bursts`, `crates/dsp/src/drums/tests.rs::an_accent_is_louder_by_the_amount`, `crates/dsp/src/drums/tests.rs::the_same_hit_gives_the_same_samples`, `crates/dsp/src/drums/tests.rs::every_key_plays_a_pad_and_general_midi_its_own`, `crates/dsp/src/drums/tests.rs::the_cowbell_sits_in_its_band_and_falls_in_two_stages`, `crates/dsp/src/drums/tests.rs::the_claves_ring_at_their_tune_and_stop_short`, `crates/dsp/src/drums/tests.rs::the_congas_sit_above_the_toms`, `crates/dsp/src/engine/tests.rs::a_kit_slot_plays_its_pads`, `crates/dsp/src/engine/tests.rs::a_pad_hit_again_retriggers_its_own_voice`, `crates/dsp/src/engine/tests.rs::the_closed_hat_chokes_the_open_hat`, `crates/dsp/src/engine/tests.rs::a_hard_hit_is_accented_and_the_knobs_reach_the_pads`, `crates/dsp/src/engine/tests.rs::channel_ten_plays_on_a_kit_slot`, `crates/dsp/src/engine/tests.rs::a_pad_on_a_group_goes_only_through_that_group`, `crates/dsp/src/engine/tests.rs::a_pad_is_panned_into_its_group`, `crates/dsp/src/engine/tests.rs::a_pad_is_panned_on_the_kits_own_strip`, `crates/dsp/src/engine/tests.rs::solos_follow_a_kits_individual_outs`, `crates/dsp/src/drums/tests.rs::the_909_kick_settles_faster_at_its_tune`, `crates/dsp/src/drums/tests.rs::the_909s_hats_and_cymbals_sit_high`, `crates/dsp/src/drums/tests.rs::each_machine_stands_in_its_nearest_voice`, `crates/dsp/src/drums/tests.rs::the_909s_closed_hat_chokes_its_open_hat`, `crates/dsp/src/engine/tests.rs::an_808_beat_plays_on_a_909`

### Requirement 2: Fixed mixer [MUST]

The mixer SHALL own every strip parameter (`Level`, `Pan`, `Send1`–`Send4` with their `SendNPre` and `SendNOn` switches, `Mute`, `Solo`, `Out`, `Key` and the insert slots' `I1`–`I3` parameters; `Param::is_strip`). Each synth's strip SHALL pass through its three insert slots, a fader and pan, and four sends (P1–P4), each taken after the fader (the default) or before it and switched on or off without losing its level (`SendNPre`, `SendNOn`, #144; mute silences both kinds), into a master chain of equalizer, compressor and limiter. The equalizer SHALL have a low shelf, two parametric bands (frequency, gain, Q) and a high shelf, with coefficients computed when a parameter changes; a band at 0 dB SHALL be skipped, so a flat equalizer is bit-exact. The insert order MAY change; nothing in the mixer SHALL allocate after init.

The mixer SHALL also hold eight group buses and three insert slots on every strip and group (ADR-0010, epic #62). Strip indices 0–15 are the synths and 16–23 the groups; a strip parameter is the same id on all of them. Each strip and group SHALL have an `Out` (0 is the master, 1–8 a group, 9 nowhere: the strip still feeds its sends and any key, #161); a group SHALL feed only the master or a higher-numbered group, and an invalid route SHALL be ignored. An insert slot SHALL have a type (off, overdrive, distortion, fuzz, EQ, compressor, vocoder) and five knobs A–E in 0..=1 whose meaning depends on the type; every slot SHALL hold every type, allocated at init, and an Off slot SHALL pass the signal bit for bit. A vocoder (#161) SHALL shape its strip's signal (the carrier) by the spectrum of its key: the raw signal of the synth the strip's `Key` names (1–16), taken before that synth's inserts, fader, pan and mute, so any strip can key any other and no route can loop; sixteen 4th-order bands from 100 Hz to 8 kHz with envelope followers, knobs for band shift, release, unvoiced noise, band width and dry. Without a key, or keyed to itself, it SHALL be silent. A solo SHALL keep the groups a soloed strip passes through audible; a mute on a group SHALL silence what it carries.

The four sends SHALL feed four effect processors, P1–P4. Each SHALL have a type (off, echo, reverb, chorus, flanger), a return level and five knobs A–E in 0..=1 whose meaning depends on the type (echo: time, feedback, tone, ping-pong; reverb: size, damping, pre-delay; chorus: rate, depth, delay, spread, tone; flanger: rate, depth, manual, feedback of either sign, tone). A processor P2–P4 SHALL be able to take its input from the processor before it (`P2In`–`P4In`) as well as from its sends, so effects combine in series and a chain cannot loop; P1 has no input setting. A chained processor SHALL hear the previous one's output before its return level, so the previous one's return may be 0, and a processor that is Off SHALL pass nothing on. Chorus and flanger SHALL take the stereo output, echo and reverb its mono sum. The effects SHALL make a wet signal at full level and the slot SHALL scale it by the return. The chorus and flanger SHALL be modulated delays with stereo in and out whose modulator uses no transcendental function per sample, and the flanger's feedback SHALL be clamped within ±0.95. All types of a slot SHALL be allocated at init; switching type SHALL hand the input to the new type and let the old one ring out, without a click or an allocation. The echo SHALL be a stereo delay of up to 2 s set in milliseconds, with feedback clamped below 1, a low-pass in the loop and ping-pong. The reverb SHALL be an 8-line feedback delay network with a Hadamard matrix. With every return at 0 the processors SHALL be silent. The master compressor SHALL be feed-forward and stereo-linked, with threshold, ratio (1 is off), attack, release and make-up, and report its gain reduction; it SHALL work out its gain at control rate, not per sample. The limiter SHALL be the last stage: it never passes ±1 and turns NaN and infinity into silence.

The engine SHALL report peak meters for the view: each synth strip after its fader (0 when muted or silenced by a solo), master left and right after the limiter, and each processor's return, as the highest linear level since the view last read and cleared them (`meters_ptr`, `meters_len`, `meters_clear`).

**Implementation:** `crates/dsp/src/fx/insert.rs::Insert` (insert slots, #58), `crates/dsp/src/fx/vocoder.rs::Vocoder` (#161), `crates/dsp/src/mixer.rs::Mixer` (strips, group buses, routing and sends), `crates/dsp/src/fx/processor.rs::Processor` (P1–P4), `crates/dsp/src/fx/echo.rs::Echo`, `crates/dsp/src/fx/reverb.rs::Reverb`, `crates/dsp/src/fx/chorus.rs::Chorus`, `crates/dsp/src/fx/flanger.rs::Flanger`, `crates/dsp/src/fx/drive.rs::Drive` (the insert's drive types, spec 004 Req 11), `crates/dsp/src/fx/eq.rs::Equalizer`, `crates/dsp/src/fx/compressor.rs::Compressor`, `crates/dsp/src/fx/limiter.rs::Limiter`, `crates/dsp/src/engine.rs::Engine::meters`

#### Scenario: strips

- GIVEN a synth playing
- WHEN its pan is −1, 0 or +1, its level 0, or another synth soloed
- THEN it sounds left only, 3 dB down on both sides, right only, or not at all

#### Scenario: returns

- GIVEN an impulse on the echo send
- WHEN the time is 100 ms and the feedback 0.5
- THEN echoes land every 100 ms, each half the one before

#### Scenario: meters

- GIVEN a synth playing through its strip
- WHEN the view reads the meters and then clears them
- THEN the strip, the master and any fed return show their peaks, an idle or muted strip shows 0, and the fader scales the strip's reading

**Tests:** `crates/dsp/src/engine/tests.rs::meters_read_the_post_fader_peaks_and_start_over`, `crates/dsp/src/engine/tests.rs::muted_and_unsoloed_strips_read_zero`, `crates/dsp/src/fx/eq.rs::tests::flat_is_bit_exact`, `crates/dsp/src/fx/eq.rs::tests::each_band_reaches_its_gain`, `crates/dsp/src/fx/eq.rs::tests::q_narrows_a_band`, `crates/dsp/src/fx/compressor.rs::tests::above_the_threshold_the_rise_is_divided_by_the_ratio`, `crates/dsp/src/fx/compressor.rs::tests::attack_and_release_follow_their_times`, `crates/dsp/src/fx/limiter.rs::tests::nothing_passes_full_scale_and_it_recovers`, `crates/dsp/src/engine/tests.rs::the_master_compressor_turns_a_loud_mix_down`, `crates/dsp/src/fx/processor.rs::tests::switching_type_lets_the_old_tail_ring_out`, `crates/dsp/src/fx/processor.rs::tests::the_echo_knobs_set_time_and_feedback`, `crates/dsp/src/fx/processor.rs::tests::the_reverb_knob_sets_the_size`, `crates/dsp/src/fx/echo.rs::tests::echoes_land_at_the_set_time_and_decay_by_the_feedback`, `crates/dsp/src/fx/echo.rs::tests::feedback_is_clamped_and_the_loop_stays_bounded`, `crates/dsp/src/fx/reverb.rs::tests::the_tail_falls_60_db_in_about_the_size_setting`, `crates/dsp/src/fx/reverb.rs::tests::no_dc_and_always_bounded`, `crates/dsp/src/engine/tests.rs::the_effects_only_sound_through_a_send_and_a_return`, `crates/dsp/src/engine/tests.rs::sixteen_synths_through_both_effects_stay_bounded`, `crates/dsp/src/engine/tests.rs::pan_is_equal_power`, `crates/dsp/src/engine/tests.rs::fader_mute_and_solo`, `crates/dsp/src/engine/tests.rs::sends_follow_the_fader_and_leave_the_mix_alone`, `crates/dsp/src/engine/tests.rs::sends_before_the_fader_and_switched_off`, `crates/dsp/src/fx/vocoder.rs::tests::a_key_in_one_band_lets_only_that_band_of_the_carrier_through`, `crates/dsp/src/fx/vocoder.rs::tests::a_silent_key_gives_silence_and_unvoiced_adds_only_highs`, `crates/dsp/src/fx/vocoder.rs::tests::full_scale_in_stays_bounded_without_dc`, `crates/dsp/src/engine/tests.rs::a_vocoder_follows_its_key_even_muted_or_routed_nowhere`, `crates/dsp/src/engine/tests.rs::a_strip_routed_nowhere_leaves_no_trace_but_feeds_its_sends`, `crates/dsp/src/engine/tests.rs::sixteen_full_synths_stay_bounded_in_stereo`

### Requirement 3: Fragments [MUST]

A fragment SHALL be a loop of events (note or pad, velocity, start, length, probability) on a beat grid, of any length, playing on one track. A drum fragment SHALL be one lane per pad, one step per character: `x` a hit, `X` an accented hit, `o` a ghost note (well under a hit), `f` a flam (one soft grace stroke 20 ms before the hit), `d` a drag (two, 30 and 15 ms before), `.` a rest; on a grid of `/12`, `/16`, `/24`, `/32` or `/48` steps a bar (ADR-0026, #353), each hit on its exact sample, swung with its step, the graces before a hit that stays on its step and closing up to fit between close hits. A pitched fragment SHALL be written in mini-notation (a quoted sequence divides one cycle; `[ ]` subdivides, `~` rests, `*n` repeats, `<a b>` alternates per cycle, `?` plays with a probability, `&` slides into the next note) or as classic notes with durations (`c4:4`, `e4:8.`), laid out one after another; mixing the two in one sequence SHALL be a parse error. A `sampler` track SHALL take lanes of pad names, as a drum track does, for a pad sampler, or note fragments for a multisampler, never both in one fragment; an unknown pad name SHALL be a parse error.

**Implementation:** drum fragments `crates/dsp/src/song.rs::Fragment` (lanes of up to 64 steps, each lane looping on its own length, on the frag's grid), played by `crates/dsp/src/engine.rs::Engine::play_step` and `Engine::lane_hits` (a hit at velocity 0.75, an accent at 1.0, a ghost at 0.35, a grace at 0.4; hits between steps queued, graces a step ahead, ADR-0026); pitched fragments `crates/dsp/src/notes.rs::Notes` (mini-notation and classic durations parsed, printed and compiled to events on 48 ticks to the bar, ADR-0016), held by `crates/dsp/src/song.rs::Fragment` on a `synth` track and played by `crates/dsp/src/engine.rs::Engine::play_tick` on the clock's ticks (`crates/dsp/src/clock.rs::Clock::due_sub`), with a fixed note-off table

#### Scenario: a drum lane

- GIVEN `bd x...x...x...x...` at 120 BPM and 48 kHz
- WHEN one bar is rendered
- THEN the kick starts on samples 0, 24000, 48000 and 72000

#### Scenario: a slide

- GIVEN `"c2& e2"` on a legato synth with glide
- WHEN it is compiled and played
- THEN the first note is one tick longer than its share, so it is still held when the second starts, and the pitch glides to it on one gate

**Tests:** `crates/dsp/src/notes/tests.rs::a_slide_runs_one_tick_into_the_next_note`, `crates/dsp/src/engine/tests.rs::a_slide_glides_into_the_next_note`

#### Scenario: drum grids, ghosts, flams and drags

- GIVEN lanes of x on /12, /16, /24, /32 and /48 at 120 BPM and 48 kHz, and `sn ....f.......d...` on /16
- WHEN they are played
- THEN each lane hits every 96000 / grid samples from 0; the flam's grace falls 960 samples before its hit on 24000 and the drag's two 1440 and 720 before 72000; a ghost and a grace sound softer than a hit

**Tests:** `crates/dsp/src/engine/tests.rs::lanes_hit_on_their_grid`, `crates/dsp/src/engine/tests.rs::mixed_grids_keep_time_and_follow_swing`, `crates/dsp/src/engine/tests.rs::flams_and_drags_put_their_graces_before_the_hit`, `crates/dsp/src/engine/tests.rs::a_flam_on_the_first_step_graces_from_the_second_bar`, `crates/dsp/src/engine/tests.rs::graces_fit_between_close_hits`, `crates/dsp/src/engine/tests.rs::a_grace_is_softer_than_its_hit`, `crates/dsp/src/engine/tests.rs::a_ghost_note_is_softer_than_a_hit`, `crates/dsp/src/engine/tests.rs::a_grid_prints_back_and_a_bad_one_is_refused`, `crates/dsp/src/song/tests.rs::a_ghost_note_parses_and_prints_back`

#### Scenario: classic durations

- GIVEN `"c4:4 e4:8 g4:8 c5:2"`
- WHEN it is compiled
- THEN the notes start on beats 0, 1, 1.5 and 2, and the fragment is one bar long

**Tests:** `crates/dsp/src/notes/tests.rs::a_quoted_sequence_divides_the_bar`, `crates/dsp/src/notes/tests.rs::brackets_subdivide_and_three_is_a_triplet`, `crates/dsp/src/notes/tests.rs::classic_durations_lay_notes_one_after_another`, `crates/dsp/src/notes/tests.rs::print_then_parse_is_identity`, `crates/dsp/src/notes/tests.rs::every_error_says_where`, `crates/dsp/src/notes/tests.rs::it_never_panics_on_garbage`, `crates/dsp/src/song/tests.rs::note_fragments_parse_and_print_back`, `crates/dsp/src/song/tests.rs::note_errors_say_line_and_column`, `crates/dsp/src/engine/tests.rs::note_fragments_sound_at_their_samples_and_pitches`, `crates/dsp/src/engine/tests.rs::a_note_lasts_its_written_length`, `crates/dsp/src/engine/tests.rs::a_chord_uses_the_voice_pool`, `crates/dsp/src/engine/tests.rs::a_triplet_lands_between_the_sixteenths`, `crates/dsp/src/song/tests.rs::sampler_tracks_hold_lanes_or_notes`, `crates/dsp/src/song/tests.rs::sampler_errors_say_where`, `crates/dsp/src/engine/tests.rs::a_sampler_track_with_lanes_plays_the_pad_sampler`, `crates/dsp/src/engine/tests.rs::a_sampler_track_with_notes_plays_the_multisampler_at_pitch`

### Requirement 4: The arrangement [MUST]

An arrangement SHALL be a list of sections, each a number of bars and the fragments that play in it; a fragment SHALL loop inside its section. A section MAY repeat an earlier one by name. A loop region SHALL repeat a range of bars.

In the text (ADR-0015): `section <name> <bars>: <frags…>` (1 to 256 bars, the frags defined above it, none for silent bars), one `arrange <sections…>` line, and `loop <first> <last>` over the arrangement's bars, counted from 1 and inclusive. A bar is sixteen clock steps. A fragment SHALL start on its section's first step, each lane looping on its own length and cut at the section's end. Inside a loop region the song SHALL wrap from its last bar to its first; after the arrangement's last bar the song SHALL stop and go back to the top. A song without `arrange` SHALL play every fragment as a loop. The engine SHALL report the entry playing and the steps into it, the arrangement's length in bars, and SHALL seek to a bar. A fragment MAY be *cued* (#375): it then plays alone from its first bar, looping on its own length, without the arrangement's sections, scenes, automation lanes or end, while the song's modulations and its own parameter methods go on; the cue SHALL follow its fragment by name through a new song text and end when the fragment goes, and Stop SHALL end it, leaving the song as it was.

**Implementation:** `crates/dsp/src/song.rs::Song::at`, `crates/dsp/src/song.rs::Section`, `crates/dsp/src/engine.rs::Engine::play_step`, `crates/dsp/src/engine.rs::Engine::song_cue`, `crates/dsp/src/ffi.rs` (`song_seek_bar`, `song_bars`, `song_entry`, `song_local`, `song_cue`, `song_cued`)

#### Scenario: sections in order

- GIVEN `section one 1: a`, `section two 1: b` and `arrange one two`
- WHEN the song plays
- THEN `a` plays bar 1 and `b` bar 2, each from its first step on the exact sample, and the song stops after bar 2

#### Scenario: a loop region

- GIVEN `arrange one two one` and `loop 2 2`
- WHEN the song passes bar 2
- THEN bar 2 plays again, without end

**Tests:** `crates/dsp/src/song/tests.rs::sections_and_the_arrangement_parse_and_print`, `crates/dsp/src/song/tests.rs::a_step_falls_in_its_section_and_the_loop_wraps`, `crates/dsp/src/song/tests.rs::arrangement_errors_say_where`, `crates/dsp/src/engine/tests.rs::sections_play_in_order_and_the_song_ends`, `crates/dsp/src/engine/tests.rs::a_fragment_restarts_with_its_section`, `crates/dsp/src/engine/tests.rs::the_loop_region_repeats_and_seek_lands_on_a_bar`, `crates/dsp/src/engine/tests.rs::a_cued_fragment_plays_alone`, `crates/dsp/src/engine/tests.rs::a_cue_follows_its_fragment_through_an_edit`

### Requirement 5: Sample-accurate clock in the engine [MUST]

The engine SHALL run the transport (tempo, swing, play, pause, stop, position) inside `render` and fire events on their exact sample. The UI SHALL NOT schedule notes. The clock SHALL be the only clock and play the song (ADR-0022): `song_play` SHALL go on from where the song paused or stopped, `song_pause` SHALL hold the place and release the song's notes, and `song_stop` SHALL go back to the top. The clock SHALL count sixteenth steps, compute each step's sample from its index (no drift), take a tempo change from the next step on without moving that step's place on the grid, and delay odd steps by swing (50% straight to 75%). Tempo and swing belong to the song (ADR-0012), not to the parameter registry.

**Implementation:** `crates/dsp/src/clock.rs::Clock`, `crates/dsp/src/engine.rs::Engine::render` (blocks split at clock steps), `crates/dsp/src/ffi.rs` (`tempo`, `swing`, `clock_step`, `song_play`, `song_pause`, `song_stop`, `song_playing`)

#### Scenario: tempo accuracy

- GIVEN 120 BPM at 48 kHz and a pattern of sixteenth notes
- WHEN 4 bars are rendered offline
- THEN note onsets fall exactly 6000 samples apart

**Tests:** `crates/dsp/src/clock.rs::tests::sixteenths_at_120_bpm`, `crates/dsp/src/clock.rs::tests::swing_delays_the_off_beats`, `crates/dsp/src/clock.rs::tests::a_tempo_change_keeps_the_next_step`, `crates/dsp/src/clock.rs::tests::a_swing_change_moves_the_next_off_beat_not_the_grid`, `crates/dsp/src/clock.rs::tests::no_drift_over_a_thousand_bars`, `crates/dsp/src/clock.rs::tests::seek_lands_on_the_next_step`, `crates/dsp/src/engine/tests.rs::clock_steps_land_on_their_samples_through_render`, `crates/dsp/src/engine/tests.rs::pause_holds_the_place_and_stop_goes_to_the_top`, `crates/dsp/src/engine/tests.rs::pause_releases_song_voices_but_not_live_ones`, `crates/dsp/src/ffi.rs::tests::midi_import_and_the_transport_through_the_abi`

### Requirement 6: The song is text [MUST]

The view SHALL send the song to the engine as text (ADR-0012). The engine SHALL parse and compile it outside `render`; `render` SHALL only read the compiled song. A text that does not parse SHALL be rejected with a line, a column and a message, and the song that is playing SHALL keep playing; a new song SHALL play at once when the song is stopped, and from the next bar line while it plays (#208, Sonic Pi's `live_loop`), each lane keeping its place against the clock; the old song SHALL play on until then. The load SHALL prepare the new song, its live buffers and track routes outside `render`, so taking over on the bar line only moves them, and the old song SHALL be freed at the next load, never in `render` (ADR-0002); an edit from a view before the bar line SHALL edit the new song. The engine SHALL report the takeover (`song_taken`) so the view draws the song that plays. The parser SHALL never panic. The engine SHALL print a song canonically, and parsing a printed song SHALL give the same song. The printed text SHALL keep the song's `#` comments, each with the item it is above or beside (`crates/dsp/src/song/comments.rs::Comments`, #199); a comment whose item is gone SHALL move to the end, never be lost. Edits from a view (`set_step`) SHALL change the song in the engine, which returns the printed text. A track SHALL be routed to a synth as a MIDI channel is: on load a track without a synth goes to the first drum kit, and the view MAY route it elsewhere or mute it.

**Implementation:** `crates/dsp/src/song.rs::Song` (`parse`, `print`, `set_step`), `crates/dsp/src/engine.rs::Engine::load_song`, `crates/dsp/src/ffi.rs` (`song_buf`, `song_load`, `song_error_*`, `song_text_*`, `set_step`, `song_tracks`, `song_route`, `song_routed`, `song_frags`, `frag_*`, `lane_*`, `step_level`, `song_taken`), `crates/dsp/src/engine.rs::Engine::commit_song`, `web/public/worklet.js` (`loadSong`, `sendSong`)

#### Scenario: round trip

- GIVEN any song the parser accepts
- WHEN it is printed and parsed again
- THEN the result equals the first parse

#### Scenario: an edit takes over at the bar

- GIVEN a song with four kicks a bar playing, a third of the way into its first bar
- WHEN a song with one kick a bar is loaded
- THEN the four kicks play to the end of the bar and the new song plays from the next

#### Scenario: a bad edit

- GIVEN a song playing
- WHEN a text with an error on line 7 is sent
- THEN the engine reports line 7 and the song plays on unchanged

**Tests:** `crates/dsp/src/song/tests.rs::print_then_parse_is_identity`, `crates/dsp/src/song/tests.rs::never_panics_on_garbage`, `crates/dsp/src/song/tests.rs::every_error_says_where`, `crates/dsp/src/song/tests.rs::the_print_is_canonical_and_parses_back`, `crates/dsp/src/song/tests.rs::set_step_changes_one_step`, `crates/dsp/src/song/tests.rs::comments_survive_print_and_parse`, `crates/dsp/src/song/tests.rs::a_comment_stays_with_its_item_through_an_edit`, `crates/dsp/src/song/tests.rs::a_comment_whose_item_is_gone_moves_to_the_end`, `crates/dsp/src/engine/tests.rs::a_drum_lane_hits_on_its_exact_samples`, `crates/dsp/src/engine/tests.rs::each_lane_loops_on_its_own_length`, `crates/dsp/src/engine/tests.rs::a_bad_text_is_reported_and_the_song_plays_on`, `crates/dsp/src/engine/tests.rs::the_song_sets_the_clock_and_tracks_find_a_kit`, `crates/dsp/src/engine/tests.rs::set_step_edits_the_playing_song_and_its_text`, `crates/dsp/src/engine/tests.rs::a_song_loaded_while_playing_takes_over_at_the_next_bar`, `crates/dsp/src/engine/tests.rs::an_edit_before_the_bar_line_edits_the_new_song`, `crates/dsp/tests/render_no_alloc.rs::a_busy_song_renders_without_allocating`, `crates/dsp/src/ffi.rs::tests::song_round_trip_through_the_abi`

### Requirement 7: Deterministic generators [MUST]

A generator SHALL be a function in the notation (`euclid`, `walk`, `arp`, `markov`, `mutate`) that produces a fragment's events from its parameters, a scale and an explicit seed. With the same seed and parameters it SHALL produce the same events. A live fragment SHALL regenerate every cycle; freezing SHALL replace the call with the events it produced, in the same notation. `scale <root> <mode>` SHALL set the song's scale, which `walk` and a Euclid note line follow; the modes SHALL be `major`, `minor`, `dorian`, `phrygian`, `lydian`, `mixolydian`, `locrian`, `pentatonic`, `blues`, `phrygian-dominant` and `harmonic-minor` (#201), and any other mode SHALL be a parse error that lists them.

**Implementation:** `crates/dsp/src/algo.rs` (`Rng`, `Euclid`, `Scale`; `euclid(k,n,rot)` for a drum lane and for notes, `scale <root> <mode>`, `scale <note>` walks); `arp`, `walk`, `markov` and `mutate` in `crates/dsp/src/notes/generate.rs::Gen` (`arp([c4,e4,g4],up,16)`, `walk(c4,8,1)`, `markov(1,riff,3)`, `mutate(riff,30,5)`: seeded, at most 32 notes in a walk, 8 in a chord, a pitch range of two octaves for walks, pitches of the source for markov); `live` frags (`frag w = lead live`) regenerate each cycle from `mix(base seed, cycle)` into buffers reserved at load, `crates/dsp/src/engine.rs::Engine::step_live`; the pattern itself is a pure function in `crates/dsp/src/arp.rs::arp_note` (held notes, mode, octaves, step, seed), shared by the notation's `arp` and by the live arpeggiator of every Mono and Poly synth (#110): `ArpOn`, `ArpMode` (up, down, up-down, as played, random), `ArpOctaves`, `ArpRate` (1/8, 1/16, 1/8T, 1/16T), `ArpGate`, `ArpLatch`, `ArpFree`, `ArpSeed`; with it on, `Engine::note_on` feeds the synth's held set (a fixed array) and `Engine::arp_tick` plays it on the clock's tick grid, starting at its first note on the next step; with the clock stopped it holds still, or with `ArpFree` runs on its own grid; `freeze` (`crates/dsp/src/notes.rs::freeze`, `crates/dsp/src/engine.rs::Engine::freeze`, C ABI `freeze`, `frag_live`) writes the events back as mini-notation

#### Scenario: Euclid

- GIVEN Euclid(k=3, n=8, rotation=0)
- WHEN it generates
- THEN the hits are on steps 0, 3 and 6

#### Scenario: a raised mode

- GIVEN `scale e phrygian-dominant`
- WHEN a walk climbs eight notes from E4
- THEN it plays e f g♯ a b c d e

**Tests:** `crates/dsp/src/algo.rs::tests::euclid_3_8`, `crates/dsp/src/algo.rs::tests::the_published_euclidean_rhythms`, `crates/dsp/src/algo.rs::tests::rotation_moves_the_pattern_left`, `crates/dsp/src/algo.rs::tests::a_scale_walk_climbs_the_scale`, `crates/dsp/src/algo.rs::tests::the_raised_modes_walk_their_raised_notes`, `crates/dsp/src/song/tests.rs::generators_and_scales_parse_and_print_back`, `crates/dsp/src/song/tests.rs::generator_errors_say_where`, `crates/dsp/src/engine/tests.rs::a_euclid_lane_plays_like_a_written_one`, `crates/dsp/src/engine/tests.rs::a_euclid_note_line_walks_the_scale_deterministically`, `crates/dsp/src/arp.rs::tests::each_mode_orders_a_held_chord`, `crates/dsp/src/arp.rs::tests::octaves_extend_the_pattern`, `crates/dsp/src/arp.rs::tests::random_repeats_with_its_seed`, `crates/dsp/src/arp.rs::tests::latch_keeps_the_chord_until_a_new_one`, `crates/dsp/src/arp.rs::tests::gate_sets_the_note_length`, `crates/dsp/src/engine/tests.rs::the_arp_steps_on_the_clock_from_the_next_step`, `crates/dsp/src/engine/tests.rs::the_gate_is_a_fraction_of_the_step`, `crates/dsp/src/engine/tests.rs::latch_keeps_playing_after_the_keys_go`, `crates/dsp/src/engine/tests.rs::a_stopped_clock_silences_the_arp_unless_it_runs_free`, `crates/dsp/src/engine/tests.rs::turning_the_arp_off_lets_go_and_live_input_plays_again`, `crates/dsp/src/notes/tests.rs::an_arpeggio_cycles_the_chord_at_its_rate`, `crates/dsp/src/notes/tests.rs::a_walk_stays_on_the_scale_and_in_range`, `crates/dsp/src/notes/tests.rs::markov_keeps_the_rhythm_and_the_pitch_set`, `crates/dsp/src/notes/tests.rs::mutate_changes_about_the_amount`, `crates/dsp/src/notes/tests.rs::generator_errors_say_where`, `crates/dsp/src/notes/tests.rs::damaged_generator_calls_never_panic`, `crates/dsp/src/song/tests.rs::generator_calls_read_earlier_frags_and_print_back`, `crates/dsp/src/engine/tests.rs::a_live_fragment_changes_each_cycle_and_repeats_each_run`, `crates/dsp/src/engine/tests.rs::a_live_fragment_does_not_grow_its_buffers`, `crates/dsp/src/engine/tests.rs::freezing_keeps_the_bar_that_was_playing`, `crates/dsp/src/notes/tests.rs::freezing_writes_the_events_back_exactly`, `crates/dsp/src/notes/tests.rs::freezing_handles_chords_rests_and_several_bars`, `crates/dsp/src/notes/tests.rs::freezing_refuses_what_it_cannot_write`, `crates/dsp/src/song/tests.rs::a_live_frag_prints_and_parses_back`, `crates/dsp/src/song/tests.rs::freezing_replaces_the_call_with_notes`

### Requirement 8: Score import [SHOULD]

The engine SHALL parse Standard MIDI Files (types 0 and 1) without panicking on malformed input, and SHALL convert the loaded file into the song text (ADR-0015, #173): each channel with notes a `synth` track named after the file's track it came from, the tracks in channel order on synths 0, 1, 2… (ADR-0022); its notes, snapped to the grid of 48 ticks to the bar, as fragments of timed notes (`pitch@start:length:velocity`, ADR-0016) with their bars given (`bars N`); the song cut into sections of 8 bars (4, 2 or 1 when a chunk holds more than a line's notes or the song more fragments than it may hold), identical chunks sharing a fragment and identical sections a section, played by `arrange`, which SHALL last until the last note ends; the tempo the file's first, with every tick placed by its time through the file's tempo map, so a tempo change keeps its notes at their moments (#295). Bars SHALL be 4/4: a 3/4 file, or one whose tempo changes, keeps its timing and not its bar lines. Channel 10 SHALL be a `synth` track like the others (a drum track holds no timed notes); on a kit it plays the General MIDI pads. A file the song cannot hold (its tracks, fragments, sections or bars), or without notes, SHALL be refused with a code. The converted text SHALL parse and print back unchanged.

**Implementation:** `crates/dsp/src/smf.rs::parse` (the parser), `crates/dsp/src/midi_import.rs::import` (the conversion), `crates/dsp/src/engine.rs::Engine::import_midi`, `crates/dsp/src/ffi.rs` (`midi_import`), `crates/dsp/src/notes.rs` (`Seq::Timed`), `web/src/audio/engine.ts::loadMidi` (opening a file imports it)

#### Scenario: the demo as a song

- GIVEN the demo Canon
- WHEN it is imported as the song and the song plays
- THEN the synth of each channel starts every note of the file at its time in the file

#### Scenario: a repeat

- GIVEN a part whose bars 17–24 repeat bars 1–8
- WHEN it is imported
- THEN the two chunks share one fragment and one section, and `arrange` plays it twice

#### Scenario: malformed files

- GIVEN truncated, oversized or random bytes
- WHEN they are parsed
- THEN the parser returns an error and never panics

**Tests:** `crates/dsp/src/smf.rs::tests::never_panics_on_garbage`, `crates/dsp/src/smf.rs::tests::rejects_what_it_cannot_play`, `crates/dsp/src/smf.rs::tests::running_status_and_zero_velocity_off`, `crates/dsp/src/smf.rs::tests::tempo_and_name`, `crates/dsp/src/midi_import/tests.rs::notes_land_on_the_grid_with_their_lengths`, `crates/dsp/src/midi_import/tests.rs::identical_chunks_share_a_fragment_and_a_section`, `crates/dsp/src/midi_import/tests.rs::a_dense_part_gets_shorter_chunks`, `crates/dsp/src/midi_import/tests.rs::no_notes_is_an_error_and_names_are_made_safe`, `crates/dsp/src/midi_import/tests.rs::never_panics_on_odd_files`, `crates/dsp/src/midi_import/tests.rs::a_held_last_note_rings_out`, `crates/dsp/src/midi_import/tests.rs::a_tempo_change_keeps_notes_at_their_moments`, `crates/dsp/src/engine/tests.rs::an_imported_held_note_rings_to_its_end`, `crates/dsp/src/engine/tests.rs::the_demo_imported_plays_the_file`, `crates/dsp/src/engine/tests.rs::demo_file_imports`, `crates/dsp/src/notes/tests.rs::timed_notes_parse_compile_and_print`

### Requirement 9: A MIDI file is imported, not played [SHOULD]

There SHALL be no MIDI player beside the song (ADR-0022): opening a MIDI file SHALL import it as the song (Req 8) and it SHALL play from the one transport (Req 5), each note starting on its exact sample. A file that fails to parse or import SHALL leave the loaded song untouched. A track SHALL play on one synth, of any model, or be muted.

**Implementation:** `crates/dsp/src/engine.rs::Engine::import_midi`, `crates/dsp/src/ffi.rs` (`midi_buf`, `midi_import`), `web/public/worklet.js` (`importMidi`)

#### Scenario: sample-accurate start

- GIVEN a file with a note at a known tick
- WHEN it is imported and the song plays
- THEN the note starts on the sample its tick gives at the file's tempo

**Tests:** `crates/dsp/src/engine/tests.rs::an_imported_note_starts_on_its_exact_sample`, `crates/dsp/src/engine/tests.rs::an_imported_track_plays_on_its_routed_synth`, `crates/dsp/src/engine/tests.rs::bad_files_are_rejected_and_keep_the_old_song`, `crates/dsp/src/ffi.rs::tests::midi_import_and_the_transport_through_the_abi`

### Requirement 10: Scenes and automation [SHOULD]

The song SHALL move any parameter but the model and `Out` over time (ADR-0015, #172). `auto <name> = <target>.<Param> <values…> /<bars>` SHALL spread its values evenly over its bars and `… ramp <a> <b> /<bars>` SHALL move linearly between two; a lane SHALL be placed in sections like a fragment and loop inside them, and without an arrangement every lane SHALL loop. `scene <name>: <target>.<Param> <value>, …` SHALL set its values on the first step of a section that lists `[name]`. A target SHALL be a track (its routed synth), `strip1`–`strip16`, `group1`–`group8` or `master`, and a parameter its registry name in any case (`kit.cutoff` is `kit.Cutoff`, ADR-0019), printed as the registry writes it; a parameter that does not belong to its target SHALL be a parse error. The engine SHALL apply lanes once per block through `set_param`, writing a lane only when its value changes, never allocating in `render`; it SHALL report the strips automation changed so the view can show their values.

**Implementation:** `crates/dsp/src/song.rs::Auto`, `crates/dsp/src/song.rs::Scene`, `crates/dsp/src/engine.rs::Engine::run_automation`, `crates/dsp/src/engine.rs::Engine::apply_scenes`, `crates/dsp/src/ffi.rs` (`auto_touched`), `web/public/worklet.js`

#### Scenario: a scene on its bar

- GIVEN `scene s: strip1.Send2 0.25` in the second section of `arrange one two`
- WHEN the song plays
- THEN the send changes on the first sample of bar 2

#### Scenario: automation equals a hand

- GIVEN one engine automating `strip1.Level 0.3` and another with the level set to 0.3 by hand
- WHEN both play the same song
- THEN their output is bit-identical

**Tests:** `crates/dsp/src/song/tests.rs::automation_and_scenes_parse_and_print`, `crates/dsp/src/song/tests.rs::a_lane_steps_or_ramps_over_its_length_and_loops`, `crates/dsp/src/song/tests.rs::automation_errors_say_where`, `crates/dsp/src/song/tests.rs::a_parameter_name_is_read_in_any_case`, `crates/dsp/src/params.rs::tests::names_are_found_in_any_case_and_never_collide`, `crates/dsp/src/engine/tests.rs::a_scene_lands_on_its_sections_first_sample`, `crates/dsp/src/engine/tests.rs::a_ramp_reaches_its_end_value`, `crates/dsp/src/engine/tests.rs::automation_is_bit_identical_to_a_hand_set_value`

### Requirement 11: A track picks its synth and settings [SHOULD]

A track SHALL name its synth after its kind (#210): `track <name> <kind> <model> <preset>` with a factory preset of that model, `<model>` alone, or a setting. `setting <name> = <model> <preset>: <Param> <value>, …` SHALL be a patch that lives only in the song: a factory preset and changes to the synth's own parameters, written before the tracks. Without a model the composer SHALL pick one from the track's role: chords make a pad; a name with bass, pad, arp, keys or lead says it; otherwise only arps make an arp, notes mostly below C3 a bass, and anything else a lead. A drum track SHALL play a free kit already in the rack (an 808, a 909 or a pad sampler) and name it in the text; without one it SHALL get an 808 kit, or a 909 when its name says 909. A sampler track without a model SHALL keep the samples already loaded. The pick SHALL be printed in the canonical text. A model that does not play the track's kind, or a preset of another model, SHALL be a parse error. On load the engine SHALL route each new track to a synth of its own (an unclaimed one already on the patch's model, else the first unclaimed) and set the patch on it; a reload SHALL set it again only when the text changes the patch. A MIDI import SHALL set every part's patch on the synth it routes the part to (synths 0, 1, 2…), so the synths are what its text says, as a reload of that text gives (#327). A track line MAY end with `mute` and `solo` (#355): a frag plays only while its track is heard, which with any track soloed means soloed (solo outranks mute) and otherwise not muted; drum lanes, hits between steps and note frags alike, the synth itself untouched. Muting a track SHALL let its sounding notes go, and setting the flags from the view SHALL rewrite the text.

**Implementation:** `crates/dsp/src/song.rs::Setting`, `crates/dsp/src/song.rs::Song::patch`, `crates/dsp/src/engine.rs::Engine::load_song`, `web/public/worklet.js`, `web/src/audio/engine.ts::applySong`

#### Scenario: a song sets up its synths

- GIVEN `track kit drums`, `track lead synth` and `track bass synth` on a fresh engine
- WHEN the song loads
- THEN the tracks play on synths 0, 1 and 2 as an 808 kit, a ProOne lead and a Minimoog bass, and the text says so

#### Scenario: a knob survives a re-parse

- GIVEN a track playing a setting, and its synth's cutoff turned by hand
- WHEN the same text loads again
- THEN the cutoff stays, and it is set again only when the setting's text changes

**Tests:** `crates/dsp/src/engine/tests.rs::a_midi_import_sets_the_synths_its_text_names`, `crates/dsp/src/engine/tests.rs::a_picked_drum_track_plays_the_kit_in_the_rack`, `crates/dsp/src/song/tests.rs::a_track_without_a_model_gets_one_for_its_role`, `crates/dsp/src/song/tests.rs::a_model_alone_gets_its_preset_for_the_role`, `crates/dsp/src/song/tests.rs::settings_parse_and_print_back`, `crates/dsp/src/song/tests.rs::model_and_setting_errors_say_where`, `crates/dsp/src/engine/tests.rs::each_track_gets_its_own_synth_and_patch`, `crates/dsp/src/song/tests.rs::track_mute_and_solo_parse_print_and_decide_who_is_heard`, `crates/dsp/src/engine/tests.rs::a_muted_track_is_silent_and_its_synth_plays_on`, `crates/dsp/src/engine/tests.rs::solo_plays_only_the_soloed_tracks`, `crates/dsp/src/engine/tests.rs::muting_a_track_lets_its_notes_go_and_writes_the_text`

### Requirement 12: Song limits [MUST]

A song SHALL be bounded so the engine can reserve its memory at load and never allocate in `render` (ADR-0002). A text over 1 MiB SHALL be refused before it is parsed. A name (of a track, frag, section, auto, scene or setting) at most 32 characters, a letter then letters, digits or `_`. A song SHALL hold at most 16 tracks, 16 settings of at most 32 changes each, 256 frags, 256 sections of 1 to 256 bars, 256 entries in its arrangement, 32 automation lanes of at most 64 values, 32 scenes of at most 32 settings, and 32 modulations whose signals hold at most 256 nodes together, nested at most 32 deep, and 16 voices of at most 32 nodes each (spec 005 Req 11). A drum lane SHALL have at most 64 steps; a note fragment at most 512 events over at most 32 bars, with brackets nested at most four deep; a walk at most 32 notes and a chord at most 8; a Euclid pattern at most 64 pulses. Past any other limit the text SHALL be refused with a line, a column and a message, never truncated, and the playing song SHALL play on (Req 6).

**Implementation:** `crates/dsp/src/song.rs` (`MAX_TEXT`, `MAX_TRACKS`, `MAX_FRAGS`, `MAX_STEPS`, `MAX_SECTIONS`, `MAX_ARRANGE`, `MAX_BARS`, `MAX_AUTOS`, `MAX_VALUES`, `MAX_SCENES`, `MAX_SETS`, `MAX_MODS`, `MAX_VOICES`), `crates/dsp/src/song/signal.rs` (`MAX_NODES`), `crates/dsp/src/notes.rs` (`MAX_EVENTS`, `MAX_BARS`), `crates/dsp/src/notes/generate.rs::Gen`, `crates/dsp/src/algo.rs` (`MAX_PULSES`), `crates/dsp/src/engine.rs::Engine::song_buffer`

#### Scenario: one track too many

- GIVEN a text with 17 `track` lines
- WHEN it is parsed
- THEN it is refused on line 17 with "a song has at most 16 tracks"

#### Scenario: a lane too long

- GIVEN a drum lane of 65 steps
- WHEN it is parsed
- THEN it is refused with "a lane has at most 64 steps"

**Tests:** `crates/dsp/src/song/tests.rs::limits_hold`, `crates/dsp/src/notes/tests.rs::too_much_is_an_error_not_a_hang`, `crates/dsp/src/engine/tests.rs::a_live_fragment_does_not_grow_its_buffers`, `crates/dsp/tests/render_no_alloc.rs::a_busy_song_renders_without_allocating`

### Requirement 13: Chords by name [SHOULD]

A pitched fragment SHALL take chords by name (#103), in mini-notation and in classic notes. A symbol is a root `a`–`g` with `#` or `b`, an optional octave (4 when left out) and `:quality`, one of `maj m 7 maj7 m7 m7b5 dim dim7 aug sus2 sus4 6 m6 9 m9 add9`; a bare root SHALL be a major triad, and a root with an octave and no quality SHALL stay a single note (`c3`). In classic notes the last `:` SHALL be the duration's (`c:m7:2`). A roman numeral SHALL be a degree of the song's seven-note scale, shifted by `b` or `#`, its case the quality (upper major, lower minor), then `o`, `o7`, `+`, `7` or `maj7`; without such a scale it SHALL be a parse error. `arp` SHALL take a chord by name, or the name of an earlier frag, whose chords it SHALL arpeggiate one after another, each from its own start; `root(frag[,octave])` SHALL play the lowest pitch class of each chord of an earlier frag, in octave 2 unless given; `prog(bars,seed)` SHALL write a seeded progression of one diatonic triad a bar on the song's seven-note scale, starting on the tonic, ending on the dominant, and moving between tonic, subdominant and dominant functions. A name SHALL print back as written. `frag … voicing` SHALL move each chord to the octave of each note nearest the previous chord, within C3–C6, keeping its pitch classes; it is for written notes, not drum or live frags.

**Implementation:** `crates/dsp/src/notes/chord.rs`, `crates/dsp/src/notes.rs::Symbol`, `crates/dsp/src/algo.rs::Scale::degree`, `crates/dsp/src/song.rs::Fragment` (`voicing`), `crates/dsp/src/song/lex.rs`

#### Scenario: a progression follows the key

- GIVEN `"<i VI III VII>"` after `scale c minor`
- WHEN it is compiled
- THEN it plays Cm, Ab, Eb and Bb, one a bar; after `scale a minor` the same text plays Am, F, C and G

#### Scenario: voicing keeps the hands close

- GIVEN `"<I vi IV V7 iii vi ii7 V>"` in C major on a frag with `voicing`
- WHEN it is compiled
- THEN every note of each chord is within a fifth of a note of the chord before, between C3 and C6

**Tests:** `crates/dsp/src/notes/tests.rs::chord_symbols_play_their_notes`, `crates/dsp/src/notes/tests.rs::a_classic_chord_takes_its_duration_last`, `crates/dsp/src/notes/tests.rs::chord_names_print_as_written`, `crates/dsp/src/notes/tests.rs::numerals_follow_the_song_key`, `crates/dsp/src/notes/tests.rs::a_numeral_says_its_quality_in_its_case`, `crates/dsp/src/notes/tests.rs::chord_errors_say_where`, `crates/dsp/src/notes/tests.rs::voicing_moves_each_chord_to_the_nearest_inversion`, `crates/dsp/src/notes/tests.rs::an_arp_takes_a_chord_by_name`, `crates/dsp/src/song/tests.rs::a_voiced_progression_in_the_key_prints_back`, `crates/dsp/src/song/tests.rs::voicing_errors_say_where`, `crates/dsp/src/song/lex.rs::tests::chord_names_are_notes`, `crates/dsp/src/notes/tests.rs::root_plays_the_bass_of_each_chord`, `crates/dsp/src/notes/tests.rs::an_arp_over_a_progression_follows_its_chords`, `crates/dsp/src/notes/tests.rs::a_prog_walks_the_functions_from_tonic_to_dominant`, `crates/dsp/src/notes/tests.rs::progression_errors_say_where`, `crates/dsp/src/song/tests.rs::a_progression_feeds_pad_bass_and_arp`

### Requirement 14: The song sets the mix [SHOULD]

The song SHALL give the mixer its starting values (ADR-0018, #214): `strip <track|stripN>: <Param> <value>, …` for the strip of a track's synth or a strip by number, `group <n> [name]: …` for a group bus and `master: …` for the global parameters, by registry name. Insert and processor types and `Out` SHALL take their names (`I1Type Overdrive`, `P1Type Echo`, `Out group2`); a parameter of another owner, an unknown name, a group routed to a lower group or a second line for one strip SHALL be a parse error with a line and a column. On load the engine SHALL set a value when its text differs from the playing song's, or on a strip its track has just moved to, and leave the others as they are; a line taken out SHALL change nothing. The printer SHALL write the lines after the tracks, and `write_mixer` SHALL print the mixer as these lines, the values that differ from the defaults, a group keeping its name.

**Implementation:** `crates/dsp/src/song.rs::MixLine`, `crates/dsp/src/song.rs::Mix`, `crates/dsp/src/engine.rs::Engine::apply_mix`, `crates/dsp/src/engine.rs::Engine::write_mixer` (called by `Engine::fold`, Req 17)

#### Scenario: a fader moved by hand holds

- GIVEN a song with `strip bass: Level 0.8` playing, and the bass fader moved to 0.3
- WHEN the text is applied again with only the master gain changed
- THEN the fader stays at 0.3 and the master gain takes its new value

#### Scenario: the mix writes itself into the song

- GIVEN a mix changed by hand on a track's strip, another strip, a group and the master
- WHEN the hand's changes are folded into the song (Req 17) and the text is loaded on a fresh engine
- THEN the fresh engine has the same mix

**Tests:** `crates/dsp/src/song/tests.rs::mixer_lines_parse_and_print_back`, `crates/dsp/src/song/tests.rs::mixer_errors_say_where`, `crates/dsp/src/song/tests.rs::a_comment_on_the_master_line_stays_with_it`, `crates/dsp/src/engine/tests.rs::mixer_lines_set_the_mix_and_hold_a_hand`, `crates/dsp/src/engine/tests.rs::the_mixer_writes_itself_into_the_song`

### Requirement 15: Modulation by signals [SHOULD]

The song SHALL modulate any parameter that automation reaches (Req 10) with a signal (ADR-0019, #208): `mod <target>.<param> = <signal>` for the whole song, or a parameter method on a fragment's line, `frag <name> = <track> … .<param>(<signal>) …`, on its track's synth and strip while the fragment plays (#204). A signal SHALL be a number, a source from 0 to 1 (`sine saw tri square` once per bar, `rand` a new value each sixteenth, `perlin` a smooth random curve, `lfo(rate, shape)` in hertz), a sequence of numbers in mini-notation (`"<300 800>"` one a bar, `"0 1"` sharing the bar), combined with `+ - * /` and shaped by `.range(a, b)`, `.exprange(a, b)`, `.slow(n)`, `.fast(n)`, `.segment(n)` and `.lag(seconds)`. A signal SHALL be a function of the song's position, seeded where random (ADR-0005), so a song renders the same every time and after a seek. The parser SHALL compile it into a fixed array of nodes and print it canonically, the parameter in lower case; an unknown word, method or shape, a second `mod` line or a second method of one fragment on one parameter, or a limit passed (Req 12) SHALL be a parse error with a line and a column. While the song plays the engine SHALL evaluate every modulation once per block, after the lanes, and write a parameter through `set_param` only when its value changes, or after a lane or a scene wrote it, so the modulation goes last; `render` SHALL not allocate. A modulation SHALL keep the value its parameter had when it began to write and put it back when it stops: its fragment leaves the section, the song stops, or a reload drops it; one a reload keeps SHALL keep that value. The engine SHALL report the (strip, parameter) pairs its modulations write now (`mod_count`, `mod_strip`, `mod_param`), and the view SHALL mark those knobs and show their values as the engine reports them (ADR-0001). Sixteen synths, each with two modulations, SHALL stay within the CPU budget of plan.md (`make bench`, `modulated`). A fader, pan or send, and a Mono or Poly voice's oscillator, noise, ring or sub level, SHALL move in a line from one block's value to the next across the block, so a gain a modulation, lane or knob writes does not step at the block's edge (#271); a strip or a voice that was silent SHALL take its values at once. A signal with `env` or a list SHALL give each voice of a Mono or Poly synth its own value (spec 006 Req 17, ADR-0023).

**Implementation:** `crates/dsp/src/song/signal.rs::Signal`, `crates/dsp/src/song.rs::Mod`, `crates/dsp/src/voice.rs::Gains`, `crates/dsp/src/mixer.rs::send_out`, `crates/dsp/src/engine.rs::Engine::run_mods`, `crates/dsp/src/engine.rs::Engine::mods_again`, `crates/dsp/src/engine.rs::Engine::commit_song`, `crates/dsp/src/song.rs::parse_methods`, `crates/dsp/src/engine.rs::Engine::modulated`, `web/src/components/console/Knob.vue` (`modulated`), `tools/bench.mjs`

#### Scenario: a filter swept by two LFOs

- GIVEN `mod lead.cutoff = lfo(1).exprange(100, 2000) + lfo(3).range(0, 300)` on a Minimoog playing a riff
- WHEN the song is rendered twice
- THEN both renders are bit-identical and the cutoff sweeps from below 250 Hz to above 1900 Hz

#### Scenario: a fragment's method while it plays

- GIVEN `frag b = kit /16 .send1(0.5)` in the first section of `arrange one two`
- WHEN the song plays
- THEN the send is 0.5 in bar 1 and back to its value before in bar 2

#### Scenario: a modulation goes last

- GIVEN a lane, a scene and a constant `mod` on `strip1.level`
- WHEN the song plays
- THEN the level is the modulation's value from the block after each scene

#### Scenario: a modulated gain does not zipper

- GIVEN a strip's fader, pan and send, or a voice's oscillator level, written to a new value every block
- WHEN the mix is rendered
- THEN no sample moves further from the last than the ramp across a block allows

**Tests:** `crates/dsp/src/song/signal.rs::tests::sources_run_from_0_to_1_once_a_cycle`, `crates/dsp/src/song/signal.rs::tests::range_slow_fast_and_operators_shape_a_signal`, `crates/dsp/src/song/signal.rs::tests::randomness_is_seeded_and_bounded`, `crates/dsp/src/song/signal.rs::tests::a_sequence_steps_per_cycle_or_within_one`, `crates/dsp/src/song/signal.rs::tests::lag_follows_its_input_and_keeps_its_state`, `crates/dsp/src/song/signal.rs::tests::printing_is_canonical_and_parses_back`, `crates/dsp/src/song/signal.rs::tests::errors_say_where`, `crates/dsp/src/song/signal.rs::tests::lag_slots_are_unique_across_a_song`, `crates/dsp/src/song/tests.rs::a_mod_line_parses_and_prints`, `crates/dsp/src/song/tests.rs::mod_errors_say_where`, `crates/dsp/src/engine/tests.rs::a_mod_follows_its_signal`, `crates/dsp/src/engine/tests.rs::a_constant_mod_is_bit_identical_to_a_hand_set_value`, `crates/dsp/src/engine/tests.rs::a_mod_writes_after_a_lane_and_a_scene`, `crates/dsp/src/engine/tests.rs::a_swept_filter_renders_deterministically`, `crates/dsp/src/song/tests.rs::fragment_methods_parse_and_print`, `crates/dsp/src/song/tests.rs::fragment_method_errors_say_where`, `crates/dsp/src/engine/tests.rs::a_fragment_method_writes_while_its_fragment_plays`, `crates/dsp/src/engine/tests.rs::a_mod_puts_back_the_value_it_found`, `crates/dsp/src/engine/tests.rs::the_engine_names_what_its_modulations_write`, `web/src/audio/engine.test.ts` (modulated knobs), `crates/dsp/tests/render_no_alloc.rs::a_busy_song_renders_without_allocating`, `crates/dsp/src/mixer.rs::tests::a_fader_pan_and_send_moved_every_block_do_not_step`, `crates/dsp/src/mono/voice.rs::tests::a_level_moved_every_block_does_not_step`, `crates/dsp/src/voice.rs::tests::gains_ramp_in_a_line_to_their_target`

### Requirement 16: Pattern methods [SHOULD]

A frag of notes SHALL take Strudel's pattern methods on its line (ADR-0019, #215): `.fast(n)`, `.slow(n)`, `.rev()`, `.palindrome()`, `.add(n)`, `.sub(n)`, `.ply(n)`, `.iter(n)`, `.degrade(p)`, `.every(n, m)`, `.off(t, m)`, `.struct("x ~ x x")`, `.sometimes(m)` and `.scale(<root> <mode>)` (to the nearest scale note), with a bar as the cycle. They SHALL transform its events in order when the song loads, never in `render`, randomness from a fixed seed (ADR-0005), and print canonically before its parameter methods; the line SHALL stay as written. A pattern SHALL make at most 512 events over at most 32 bars. A pattern method on a live frag or a frag of lanes, an unknown method or a bad value SHALL be a parse error with a line and a column; a note edit of a patterned frag SHALL be refused.

**Implementation:** `crates/dsp/src/notes/pattern.rs::Pattern`, `crates/dsp/src/song.rs::Fragment` (`pattern`)

#### Scenario: a riff twice as fast with a reversed bar

- GIVEN `frag r = lead .fast(2) .every(2, rev)` over `"c4 d4 e4 f4"`
- WHEN the song loads
- THEN the frag plays eight eighths a bar for two bars, the first reversed

**Tests:** `crates/dsp/src/notes/pattern.rs::tests::fast_and_slow_squeeze_and_stretch`, `crates/dsp/src/notes/pattern.rs::tests::rev_and_palindrome_turn_bars_round`, `crates/dsp/src/notes/pattern.rs::tests::add_sub_and_ply_change_notes`, `crates/dsp/src/notes/pattern.rs::tests::iter_starts_each_bar_further_in`, `crates/dsp/src/notes/pattern.rs::tests::every_and_off_layer_a_method`, `crates/dsp/src/notes/pattern.rs::tests::degrade_is_seeded`, `crates/dsp/src/notes/pattern.rs::tests::struct_plays_the_notes_sounding_at_each_hit`, `crates/dsp/src/notes/pattern.rs::tests::sometimes_takes_about_half_from_the_method`, `crates/dsp/src/notes/pattern.rs::tests::scale_moves_notes_to_the_nearest_scale_note`, `crates/dsp/src/song/tests.rs::struct_sometimes_and_scale_print_back`, `crates/dsp/src/notes/pattern.rs::tests::methods_print_as_they_parse`, `crates/dsp/src/notes/pattern.rs::tests::bad_methods_say_why`, `crates/dsp/src/song/tests.rs::pattern_methods_transform_a_frags_notes`, `crates/dsp/src/song/tests.rs::pattern_method_errors_say_where`, `crates/dsp/src/song/tests.rs::a_patterned_frag_refuses_a_note_edit`, `crates/dsp/src/engine/tests.rs::a_patterned_frag_plays_its_transformed_notes`

### Requirement 17: Autocommit [SHOULD]

The song and the live synths and mixer SHALL stay one state without a commit (ADR-0027, #357):

- **Hands fold in.** A parameter, a preset, SynthDef code, a DX7 voice or a library preset set from the view SHALL be marked (`Engine::edit_param`, `edit_preset`, `mark_synth`). A few times a second, outside `render`, `Engine::fold` SHALL fold the marks into the song and print it once: a track's sound into the setting only it plays (updated in place, or a new one named after the track), the mixer into its lines. A preset picked on a synth SHALL become its tracks' preset.
- **The song's own writes stay out.** Values set by a lane, a scene or a modulation SHALL NOT be marked, and a value one of them drives SHALL keep what the text says when the rest is folded.
- **The text is the mix.** A mixer value the old text set and the new one leaves out SHALL go back to its default.
- **Every synth on screen is a track** (#360). `Engine::track_add` SHALL give a synth a track named after it and keep its sound. `Engine::track_remove` SHALL take a track without music out, its later tracks keeping their synths, and SHALL mute one with music.
- **What has no song form is reported** (#361). `Engine::live_only` SHALL report a synth's arpeggiator away from its defaults, its sample zones, its sampled pads, and a sound past a setting's room.

**Implementation:** `crates/dsp/src/engine.rs` (`edit_param`, `edit_preset`, `fold`, `driven`, `track_add`, `track_remove`, `live_only`), `crates/dsp/src/song.rs` (`fold_sound`, `add_track`, `remove_track`), `web/public/worklet.js` (`song_fold` every `FOLD_EVERY` blocks unless held)

#### Scenario: a knob reaches the text

- GIVEN a track on a factory preset
- WHEN its cutoff is turned and then its resonance
- THEN the song has one setting named after the track, holding both, and the printed song loads back to the same sound

#### Scenario: a modulation does not freeze

- GIVEN a `mod` on a track's cutoff while the song plays
- WHEN its resonance is turned and folded
- THEN the setting holds the resonance and no cutoff

**Tests:** `crates/dsp/src/engine/tests.rs::a_hand_folds_into_the_tracks_setting`, `crates/dsp/src/engine/tests.rs::faders_and_presets_fold_and_the_engine_does_not`, `crates/dsp/src/engine/tests.rs::a_driven_value_is_not_folded`, `crates/dsp/src/engine/tests.rs::a_mixer_line_taken_out_resets_its_values`, `crates/dsp/src/engine/tests.rs::a_synth_on_screen_is_a_track`, `crates/dsp/src/engine/tests.rs::what_the_song_cannot_hold_is_reported`

### Requirement 18: A deck follows the master's clock [SHOULD]

With decks (ADR-0029, #391), deck A's clock SHALL lead and every worker deck's clock SHALL follow its tempo: the worklet SHALL report the master's tempo and each worker deck SHALL take it, and a new worker deck SHALL start with it. Play on a worker deck SHALL cue it: the master SHALL name the frame of its next bar (16 steps) or 8-bar phrase (128 steps) at least 16 blocks ahead (`cue_frames`), or 16 blocks ahead for **Now** or while the master is stopped; the deck SHALL start from its top on that exact frame at the master's tempo, inside whichever block it falls (`song_play_in`), so its steps fall on the master's. Stop SHALL cancel a cued start. A cue that reaches the worker after its frame SHALL start the deck at once and say how late it was.

**Implementation:** `crates/dsp/src/clock.rs::Clock::frames_to_multiple`, `crates/dsp/src/engine.rs::Engine::cue_frames`, `crates/dsp/src/engine.rs::Engine::song_play_in`, `crates/dsp/src/ffi.rs` (`cue_frames`, `song_play_in`), `web/public/worklet.js` (`cueDeck`), `web/public/deck-worker.js` (`startNow`), `web/src/audio/decks.ts` (`playDeck`, `onCued`, `onDecks`)

#### Scenario: cued to the next bar inside a block

- GIVEN a master at 130 BPM playing, its next bar falling inside a block, and a stopped deck
- WHEN the deck is started that many frames ahead and both render on
- THEN the deck's position is the master's minus the bar's sample, and both are on the same step of the bar

#### Scenario: a faster song follows

- GIVEN deck A playing a song at 124 BPM and a song at 170 BPM loaded into deck B
- WHEN deck B's Play is pressed with Start on Next bar
- THEN deck B starts on deck A's next bar and both advance the same number of steps a second

**Tests:** `crates/dsp/src/clock.rs::tests::frames_to_the_next_bar`, `crates/dsp/src/engine/tests.rs::a_cued_deck_starts_on_the_masters_bar`, `crates/dsp/src/engine/tests.rs::stop_cancels_a_cued_start`, `crates/dsp/tests/render_no_alloc.rs::a_busy_song_renders_without_allocating`, `web/src/audio/decks.test.ts`
