# 0016: Note events on a tick grid

**Status:** Accepted · **Date:** 2026-10-04

## Context

ADR-0012 gave the song drum lanes on sixteenth steps, and the clock fires only on those steps (spec 002 Req 5). Epic #169 adds note fragments, sampler tracks and generators. Mini-notation divides a cycle freely (`"c4 [e4 g4 b4]"`, triplets), notes have lengths, and chords sound together, so a step is too coarse to place them, and a note needs an off as well as an on. ADR-0015 later adds `@bar.beat.tick` positions and MIDI import, which need the same resolution.

## Decision

**A note fragment compiles to events on a grid of 48 ticks per bar (3 per sixteenth); the engine fires each event at its tick inside a block and keeps note-offs in a fixed queue.**

- **Grid.** 48 ticks per 4/4 bar divide exactly into halves, thirds, quarters, sixths, eighths, twelfths and sixteenths, so straight and triplet subdivisions down to a triplet of sixteenths are exact; a thirty-second note (a tick and a half) is not on the grid. A fragment is a whole number of bars of events `{tick, length, note, velocity}`; velocity defaults to 0.75, accent 1.0, as drums.
- **Slides (#241).** Notes end before the next starts, so adjacent notes never overlap. A trailing `&` on a note or chord (mini or classic) adds one tick to its event's length, so the next note starts while it is held; a legato synth with glide then slides. Only the event's length changes, not where the next note is placed, and the engine is untouched. A piano-roll edit of a non-timed line clamps lengths and so drops the slides.
- **Parse, compile, play.** The parser and printer stay in `song.rs` and are total; the text keeps what was written (mini-notation or durations), never a different form. Compiling to events happens at load, outside `render` (ADR-0002). `render` reads a sorted, preallocated event list per fragment and a fixed-size note-off queue; nothing allocates.
- **Clock.** The step clock stays. A tick's sample is computed from the same anchor as a step (no drift), and swing delays the ticks of an odd step with it. Drum lanes keep playing on steps and are unchanged.
- **Cycles.** `<a b>` alternates per cycle and `?` draws per cycle from a small integer generator seeded by the fragment's position and the cycle index, so a run is reproducible. Floating point never decides an event.
- **Mono and chords.** A chord on a mono synth keeps the last note (the key stack of the mono voice); the poly voice pool plays all of it.
- **Limits.** Bounded events per fragment, bars per fragment and fragments per song, like the lane limits; text over the limit is a parse error with line and column.
- **Names stay free.** The grammar claims `track <name> synth|sampler` and quoted or duration sequences only; `section`, `arrange`, `scene`, `auto` and `midi` stay with ADR-0015.

- **Timed notes (addendum, #173).** A third line form says exactly where each note is: `pitch@start:length[:velocity]` in ticks from the line's start (`d5@0:6 f#5@6:6:90`), notes in any order and overlapping, a length of up to 32 bars, an optional velocity of 1 to 127. The frag line may give its length (`frag v_1 = violin bars 8`), so trailing silence counts; without it the line ends at the bar of its last start. It is what MIDI import writes (ADR-0015); people write the other two. It prints in event order.

## Consequences

- Notes, chords, triplets and, later, imported MIDI land on exact samples through one path.
- The fragment body is lanes or events; the printer and the view branch on the track's kind.
- A tick is finer than the step, so blocks split more often when a fragment is dense; the split cost is a lookup, not an allocation.

## Alternatives considered

- **Quantize to sixteenths.** Simple, but triplets, `[ ]` subdivision and exact MIDI import would be lost. Rejected.
- **A finer global clock (tick as the clock step).** Uniform, but drums, swing and the whole clock tests would move for no gain. Rejected.
- **960 ticks per beat as in MIDI files.** Exact for import, but a bar of 960×4 ticks would make every event list longer for notes that sit on the sixteenth grid anyway; MIDI import can round to 48 per bar or write `@bar.beat.tick` as ADR-0015 says. Rejected.
