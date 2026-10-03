# 0013: The sample store: Rust parses and resamples WAV at load, in bounded memory

**Status:** Accepted · **Date:** 2026-10-04

## Context

The samplers (epic #126) play recorded audio beside the synths. Samples are large and arrive from the browser, but ADR-0001 keeps everything musical in Rust and ADR-0002 forbids allocation in `render`.

## Decision

- **JavaScript forwards bytes, Rust does the rest.** The view writes a WAV file into an engine-owned buffer (`sample_buf`) and calls `sample_load(slot)`, as it does for MIDI files. Rust parses PCM 16/24 and 32-bit float, mono or stereo, reads the `smpl` chunk's root note and first loop, and resamples to the engine's rate (Catmull-Rom) so playback never converts rates. Loop points move with it.
- **Allocation happens at load, never in `render`.** A fixed table of 64 slots holds each sample's `f32` data; a load allocates, a replace or `sample_clear` frees. Playback only reads.
- **Memory is capped.** A file over 32 MiB is refused before it is copied, and the store holds at most 16 Mi `f32` values (64 MiB) across all slots. A load that would pass either leaves the slot as it was and returns an error code.
- **Errors are values.** `parse` is total: any byte string gives a sample or a negative code (not WAV, truncated, unsupported, empty, too large, no such slot).

## Consequences

Samples are held at the engine's rate, so a changed context rate needs a reload. A sample recorded above the engine's rate is not low-passed before decimation. Compressed formats (FLAC, Ogg) and 8-bit WAV are not read.
