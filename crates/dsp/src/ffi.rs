//! The C ABI the worklet shim calls (ADR-0001).
//!
//! No wasm-bindgen: its glue needs `TextDecoder` and friends, which the
//! AudioWorklet scope doesn't have. Each wasm instance holds one engine in a
//! thread-local (wasm32 without atomics has one thread), so the exports take
//! no pointers and nothing here dereferences raw memory. The only `unsafe` is
//! the `no_mangle` attribute itself, which is why this is the one module
//! allowed to use it.
#![allow(unsafe_code)]

use std::cell::RefCell;

use crate::engine::{BLOCK, Engine};
use crate::params::Param;
use crate::source::Source;

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

/// Run `f` on the engine; a no-op before `init` or on re-entry.
fn with_engine(f: impl FnOnce(&mut Engine)) {
    ENGINE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut()
            && let Some(engine) = guard.as_mut()
        {
            f(engine);
        }
    });
}

/// Create (or recreate) the engine. Call once from the worklet constructor.
#[unsafe(no_mangle)]
pub extern "C" fn init(sample_rate: f32) {
    ENGINE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            *guard = Some(Engine::new(sample_rate));
        }
    });
}

/// Frames per `process` call and the length of each output channel.
#[unsafe(no_mangle)]
pub extern "C" fn block_len() -> u32 {
    BLOCK as u32
}

/// Address of the planar stereo output buffer (left, then right).
#[unsafe(no_mangle)]
pub extern "C" fn out_ptr() -> *const f32 {
    ENGINE.with(|cell| match cell.try_borrow() {
        Ok(guard) => guard
            .as_ref()
            .map_or(std::ptr::null(), |e| e.output().as_ptr()),
        Err(_) => std::ptr::null(),
    })
}

/// Render the next block into the output buffer.
#[unsafe(no_mangle)]
pub extern "C" fn process(frames: u32) {
    with_engine(|e| e.render(frames as usize));
}

/// Set parameter `id` (see `params.rs`); unknown ids are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn set_param(id: u32, value: f32) {
    if let Some(p) = Param::from_id(id) {
        with_engine(|e| e.set_param(p, value));
    }
}

/// Start `note` (MIDI 0..=127) on `source`; unknown sources are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn note_on(source: u32, note: u32, velocity: f32) {
    if let Some(s) = Source::from_id(source) {
        let note = u8::try_from(note.min(127)).unwrap_or(127);
        with_engine(|e| e.note_on(s, note, velocity));
    }
}

/// Release `note` on `source`.
#[unsafe(no_mangle)]
pub extern "C" fn note_off(source: u32, note: u32) {
    if let (Some(s), Ok(n)) = (Source::from_id(source), u8::try_from(note)) {
        with_engine(|e| e.note_off(s, n));
    }
}

/// Release every voice.
#[unsafe(no_mangle)]
pub extern "C" fn all_off() {
    with_engine(Engine::all_off);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_are_no_ops_before_init() {
        process(128);
        note_on(0, 60, 1.0);
        set_param(0, 1.0);
        assert!(out_ptr().is_null());
    }

    #[test]
    fn exports_drive_the_engine() {
        init(48_000.0);
        assert!(!out_ptr().is_null());
        note_on(0, 69, 1.0);
        note_on(99, 69, 1.0); // unknown source: ignored
        process(128);
        let mut active = 0;
        with_engine(|e| active = e.active_voices());
        assert_eq!(active, 1);
    }
}
