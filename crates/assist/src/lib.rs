//! The assist server and its song tools (ADR-0028, epic #381): a language
//! model writes the song; the tools check and render it on the engine; the
//! server runs the loop over one of five providers and streams its steps.

pub mod assist;
pub mod config;
pub mod provider;
pub mod server;
pub mod tools;
