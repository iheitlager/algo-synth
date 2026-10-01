# algo-synth

An algorithmic synthesizer that runs entirely in the browser: three sound sources, a sequencer and algo loops, with the whole engine in Rust compiled to wasm on the audio thread.

## Version: 0.2.0

- **Sources:** Mono (ARP 2600-style semi-modular), Wave (PPG-style wavetable), Drums (analog-style kit).
- **Compose three ways:** by hand, with seeded generators (Euclid, walk, arp, Markov), or from a MIDI score played by an ensemble of mono voices (six 2600s playing Vivaldi).
- **No backend:** the container serves static files.

The base (v0.1.0) is the pipeline: a test voice from Rust through the AudioWorklet, in the four-pane layout. v0.2.0 adds a MIDI file player in the engine, each channel routed to a source with a first timbre of its own. See [.openspec/plan.md](.openspec/plan.md) for the road from one mono voice to a true algo synth.

## Quick start

```bash
make dev      # Vite on http://localhost:6341 (rebuilds dsp.wasm first)
make serve    # Podman + Caddy on http://localhost:6340
make check    # every CI gate: lint, deny, tests, typecheck, build
make          # all targets
```

Needs Rust (stable, the `wasm32-unknown-unknown` target comes from `rust-toolchain.toml`), Node 24, and for `make serve` Podman (`podman machine init && podman machine start` once on a Mac). Open it in Chrome and press **Power on**; play with the on-screen keys or the computer keyboard (`a`…`;`).

## Layout

```
crates/dsp/     the engine (cdylib → dsp.wasm): C ABI, voices, params; later sequencer, generators
web/            Vue view; public/worklet.js is the audio-thread shim
.openspec/      vision, plan, ADRs, specs
Containerfile   wasm → web → Caddy
```

## Documentation

- [Vision](.openspec/vision.md) · [Plan](.openspec/plan.md) · [ADRs](.openspec/adr/index.md) · [Specs](.openspec/README.md#specifications)

## Licence

Apache-2.0
