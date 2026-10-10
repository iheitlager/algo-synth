# 0017: The Spectral Lab: a third window with its own engine, analysis in Rust off the audio thread

**Status:** Proposed · **Date:** 2026-10-10 · **Amends:** 0001

## Context

Epic #192 takes a WAV apart and builds it back: a short-time Fourier analysis tracked into partials, an additive resynthesis read straight off them, a noise residual, then a Synclavier-style additive model and a DX7 patch fitted to the sound. ADR-0020 already names the lab as its own window, apart from the Sound screen.

What is there to build on:

- ADR-0001 puts everything musical in Rust in one wasm module, behind a C ABI, with a JavaScript shim that holds no logic. ADR-0029 lets that module run in Web Workers too, again with no logic in the worker shim.
- The app already has two Vite entries, `index.html` and `assistant.html` (#387). The Assistant window talks to the main window over a `BroadcastChannel` and never touches the engine.
- WAV files are parsed and resampled in Rust (`sample.rs`, ADR-0013); the browser never decodes audio.
- ADR-0002 forbids allocation, locks and panics in `render`, and nothing in the engine runs for seconds at a time.

Analysis is unlike everything the engine does today. An STFT of a ten-second sound at 48 kHz with a 2048-point window and a hop of 256 is about 1900 FFTs, and the partial tracks that come out of it are sized by the sound, not fixed. A CMA-ES search (stage 6) runs thousands of offline renders. Neither can run on the audio thread, and neither belongs in the main window, where the song plays.

## Decision

**The Spectral Lab is a third Vite entry, `spectral-lab.html`, a window with its own AudioContext and its own `dsp.wasm` engine. Analysis, resynthesis and later matching are Rust in `crates/dsp`, called offline from a Web Worker that holds a second instance of the same module. The worker and the window's view only relay and draw.**

- **A third entry.** `spectral-lab.html` beside `index.html` and `assistant.html` in `vite.config.ts`, opened from the main app. It runs side by side with the app and never touches the main window's engine (as the Assistant does not).
- **Its own engine for listening.** The lab's AudioContext loads the same worklet and `dsp.wasm` as the main window. Its synths play the A/B: the original as a Sampler zone, the resynthesis as a second one. When the Synclavier model lands (stage 5) the resynthesis plays on it instead, as an instrument.
- **Analysis off the audio thread, in a worker (amends ADR-0001 as ADR-0029 did).** A Web Worker holds a second instance of `dsp.wasm` and calls the analysis and resynthesis functions through the C ABI. It copies bytes in and results out and holds no logic. The worker exists only to keep long offline calls off both the audio thread and the view's thread.
- **Offline calls may allocate; `render` still may not (ADR-0002).** Analysis and resynthesis are not `render`: they run in the worker's instance, take a WAV, and allocate what the sound needs, within a cap: at most 60 s of audio, at most 256 partials per frame, and an error code past either, never a panic. A resynthesis comes out as PCM, which the view loads into the lab's engine as a sample (ADR-0013), so playback follows the real-time rules as every sample does.
- **The results are arrays in wasm memory.** The analysis leaves frames of partial frequencies, amplitudes and phases in buffers the view reads by pointer and length, as `sample_peaks` does, and draws. No JSON and no wasm-bindgen (ADR-0001).
- **Send to the main window.** The lab sends a resynthesis (and later a preset or a `.syx`) to the main window over a `BroadcastChannel`; the main window loads it as it loads anything else. Only the main window touches its own engine.
- **A native harness.** `crates/dsp/examples/spectral.rs` analyses and resynthesises a WAV from the command line, as `filter_compare.rs` compares filters. It is how the analysis is tested on real material and timed before the search is built.

## Consequences

- The analysis is tested natively, like the rest of the DSP: render a known sound offline, analyse it, compare.
- The lab costs a second AudioContext and two more wasm instances (the lab's engine and its worker), about 8.5 MB each plus samples, only while the window is open.
- `crates/dsp` gains an `analysis` module (FFT, STFT, peaks, tracks) and an additive renderer. The private FFT in `mono/noise.rs`'s tests moves there.
- Matching (stage 6) runs in the same worker: a CMA-ES step API in Rust, called in a loop by the worker shim, posting the best patch so far to the view.
- Spec 009 holds analysis, resynthesis, the residual, matching and the lab window. The Synclavier model's requirements go into spec 006 with the other polyphonic models.

## Alternatives considered

- **The lab as a view in the main app.** One window, one engine. But a long analysis would block the view that plays the song, and an A/B in the main engine would take synths from the song. Rejected; ADR-0020 already kept the lab apart from the Sound screen.
- **Analysis on the lab's audio thread.** No worker. A one-second call would make the lab's audio drop out and break ADR-0002's spirit for no gain. Rejected.
- **Analysis in JavaScript or with the Web Audio `AnalyserNode`.** Quick to build, but it is logic outside Rust (ADR-0001), untestable natively, and an `AnalyserNode` gives magnitudes per block, not tracked partials. Rejected.
- **An FFT crate.** `rustfft` is fast and tested, but the crate is std-only by choice (ADR-0001) and a radix-2 FFT is a page of code that already exists in the tests. Rejected for now; revisit if the search shows the FFT is the bottleneck.
