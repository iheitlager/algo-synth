# 0005: The composition model

**Status:** Accepted · **Date:** 2026-09-30

## Context

algo-synth combines three things that are usually separate programs: synth voices, a sequencer/arranger, and generative "algo" loops. It also wants a non-generative way in: a score played by an ensemble (six 2600s playing Vivaldi). They need one model, or every feature is built three times.

## Decision

**Sources.** A **track** owns exactly one **source** instance: `Mono` (2600-style semi-modular), `Wave` (PPG-style wavetable) or `Drums` (analog-style kit). Sources are variants of one Rust enum with fixed voice pools, not trait objects or plugins.

**Effects.** A fixed mixer, not a free graph: `source → up to 4 inserts → fader/pan → 2 sends (delay, reverb) → master (comp/limiter)`. Inserts are an enum; their order is editable, their count fixed and preallocated. Patching lives *inside* the Mono voice (normalled routing with overrides), not between tracks.

**Sequences, in layers:**

| Layer | What it is |
|---|---|
| **Pattern** | Steps on a grid: note, velocity, length, probability, parameter locks. Any length (polymeter). |
| **Clip** | A pattern placed on a track at a bar, for a number of bars, looping inside its span. |
| **Arrangement** | Tracks × bars; the bottom pane. |
| **Algo loop** | A seeded **generator** (Euclid, walk, arp, Markov, mutate) + scale + target track. *Live*: writes a new pattern each cycle. *Frozen*: committed to a clip. |
| **Score** | A MIDI file imported as tracks and clips, with its tempo map. |

A clip records its **origin** (`hand`, `algo`, `score`) but plays identically.

**The clock is in the engine.** The sequencer runs inside `render`, on the audio thread, sample-accurately. The UI sends the song as data; it never schedules notes with JavaScript timers. Live input (keys, MIDI) goes straight to `note_on`.

**Generators are deterministic.** Same seed, same parameters, same music. The PRNG lives in the engine.

## Consequences

- One sequencer serves hand-played, generated and imported music.
- The fixed mixer keeps `render` allocation-free (ADR-0002) at the cost of flexibility; a patch graph between tracks would be a new ADR.
- The song format (UI → engine) is the next design task (spec 002, before MVP 3).
