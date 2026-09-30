//! algo-synth engine.
//!
//! Everything musical runs here, as wasm inside the browser's AudioWorklet
//! (ADR-0001): sound sources, mixer, and later the sequencer and the
//! generators. The JavaScript around it only forwards messages and copies
//! the output block.
//!
//! The base ships one test voice (a table sine with an AR envelope) for all
//! three sources, which proves the pipeline end to end (plan.md, MVP 1).

pub mod engine;
pub mod params;
pub mod source;

mod ffi;
