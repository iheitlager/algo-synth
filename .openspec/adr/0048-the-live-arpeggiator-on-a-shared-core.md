# 0048: The live arpeggiator and the notation's `arp` on one core

**Status:** Accepted · **Date:** 2026-10-17 · **Extends:** 0012, 0016

## Context

#110 asks for a live arpeggiator per synth, as on an SH-101 or a Jupiter-8: hold a chord on the keyboard and the engine plays it as a pattern in time with the clock. The notation already had `arp` (#107, spec 002 Req 7), a generator that writes a chord's notes out as a clip.

Two arpeggiators would play the same words two ways. #612 found it had happened to the modes anyway: the live arp knew `Up Down UpDown AsPlayed Random`, the text arp its own enum `up down updown random`. Tidal's `arp` knows thirteen orders.

The arp runs inside `render`, so it may not allocate (ADR-0002), and it has to land on the song's grid (ADR-0016) to play with the clips.

## Decision

**The pattern is one pure function, `arp_note(held, mode, octaves, step, seed)` in `crates/dsp/src/arp.rs`, read by the notation's `arp` and by each synth's live arpeggiator; the live arp adds only a held set and a position on the clock's tick grid.**

- **A pure core.** `arp_note` keeps no state: the note at a step is a function of the held notes, the mode, the octaves (1–4, a twelve higher each time) and the step. Random picks by `mix(seed, step)`, the same every time.
- **One mode set.** `ArpMode` holds Up, Down, UpDown, AsPlayed and Random, and since #612 Tidal's DownUp, UpAndDown, DownAndUp, Converge, Diverge, Disconverge, PinkyUp, PinkyUpDown, ThumbUp and ThumbUpDown, each an index into the span. New modes go at the end, so a saved `ArpMode` keeps its order. `ArpMode::WORDS` are the song's names, Tidal's where it has one.
- **A fixed held set.** `Held` is an 8-slot array (`MAX_HELD`); a ninth key is ignored. Each of the 16 synth slots has its `Arp`, built in `Engine::new`.
- **Settings are parameters.** `ArpOn`, `ArpMode`, `ArpOctaves`, `ArpRate` (1/8, 1/16, 1/8T, 1/16T), `ArpGate`, `ArpLatch`, `ArpFree` and `ArpSeed` (483–490) are per-synth ids in `params.rs` and `params.ts` (ADR-0004); `Param::is_arp` names them, and presets leave them alone.
- **On the clock's grid.** With the arp on, `Engine::note_on` and `note_off` feed the held set instead of the voice. `Engine::play_tick` calls `Engine::arp_tick` on every tick of the clock (ADR-0016); a chord starts at its first note on the next step boundary.
- **Stopped means still.** With the clock stopped the arp holds still, unless `ArpFree` is on: then it runs on a grid of its own at the song's tempo. Changing grid with the transport lets the sounding note go.
- **Release before press.** A step ends the note before it starts the next, so every step retriggers, even with `Legato` and a full gate.
- **In the song, in the track's setting (#228).** The arp's parameters fold into the track's `setting` like any knob (ADR-0027), and a song's patch sets them: it starts from the arp's defaults (`ARP_DEFAULTS`), so a setting without them plays no arp. A preset picked on the panel keeps the arp as it is.
- **On the Mono, Poly and Chips faceplates.** The view adds the Arp section to those families; the drum machines, the samplers, Modular and General MIDI have none.

## Consequences

- The text and the keyboard play the same order for every deterministic mode: `arp([c4,e4,g4],converge,16)` sounds as a held C-E-G on Converge.
- The notation's Random is the exception: it draws from its own seeded stream (`rng(seed, 0xA4)`), as it did before #110, so songs and frozen clips that use it did not change.
- `render` allocates nothing for the arp: the held set, the sounding note and the grid position are plain `Copy` data.
- A song holds its arps: `Engine::live_only` no longer reports one, and a saved or reloaded song plays the arp it was saved with.
- `is_arp()` parameters are read by every model (`Family::Any`), so ADR-0037's check never refuses one.

## Alternatives considered

- **A live arp of its own beside the text arp:** the two had already drifted apart in modes before #612 merged them; one function keeps one behaviour.
- **An arp on wall-clock time:** it would drift against the clips and could not start on a step; the clock's tick grid keeps it in time, and the free-run grid takes the song's tempo for the stopped case.
- **An `arp` line of its own in the song:** eight values that are already parameters, with a second syntax for them. The setting holds them as it holds every other knob.
- **The arp settings live-only** (as built first, ADR-0027): a saved or reloaded song lost its arps, and the view had to warn about it.
- **The arp in the view:** a pattern played from JavaScript breaks ADR-0001 and would land on the main thread's timers, not the engine's sample-exact grid.
