# 0008: Mono only, straight to the ensemble

**Status:** Accepted · **Date:** 2026-10-03

## Context

ADR-0005 sets out three sources (Mono, Wave, Drums), an arrangement and algo loops, and plan.md builds them in that order before the ensemble (MVP 5: six 2600s play Vivaldi). After MVP 2 the Mono voice is the part that works: the ARP 2600 voice plays well and the MIDI file player drives it. Wave and Drums were stand-in timbres (`voice.rs`), and the algo and arrangement panes were mock-ups over a demo model in TypeScript, with no engine behind them. They cost code, tests and screen space without moving towards the ensemble.

## Decision

**Narrow the product to the Mono voice and go straight to MVP 5.**

- **Removed:** the Wave and Drums sources and their voice pool, the `Source` id list, Wave's `Attack`/`Release` parameters, the algo pane, the arrangement pane and their demo model. Git history keeps them.
- **Deferred, not cancelled:** MVP 3-4 (step sequencer, arrangement), MVP 6-7 (Drums, Wave) and MVP 9-10 (algo loops). They come back after the ensemble, each with its own ADR if the model changes.
- **Next:** independent Mono instances, each with its own patch, so a MIDI file's channels play on differently patched 2600s (#19).

## Consequences

- The C ABI loses its source argument: `note_on(note, velocity)`, `note_off(note)`, and `route(channel, target)` plays on Mono (target 0) or mutes.
- ADR-0005 still describes where the model goes. Its sources, mixer, patterns, clips and generators wait for the deferred MVPs, and spec 002 Req 1-7 stay planned.
- The view has three areas: transport, synths, MIDI player (spec 003).
