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

**Implementation:** `crates/dsp/src/smf.rs` *(planned, MVP 5)*

#### Scenario: the ensemble

- GIVEN a six-part MIDI score
- WHEN it is imported
- THEN six Mono tracks exist, each with its own patch, and the arrangement shows their clips
