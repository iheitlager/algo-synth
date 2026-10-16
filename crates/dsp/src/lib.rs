//! algo-synth engine.
//!
//! Everything musical runs here, as wasm inside the browser's AudioWorklet
//! (ADR-0001): the ARP 2600-style Mono voice (`mono`), its voice pool (`poly`), the mixer, MIDI
//! file import (`smf`, `midi_import`), the clock (`clock`) and the drum kit (`drums`). The JavaScript around it only forwards
//! messages and copies the output block.

pub mod algo;
pub mod analysis;
pub mod arp;
pub mod bounce;
pub mod chip;
pub mod clock;
pub mod deck;
pub mod drums;
pub mod engine;
pub mod fm;
pub mod fx;
pub mod la;
pub mod launch;
pub mod midi;
pub mod midi_import;
pub mod mixer;
pub mod modular;
pub mod mono;
pub mod notes;
pub mod padsampler;
pub mod params;
pub mod poly;
pub mod sample;
pub mod sampler;
pub mod smf;
pub mod song;
pub mod spectrum;
pub mod synth;
pub mod table;
pub mod usage;
pub mod voice;

// The worklet's ABI, only for wasm (and its tests): a native build that
// links the engine (algo-synth, song-lsp) would carry the same no_mangle
// symbols, whose coverage records then clash with the tests' (#535).
#[cfg(any(target_arch = "wasm32", test))]
mod ffi;
