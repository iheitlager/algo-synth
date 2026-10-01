//! algo-synth engine.
//!
//! Everything musical runs here, as wasm inside the browser's AudioWorklet
//! (ADR-0001): sound sources, mixer, and later the sequencer and the
//! generators. The JavaScript around it only forwards messages and copies
//! the output block.
//!
//! v0.2 adds a MIDI file player (`smf`, `player`) that spreads a file's
//! channels over the sources, and a first timbre per source (`voice`).

pub mod engine;
pub mod params;
pub mod player;
pub mod smf;
pub mod source;
pub mod voice;

mod ffi;
