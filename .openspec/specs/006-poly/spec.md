# 006: Polyphony

Epic #78: each synth owns a voice pool, and eight polyphonic instruments join the models of spec 005: Prophet-5, Juno-106, Jupiter-8, Matrix-12, PPG Wave, Roland D-50, Yamaha DX7 and the Polymoog. Decisions: ADR-0001, ADR-0002, ADR-0004, ADR-0009, ADR-0011.

Common to every requirement: `render` follows ADR-0002 (no allocation, no panic, no per-sample transcendentals), every parameter and id is mirrored in `web/src/audio/params.ts` (ADR-0004), and a model's sound is an interpretation of the instrument: each requirement names the property it must have and is tested on that. Tests render offline at 48 kHz.

### Requirement 1: Voice pool [MUST]

Each synth SHALL own a pool of 16 voices, allocated in `Engine::new`, of which a model uses `Model::voices()`. A monosynth model SHALL keep one voice per owner with note priority, legato and glide on that voice (spec 004 Req 6), so every monosynth renders exactly as it did before the pool. A poly model SHALL press each note on a voice of its own. A voice is of the kind its model plays (Mono, LA, FM, a drum pad, the samplers' or the Modular graph voice of spec 005 Req 11), and a voice of another kind SHALL be rebuilt when a note presses it. A voice SHALL remember its owner (live keys or a song track) and its note, so a note-off releases its own voice, and live keys and song tracks on one synth SHALL NOT release each other. Rerouting a channel or `all_off` SHALL release its voices.

**Implementation:** `crates/dsp/src/poly.rs::Pool`, `crates/dsp/src/engine.rs::Engine` (#80)

#### Scenario: a chord

- GIVEN a poly model with 6 voices and a chord of 4 notes
- WHEN the notes are pressed and one is released
- THEN 4 voices sound, and after the release only that voice falls silent

#### Scenario: owners do not mix

- GIVEN live keys and a song track both playing a poly synth
- WHEN the track releases a note the live keys also hold
- THEN only the track's voice is released

**Tests:** `crates/dsp/src/poly.rs::tests::a_chord_sounds_one_voice_per_note_and_releases_its_own`, `crates/dsp/src/poly.rs::tests::a_monophonic_synth_keeps_one_voice_per_owner`, `crates/dsp/src/poly.rs::tests::owners_do_not_release_each_other`, `crates/dsp/src/engine/tests.rs::a_poly_synth_plays_chords_from_live_keys_and_a_track`, `crates/dsp/src/mono/preset.rs::tests::arp_presets_keep_their_sound`

### Requirement 2: Allocation and stealing [MUST]

A new note SHALL take a free voice, rotating from the last one used. A note already sounding on its owner SHALL re-use that voice. When no voice is free, or the synth's polyphony (`Polyphony`, at most the model's voices) or the global voice budget (64) is full, the voice in release that has sounded longest SHALL be stolen, else the oldest held note, and a stolen voice SHALL restart clean.

**Implementation:** `crates/dsp/src/poly.rs::Pool::allocate` (#80)

#### Scenario: stealing

- GIVEN 6 voices, all held, and a seventh note
- WHEN it is pressed
- THEN the oldest held note is stolen, and with one voice in release that voice is stolen first

**Tests:** `crates/dsp/src/poly.rs::tests::a_full_pool_steals_the_release_first_then_the_oldest_held`, `crates/dsp/src/poly.rs::tests::rotation_does_not_cut_a_released_tail_first`, `crates/dsp/src/poly.rs::tests::the_same_note_again_retriggers_its_voice`, `crates/dsp/src/poly.rs::tests::every_sample_is_finite_and_bounded_at_full_polyphony`

### Requirement 3: Unison and analog variance [MUST]

`Assign` SHALL be Poly or Unison. In Unison a note presses on every voice of the pool up to `Polyphony`, spread by `UnisonDetune` (0..50 cents either side, evenly), and stays bounded. `Analog` (0..1) SHALL give each voice a seeded static detune (up to ±6 cents), a slow drift (up to ±3 cents) and a small cutoff offset (up to ±0.5 semitone), worked out once per block; the same seed SHALL give the same sound, and `Analog` 0 SHALL be exact.

**Implementation:** `crates/dsp/src/poly.rs::Pool`, `crates/dsp/src/mono/voice.rs::MonoVoice` (#81)

#### Scenario: unison width

- GIVEN unison on 6 voices with 20 cents of detune
- WHEN a note is held
- THEN six distinct pitches sound within ±20 cents of the note and the sum stays within ±1

**Tests:** `crates/dsp/src/poly.rs::tests::unison_spreads_every_voice_around_the_note`, `crates/dsp/src/poly.rs::tests::unison_falls_back_to_the_last_key_held`, `crates/dsp/src/poly.rs::tests::analog_variance_is_bounded_repeatable_and_off_at_zero`

### Requirement 4: Shared LFO [MUST]

A poly model's pool SHALL run one LFO and one sample-and-hold, and every voice SHALL read them, so a vibrato is the same on every voice and a voice started later joins it in phase.

**Implementation:** `crates/dsp/src/poly.rs::Pool::render`, `crates/dsp/src/mono/voice.rs::SharedMod` (#80)

#### Scenario: coherent vibrato

- GIVEN vibrato on two voices started a quarter of an LFO cycle apart
- WHEN both render
- THEN their pitch modulation is identical

**Tests:** `crates/dsp/src/poly.rs::tests::the_lfo_is_shared_by_every_voice`

### Requirement 5: Voice budget [MUST]

At most 64 voices SHALL sound at once across all synths; a note past it SHALL steal the oldest voice in release anywhere, else the oldest held note of its own pool. `make bench` SHALL measure full polyphony on every poly model against the 25% budget of plan.md.

**Implementation:** `crates/dsp/src/engine.rs::Engine::start_voice`, `tools/bench.mjs` (#80, #91)

#### Scenario: the cap

- GIVEN 16 synths of 8 voices each, all pressed
- WHEN the chords are held
- THEN no more than 64 voices sound and every sample is finite and within ±1

**Tests:** `crates/dsp/src/engine/tests.rs::the_voice_budget_caps_the_voices_across_synths`, `crates/dsp/src/engine/tests.rs::a_note_at_the_budget_takes_a_released_voice_first`

### Requirement 6: Prophet-5 [MUST]

The Prophet-5 SHALL have 5 voices of two oscillators (A is VCO 2, B is VCO 1, A synced to B), noise, poly-mod, a 4-pole low-pass in the Pro-One voicing, a filter ADSR and a loudness ADSR, an LFO and unison; `Analog` SHALL default to a clearly audible drift.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Prophet5`, `web/src/audio/models.ts` (#82)

#### Scenario: five voices

- GIVEN a five-note chord, then a sixth note
- WHEN they are pressed
- THEN five voices sound and the sixth steals one

**Tests:** `crates/dsp/src/engine/tests.rs::the_prophet_5_has_five_voices_and_the_sixth_steals_one`, `crates/dsp/src/engine/tests.rs::the_prophet_bass_plays_one_note_on_all_five_voices`, `crates/dsp/src/mono/model.rs::tests::polyphonic_models_have_their_own_voice_count`, `crates/dsp/src/mono/preset.rs::tests::every_poly_preset_plays_a_full_chord`

### Requirement 7: Juno-106 [MUST]

The Juno-106 SHALL have 6 voices of one DCO (saw and pulse with pulse-width modulation from the LFO or by hand, a sub-oscillator), noise, a 4-pole low-pass, a high-pass in four steps, one ADSR driving filter and loudness, an LFO, and a stereo BBD-style chorus with modes I, II and I+II that decorrelates the left and right outputs and is off for mode off.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Juno106`, `crates/dsp/src/fx/ensemble.rs::Ensemble`, `crates/dsp/src/mixer.rs::Mixer::widen` (#83)

#### Scenario: the chorus widens

- GIVEN a held note with the chorus off, then in mode II
- WHEN left and right are compared
- THEN off is identical on both sides and the chorus makes them differ

**Tests:** `crates/dsp/src/fx/ensemble.rs::tests::off_is_identical_on_both_sides_and_the_chorus_widens`, `crates/dsp/src/fx/ensemble.rs::tests::every_mode_stays_bounded_and_keeps_the_level`, `crates/dsp/src/fx/ensemble.rs::tests::the_modes_sweep_at_their_own_rates`, `crates/dsp/src/engine/tests.rs::the_juno_chorus_makes_the_two_sides_differ`, `crates/dsp/src/engine/tests.rs::the_juno_106_has_six_voices`, `crates/dsp/src/mono/voice.rs::tests::juno_high_pass_steps_thin_the_bass`, `crates/dsp/src/mono/voice.rs::tests::sh101_saw_and_pulse_add_up`

### Requirement 8: Jupiter-8 [MUST]

The Jupiter-8 SHALL have 8 voices of two VCOs (VCO 2 synced to VCO 1) with cross-modulation of VCO 1 by VCO 2, noise, a low-pass of 12 dB or 24 dB per octave by a switch, a high-pass, two envelopes (filter and loudness), an LFO, and poly or unison assignment.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::Jupiter8`, `crates/dsp/src/mono.rs::MonoParams::filter` (#84)

#### Scenario: the slope switch

- GIVEN the filter in 12 dB and in 24 dB
- WHEN the response is measured two and three octaves above the cutoff
- THEN it falls by about 12 and 24 dB per octave

**Tests:** `crates/dsp/src/mono/voice.rs::tests::jupiter_slope_switch_is_12_or_24_db_per_octave`, `crates/dsp/src/mono/voice.rs::tests::cross_mod_moves_vco1_from_vco2`, `crates/dsp/src/engine/tests.rs::the_jupiter_8_has_eight_voices`, `crates/dsp/src/mono/model.rs::tests::only_the_jupiter_has_a_slope_switch`

### Requirement 9: Matrix-12 [MUST]

The Matrix-12 SHALL have 12 voices of two oscillators with sync, a filter with a switchable slope and a high-pass, three envelopes and two LFOs, and a modulation matrix of 20 slots reading every source (including the second LFO and a ramp that restarts with each note) into every destination, including the high-pass cutoff and the second LFO's rate. Its slots SHALL add to a destination and SHALL NOT take it over from the normals, as the 8-slot patch of the other models does.

**Implementation:** `crates/dsp/src/mono/patch.rs::Patch`, `crates/dsp/src/mono/voice.rs::MonoVoice::render`, `crates/dsp/src/mono/model.rs::Model::Matrix12` (#85)

#### Scenario: the matrix

- GIVEN 20 slots joining the second LFO, the ramp and the envelopes to destinations
- WHEN a note is held
- THEN each destination receives the sum of its slots

**Tests:** `crates/dsp/src/mono/patch.rs::tests::twenty_slots_add_up_per_destination`, `crates/dsp/src/mono/voice.rs::tests::the_ramp_runs_over_its_time_and_restarts`, `crates/dsp/src/mono/voice.rs::tests::the_second_lfo_moves_what_it_is_patched_to`, `crates/dsp/src/engine/tests.rs::the_matrix_12_has_twelve_voices`

### Requirement 10: Table oscillator [MUST]

A table oscillator SHALL play single-cycle wavetables generated by the engine at start (no ROM data) with a position between waves, optional stepped positions, and an attack-plus-loop mode for sampled transients, reading its table by linear interpolation, with no allocation in `render`.

**Implementation:** `crates/dsp/src/table.rs::{Tables, TableOsc, SampleOsc}` (#86)

#### Scenario: pitch and position

- GIVEN a table oscillator on any wave at notes 24 to 108
- WHEN it renders
- THEN its pitch is within 1 cent, and moving the position moves between distinct spectra

**Tests:** `crates/dsp/src/table.rs::tests::pitch_is_within_a_cent`, `crates/dsp/src/table.rs::tests::position_moves_between_distinct_spectra`, `crates/dsp/src/table.rs::tests::steps_hold_a_wave_and_crossfades_blend_two`, `crates/dsp/src/table.rs::tests::every_wave_is_finite_bounded_and_closes_its_cycle`, `crates/dsp/src/table.rs::tests::the_samples_are_ours_finite_bounded_and_end_or_loop`, `crates/dsp/src/table.rs::tests::a_sample_plays_an_octave_up_twice_as_fast`, `crates/dsp/src/table.rs::tests::bad_increments_are_silent_not_nan`

### Requirement 11: PPG Wave [MUST]

The PPG Wave SHALL have 8 voices of two wavetable oscillators whose wave position is moved by the filter envelope and the LFO, optional stepped positions, a 4-pole analog-style low-pass, two envelopes and an LFO.

**Implementation:** `crates/dsp/src/mono/model.rs::Model::PpgWave`, `crates/dsp/src/mono/voice.rs::MonoVoice::render`, `crates/dsp/src/table.rs::Tables::shared` (#87)

#### Scenario: a sweep

- GIVEN the filter envelope into the wave position
- WHEN a note is held
- THEN the spectrum changes over the envelope

**Tests:** `crates/dsp/src/mono/voice.rs::tests::ppg_envelope_sweeps_the_wave_position`, `crates/dsp/src/mono/voice.rs::tests::only_the_ppg_reads_wavetables`, `crates/dsp/src/engine/tests.rs::the_ppg_wave_has_eight_voices`

### Requirement 12: Roland D-50 [MUST]

The D-50 SHALL have 16 voices of two partials, each a synthesised oscillator (saw, pulse or triangle with pulse-width modulation) or a PCM attack from the table oscillator (one of eight, with a loop on the vowel), each with its own resonant filter, filter envelope and amplifier envelope, the pair added, synced (partial 2 to 1, when both are synthesised) or ring-modulated. Its PCM samples SHALL be generated by the engine, not taken from any ROM. Dual tones (four partials to a voice) are not built.

**Implementation:** `crates/dsp/src/la.rs::LaVoice`, `crates/dsp/src/poly.rs::PolyVoice`, `crates/dsp/src/mono/model.rs::Model::D50` (#88)

#### Scenario: attack over a body

- GIVEN a PCM attack partial over a synthesised body
- WHEN a note is held
- THEN the transient sounds in the first tens of milliseconds and the body carries on

**Tests:** `crates/dsp/src/engine/tests.rs::the_d50_attack_sounds_first_and_the_body_carries_on`, `crates/dsp/src/engine/tests.rs::the_d50_partials_add_ring_and_sync`, `crates/dsp/src/engine/tests.rs::the_d50_has_sixteen_voices`, `crates/dsp/src/engine/tests.rs::a_model_change_replaces_the_voices`, `crates/dsp/src/la.rs::tests::a_voice_is_idle_until_pressed`

### Requirement 13: FM voice and DX7 [MUST]

The DX7 SHALL have 16 voices of 6 sine operators, each with a frequency ratio or fixed frequency, detune (−7..+7), output level, velocity sensitivity and a 4-rate, 4-level envelope; 32 algorithms; feedback on operator 6; an LFO (six waveforms, a delay, started with each note) and a pitch envelope, and key scaling of level and rate and velocity sensitivity per operator. An operator's carrier or modulator role SHALL follow the algorithm, the envelopes SHALL run at the DX7's rates (the slowest decay halves the level in 2^20 samples, and every four steps of rate double the speed), and every output SHALL stay finite and bounded. Amplitude modulation by the LFO (the AMS and AMD settings) is held in the patch but not applied, as in the reference.

**Implementation:** `crates/dsp/src/fm.rs::FmVoice`, `crates/dsp/src/fm/algorithms.rs`, `crates/dsp/src/fm/envelope.rs`, `crates/dsp/src/fm/patch.rs`, `crates/dsp/src/mono/model.rs::Model::Dx7`, `web/src/audio/dx7.ts` (#89)

#### Scenario: algorithms

- GIVEN each of the 32 algorithms with a modulator at a high level
- WHEN a note renders
- THEN the carriers sound at the played pitch, and the modulators add partials only through their routes

**Tests:** `crates/dsp/src/fm.rs::tests::a_carrier_sounds_at_the_played_pitch`, `crates/dsp/src/fm.rs::tests::modulators_add_partials_only_through_their_routes`, `crates/dsp/src/fm.rs::tests::feedback_adds_harmonics_to_operator_6`, `crates/dsp/src/fm.rs::tests::every_algorithm_stays_bounded_at_full_modulation`, `crates/dsp/src/fm.rs::tests::velocity_sensitivity_shapes_the_level`, `crates/dsp/src/fm.rs::tests::a_released_voice_falls_silent_and_ends`, `crates/dsp/src/fm.rs::tests::the_pitch_envelope_bends_the_note`, `crates/dsp/src/fm.rs::tests::key_scaling_follows_the_curves`, `crates/dsp/src/fm/envelope.rs::tests::decay_rates_follow_the_hardware`, `crates/dsp/src/fm/algorithms.rs::tests::every_algorithm_is_a_sound_routing`, `crates/dsp/src/fm/algorithms.rs::tests::the_typescript_copy_matches`, `crates/dsp/src/engine/tests.rs::the_dx7_has_sixteen_voices`, `web/src/audio/dx7.test.ts`

### Requirement 14: DX7 SysEx import [SHOULD]

The engine SHALL parse a DX7 single-voice SysEx message and a 32-voice bank (packed format) into the DX7 parameters, total over any input (it never panics, and reports an error for what it cannot read), and JavaScript SHALL only forward the bytes.

**Implementation:** `crates/dsp/src/fm/sysex.rs`, `crates/dsp/src/engine.rs`, `crates/dsp/src/ffi.rs`, `web/src/components/synth/SysexLoader.vue`

#### Scenario: a bank

- GIVEN a packed bank of 32 voices and a malformed one
- WHEN they are parsed
- THEN the first yields 32 voices and the second an error without panic

**Tests:** `crates/dsp/src/fm/sysex.rs::tests::a_bank_and_a_single_voice_are_read`, `crates/dsp/src/fm/sysex.rs::tests::bad_input_is_an_error_not_a_panic`, `crates/dsp/src/engine/tests.rs::a_sysex_voice_sets_the_dx7_parameters`

### Requirement 15: Polyphonic budget [MUST]

Chord pads on every poly model SHALL render within the performance budget, and every preset of every poly model SHALL be finite, bounded, audible and silent after release at full polyphony.

**Implementation:** `tools/bench.mjs`, `crates/dsp/src/mono/preset.rs`

#### Scenario: all of them at once

- GIVEN a chord on each poly model
- WHEN they are held
- THEN the load is within 25% of a core and every sample is within ±1

**Tests:** `tools/bench.mjs` (`make bench`: the `poly pads` and `poly worst` scenarios), `crates/dsp/src/mono/preset.rs::tests::every_preset_is_bounded`, `crates/dsp/src/mono/preset.rs::tests::every_poly_preset_plays_a_full_chord`

### Requirement 16: Polymoog [SHOULD]

The Polymoog model SHALL play sixteen voices, each with its own resonant filter and envelopes, and ship the Strings, Vox Humana, Funk and Brass registrations as presets. The Vox Humana's resonance peak SHALL move with the filter envelope.

**Implementation:** `crates/dsp/src/mono/model.rs`, `crates/dsp/src/mono/preset.rs`, `web/src/audio/models.ts`

#### Scenario: a vocal peak

- GIVEN the Vox Humana preset and a held note
- WHEN the filter envelope falls from its peak to its sustain
- THEN the most prominent harmonic stands out by several dB and moves down

**Tests:** `crates/dsp/src/engine/tests.rs::the_polymoog_has_sixteen_voices`, `crates/dsp/src/engine/tests.rs::the_vox_humana_resonance_peak_follows_the_filter_envelope`, `crates/dsp/src/mono/preset.rs::tests::every_preset_is_bounded`

### Requirement 17: Per-voice values [SHOULD]

A song's signal that uses `env(…)` or a list `[a, b]` SHALL give each voice of a Mono or Poly synth its own value of `cutoff`, `resonance`, `vco1level`, `vco2level`, `vco3level`, `noiselevel`, `ringlevel` or `sublevel` (ADR-0023, #273). The engine SHALL evaluate it once per block for each sounding voice, `env` from the times since the voice's note began and since its key was let go, a list by voice slot round the list, and the voice SHALL read that value instead of the synth's while the modulation writes it; the synth's own value SHALL be left alone. Such a signal on a strip, another parameter, a drum, sampler or Modular track, or with `.lag`, SHALL be a parse error with its column. `render` SHALL not allocate, and 64 voices with two per-voice signals each SHALL stay within the CPU budget.

**Implementation:** `crates/dsp/src/song/signal.rs::Voice`, `crates/dsp/src/mono/voice.rs::VoiceSet`, `crates/dsp/src/poly.rs::Pool::set_voice`, `crates/dsp/src/engine.rs::Engine::run_mods`, `crates/dsp/src/song.rs::per_voice_fits`, `tools/bench.mjs`

#### Scenario: an envelope per note

- GIVEN `mod lead.cutoff = env(perc).exprange(200, 4000)` on a Juno-106 playing `"c3 ~ e3 ~"`
- WHEN the song plays
- THEN each note's voice starts above 2500 Hz and falls below 300 Hz, and the synth's own cutoff is unchanged

#### Scenario: a rate per voice

- GIVEN `mod lead.cutoff = lfo([1, 3]).exprange(200, 4000)` and a held chord of two notes
- WHEN the song plays
- THEN the two voices hold different cutoffs

**Tests:** `crates/dsp/src/engine/tests.rs::an_envelope_on_the_cutoff_restarts_with_each_note_of_a_poly_synth`, `crates/dsp/src/engine/tests.rs::a_list_gives_two_held_voices_their_own_values`, `crates/dsp/src/song/signal.rs::tests::env_and_lists_are_per_voice`, `crates/dsp/src/song/tests.rs::per_voice_mod_errors_say_where`, `crates/dsp/tests/render_no_alloc.rs::a_busy_song_renders_without_allocating`, `tools/bench.mjs` (`make bench`: the `per-voice` scenario)
