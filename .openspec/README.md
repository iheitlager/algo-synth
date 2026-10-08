# OpenSpec: algo-synth

Specifications and architectural decisions for **algo-synth**, an algorithmic synthesizer that runs entirely in the browser.

## Goal

A synth you compose *with*, not just play: sound sources, a sequencer and algorithmic loops in one instrument, with no install and no server-side audio.

- **Everything musical is wasm.** Sources, mixer, sequencer, generators and the song model are Rust compiled to one wasm module, running on the browser's audio thread (ADR-0001). JavaScript only forwards messages and draws.
- **Real time, no excuses.** The render loop never allocates, locks or panics (ADR-0002).
- **The song is text.** A fragment is written by hand, by a generator (an algo loop), or converted from a score (a MIDI file), all in one song the engine parses and prints (ADR-0005, ADR-0012, ADR-0015).
- **Working to working.** One good monophonic voice, then the ensemble, then time, then more sources, then the algorithms ([plan.md](plan.md)).

## Architecture

```
 Browser, main thread                         Browser, audio thread
 ┌──────────────────────────────┐   port     ┌──────────────────────────────┐
 │ Vue view (web/src)           │ ─────────▶ │ worklet.js  (shim, no logic) │
 │  synths · mixer · composer   │  messages  │   └─ dsp.wasm  (crates/dsp)  │
 │ AnalyserNode → scope         │ ◀───────── │      sources · mixer ·       │
 └──────────────────────────────┘   audio    │      sequencer · generators  │
                                              └──────────────────────────────┘
 Podman + Caddy: static files only (ADR-0006)
```

## Vision and plan

- [vision.md](vision.md): what algo-synth is, the instruments, the song as text, the sound path, and what it is not.
- [plan.md](plan.md): from a sine through one monophonic voice to a true algo synth, working to working.

## Decisions

See [adr/index.md](adr/index.md).

## Specifications

| Spec | Scope |
|---|---|
| [001-engine](specs/001-engine/spec.md) | The wasm engine: C ABI, render loop, voice pools, parameters and their TypeScript mirror |
| [002-composition](specs/002-composition/spec.md) | Synth slots and drum machines, the mixer and effects, the song language (fragments, arrangement, generators, scales, automation, settings, limits), MIDI import and playback, the clock |
| [003-ui](specs/003-ui/spec.md) | The wide-screen view: transport, synth faceplates, mixer console, composer and song editor, arranger, MIDI player, names, presets, build info |
| [004-mono](specs/004-mono/spec.md) | The Mono voice: VCOs, noise, filters, envelopes, modulation, note handling, normalled routing, presets; MIDI input (planned) |
| [005-models](specs/005-models/spec.md) | The monosynth models: ARP 2600, Minimoog, Pro-One, MS-20, CS-15, SH-101 and Odyssey, each with its own sound, panel and colours |
| [006-poly](specs/006-poly/spec.md) | Polyphony: the voice pool, allocation and stealing, unison, analog variance, and the Prophet-5, Juno-106, Jupiter-8, Matrix-12, PPG Wave, D-50, DX7 and Polymoog |
| [007-samplers](specs/007-samplers/spec.md) | The sample store, the multisampler, the pad sampler and the sample packs |
| [008-assist](specs/008-assist/spec.md) | The assistant: song tools, providers behind one interface, the loop and the server (ADR-0028) |

Every `**Implementation:**` and `**Tests:**` reference must resolve: a backticked path (`crates/…`, `web/…`, `tools/…`) must exist, and in `path.rs::Type::name` the last segment must be defined in that file (or in its `name/` module directory). `tools/test_specs.py` checks this and runs in `make test-tools`. A requirement that is not built yet starts its line with `(planned)`, as in `**Implementation:** (planned, #10) …`; the check skips that line, and it names no path as if it existed.
