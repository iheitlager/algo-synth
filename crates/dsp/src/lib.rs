//! algo-synth engine.
//!
//! Everything musical runs here, as wasm inside the browser's AudioWorklet
//! (ADR-0001): the ARP 2600-style Mono voice (`mono`), its voice pool (`poly`), the mixer, the MIDI
//! file player (`smf`, `player`), the clock (`clock`) and the drum kit (`drums`). The JavaScript around it only forwards
//! messages and copies the output block.

pub mod algo;
pub mod clock;
pub mod drums;
pub mod engine;
pub mod fm;
pub mod fx;
pub mod la;
pub mod midi_import;
pub mod mixer;
pub mod mono;
pub mod notes;
pub mod padsampler;
pub mod params;
pub mod player;
pub mod poly;
pub mod sample;
pub mod sampler;
pub mod smf;
pub mod song;
pub mod table;
pub mod voice;

mod ffi;
