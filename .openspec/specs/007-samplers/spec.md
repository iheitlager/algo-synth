# 007: Samplers

The sample store and the two sampler models that play it: the multisampler (`Model::Sampler`) and the pad sampler (`Model::PadSampler`). Epic #126. Decisions: ADR-0001, ADR-0002, ADR-0004, ADR-0010, ADR-0011, ADR-0013.

Common to every requirement: `render` follows ADR-0002 (loading allocates, playback only reads), every field id is mirrored in `web/src/audio/params.ts` (ADR-0004), and tests render offline at 48 kHz.

### Requirement 1: The sample store [MUST]

The engine SHALL hold samples in a store of 64 slots. The view SHALL write a WAV file's bytes into an engine buffer (`sample_buf`) and call `sample_load(slot)`; Rust SHALL parse PCM 16-bit, 24-bit and 32-bit float, mono or stereo, read the `smpl` chunk's root note and first loop, and resample to the engine's rate so playback never converts rates, moving the loop points with it. Parsing SHALL be total: any bytes give a sample or a negative code (not WAV, truncated, unsupported, empty, too large, no such slot), never a panic. A file over 32 MiB, or a load that would take the store past 16 Mi values (64 MiB), SHALL be refused and leave the slot as it was. A load into a used slot SHALL replace it, and `sample_clear` SHALL free it. The engine SHALL report each slot's frames, root and loop, the store's use and cap, and a waveform's min/max peaks (`sample_peaks`) for the view to draw.

**Implementation:** `crates/dsp/src/sample.rs::parse`, `crates/dsp/src/sample.rs::SampleStore`, `crates/dsp/src/sample.rs::Sample`, `crates/dsp/src/engine.rs::Engine::load_sample`, `crates/dsp/src/ffi.rs` (`sample_buf`, `sample_load`, `sample_clear`, `sample_slots`, `sample_frames`, `sample_root`, `sample_loop`, `sample_used`, `sample_cap`, `sample_peaks`)

#### Scenario: a looped sample at another rate

- GIVEN a 44.1 kHz WAV with a `smpl` loop, loaded into a 48 kHz engine
- WHEN it is read back
- THEN it is at 48 kHz with the same pitch, and its loop points are scaled by 48000/44100

#### Scenario: bad files

- GIVEN truncated, oversized, 8-bit or random bytes
- WHEN they are loaded
- THEN each gives its negative code, the slot keeps what it held, and nothing panics

**Tests:** `crates/dsp/src/sample.rs::tests::pcm16_mono_reads_back`, `crates/dsp/src/sample.rs::tests::pcm24_stereo_reads_back`, `crates/dsp/src/sample.rs::tests::float_is_bounded`, `crates/dsp/src/sample.rs::tests::smpl_chunk_gives_root_and_loop`, `crates/dsp/src/sample.rs::tests::resampling_keeps_pitch_and_scales_loop`, `crates/dsp/src/sample.rs::tests::bad_files_are_errors_not_panics`, `crates/dsp/src/sample.rs::tests::oversized_data_chunk_length_is_truncated`, `crates/dsp/src/sample.rs::tests::store_replaces_clears_and_caps`, `crates/dsp/src/sample.rs::tests::peaks_bracket_each_run`, `crates/dsp/src/ffi.rs::tests::samples_load_through_the_abi`

### Requirement 2: The multisampler [MUST]

A synth on `Model::Sampler` SHALL play zones of the store through its voice pool (ADR-0011), so polyphony, stealing and the song work as for every other model. Each synth SHALL own 64 zones in fixed arrays, set field by field through `zone_set` and read back through `zone_get`: sample slot, key and velocity range, root (else the sample's), tune, level, loop mode (off, loop, sustain) with loop start and end, a round-robin length and position, and a release flag. Every field SHALL be clamped, a NaN taken as 0. A voice SHALL pick its zone when it starts: the first that covers the note and velocity, round-robin zones taking turns per key, and play it with an interpolated read at the pitch of note against root, bent by the pitch wheel (`PitchBend` × `BendRange` semitones, #439), through the synth's own filter and amplifier envelope. A sustain loop SHALL hold while the key is down and wrap without a click; a release zone SHALL sound once when the key comes up. Stereo samples SHALL be mixed to mono, as a strip is mono until its pan (ADR-0010). A key without a zone, or a zone whose sample is gone, SHALL be silent.

**Implementation:** `crates/dsp/src/sampler.rs::ZoneMap`, `crates/dsp/src/sampler.rs::Zone`, `crates/dsp/src/sampler.rs::SamplerVoice`, `crates/dsp/src/poly.rs::Pool`, `crates/dsp/src/ffi.rs` (`zone_set`, `zone_get`, `zone_fields`, `zone_count`)

#### Scenario: pitch

- GIVEN a zone over every key with its root at C4
- WHEN C4 and C5 are played
- THEN C5 sounds at twice C4's frequency

#### Scenario: velocity layers and round robin

- GIVEN two zones on one key, soft and hard, and two round-robin zones on another
- WHEN the first key is hit softly then hard, and the second twice
- THEN each velocity picks its own sample, and the round-robin zones take turns

**Tests:** `crates/dsp/src/sampler.rs::tests::fields_clamp_and_ignore_nan`, `crates/dsp/src/sampler.rs::tests::get_reads_back_what_set_stored`, `crates/dsp/src/sampler.rs::tests::pick_follows_key_and_velocity`, `crates/dsp/src/sampler.rs::tests::round_robin_takes_turns_per_key`, `crates/dsp/src/sampler.rs::tests::release_zones_are_separate`, `crates/dsp/src/sampler.rs::tests::plays_at_its_root_and_an_octave_up_at_double`, `crates/dsp/src/sampler.rs::tests::the_pitch_wheel_bends_the_sample`, `crates/dsp/src/sampler.rs::tests::silent_without_a_zone_for_the_key`, `crates/dsp/src/sampler.rs::tests::velocity_layers_pick_their_sample`, `crates/dsp/src/sampler.rs::tests::a_sustain_loop_holds_the_note_and_the_wrap_is_seamless`, `crates/dsp/src/sampler.rs::tests::a_release_zone_sounds_when_the_key_comes_up`, `crates/dsp/src/sampler.rs::tests::many_notes_steal_voices_and_stay_bounded`, `crates/dsp/src/sampler.rs::tests::a_missing_sample_is_silence_not_a_panic`, `crates/dsp/src/engine/tests.rs::a_sampler_track_with_notes_plays_the_multisampler_at_pitch`

### Requirement 3: The pad sampler [MUST] A hit SHALL play only the part of its sample its note carries (#716, ADR-0039), from and to fractions resolved to whole frames as it starts, nothing per sample; a soft hit's VelStart SHALL start it as far into its part as it would into the whole. The multisampler SHALL play the part too, without its zone's loop. A fitted note (#717) SHALL play its part over the output frames of its length at the tempo it starts at, its rate worked out once as it starts, whatever its tune or pitch. A note's speed (#718) SHALL multiply its rate, below 0 reading from its part's end down to its start, change in a line to its end speed over its length, and hold each output value its coarseness in samples, all worked out as it starts, a step per sample adding one number.

A synth on `Model::PadSampler` SHALL be sixteen pads after an Akai MPC: pad *i* SHALL answer MIDI note 36 + *i*, so a drum channel, a drum lane and a `sampler` track's pad lanes play it; the General MIDI cowbell (56), maracas (70), low conga (64) and high conga (62) SHALL play pads 8, 5, 4 and 12, where the kits put them (ADR-0036). Each pad SHALL have a sample, tune (±24 semitones), level, pan, decay (0 plays the sample out), a choke group (1–8, 0 none), how much velocity moves its level and its start point, one-shot or not, and an `Out` (0 the sampler's strip, 1–8 straight into that group bus, #220), set through `pad_set` and read through `pad_get`, clamped. A pad SHALL be one voice, retriggered when hit again; a hit SHALL silence the other pads of its choke group; a one-shot pad SHALL ignore the key coming up and any other pad SHALL fade when it does. The pads SHALL write both sides, panned, so the strip balances rather than pans. A pad routed to a group SHALL bypass the sampler's strip (its inserts, fader and mute), and soloing the sampler SHALL keep those groups heard, as for a drum kit's individual outs (spec 002 Req 1).

**Implementation:** `crates/dsp/src/padsampler.rs::PadKit`, `crates/dsp/src/padsampler.rs::PadVoice`, `crates/dsp/src/padsampler.rs::pad_of`, `crates/dsp/src/poly.rs::Pool`, `crates/dsp/src/ffi.rs` (`pad_set`, `pad_get`, `pad_count`, `pad_fields`)

#### Scenario: a choke group

- GIVEN pads 1 and 2 in choke group 1
- WHEN pad 1 is hit and then pad 2
- THEN pad 1 falls silent and pad 2 sounds

#### Scenario: sixteen at once

- GIVEN sixteen pads with full-scale samples at full level
- WHEN all are hit together
- THEN every sample is finite and bounded

**Tests:** `crates/dsp/src/padsampler.rs::tests::notes_map_to_pads`, `crates/dsp/src/padsampler.rs::tests::fields_clamp_and_read_back`, `crates/dsp/src/padsampler.rs::tests::a_hit_plays_its_sample_and_tune_moves_the_pitch`, `crates/dsp/src/padsampler.rs::tests::a_pad_without_a_sample_or_a_note_off_the_pads_is_silent`, `crates/dsp/src/padsampler.rs::tests::a_one_shot_ignores_the_key_coming_up_and_another_pad_fades`, `crates/dsp/src/padsampler.rs::tests::decay_shortens_a_pad`, `crates/dsp/src/padsampler.rs::tests::a_choke_group_silences_the_others_and_a_pad_retriggers`, `crates/dsp/src/padsampler.rs::tests::velocity_moves_level_and_start`, `crates/dsp/src/padsampler.rs::tests::pan_places_the_pad_between_the_sides`, `crates/dsp/src/padsampler.rs::tests::sixteen_pads_at_once_stay_bounded`, `crates/dsp/src/padsampler.rs::tests::a_pad_on_a_group_goes_only_through_that_group`, `crates/dsp/src/padsampler.rs::tests::a_pad_keeps_its_pan_into_its_group`, `crates/dsp/src/padsampler.rs::tests::solos_follow_a_pad_samplers_outs`, `crates/dsp/src/engine/tests.rs::a_song_track_plays_a_pad_sampler_on_the_clock`, `crates/dsp/src/engine/tests.rs::a_sampler_track_with_lanes_plays_the_pad_sampler`, `crates/dsp/src/engine/tests.rs::lanes_reach_the_pads_past_general_midis`, `crates/dsp/src/engine/tests.rs::channel_ten_plays_on_a_pad_sampler_slot`, `crates/dsp/src/engine/tests.rs::a_pad_samplers_outs_are_reported_for_the_solos`, `crates/dsp/src/padsampler.rs::tests::a_region_plays_exactly_its_part_of_the_sample`, `crates/dsp/src/padsampler.rs::tests::speed_reverses_and_doubles_a_hit`, `crates/dsp/src/sample.rs::tests::a_cursor_sweeps_its_speed_and_holds_when_coarse`

### Requirement 4: Sample packs [SHOULD]

`make samples` SHOULD fetch the free packs, kits and voices listed in `tools/samples/packs.json`, each pinned by SHA-256 and with its licence, convert them to mono 16-bit WAV with their loops in a `smpl` chunk, and write them with a manifest of zones and pads into `web/public/samples/` (gitignored; the static server ships it, ADR-0006). The view SHOULD offer the packs on a sampler's panel and lay a pack out through `sample_load`, `zone_set` and `pad_set` only (ADR-0001): the sampler panel shows the store's slots, a multisampler's zones on a key and velocity map with the waveform, and a pad sampler's pads in a grid. Replacing a pack SHOULD free its own slots but not those another synth plays.

**Implementation:** `tools/fetch_samples.py`, `tools/samples/packs.json`, `web/src/audio/sampler.ts`, `web/src/components/synth/SamplerPane.vue`, `web/src/components/synth/SampleSlots.vue`, `web/src/components/synth/PadGrid.vue`

#### Scenario: a pack becomes calls

- GIVEN a manifest with one well-formed instrument and one broken one
- WHEN it is read
- THEN the broken one is dropped, each file is listed once, and the zones become `zone_set` calls

**Tests:** `tools/test_fetch_samples.py`, `web/src/audio/sampler.test.ts`

### Requirement 5: Zones and pads in the song [SHOULD]

A multisampler's zones SHOULD survive a reload as the song's `zone` lines (ADR-0033, #522): `zone <track> <n> = <source> [: <field> <value>, …]`, the source a pack's sample (`<pack>/<sample>`) or a dropped file's hash (`@` and 32 hex digits), the fields those of Req 2 that differ from their defaults, keys as notes. The view SHALL keep a dropped or lab-made WAV in the browser by its hash and tell the engine each loaded slot's source; a zone a hand sets SHALL fold into its track's lines once every zone's sample has a source, replacing a `samples` line, and picking a pack SHALL take them away. A pack's or the song's layout SHALL NOT fold. When a song loads, the view SHALL load each source its synth does not hold into a slot and lay the zones out; one it cannot find SHALL leave its zone empty and say so on the sampler's panel. A pad sampler's pads SHOULD likewise be `pad <track> <n> = <source> [: …]` lines (#527), `n` 1 to 16, on a drums or sampler track, a kit's `samples` line or its pad lines; a kit's layout SHALL NOT fold. A zone line on a track that isn't a sampler, a pad line on a synth track, either beside a `samples` line or twice for one zone or pad, with a source of another kind or a field out of range SHALL be a parse error with a line and a column.

**Implementation:** `crates/dsp/src/song/zone.rs::SongZone`, `crates/dsp/src/song.rs::Song::set_zones`, `crates/dsp/src/song/pad.rs::SongPad`, `crates/dsp/src/song.rs::Song::set_pads`, `crates/dsp/src/engine.rs` (`set_zone`, `settle_zones`, `set_pad`, `settle_pads`, `song_pad_held`, `set_sample_source_from_buffer`, `song_zone_held`, `fold`, `live_only`), `crates/dsp/src/ffi.rs` (`sample_source_set`, `zones_settle`, `song_zone_count`, `song_zone_synth`, `song_zone_index`, `song_zone_source_ptr`, `song_zone_values_ptr`, `song_zone_held`, `pads_settle`, `song_pad_*`), `web/src/audio/engine.ts` (`loadDroppedSample`, `loadSongLayout`), `web/src/audio/audiostore.ts`, `web/public/worklet.js` (`zoneSets`, `padSets`, `sampleSource`)

#### Scenario: a dropped sample comes back

- GIVEN a WAV dropped on a Sampler and its zone narrowed by hand
- WHEN the song folds and is loaded again in a fresh engine
- THEN its zone line names the file's hash, the view loads the kept file and lays the zone out as it was

#### Scenario: a missing file is said

- GIVEN a zone line naming a hash this browser did not keep
- WHEN the song loads
- THEN the zone stays empty, the sampler's panel says the file is not kept in this browser, and the song plays on

**Tests:** `crates/dsp/src/song/tests.rs::zone_lines_parse_print_and_say_where_they_are_wrong`, `crates/dsp/src/song/zone.rs::tests::a_zone_prints_only_its_changes`, `crates/dsp/src/song/zone.rs::tests::bad_fields_say_what_they_take`, `crates/dsp/src/engine/tests.rs::hand_set_zones_fold_into_zone_lines`, `crates/dsp/src/song/tests.rs::pad_lines_parse_print_and_say_where_they_are_wrong`, `crates/dsp/src/song/pad.rs::tests::a_pad_prints_only_its_changes`, `crates/dsp/src/song/pad.rs::tests::bad_fields_say_what_they_take`, `crates/dsp/src/engine/tests.rs::hand_set_pads_fold_into_pad_lines`, `web/src/audio/engine.test.ts`
