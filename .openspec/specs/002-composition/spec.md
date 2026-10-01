# 002: Composition

Tracks, sources, effects, patterns, clips, algo loops, score import and the clock. Decision: ADR-0005. Draft: every requirement here is planned (plan.md MVP 2-10); paths name where the code will land.

### Requirement 1: A track owns one source [MUST]

A track SHALL own exactly one source instance (`Mono`, `Wave`, `Drums`), with its own fixed voice pool and parameters. Parameters SHALL be addressed as (track, parameter).

**Implementation:** `crates/dsp/src/track.rs::Track` *(planned, MVP 4)*

**Tests:** `crates/dsp/src/track.rs::tests` *(planned)*

### Requirement 2: Fixed mixer [MUST]

Each track SHALL pass through up to four inserts, a fader and pan, and two sends (delay, reverb) into a master bus with a compressor and limiter. The insert order MAY change; nothing in the mixer SHALL allocate after init.

**Implementation:** `crates/dsp/src/mixer.rs` *(planned, MVP 8)*

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

Until tracks and clips exist (MVP 4-5), the engine SHALL play a loaded MIDI file directly: ticks SHALL become samples once, at load, through the tempo map, and each note SHALL start on its exact sample in `render`. Each MIDI channel SHALL route to one source or be muted, channel 10 defaulting to Drums. A file that fails to parse SHALL leave the loaded song untouched. Stop SHALL release the player's voices and leave live ones sounding.

**Implementation:** `crates/dsp/src/player.rs::Sequence`, `crates/dsp/src/engine.rs::Engine::load_midi`, `crates/dsp/src/ffi.rs` (`midi_buf`, `midi_load`, `part_*`, `event_*`, `song_length`, `song_bar`, `play`, `stop`, `seek`, `position`, `playing`, `route`, `routed`)

#### Scenario: sample-accurate start

- GIVEN a file with a note at a known tick
- WHEN it is played
- THEN the note starts on the sample the tempo map gives

**Tests:** `crates/dsp/src/player.rs::tests::ticks_become_samples_through_the_tempo_map`, `crates/dsp/src/engine.rs::tests::player_note_starts_on_its_exact_sample`, `crates/dsp/src/engine.rs::tests::channel_ten_is_drums_and_mute_silences`, `crates/dsp/src/engine.rs::tests::bad_files_are_rejected_and_keep_the_old_song`, `crates/dsp/src/engine.rs::tests::stop_releases_player_voices_but_not_live_ones`, `crates/dsp/src/ffi.rs::tests::midi_round_trip_through_the_abi`
