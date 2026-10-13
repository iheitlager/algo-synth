# 0034: Chip instruments at register precision, shared with the Modular

**Status:** Accepted · **Date:** 2026-10-13 · **Extends:** 0007, 0009, 0024, 0025

## Context

#112 asks for the two retro sound chips everyone knows: the C64's MOS 6581/8580 SID and the NES's Ricoh 2A03, with the 2A03's delta-modulation channel (DMC) playing samples. The issue was written before #227 and ADR-0025: it names "source ids" and `source.rs`, which no longer exist. A track plays a synth with a model, and a model's definition names the engine its voice runs on.

What makes these chips sound as they do is the grid their registers put everything on:

- a 16-bit frequency register on a 24-bit accumulator, or an 11-bit timer;
- 12-bit waveforms and 4-bit volumes;
- an 8-bit envelope stepped by a table of rate periods (with the SID's rate-counter "ADSR bug");
- non-linear DACs and mixers;
- player routines that write the registers once per video frame.

A chip at its own clock (about 1 MHz for the SID, 1.79 MHz for the NES CPU) is too dear to run per voice in WebAssembly.

The best-known instruments split the same way. Plogue's chipsynth C64 and chipsounds model the chips cycle by cycle, commercially. reSID and resid-rs do it too, under the GPL. jsSID (WTFPL) and dSID (MIT) step once per output sample and sound close.

## Decision

**Each chip is a model of its own, `Sid` and `Nes`, in a new Chips family, on an engine of its own (`Engine::Sid`, `Engine::Nes`). A note plays a whole chip. The chip's parts live in `crates/dsp/src/chip/`, and the Modular reaches the same parts as UGens.**

- **Register precision without the chip clock.** Each part steps once per output sample by the chip cycles that sample holds, counted in 16.16 fixed point, so nothing drifts. Integer state keeps the chip's grid:
  - the SID: the 24-bit accumulator, the 23-bit noise LFSR (clocked on accumulator bit 19, read as 8 bits), the 15-bit rate counter against the period table, the 8-bit envelope with its exponential divider;
  - the NES: the 11-bit timers, the duty sequences, the 32-step triangle, the 15-bit LFSR with its short mode, the 7-bit DMC counter at the rate table.
  
  Notes are snapped to the frequency register or timer the chip would hold.
- **Analog parts as measured or derived.**
  - **SID DACs:** R-2R ladders, ideal on the 8580 and 2R/R = 2.2 without termination on the 6581, solved by superposition at compile time into static tables.
  - **SID filter:** a zero-delay-feedback state-variable filter from the 11-bit cutoff (each chip's curve) and 4-bit resonance; the 6581's input clips softly.
  - **NES mixer:** the console's non-linear formulas from the NESdev wiki, applied to the integer levels, and the Famicom's or NES's output filters after it.
- **Band-limited at the steps the registers make** (ADR-0007), or raw by a switch.
  - **SID:** the saw and pulse edges and any sync reset get the BLEP table's residual, sized by the DAC's step.
  - **NES:** the mixed output is corrected at every channel step, so the mixer's compression acts on true levels.
- **A note is a chip.** The SID's three oscillators keep the chip's wiring: ring and sync from 1←3, 2←1, 3←2. The NES's five channels mix through its mixer. Three notes of one oscillator each are the chip's three voices; up to eight chips sound at once.
- **A player routine per voice.** Once per video frame (SID: 50/60 Hz; NES: the frame counter's sequence) it:
  - sweeps the SID's pulse width and cutoff;
  - steps a chord arpeggio;
  - runs a delayed triangle vibrato.

  Trackers make chords and vibrato the same way.
- **The NES as a drum machine.** With `NesKit`, a General MIDI key plays the drums NES scores make of the channels: a diving triangle and a noise click for the kick, noise for snares and hats, triangle toms. Other keys play the DMC's sample. A `drums` track takes `Nes`, so lanes play it.
- **The DMC reads the sample store** (ADR-0013): a slot by `NesDmcSample`. It delta-encodes the slot's audio on the fly, one bit at a time at the rate table, or writes 7-bit levels (`$4011` PCM). Playback is capped at the chip's longest sample, 4081 bytes.
- **Modular addresses the chips** (ADR-0024): `SidOsc`, `SidEnv`, `SidFilter`, `NesPulse`, `NesTri`, `NesNoise` and `NesDmc` run the same code as the models, with knobs at the registers' widths.
- **Written from public documentation:** datasheets, Bob Yannes's interviews, the NESdev wiki and published measurements. No reSID, resid-rs, Mesen or blip_buf code. Not cycle-exact.

## Consequences

- Two models and 95 parameters (`Sid*` 540–591, `Nes*` 592–634), mirrored in `params.ts`. Nine presets, among them `ModularSid`.
- `render` stays allocation-free: chip state is plain `Copy` data in the voice and in the Modular's chip slots (at most 16 a voice). The DAC tables are statics.
- Per-sample stepping means a raw (unband-limited) SID or NES aliases where the chip would not, which is why band-limiting is on by default.
- Left out: the SID's analog bleed between combined waveforms (here an AND of their bits) and its constant DC offsets (the envelope-scaled offset of the 6581 is kept). On the NES, the length counter is left out (notes end on release) and the DMC's sample-buffer timing. `.sid`, `.nsf` and register dumps are not read. Expansion chips (VRC6, FDS, N163) are not built.

## Alternatives considered

- **Cycle-exact cores at the chip clock**, resampled: the most faithful, but about 1 M steps a second per voice, too dear for a pool in WebAssembly. Ports of the open cores are GPL.
- **Chips as Mono-voice models** (ADR-0009): the Mono voice's oscillators, envelopes and filter are none of the chips', and its parameters would not hold the registers.
- **One voice per chip channel**, with the whole chip as the pool: authentic voice allocation, but ring modulation and sync would cross voices, and a chord would need channels free. One chip per note keeps every patch self-contained.
- **A drum kit model of its own for the NES** (`Engine::Drums`): it would share the 808's pads, not the chip's channels.
