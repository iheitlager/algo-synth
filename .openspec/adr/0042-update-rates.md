# 0042: Update rates

**Status:** Accepted · **Date:** 2026-10-17 · #754

## Context

The engine moves things at several rates: every sample, across a block, once a block, at a note's press, on the note grid. Each choice was made where it was needed (ADR-0002, ADR-0019, ADR-0023, ADR-0038, ADR-0039, #271) and written down there, but nowhere side by side. The cost showed in #744: a per-note pitch envelope added to a voice's pitch trim once a block steps a 12-semitone, 50 ms drop in about 19 stairs of 0.63 semitone, which a sine kick lets you hear.

Csound has the same split: audio rate every sample, control rate every `ksmps` samples. Our block plays `ksmps`'s part. At 48 kHz it is 128 samples, 2.67 ms, a control rate of 375 Hz, coarser than a typical Csound setup (`ksmps` 10 to 64). Csound also lets an instrument lower it for itself (`setksmps`): the need is local.

## Decision

**The control rate is the render block, `engine::BLOCK`, 128 samples. It is fixed: it is the Web Audio worklet's render quantum, and every render is bit-exact against it. A signal that moves faster than the block can follow gets a finer rate of its own, where it is, not a song-wide setting.**

What runs when:

| Rate | What | Where |
|---|---|---|
| every sample | oscillators and envelopes; the Mono voice's pitch turned into its step through the pitch table; the tremolo's, ring's and frequency shifter's phasors; a queued hit (strum #604, graces #605, a clip's own swing #626, humanize #21) pressed on its exact sample | the voices, `fx::ring::Quad`, `Engine::fire_hits` |
| ramped across a block | faders, pans, sends and a Mono voice's oscillator levels: the block's value reached in a line, so a knob, a lane or a `mod` never steps (#271) | `voice::Gains`, `mixer` |
| smoothed, 2 ms | the ladder's and the SVF's cutoff, in pitch | `mono::ladder` (`SMOOTH_SECONDS`), `mono::svf` |
| every 32 samples | the compressor's level (`SUB`); a moving per-note pitch envelope's trim (`PITCH_SUB`, #754) | `fx::compressor`, `poly::Pool::render_into` |
| every 16 samples | the phaser's sweep (`EVERY`) | `fx::phaser` |
| once a block | `mod` lines, clip parameter methods and autos (ADR-0019); per-voice values (ADR-0023); a pool's pitch and cutoff trim: unison, drift, a note's cents, `Detune`, the #744 envelopes; an insert's and a processor's knobs | `Engine::run_mods`, `poly::Pool::retrim` |
| at a note's press | `.onNote`, the envelope times (ADR-0038), the `.velocity`, `.clip`, `.n` and `.slice` signals and the #744 envelope values | `Engine::pick`, `Engine::note_shape` |
| on the grid | note events on 48 ticks a bar (ADR-0016); lanes and swing on 16 clock steps a bar | `notes`, `clock` |

- **A pitch envelope moves in sub-blocks.** While a voice's pitch envelope is still moving, its pool renders it in sub-blocks of `PITCH_SUB` (32) samples and sets its trim before each, at the envelope's value there; the shared LFO is read from the sub-block's start. Every model that reads its trim follows; a voice without a moving envelope renders exactly as before. The #744 drop is off its straight line by at most about 0.12 semitone, where a step a block was 0.6.
- **The rest stays a block.** The filter envelope rides the cutoff's 2 ms smoothing, and gains are ramped, so neither steps.

## Consequences

- A song renders the same bytes whatever it holds; there is no rate to set.
- A voice with a moving pitch envelope costs four render calls a block instead of one, for the length of its envelope; `make bench`, alternated with the version before, shows no change for songs without one.
- A new fast signal says its rate here: added to the table, a constant named in the code.

## Alternatives considered

- **A control rate per song**, as Csound's `ksmps` in an orchestra. A song could trade smoothness for CPU. But 128 is the most a block can be, so it only goes finer, at a cost on every synth and every `mod`, and it changes what every song renders. Rejected for a rate per signal, as Csound's `setksmps` is per instrument.
- **A per-sample ramp of the pitch trim.** Smoother still, but every voice model reads its trim once a render call, so it means a change in each of them. Rejected for sub-blocks, which need none.
- **An envelope generator in each voice.** Exact, and the same change in nine models (ADR-0039's amendment). Rejected.
