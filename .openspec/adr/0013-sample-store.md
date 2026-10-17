# 0013: The sample store: Rust parses and resamples WAV at load, in bounded memory

**Status:** Accepted, amended by #728 (samples from the web), extended by 0044 · **Date:** 2026-10-04

## Context

The samplers (epic #126) play recorded audio beside the synths. Samples are large and arrive from the browser, but ADR-0001 keeps everything musical in Rust and ADR-0002 forbids allocation in `render`.

## Decision

- **JavaScript forwards bytes, Rust does the rest.** The view writes a WAV file into an engine-owned buffer (`sample_buf`) and calls `sample_load(slot)`, as it does for MIDI files. Rust parses PCM 16/24 and 32-bit float, mono or stereo, reads the `smpl` chunk's root note and first loop, and resamples to the engine's rate (Catmull-Rom) so playback never converts rates. Loop points move with it.
- **Allocation happens at load, never in `render`.** A fixed table of 64 slots holds each sample's `f32` data; a load allocates, a replace or `sample_clear` frees. Playback only reads.
- **Memory is capped.** A file over 32 MiB is refused before it is copied, and the store holds at most 16 Mi `f32` values (64 MiB) across all slots. A load that would pass either leaves the slot as it was and returns an error code.
- **Errors are values.** `parse` is total: any byte string gives a sample or a negative code (not WAV, truncated, unsupported, empty, too large, no such slot).
- **The multisampler is a synth model** (`Model::Sampler`), a variant of the pool's voice beside Mono, LA and FM (ADR-0011), so polyphony, stealing, the MIDI player and the mixer strip are shared. Each synth owns 64 *zones* (key and velocity range, sample slot, root, tune, level, loop mode and points, round-robin position, release flag) in fixed arrays, set field by field through `zone_set`; a voice picks its zone when it starts and reads the store with Catmull-Rom interpolation, through the synth's own filter and amplifier envelope. Sustain loops are crossfaded; a release zone sounds once when the key comes up. Stereo samples are mixed to mono, as strips are mono until the pan (ADR-0010).
- **The view only forwards and draws.** The worklet copies WAV bytes in and reads back the summary, the waveform's min/max peaks (computed in Rust, `sample_peaks`) and each synth's zones (`zone_get`); the pane sets zones through `zone_set` and holds no music logic. Packs are plain files served statically (`make samples`, #129) with a manifest of zones the view lays out through the same calls.

## Consequences

Samples are held at the engine's rate, so a changed context rate needs a reload. A sample recorded above the engine's rate is not low-passed before decimation. Compressed formats (FLAC, Ogg) and 8-bit WAV are not read.

## Amendment (#728): samples from the web

A song may name a Strudel sample map (`samples brk github:yaxu/clean-breaks`) and give its pads the map's sounds by name (`pad brk 1 = amen`), as Strudel's `samples('github:…')` and `s("amen")` do.

- **The view fetches; the engine still can't.** The view fetches the map's JSON and the WAV files the pad lines name, and hands their bytes to the engine as it does a pack's. The engine only parses the lines and reports each track's map.
- **Only what is named.** A map is fetched once a session per URL; of its sounds, only those a pad line names. Slots are keyed by the map's URL and the sound, so two maps' `amen` never share one. Nothing is cached across sessions.
- **https and WAV only.** A map is `github:user/repo[/branch]` (its `strudel.json` on raw.githubusercontent.com) or an https URL to a `.json`; its files must be https too. Only WAV loads, as before: a map pointing at MP3 or OGG says so instead of decoding in the browser. A cross-origin fetch works under the app's cross-origin isolation because it is a CORS request (GitHub answers `Access-Control-Allow-Origin: *`); a host that doesn't is an error, never a `no-cors` fetch.
- **Nothing is redistributed.** The song names a URL; the sounds come from where the user points and are never fetched by `make samples` or committed.
- **Errors at the line.** A map or a sound that can't be had shows at its `samples` or `pad` line; the rest of the song plays.

## Later decisions

- **Extended by ADR-0044:** packs are converted to mono 16-bit WAV at build time, so the store needs no decoder; the manifest is generated and untracked (#672).
