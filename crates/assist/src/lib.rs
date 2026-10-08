//! The assist server's side of the song (ADR-0028, epic #381): tools a
//! language model calls while it writes a song, on the engine itself so they
//! never drift from it (#384). The server (#385) comes later in this crate.

pub mod tools;
