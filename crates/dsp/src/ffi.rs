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
use crate::mono::preset::Preset;
use crate::params::Param;
use crate::player::Part;
use crate::source::Source;

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

/// Run `f` on the engine and return its result, or `default` before `init`
/// or on re-entry.
fn query<R>(default: R, f: impl FnOnce(&mut Engine) -> R) -> R {
    ENGINE.with(|cell| match cell.try_borrow_mut() {
        Ok(mut guard) => guard.as_mut().map_or(default, f),
        Err(_) => default,
    })
}

/// Run `f` on the engine; a no-op before `init` or on re-entry.
fn with_engine(f: impl FnOnce(&mut Engine)) {
    query((), f);
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

/// Number of parameter ids; they run from 0 without gaps.
#[unsafe(no_mangle)]
pub extern "C" fn param_count() -> u32 {
    Param::ALL.len() as u32
}

/// The value parameter `id` was last set to, after clamping; 0 if unknown.
#[unsafe(no_mangle)]
pub extern "C" fn param_value(id: u32) -> f32 {
    Param::from_id(id).map_or(0.0, |p| query(0.0, |e| e.param_value(p)))
}

/// Load Mono preset `id` (see `mono/preset.rs`); unknown ids are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn mono_preset(id: u32) {
    if let Some(p) = Preset::from_id(id) {
        with_engine(|e| e.preset(p));
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

// --- MIDI player (spec 002, Req 9) -------------------------------------------
//
// Loading: JavaScript calls `midi_buf(len)`, writes the file's bytes at the
// returned address, then calls `midi_load()`. The engine owns the buffer, so
// Rust never dereferences a pointer it was handed.

/// Size the engine's MIDI buffer and return its address; null if too large.
#[unsafe(no_mangle)]
pub extern "C" fn midi_buf(len: u32) -> *mut u8 {
    query(std::ptr::null_mut(), |e| {
        e.midi_buffer(len as usize)
            .map_or(std::ptr::null_mut(), |b| b.as_mut_ptr())
    })
}

/// Parse the buffer: the number of parts, or a negative `smf::Error` code
/// (−5 before `init`).
#[unsafe(no_mangle)]
pub extern "C" fn midi_load() -> i32 {
    query(-5, |e| match e.load_midi() {
        Ok(parts) => i32::try_from(parts).unwrap_or(i32::MAX),
        Err(err) => err.code(),
    })
}

fn seconds(e: &Engine, samples: u64) -> f32 {
    (samples as f64 / f64::from(e.sample_rate())) as f32
}

fn with_part<R: Copy>(default: R, i: u32, f: impl FnOnce(&Engine, &Part) -> R) -> R {
    query(default, |e| {
        let e: &Engine = e;
        match e.sequence().parts().get(i as usize) {
            Some(p) => f(e, p),
            None => default,
        }
    })
}

/// Parts (MIDI channels with notes) in the loaded file.
#[unsafe(no_mangle)]
pub extern "C" fn part_count() -> u32 {
    query(0, |e| e.sequence().parts().len() as u32)
}

/// MIDI channel (0..=15) of part `i`.
#[unsafe(no_mangle)]
pub extern "C" fn part_channel(i: u32) -> u32 {
    with_part(0, i, |_, p| u32::from(p.channel))
}

/// Note count of part `i`.
#[unsafe(no_mangle)]
pub extern "C" fn part_notes(i: u32) -> u32 {
    with_part(0, i, |_, p| p.notes)
}

/// First note of part `i`, in seconds.
#[unsafe(no_mangle)]
pub extern "C" fn part_start(i: u32) -> f32 {
    with_part(0.0, i, |e, p| seconds(e, p.start))
}

/// Last event of part `i`, in seconds.
#[unsafe(no_mangle)]
pub extern "C" fn part_end(i: u32) -> f32 {
    with_part(0.0, i, |e, p| seconds(e, p.end))
}

/// Address of part `i`'s name bytes (decoded on the main thread).
#[unsafe(no_mangle)]
pub extern "C" fn part_name_ptr(i: u32) -> *const u8 {
    with_part(std::ptr::null(), i, |_, p| p.name.as_ptr())
}

/// Length of part `i`'s name in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn part_name_len(i: u32) -> u32 {
    with_part(0, i, |_, p| p.name.len() as u32)
}

/// Events (note ons and offs) in the loaded file, for drawing.
#[unsafe(no_mangle)]
pub extern "C" fn event_count() -> u32 {
    query(0, |e| e.sequence().events().len() as u32)
}

/// Event `i` packed for the view: `on << 15 | channel << 8 | note`; 0 if out of range.
#[unsafe(no_mangle)]
pub extern "C" fn event_packed(i: u32) -> u32 {
    query(0, |e| {
        e.sequence().events().get(i as usize).map_or(0, |ev| {
            u32::from(ev.on) << 15 | u32::from(ev.channel) << 8 | u32::from(ev.note)
        })
    })
}

/// Time of event `i`, in seconds.
#[unsafe(no_mangle)]
pub extern "C" fn event_time(i: u32) -> f32 {
    query(0.0, |e| {
        let s = e
            .sequence()
            .events()
            .get(i as usize)
            .map_or(0, |ev| ev.sample);
        seconds(e, s)
    })
}

/// Length of the loaded file, in seconds.
#[unsafe(no_mangle)]
pub extern "C" fn song_length() -> f32 {
    query(0.0, |e| seconds(e, e.sequence().length()))
}

/// One 4/4 bar at the opening tempo, in seconds.
#[unsafe(no_mangle)]
pub extern "C" fn song_bar() -> f32 {
    query(0.0, |e| seconds(e, e.sequence().bar()))
}

#[unsafe(no_mangle)]
pub extern "C" fn play() {
    with_engine(Engine::play);
}

#[unsafe(no_mangle)]
pub extern "C" fn stop() {
    with_engine(Engine::stop);
}

/// Jump to `seconds`.
#[unsafe(no_mangle)]
pub extern "C" fn seek(seconds: f32) {
    with_engine(|e| {
        let s = (f64::from(seconds.max(0.0)) * f64::from(e.sample_rate())) as u64;
        e.seek(s);
    });
}

/// Playback position, in seconds.
#[unsafe(no_mangle)]
pub extern "C" fn position() -> f32 {
    query(0.0, |e| seconds(e, e.sequence().position()))
}

/// 1 while playing.
#[unsafe(no_mangle)]
pub extern "C" fn playing() -> u32 {
    query(0, |e| u32::from(e.sequence().playing()))
}

/// Route MIDI `channel` to `source`; any unknown source id (e.g. 255) mutes it.
#[unsafe(no_mangle)]
pub extern "C" fn route(channel: u32, source: u32) {
    if let Ok(ch) = u8::try_from(channel) {
        with_engine(|e| e.route(ch, Source::from_id(source)));
    }
}

/// The source id MIDI `channel` plays on, or 255 if muted.
#[unsafe(no_mangle)]
pub extern "C" fn routed(channel: u32) -> u32 {
    let ch = u8::try_from(channel).unwrap_or(u8::MAX);
    query(255, |e| e.routed(ch).map_or(255, |s| s as u32))
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
        assert_eq!(query(0, |e| e.active_voices()), 1);
    }

    #[test]
    fn midi_round_trip_through_the_abi() {
        init(48_000.0);
        assert_eq!(midi_load(), -1); // empty buffer: not MIDI
        let bytes = include_bytes!("../../../web/public/demo.mid");
        let ptr = midi_buf(bytes.len() as u32);
        assert!(!ptr.is_null());
        query((), |e| {
            e.midi_buffer(bytes.len())
                .expect("fits")
                .copy_from_slice(bytes)
        });
        assert_eq!(midi_load(), 5);
        assert_eq!(part_channel(4), 9);
        assert_eq!(routed(9), 2); // Drums
        route(9, 255);
        assert_eq!(routed(9), 255);
        assert!(part_name_len(1) > 0);
        assert!(song_length() > 60.0);
        assert!((song_bar() - 4.0 * 60.0 / 72.0).abs() < 1.0e-3);
        play();
        process(128);
        assert_eq!(playing(), 1);
        seek(10.0);
        assert!((position() - 10.0).abs() < 1.0e-3);
        stop();
        assert_eq!(playing(), 0);
        assert_eq!(part_notes(99), 0);
        assert!(midi_buf(u32::MAX).is_null());
    }
}
