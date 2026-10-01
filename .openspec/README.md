# OpenSpec: algo-synth

Specifications and architectural decisions for **algo-synth**, an algorithmic synthesizer that runs entirely in the browser.

## Goal

A synth you compose *with*, not just play: sound sources, a sequencer and algorithmic loops in one instrument, with no install and no server-side audio.

- **Everything musical is wasm.** Sources, mixer, sequencer, generators and the song model are Rust compiled to one wasm module, running on the browser's audio thread (ADR-0001). JavaScript only forwards messages and draws.
- **Real time, no excuses.** The render loop never allocates, locks or panics (ADR-0002).
- **Three ways to fill a clip.** By hand, by a generator (an algo loop), or from a score (a MIDI file played by an ensemble of mono voices), all in one composition model (ADR-0005).
- **Mono first.** One good monophonic voice, then time, then more sources, then the algorithms ([plan.md](plan.md)).

## Architecture

```
 Browser, main thread                         Browser, audio thread
 ┌──────────────────────────────┐   port     ┌──────────────────────────────┐
 │ Vue view (web/src)           │ ─────────▶ │ worklet.js  (shim, no logic) │
 │  algo · instruments · bars   │  messages  │   └─ dsp.wasm  (crates/dsp)  │
 │ AnalyserNode → scope         │ ◀───────── │      sources · mixer ·       │
 └──────────────────────────────┘   audio    │      sequencer · generators  │
                                              └──────────────────────────────┘
 Podman + Caddy: static files only (ADR-0006)
```

## Vision and plan

- [vision.md](vision.md): what algo-synth is, the three sources, the composition model, and what it is not.
- [plan.md](plan.md): from a sine through one monophonic voice to a true algo synth, working to working.

## Decisions

See [adr/index.md](adr/index.md).

## Specifications

| Spec | Scope |
|---|---|
| [001-engine](specs/001-engine/spec.md) | The wasm engine: C ABI, render loop, voices, parameters |
| [002-composition](specs/002-composition/spec.md) | Tracks, sources, effects, patterns, clips, algo loops, score import, clock |
| [003-ui](specs/003-ui/spec.md) | The wide-screen view: transport, algo pane, instruments, arrangement |
| [004-mono](specs/004-mono/spec.md) | The Mono voice: VCOs, noise, ladder filter, modulation, mono note handling, normalled routing, MIDI input, presets |

Specs are in draft. `**Implementation:**` and `**Tests:**` lines name the *planned* paths for requirements that aren't built yet; they become real links as the code lands.
