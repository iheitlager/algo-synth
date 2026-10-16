# 0041: Strudel's randomness

**Status:** Accepted · **Date:** 2026-10-16 · **Extends:** 0005, 0019

## Context

#620 lets a signal write notes: `irand(10).sub(7).seg(16)` on a note line. In Strudel the way to keep a random line found by ear is `rib(offset, cycles)`, which loops one stretch of the endless random signal: `n(irand(10).sub(7).seg(16)).scale("c:minor").rib(46,1)` is one bar someone chose by listening. Our `rand` was our own seeded hash (ADR-0005), so the same expression gave a different bar here, and the offset found in Strudel was worthless.

## Decision

**The random sources are Strudel's own, to the bit.** `rand`, `rand2`, `irand`, `brand`, `brandBy` and `perlin` compute Strudel 1.2's `timeToRand`: the time over 300 cycles stretched to 2^29, an xorshift in JavaScript's 32-bit integers, `perlin` joining the values at whole cycles with a smootherstep. A test pins values read from `@strudel/core`.

- **A note line reads them at the moment**, as Strudel samples a signal at each event's start (`Signal::sample`), so `seg(3)` or `seg(32)` gives Strudel's values too.
- **A parameter holds each value a sixteenth**, as before: a `mod` or `auto` evaluated once a block would otherwise change every block. The value held is Strudel's at the start of the sixteenth.
- Still deterministic and the same after a seek (ADR-0005); there is no seed, as there is none in Strudel.

## Consequences

- A line pasted from Strudel, `rib` and all, plays the notes it plays there.
- Songs that used a random signal (a `mod`, an `auto`, `.add(perlin…)`) take new random values once, still the same on every run.
- The mini-notation's `?` and `degrade`, and the generators, keep their own seeded draws: they are not signals, and Strudel's `?` depends on its query structure, which we do not copy.

## Alternatives considered

- **Keep our hash.** Nothing existing changes, but `rib(46,1)` freezes a different bar than in Strudel, and porting a riff means finding it again by ear.
- **Strudel's hash on note lines only.** Nothing existing changes, but one expression gives different values in a note line and in a `mod` line.
