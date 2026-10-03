//! algo-synth engine.
//!
//! Everything musical runs here, as wasm inside the browser's AudioWorklet
//! (ADR-0001): the ARP 2600-style Mono voice (`mono`), the mixer and the MIDI
//! file player (`smf`, `player`), the clock (`clock`) and the drum kit (`drums`). The JavaScript around it only forwards
//! messages and copies the output block.

pub mod clock;
pub mod drums;
pub mod engine;
pub mod fx;
pub mod mixer;
pub mod mono;
pub mod params;
pub mod player;
pub mod smf;
pub mod voice;

mod ffi;
