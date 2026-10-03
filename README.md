# algo-synth

An algorithmic synthesizer that runs entirely in the browser, with the whole engine in Rust compiled to wasm on the audio thread. For now it is a family of six monosynths (ARP 2600, Minimoog, Pro-One, MS-20, CS-15, SH-101) and a MIDI player, on the way to six different synths playing Vivaldi; the sequencer, more sources and algo loops come after (ADR-0008).

## Version: 0.9.0

- **Mono:** one shared voice: three band-limited VCOs, ring mod and sub, noise, a 4-pole ladder or a 12 dB high-pass/low-pass pair, ADSR, filter ADSR and AR, LFO, normalled routing with patch overrides, poly-mod.
- **Up to 16 synths, six models:** add synths as you need them, each an ARP 2600, Minimoog, Pro-One, MS-20, CS-15 or SH-101 with its own panel, colours and patch. A MIDI file plays each part on its own synth, on the way to six different synths playing Vivaldi.
- **Setups:** save the synths, their models, patches and routing as `<song>.synths.json` next to the MIDI file, and open both together.
- **No backend:** the container serves static files.

The base (v0.1.0) is the pipeline: a test voice from Rust through the AudioWorklet, in the four-pane layout. v0.2.0 adds a MIDI file player in the engine, each channel routed to a source with a first timbre of its own. v0.3.0 is the Mono voice (MVP 2): three band-limited VCOs, noise, a 4-pole ladder, ADSR, LFO and four presets. v0.4.0 makes it playable: note priority, legato and glide, and normalled routing with an 8-slot patch. v0.5.0 adds a mixer, a drive insert and send effects. v0.6.0 is the family of monosynths: up to 16 synths, each an ARP 2600, Minimoog, Pro-One, MS-20, CS-15 or SH-101. v0.7.0 saves a setup (synths, models, patches, routing, effects) as `<song>.synths.json` next to the MIDI file. See [.openspec/plan.md](.openspec/plan.md) for the road from one mono voice to a true algo synth.

## Quick start

```bash
make dev      # Vite on http://localhost:6341 (rebuilds dsp.wasm first)
make serve    # Podman + Caddy on http://localhost:6340
make check    # every CI gate: lint, deny, tests, typecheck, build
make bench    # 16 Mono voices in V8 against the 25% CPU budget
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
