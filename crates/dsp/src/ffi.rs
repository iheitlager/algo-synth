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

use crate::deck::DeckField;
use crate::engine::{BLOCK, Engine, METERS, SYNTHS};
use crate::mixer::STRIPS;
use crate::mono::preset::Preset;
use crate::padsampler::{PADS, PadField};
use crate::params::Param;
use crate::sample;
use crate::sampler::{ZONES, ZoneField};
use crate::song::{Kind, Lane};

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

/// How many meters `meters_ptr` points at.
#[unsafe(no_mangle)]
pub extern "C" fn meters_len() -> u32 {
    METERS as u32
}

/// Address of the peak meters: one per synth strip, master left and right,
/// then each processor's return, as the highest linear level since the last
/// `meters_clear`.
#[unsafe(no_mangle)]
pub extern "C" fn meters_ptr() -> *const f32 {
    ENGINE.with(|cell| match cell.try_borrow() {
        Ok(guard) => guard
            .as_ref()
            .map_or(std::ptr::null(), |e| e.meters().as_ptr()),
        Err(_) => std::ptr::null(),
    })
}

/// Start the meters over, once the view has read them.
#[unsafe(no_mangle)]
pub extern "C" fn meters_clear() {
    with_engine(Engine::clear_meters);
}

// --- Decks (ADR-0029) ----------------------------------------------------

/// Where deck `deck` (1–3, B–D) writes its next block: `2 * block_len()`
/// values, left then right; null for another deck or before `init`.
#[unsafe(no_mangle)]
pub extern "C" fn deck_in_ptr(deck: u32) -> *mut f32 {
    query(std::ptr::null_mut(), |e| {
        e.deck()
            .input_mut(deck as usize)
            .map_or(std::ptr::null_mut(), |b| b.as_mut_ptr())
    })
}

/// Deck `deck` (1–3) wrote its block: it plays in the next `process`. An
/// unfed deck is silent for that block.
#[unsafe(no_mangle)]
pub extern "C" fn deck_fed(deck: u32) {
    with_engine(|e| e.deck().fed(deck as usize));
}

/// Set a deck's `DeckField` (0–3 for A–D); unknown ids are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn deck_set(deck: u32, field: u32, value: f32) {
    if let Some(f) = DeckField::from_id(field) {
        with_engine(|e| e.deck().set(deck as usize, f, value));
    }
}

/// The crossfader, 0 (left) to 1 (right).
#[unsafe(no_mangle)]
pub extern "C" fn deck_crossfade(x: f32) {
    with_engine(|e| e.deck().set_crossfade(x));
}

/// Frames from now to the master's next multiple of `every` steps (16 a
/// bar, 128 an 8-bar phrase) at least `at_least` frames away; −1 while the
/// song is stopped. Where a deck is cued to.
#[unsafe(no_mangle)]
pub extern "C" fn cue_frames(every: u32, at_least: u32) -> i32 {
    query(-1, |e| {
        e.cue_frames(u64::from(every), u64::from(at_least))
            .and_then(|f| i32::try_from(f).ok())
            .unwrap_or(-1)
    })
}

/// Frames past the master's bar line `at_least` frames from now; −1 while
/// the song is stopped. How far into its bar a deck started then begins.
#[unsafe(no_mangle)]
pub extern "C" fn cue_into(at_least: u32) -> i32 {
    query(-1, |e| {
        e.cue_into(u64::from(at_least))
            .and_then(|f| i32::try_from(f).ok())
            .unwrap_or(-1)
    })
}

/// Start the song `frames` from now, on that exact sample (a cued deck).
#[unsafe(no_mangle)]
pub extern "C" fn song_play_in(frames: u32) {
    with_engine(|e| e.song_play_in(frames as usize));
}

/// The same, `into` frames into its first bar: in phase with the master's.
#[unsafe(no_mangle)]
pub extern "C" fn song_play_in_bar(frames: u32, into: u32) {
    with_engine(|e| e.song_play_in_bar(frames as usize, u64::from(into)));
}

/// The arrangement entry bar `bar` (from 0) of the clock falls in, through
/// the loop; −1 without an arrangement or past its end.
#[unsafe(no_mangle)]
pub extern "C" fn song_bar_entry(bar: u32) -> i32 {
    query(-1, |e| {
        e.bar_entry(u64::from(bar))
            .map_or(-1, |i| i32::try_from(i).unwrap_or(-1))
    })
}

/// In `frames` frames the master is on a bar line: pull this song's nearest
/// bar line onto it if it is off (sync lock).
#[unsafe(no_mangle)]
pub extern "C" fn song_sync_bar_in(frames: u32) {
    with_engine(|e| e.sync_bar_in(frames as usize));
}

/// How far the last sync found the song off the master's bar, in frames
/// (positive: it was ahead).
#[unsafe(no_mangle)]
pub extern "C" fn sync_error() -> i32 {
    query(0, |e| i32::try_from(e.sync_error()).unwrap_or(i32::MAX))
}

/// Deck `deck`'s (0–3) peak after its gain since the last call, linear.
#[unsafe(no_mangle)]
pub extern "C" fn deck_peak(deck: u32) -> f32 {
    query(0.0, |e| e.deck().take_peak(deck as usize))
}

/// Render the next block into the output buffer.
#[unsafe(no_mangle)]
pub extern "C" fn process(frames: u32) {
    with_engine(|e| e.render(frames as usize));
}

/// Set parameter `id` (see `params.rs`) of `synth`; unknown ids and synths
/// are ignored. `MasterGain` is global.
#[unsafe(no_mangle)]
pub extern "C" fn set_param(synth: u32, id: u32, value: f32) {
    if let Some(p) = Param::from_id(id) {
        // From the view: a hand, folded into the song (ADR-0027).
        with_engine(|e| e.edit_param(synth as usize, p, value));
    }
}

/// Strips the mixer holds: the synths, then the group buses.
#[unsafe(no_mangle)]
pub extern "C" fn strip_count() -> u32 {
    STRIPS as u32
}

/// Models the engine knows (`Model::ALL`). The view compares it with its own list to catch
/// a `dsp.wasm` older than the JavaScript (a model it does not know silently stays a Mono).
#[unsafe(no_mangle)]
pub extern "C" fn model_count() -> u32 {
    crate::mono::model::Model::ALL.len() as u32
}

/// The workspace version as `major * 10000 + minor * 100 + patch`, so the view can show which
/// build of the engine it got (#197) without a string crossing the ABI.
#[unsafe(no_mangle)]
pub extern "C" fn version_code() -> u32 {
    VERSION_CODE
}

/// The git commit this wasm was built from, its first eight hex digits as a number; 0 when the
/// build did not say (`ALGO_BUILD_SHA` unset, #197).
#[unsafe(no_mangle)]
pub extern "C" fn build_id() -> u32 {
    BUILD_ID
}

const VERSION_CODE: u32 = parse(env!("CARGO_PKG_VERSION_MAJOR"), 10) * 10_000
    + parse(env!("CARGO_PKG_VERSION_MINOR"), 10) * 100
    + parse(env!("CARGO_PKG_VERSION_PATCH"), 10);
const BUILD_ID: u32 = match option_env!("ALGO_BUILD_SHA") {
    Some(sha) => parse(sha, 16),
    None => 0,
};

/// The leading digits of `s` in `radix` (at most eight for hex); 0 if a character is not one.
const fn parse(s: &str, radix: u32) -> u32 {
    let mut rest = s.as_bytes();
    let mut v = 0u32;
    let mut taken = 0;
    while let [c, tail @ ..] = rest {
        if radix == 16 && taken == 8 {
            break;
        }
        let d = match *c {
            c @ b'0'..=b'9' => c - b'0',
            c @ b'a'..=b'f' if radix == 16 => c - b'a' + 10,
            c @ b'A'..=b'F' if radix == 16 => c - b'A' + 10,
            _ => return 0,
        };
        v = v * radix + d as u32;
        rest = tail;
        taken += 1;
    }
    v
}

/// Number of Mono synths; ids run from 0.
#[unsafe(no_mangle)]
pub extern "C" fn synth_count() -> u32 {
    SYNTHS as u32
}

/// Put `synth` back to the default patch.
#[unsafe(no_mangle)]
pub extern "C" fn synth_reset(synth: u32) {
    with_engine(|e| e.reset(synth as usize));
}

/// Put `synth`'s sound back to the defaults, its strip unchanged (ADR-0014).
#[unsafe(no_mangle)]
pub extern "C" fn synth_defaults(synth: u32) {
    with_engine(|e| {
        e.synth_defaults(synth as usize);
        e.mark_synth(synth as usize);
    });
}

/// Number of parameter ids; they run from 0 without gaps.
#[unsafe(no_mangle)]
pub extern "C" fn param_count() -> u32 {
    Param::ALL.len() as u32
}

/// The value parameter `id` of `synth` was last set to, after clamping; 0
/// if unknown.
#[unsafe(no_mangle)]
pub extern "C" fn param_value(synth: u32, id: u32) -> f32 {
    Param::from_id(id).map_or(0.0, |p| query(0.0, |e| e.param_value(synth as usize, p)))
}

/// Load Mono preset `id` (see `mono/preset.rs`) on `synth`; unknown ids are
/// ignored.
#[unsafe(no_mangle)]
pub extern "C" fn mono_preset(synth: u32, id: u32) {
    if let Some(p) = Preset::from_id(id) {
        with_engine(|e| e.edit_preset(synth as usize, p));
    }
}

/// Start `note` (MIDI 0..=127) on `synth`'s live voice.
#[unsafe(no_mangle)]
pub extern "C" fn note_on(synth: u32, note: u32, velocity: f32) {
    let note = u8::try_from(note.min(127)).unwrap_or(127);
    with_engine(|e| e.note_on(synth as usize, note, velocity));
}

/// One MIDI message from a controller, as Web MIDI delivered it (#10);
/// the engine reads it (`Engine::midi_in`).
#[unsafe(no_mangle)]
pub extern "C" fn midi_in(status: u32, d1: u32, d2: u32) {
    let byte = |b: u32| u8::try_from(b).unwrap_or(0xFF);
    with_engine(|e| e.midi_in(byte(status), byte(d1), byte(d2)));
}

/// The synth MIDI input plays: the one selected in the view (#10).
#[unsafe(no_mangle)]
pub extern "C" fn midi_target(synth: u32) {
    with_engine(|e| e.set_midi_target(synth as usize));
}

/// Release `note` on `synth`'s live voice.
#[unsafe(no_mangle)]
pub extern "C" fn note_off(synth: u32, note: u32) {
    if let Ok(n) = u8::try_from(note) {
        with_engine(|e| e.note_off(synth as usize, n));
    }
}

/// Start over (#325): see `Engine::clear`.
#[unsafe(no_mangle)]
pub extern "C" fn engine_clear() {
    with_engine(Engine::clear);
}

/// A track for synth `s` on factory preset `preset` (ADR-0027): its index,
/// or −1 when refused.
#[unsafe(no_mangle)]
pub extern "C" fn track_add(s: u32, preset: u32) -> i32 {
    query(-1, |e| {
        Preset::from_id(preset)
            .and_then(|p| e.track_add(s as usize, p))
            .map_or(-1, |t| t as i32)
    })
}

/// Synth `s` taken off the screen (ADR-0027): 1 its track removed, 0 muted
/// (it has music), −1 it had none.
#[unsafe(no_mangle)]
pub extern "C" fn track_remove(s: u32) -> i32 {
    query(-1, |e| e.track_remove(s as usize))
}

/// What of synth `s`'s sound the song cannot hold yet (#361): bits of
/// `LIVE_ARP` 1, `LIVE_ZONES` 2, `LIVE_PADS` 4, `LIVE_FULL` 8.
#[unsafe(no_mangle)]
pub extern "C" fn live_only(s: u32) -> u32 {
    query(0, |e| e.live_only(s as usize))
}

/// Fold what the hands changed into the song (ADR-0027): 1 when its text
/// changed and should be sent to the view, 0 when not.
#[unsafe(no_mangle)]
pub extern "C" fn song_fold() -> u32 {
    query(0, |e| u32::from(e.fold()))
}

/// Release every voice.
#[unsafe(no_mangle)]
pub extern "C" fn all_off() {
    with_engine(Engine::all_off);
}

/// Voices still sounding (gated or releasing), for the view's counter.
#[unsafe(no_mangle)]
pub extern "C" fn active_voices() -> u32 {
    query(0, |e| e.active_voices() as u32)
}

/// The master compressor's gain reduction in dB, for the view's meter.
#[unsafe(no_mangle)]
pub extern "C" fn gain_reduction_db() -> f32 {
    query(0.0, |e| e.gain_reduction_db())
}

// --- MIDI files (imported, spec 002 Req 8, ADR-0022) -------------------------
//
// Loading: JavaScript calls `midi_buf(len)`, writes the file's bytes at the
// returned address, then calls `midi_import()`. The engine owns the buffer, so
// Rust never dereferences a pointer it was handed.

/// Size the engine's MIDI buffer and return its address; null if too large.
#[unsafe(no_mangle)]
pub extern "C" fn midi_buf(len: u32) -> *mut u8 {
    query(std::ptr::null_mut(), |e| {
        e.midi_buffer(len as usize)
            .map_or(std::ptr::null_mut(), |b| b.as_mut_ptr())
    })
}

// --- DX7 SysEx (spec 006, Req 14) ----------------------------------------------
//
// Same shape as the MIDI file: `sysex_buf(len)`, write the bytes, `sysex_load()`.
// Voice names are read as bytes (`sysex_name_ptr`/`sysex_name_len`); `sysex_apply`
// sets the DX7 parameters of a synth from one voice.

/// Size the engine's SysEx buffer and return its address; null if too large.
#[unsafe(no_mangle)]
pub extern "C" fn sysex_buf(len: u32) -> *mut u8 {
    query(std::ptr::null_mut(), |e| {
        e.sysex_buffer(len as usize)
            .map_or(std::ptr::null_mut(), |b| b.as_mut_ptr())
    })
}

/// Parse the buffer: the number of voices, or a negative `sysex::Error` code
/// (−5 before `init`).
#[unsafe(no_mangle)]
pub extern "C" fn sysex_load() -> i32 {
    query(-5, |e| match e.load_sysex() {
        Ok(n) => i32::try_from(n).unwrap_or(i32::MAX),
        Err(err) => err.code(),
    })
}

/// Address of voice `i`'s name (ASCII); valid until the next `sysex_load`.
#[unsafe(no_mangle)]
pub extern "C" fn sysex_name_ptr(i: u32) -> *const u8 {
    query(std::ptr::null(), |e| e.sysex_name(i as usize).as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn sysex_name_len(i: u32) -> u32 {
    query(0, |e| e.sysex_name(i as usize).len() as u32)
}

/// Set synth `synth`'s DX7 parameters from voice `i`; false if there is no such voice.
#[unsafe(no_mangle)]
pub extern "C" fn sysex_apply(synth: u32, i: u32) -> bool {
    query(false, |e| {
        e.mark_synth(synth as usize);
        e.apply_sysex(synth as usize, i as usize)
    })
}

/// Size the WAV buffer for `len` bytes and return its address for writing;
/// null if `len` is over the cap or before `init`.
#[unsafe(no_mangle)]
pub extern "C" fn sample_buf(len: u32) -> *mut u8 {
    query(std::ptr::null_mut(), |e| {
        e.sample_buffer(len as usize)
            .map_or(std::ptr::null_mut(), |b| b.as_mut_ptr())
    })
}

/// Parse the buffer into `slot`: the frame count at the engine's rate, or a
/// negative `sample::Error` code (−5 before `init`).
#[unsafe(no_mangle)]
pub extern "C" fn sample_load(slot: u32) -> i32 {
    query(-5, |e| match e.load_sample(slot as usize) {
        Ok(frames) => i32::try_from(frames).unwrap_or(i32::MAX),
        Err(err) => err.code(),
    })
}

/// Free `slot`.
#[unsafe(no_mangle)]
pub extern "C" fn sample_clear(slot: u32) {
    with_engine(|e| e.clear_sample(slot as usize));
}

/// Slots in the sample store.
#[unsafe(no_mangle)]
pub extern "C" fn sample_slots() -> u32 {
    sample::SLOTS as u32
}

/// Frames in `slot`, 0 if empty.
#[unsafe(no_mangle)]
pub extern "C" fn sample_frames(slot: u32) -> u32 {
    query(0, |e| {
        e.samples()
            .get(slot as usize)
            .map_or(0, |s| s.frames() as u32)
    })
}

/// The sample's MIDI root note, 255 if the slot is empty.
#[unsafe(no_mangle)]
pub extern "C" fn sample_root(slot: u32) -> u32 {
    query(255, |e| {
        e.samples()
            .get(slot as usize)
            .map_or(255, |s| u32::from(s.root))
    })
}

/// Loop start and end in frames: `which` 0 is the start, 1 the end; both 0
/// when the sample has no loop.
#[unsafe(no_mangle)]
pub extern "C" fn sample_loop(slot: u32, which: u32) -> u32 {
    query(0, |e| {
        let r = e.samples().get(slot as usize).and_then(|s| s.loop_range);
        r.map_or(0, |(a, b)| if which == 0 { a as u32 } else { b as u32 })
    })
}

/// `f32` values the store holds, against `sample_cap`.
#[unsafe(no_mangle)]
pub extern "C" fn sample_used() -> u32 {
    query(0, |e| e.samples().values() as u32)
}

/// The store's cap in `f32` values.
#[unsafe(no_mangle)]
pub extern "C" fn sample_cap() -> u32 {
    sample::MAX_VALUES as u32
}

/// Set field `field` (see `sampler::ZoneField`) of zone `zone` of `synth`;
/// unknown synths, zones and fields are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn zone_set(synth: u32, zone: u32, field: u32, value: f32) {
    if let Some(f) = ZoneField::from_id(field) {
        with_engine(|e| e.set_zone(synth as usize, zone as usize, f, value));
    }
}

/// A field of a zone, as `zone_set` takes it (−1 for an empty sample slot or an
/// unset root); 0 for an unknown synth, zone or field.
#[unsafe(no_mangle)]
pub extern "C" fn zone_get(synth: u32, zone: u32, field: u32) -> f32 {
    ZoneField::from_id(field).map_or(0.0, |f| {
        query(0.0, |e| e.zone_value(synth as usize, zone as usize, f))
    })
}

/// Work out `bins` (min, max) pairs for the waveform of `slot`; the number of
/// values written (twice the bins, 0 for an empty slot). Read them at `peaks_ptr`.
#[unsafe(no_mangle)]
pub extern "C" fn sample_peaks(slot: u32, bins: u32) -> u32 {
    query(0, |e| e.sample_peaks(slot as usize, bins as usize) as u32)
}

/// Address of the peaks `sample_peaks` computed.
#[unsafe(no_mangle)]
pub extern "C" fn peaks_ptr() -> *const f32 {
    ENGINE.with(|cell| match cell.try_borrow() {
        Ok(guard) => guard
            .as_ref()
            .map_or(std::ptr::null(), |e| e.peaks().as_ptr()),
        Err(_) => std::ptr::null(),
    })
}

/// Set field `field` (see `padsampler::PadField`) of pad `pad` of `synth`; unknown
/// synths, pads and fields are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn pad_set(synth: u32, pad: u32, field: u32, value: f32) {
    if let Some(f) = PadField::from_id(field) {
        with_engine(|e| e.set_pad(synth as usize, pad as usize, f, value));
    }
}

/// A field of a pad, as `pad_set` takes it (−1 for no sample); 0 for an unknown
/// synth, pad or field.
#[unsafe(no_mangle)]
pub extern "C" fn pad_get(synth: u32, pad: u32, field: u32) -> f32 {
    PadField::from_id(field).map_or(0.0, |f| {
        query(0.0, |e| e.pad_value(synth as usize, pad as usize, f))
    })
}

/// Put every pad of `synth` back to its defaults.
#[unsafe(no_mangle)]
pub extern "C" fn pads_clear(synth: u32) {
    with_engine(|e| e.clear_pads(synth as usize));
}

/// Pads a kit holds, and fields in a pad.
#[unsafe(no_mangle)]
pub extern "C" fn pad_count() -> u32 {
    PADS as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn pad_fields() -> u32 {
    PadField::ALL.len() as u32
}

/// Empty every zone of `synth`.
#[unsafe(no_mangle)]
pub extern "C" fn zones_clear(synth: u32) {
    with_engine(|e| e.clear_zones(synth as usize));
}

/// Fields in a zone: the length of `ZoneField`'s id range.
#[unsafe(no_mangle)]
pub extern "C" fn zone_fields() -> u32 {
    ZoneField::ALL.len() as u32
}

/// Zones each synth holds.
#[unsafe(no_mangle)]
pub extern "C" fn zone_count() -> u32 {
    ZONES as u32
}

// --- The song (spec 002, Req 6, ADR-0012) ----------------------------------
//
// Loading as a MIDI file: `song_buf(len)`, write the text's UTF-8 bytes,
// `song_load()`. The engine parses it and prints it back (`song_text_*`); the
// view draws the grid from the queries below and never parses the text.
// (`song_length` and `song_bar` above belong to the MIDI file.)

/// Size the song text buffer and return its address; null if too long.
#[unsafe(no_mangle)]
pub extern "C" fn song_buf(len: u32) -> *mut u8 {
    query(std::ptr::null_mut(), |e| {
        e.song_buffer(len as usize)
            .map_or(std::ptr::null_mut(), |b| b.as_mut_ptr())
    })
}

/// Parse the buffer: 0 when it plays, −1 when it does not parse (see
/// `song_error_*`; the old song plays on), −5 before `init`.
#[unsafe(no_mangle)]
pub extern "C" fn song_load() -> i32 {
    query(-5, |e| if e.load_song().is_ok() { 0 } else { -1 })
}

/// The last failed load's line and column (from 1); 0 when it parsed.
#[unsafe(no_mangle)]
pub extern "C" fn song_error_line() -> u32 {
    query(0, |e| e.song_error().map_or(0, |x| x.line as u32))
}

#[unsafe(no_mangle)]
pub extern "C" fn song_error_col() -> u32 {
    query(0, |e| e.song_error().map_or(0, |x| x.col as u32))
}

/// The last failed load's message, as bytes.
#[unsafe(no_mangle)]
pub extern "C" fn song_error_ptr() -> *const u8 {
    query(std::ptr::null(), |e| {
        e.song_error().map_or(std::ptr::null(), |x| x.msg.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn song_error_len() -> u32 {
    query(0, |e| e.song_error().map_or(0, |x| x.msg.len() as u32))
}

// The song text as coloured spans for the editor (#203). It needs no engine,
// so the view lexes on its own instance on the main thread, away from the
// audio: `lex_buf(len)`, write the UTF-8 text, `lex()` for the span count,
// then `lex_ptr()` holds (start, len, class) triples of u32, in UTF-16 units.

thread_local! {
    static LEX: RefCell<(Vec<u8>, Vec<u32>)> = const { RefCell::new((Vec::new(), Vec::new())) };
}

/// Size the text buffer to lex and return its address; null if too long.
#[unsafe(no_mangle)]
pub extern "C" fn lex_buf(len: u32) -> *mut u8 {
    LEX.with(|cell| match cell.try_borrow_mut() {
        Ok(mut lex) if len as usize <= crate::song::MAX_TEXT => {
            lex.0.clear();
            lex.0.resize(len as usize, 0);
            lex.0.as_mut_ptr()
        }
        _ => std::ptr::null_mut(),
    })
}

/// Lex the buffer; the number of spans, 0 when the text is not UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn lex() -> u32 {
    LEX.with(|cell| match cell.try_borrow_mut() {
        Ok(mut lex) => {
            let (text, out) = &mut *lex;
            out.clear();
            for s in std::str::from_utf8(text).map_or(Vec::new(), crate::song::lex::lex) {
                out.extend([s.start, s.len, s.class as u32]);
            }
            (out.len() / 3) as u32
        }
        Err(_) => 0,
    })
}

/// Lex the buffer as SuperCollider code (#329), for the Modular editor; the
/// spans come back as `lex`'s do, read with `lex_ptr`.
#[unsafe(no_mangle)]
pub extern "C" fn sc_lex() -> u32 {
    LEX.with(|cell| match cell.try_borrow_mut() {
        Ok(mut lex) => {
            let (text, out) = &mut *lex;
            out.clear();
            for s in std::str::from_utf8(text).map_or(Vec::new(), crate::modular::lex::lex) {
                out.extend([s.start, s.len, s.class as u32]);
            }
            (out.len() / 3) as u32
        }
        Err(_) => 0,
    })
}

/// The spans of the last `lex`, three u32 each.
#[unsafe(no_mangle)]
pub extern "C" fn lex_ptr() -> *const u32 {
    LEX.with(|cell| match cell.try_borrow() {
        Ok(lex) => lex.1.as_ptr(),
        Err(_) => std::ptr::null(),
    })
}

/// The song as the engine prints it, after the last load or edit.
#[unsafe(no_mangle)]
pub extern "C" fn song_text_ptr() -> *const u8 {
    query(std::ptr::null(), |e| e.song_text().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn song_text_len() -> u32 {
    query(0, |e| e.song_text().len() as u32)
}

/// Set step `step` of lane `lane` of clip `clip` to `level` (0 off, 1
/// hit, 2 accent): 0 when done, −1 when there is no such step or level.
#[unsafe(no_mangle)]
pub extern "C" fn set_step(clip: u32, lane: u32, step: u32, level: u32) -> i32 {
    query(-1, |e| {
        if e.set_step(clip as usize, lane as usize, step as usize, level) {
            0
        } else {
            -1
        }
    })
}

/// Ratchet step `step` of lane `lane` of clip `clip` (#242): it plays `r`
/// (1–4) times in its span; only a hit, accent or ghost repeats. 0 when
/// done, −1 otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn set_ratchet(clip: u32, lane: u32, step: u32, r: u32) -> i32 {
    query(-1, |e| {
        if e.set_ratchet(clip as usize, lane as usize, step as usize, r) {
            0
        } else {
            -1
        }
    })
}

fn with_lane<R: Copy>(default: R, f: u32, l: u32, get: impl FnOnce(&Lane) -> R) -> R {
    query(default, |e| {
        e.song()
            .clips
            .get(f as usize)
            .and_then(|fr| fr.lanes.get(l as usize))
            .map_or(default, get)
    })
}

/// The transport (ADR-0022): play from where it paused or stopped, pause
/// where it is, stop back to the top.
#[unsafe(no_mangle)]
pub extern "C" fn song_play() {
    with_engine(Engine::song_play);
}

#[unsafe(no_mangle)]
pub extern "C" fn song_pause() {
    with_engine(Engine::song_pause);
}

#[unsafe(no_mangle)]
pub extern "C" fn song_stop() {
    with_engine(Engine::song_stop);
}

/// Play clip `clip` alone, looping (#375); negative stops it and goes
/// back to the song.
#[unsafe(no_mangle)]
pub extern "C" fn song_cue(clip: i32) {
    with_engine(|e| e.song_cue(usize::try_from(clip).ok()));
}

/// The clip playing alone, −1 when none is cued.
#[unsafe(no_mangle)]
pub extern "C" fn song_cued() -> i32 {
    query(-1, |e| {
        e.song_cued().map_or(-1, |f| i32::try_from(f).unwrap_or(-1))
    })
}

/// Move the song to bar `bar` (from 0) of its arrangement.
#[unsafe(no_mangle)]
pub extern "C" fn song_seek_bar(bar: u32) {
    with_engine(|e| e.song_seek_bar(u64::from(bar)));
}

/// Bars in the song's arrangement; 0 without one.
#[unsafe(no_mangle)]
pub extern "C" fn song_bars() -> u32 {
    query(0, |e| u32::try_from(e.song().bars()).unwrap_or(u32::MAX))
}

/// The arrangement entry the last step fell in, −1 without one.
#[unsafe(no_mangle)]
pub extern "C" fn song_entry() -> i32 {
    query(-1, |e| {
        e.song_place()
            .map_or(-1, |(i, _)| i32::try_from(i).unwrap_or(-1))
    })
}

/// Steps into that entry, −1 without one.
#[unsafe(no_mangle)]
pub extern "C" fn song_local() -> i32 {
    query(-1, |e| {
        e.song_place()
            .map_or(-1, |(_, l)| i32::try_from(l).unwrap_or(-1))
    })
}

/// The strips the song's automation changed since the last call, a bit per
/// strip (globals on bit 0), so the view can ask for their values again.
#[unsafe(no_mangle)]
pub extern "C" fn auto_touched() -> u32 {
    query(0, Engine::take_touched)
}

/// How many (strip, parameter) pairs the song's modulations write now
/// (ADR-0019), for the view to mark their knobs.
#[unsafe(no_mangle)]
pub extern "C" fn mod_count() -> u32 {
    query(0, |e| {
        let n = (0..).take_while(|i| e.modulated(*i).is_some()).count();
        u32::try_from(n).unwrap_or(0)
    })
}

/// The strip of the `i`th modulated pair, or −1 past the last.
#[unsafe(no_mangle)]
pub extern "C" fn mod_strip(i: u32) -> i32 {
    query(-1, |e| {
        e.modulated(i as usize)
            .and_then(|(s, _)| i32::try_from(s).ok())
            .unwrap_or(-1)
    })
}

/// The parameter id of the `i`th modulated pair, or `u32::MAX` past the last.
#[unsafe(no_mangle)]
pub extern "C" fn mod_param(i: u32) -> u32 {
    query(u32::MAX, |e| {
        e.modulated(i as usize).map_or(u32::MAX, |(_, p)| p as u32)
    })
}

/// 1 when a song loaded while playing took over on a bar line since the last
/// call (#208), so the view can ask for the song again.
#[unsafe(no_mangle)]
pub extern "C" fn song_taken() -> u32 {
    query(0, |e| u32::from(e.take_taken()))
}

// --- The arrangement, for the arranger pane (#171) ---------------------------

/// An arranger edit (see `Engine::arrange_edit`): 0 when done, −1 when refused.
#[unsafe(no_mangle)]
pub extern "C" fn arr_edit(op: u32, a: u32, b: u32, c: u32) -> i32 {
    query(-1, |e| if e.arrange_edit(op, a, b, c) { 0 } else { -1 })
}

#[unsafe(no_mangle)]
pub extern "C" fn song_scenes() -> u32 {
    query(0, |e| e.song().scenes.len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn scene_name_ptr(s: u32) -> *const u8 {
    query(std::ptr::null(), |e| {
        e.song()
            .scenes
            .get(s as usize)
            .map_or(std::ptr::null(), |x| x.name.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn scene_name_len(s: u32) -> u32 {
    query(0, |e| {
        e.song()
            .scenes
            .get(s as usize)
            .map_or(0, |x| x.name.len() as u32)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn scene_bars(s: u32) -> u32 {
    query(0, |e| e.song().scenes.get(s as usize).map_or(0, |x| x.bars))
}

/// 1 when scene `s` holds clip (`kind` 0), lane (1) or snapshot (2) `item`.
#[unsafe(no_mangle)]
pub extern "C" fn scene_has(s: u32, kind: u32, item: u32) -> u32 {
    query(0, |e| {
        u32::from(e.song().scene_has(s as usize, kind, item as usize))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn arrange_len() -> u32 {
    query(0, |e| e.song().arrange.len() as u32)
}

/// The scene played at place `i` of the arrangement.
#[unsafe(no_mangle)]
pub extern "C" fn arrange_at(i: u32) -> u32 {
    query(0, |e| {
        e.song().arrange.get(i as usize).map_or(0, |s| *s as u32)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn song_autos() -> u32 {
    query(0, |e| e.song().autos.len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn auto_name_ptr(a: u32) -> *const u8 {
    query(std::ptr::null(), |e| {
        e.song()
            .autos
            .get(a as usize)
            .map_or(std::ptr::null(), |x| x.name.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn auto_name_len(a: u32) -> u32 {
    query(0, |e| {
        e.song()
            .autos
            .get(a as usize)
            .map_or(0, |x| x.name.len() as u32)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn song_snapshots() -> u32 {
    query(0, |e| e.song().snapshots.len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn snapshot_name_ptr(c: u32) -> *const u8 {
    query(std::ptr::null(), |e| {
        e.song()
            .snapshots
            .get(c as usize)
            .map_or(std::ptr::null(), |x| x.name.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn snapshot_name_len(c: u32) -> u32 {
    query(0, |e| {
        e.song()
            .snapshots
            .get(c as usize)
            .map_or(0, |x| x.name.len() as u32)
    })
}

/// The loop's first and last bar (from 1); 0 without a loop.
#[unsafe(no_mangle)]
pub extern "C" fn loop_from() -> u32 {
    query(0, |e| e.song().loop_bars.map_or(0, |l| l.0))
}

#[unsafe(no_mangle)]
pub extern "C" fn loop_to() -> u32 {
    query(0, |e| e.song().loop_bars.map_or(0, |l| l.1))
}

/// Import the loaded MIDI file as the song (#173): the number of tracks, or a
/// negative code (−1 to −4 the file's, −7 no notes, −8 too big, −9 a bug).
#[unsafe(no_mangle)]
pub extern "C" fn midi_import() -> i32 {
    query(-5, |e| match e.import_midi() {
        Ok(n) => i32::try_from(n).unwrap_or(i32::MAX),
        Err(code) => code,
    })
}

/// 1 while the song plays.
#[unsafe(no_mangle)]
pub extern "C" fn song_playing() -> u32 {
    query(0, |e| u32::from(e.clock().playing()))
}

/// Set the song's tempo in BPM; the text and the clock follow.
#[unsafe(no_mangle)]
pub extern "C" fn song_tempo(bpm: f32) {
    with_engine(|e| e.set_song_tempo(bpm));
}

/// Set the song's swing in percent (50 to 75); the text and the clock follow.
#[unsafe(no_mangle)]
pub extern "C" fn song_swing(pct: f32) {
    with_engine(|e| e.set_song_swing(pct));
}

/// The clock's tempo in BPM and swing in percent, as the song set them.
#[unsafe(no_mangle)]
pub extern "C" fn clock_tempo() -> f32 {
    query(120.0, |e| e.clock().tempo())
}

#[unsafe(no_mangle)]
pub extern "C" fn clock_swing() -> f32 {
    query(50.0, |e| e.clock().swing())
}

/// Tracks in the song.
#[unsafe(no_mangle)]
pub extern "C" fn song_tracks() -> u32 {
    query(0, |e| e.song().tracks.len() as u32)
}

// The song's `samples` lines (#214): the view loads what they name, and a
// pack or kit picked on a synth's panel is written as its id into `song_buf`
// and set with `samples_set`.

/// Lines of `samples` in the song.
#[unsafe(no_mangle)]
pub extern "C" fn samples_count() -> u32 {
    query(0, |e| e.song().samples.len() as u32)
}

/// The synth the track of `samples` line `i` plays on, or −1.
#[unsafe(no_mangle)]
pub extern "C" fn samples_synth(i: u32) -> i32 {
    query(-1, |e| {
        e.song()
            .samples
            .get(i as usize)
            .and_then(|(t, _)| e.song_routed(*t))
            .map_or(-1, |s| s as i32)
    })
}

/// The id of `samples` line `i`: its length in bytes and where it is.
#[unsafe(no_mangle)]
pub extern "C" fn samples_id_len(i: u32) -> u32 {
    query(0, |e| {
        e.song()
            .samples
            .get(i as usize)
            .map_or(0, |(_, id)| id.len() as u32)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn samples_id_ptr(i: u32) -> *const u8 {
    query(std::ptr::null(), |e| {
        e.song()
            .samples
            .get(i as usize)
            .map_or(std::ptr::null(), |(_, id)| id.as_ptr())
    })
}

/// The track on synth `s` wants the pack or kit whose id is in `song_buf`:
/// 0 when its `samples` line says so, −1 when no sampler or drums track plays
/// on `s` or the id isn't one.
#[unsafe(no_mangle)]
pub extern "C" fn samples_set(s: u32) -> i32 {
    query(-1, |e| {
        if e.set_samples_from_buffer(s as usize) {
            0
        } else {
            -1
        }
    })
}

// Group buses' names in the song (#214): read with the song, set from
// `song_buf` (empty takes the name away).

/// The length of group `g`'s name in the song (0–7), 0 for none.
#[unsafe(no_mangle)]
pub extern "C" fn group_name_len(g: u32) -> u32 {
    query(0, |e| {
        e.group_name(g as usize).map_or(0, |n| n.len() as u32)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn group_name_ptr(g: u32) -> *const u8 {
    query(std::ptr::null(), |e| {
        e.group_name(g as usize)
            .map_or(std::ptr::null(), str::as_ptr)
    })
}

/// Name group `g` with the name in `song_buf`, or none when it is empty: 0
/// when the song says so, −1 for a name that isn't one.
#[unsafe(no_mangle)]
pub extern "C" fn group_name_set(g: u32) -> i32 {
    query(-1, |e| {
        if e.set_group_name_from_buffer(g as usize) {
            0
        } else {
            -1
        }
    })
}

// A Modular synth's SuperCollider code (ADR-0024): written into `song_buf`,
// set with `code_set`; its text with the knobs' values from `code_text`.

/// Give synth `s` the SynthDef in `song_buf`: 0 when it builds, −1 when it
/// does not (see `code_error_*`), −5 before `init`.
#[unsafe(no_mangle)]
pub extern "C" fn code_set(s: u32) -> i32 {
    query(-5, |e| {
        if e.set_code_from_buffer(s as usize).is_ok() {
            e.mark_synth(s as usize);
            0
        } else {
            -1
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn code_error_line() -> u32 {
    query(0, |e| e.code_error().map_or(0, |x| x.line as u32))
}

#[unsafe(no_mangle)]
pub extern "C" fn code_error_col() -> u32 {
    query(0, |e| e.code_error().map_or(0, |x| x.col as u32))
}

#[unsafe(no_mangle)]
pub extern "C" fn code_error_ptr() -> *const u8 {
    query(std::ptr::null(), |e| {
        e.code_error().map_or(std::ptr::null(), |x| x.msg.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn code_error_len() -> u32 {
    query(0, |e| e.code_error().map_or(0, |x| x.msg.len() as u32))
}

/// Synth `s`'s code (0 bytes for none); its length, then `code_text_ptr`.
#[unsafe(no_mangle)]
pub extern "C" fn code_text(s: u32) -> u32 {
    query(0, |e| e.code_text(s as usize).len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn code_text_ptr() -> *const u8 {
    query(std::ptr::null(), |e| e.code_text_buf().as_ptr())
}

/// Modular synth `s`'s knob list (#329, `Engine::knob_list`); its length,
/// read from `knob_list_ptr`.
#[unsafe(no_mangle)]
pub extern "C" fn knob_list(s: u32) -> u32 {
    query(0, |e| e.knob_list(s as usize).len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn knob_list_ptr() -> *const u8 {
    query(std::ptr::null(), |e| e.knob_list_buf().as_ptr())
}

/// Address and length of track `t`'s name.
#[unsafe(no_mangle)]
pub extern "C" fn track_name_ptr(t: u32) -> *const u8 {
    query(std::ptr::null(), |e| {
        e.song()
            .tracks
            .get(t as usize)
            .map_or(std::ptr::null(), |x| x.name.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn track_name_len(t: u32) -> u32 {
    query(0, |e| {
        e.song()
            .tracks
            .get(t as usize)
            .map_or(0, |x| x.name.len() as u32)
    })
}

// Each track's patch and the song's settings (#213), for the composer's picker.

/// Track `t`'s factory preset (its setting's, when it plays one), −1 without.
#[unsafe(no_mangle)]
pub extern "C" fn track_preset(t: u32) -> i32 {
    query(-1, |e| {
        e.song()
            .tracks
            .get(t as usize)
            .and_then(|x| x.preset)
            .map_or(-1, |p| p as i32)
    })
}

/// The song setting track `t` plays, −1 without.
#[unsafe(no_mangle)]
pub extern "C" fn track_setting(t: u32) -> i32 {
    query(-1, |e| {
        e.song()
            .tracks
            .get(t as usize)
            .and_then(|x| x.setting)
            .map_or(-1, |i| i32::try_from(i).unwrap_or(-1))
    })
}

/// 1 when `model` plays a track of `kind` (0 drums, 1 synth, 2 sampler), the
/// rule a track line is checked by; the picker offers only those (#213).
#[unsafe(no_mangle)]
pub extern "C" fn model_fits(kind: u32, model: u32) -> u32 {
    let kind = match kind {
        0 => Kind::Drums,
        1 => Kind::Synth,
        2 => Kind::Sampler,
        _ => return 0,
    };
    u32::from(
        crate::mono::model::Model::from_id(model).is_some_and(|m| crate::song::fits(kind, Some(m))),
    )
}

/// Print the mixer as it is into the song as mixer lines (ADR-0018).
#[unsafe(no_mangle)]
pub extern "C" fn song_write_mixer() {
    with_engine(Engine::write_mixer);
}

/// A track edit (see `Engine::track_edit`): 0 when done, −1 when refused.
#[unsafe(no_mangle)]
pub extern "C" fn track_edit(op: u32, t: u32, a: u32) -> i32 {
    query(-1, |e| if e.track_edit(op, t, a) { 0 } else { -1 })
}

#[unsafe(no_mangle)]
pub extern "C" fn song_settings() -> u32 {
    query(0, |e| e.song().settings.len() as u32)
}

/// Address and length of setting `i`'s name.
#[unsafe(no_mangle)]
pub extern "C" fn setting_name_ptr(i: u32) -> *const u8 {
    query(std::ptr::null(), |e| {
        e.song()
            .settings
            .get(i as usize)
            .map_or(std::ptr::null(), |x| x.name.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn setting_name_len(i: u32) -> u32 {
    query(0, |e| {
        e.song()
            .settings
            .get(i as usize)
            .map_or(0, |x| x.name.len() as u32)
    })
}

/// The factory preset setting `i` starts from, −1 without.
#[unsafe(no_mangle)]
pub extern "C" fn setting_preset(i: u32) -> i32 {
    query(-1, |e| {
        e.song()
            .settings
            .get(i as usize)
            .map_or(-1, |x| x.preset as i32)
    })
}

/// What track `t`'s clips hold: 0 drum lanes, 1 notes, 2 lanes or notes (a sampler).
#[unsafe(no_mangle)]
pub extern "C" fn track_kind(t: u32) -> u32 {
    query(0, |e| {
        e.song().tracks.get(t as usize).map_or(0, |x| match x.kind {
            Kind::Drums => 0,
            Kind::Synth => 1,
            Kind::Sampler => 2,
        })
    })
}

/// Track `t`'s mute and solo (#355): bit 0 muted, bit 1 soloed.
#[unsafe(no_mangle)]
pub extern "C" fn track_flags(t: u32) -> u32 {
    query(0, |e| {
        e.song()
            .tracks
            .get(t as usize)
            .map_or(0, |x| u32::from(x.mute) | u32::from(x.solo) << 1)
    })
}

/// Mute and solo track `t` (#355), bits as `track_flags`: 0 when done, −1
/// for no such track.
#[unsafe(no_mangle)]
pub extern "C" fn set_track_flags(t: u32, flags: u32) -> i32 {
    query(-1, |e| {
        if e.set_track_flags(t as usize, flags & 1 != 0, flags & 2 != 0) {
            0
        } else {
            -1
        }
    })
}

/// Play song track `t` on `synth`; an unknown synth (e.g. 255) mutes it.
#[unsafe(no_mangle)]
pub extern "C" fn song_route(t: u32, synth: u32) {
    with_engine(|e| e.song_route(t as usize, Some(synth as usize)));
}

/// The synth track `t` plays on, 255 when muted.
#[unsafe(no_mangle)]
pub extern "C" fn song_routed(t: u32) -> u32 {
    query(255, |e| e.song_routed(t as usize).map_or(255, |s| s as u32))
}

/// Clips in the song.
#[unsafe(no_mangle)]
pub extern "C" fn song_clips() -> u32 {
    query(0, |e| e.song().clips.len() as u32)
}

/// Address and length of clip `f`'s name.
#[unsafe(no_mangle)]
pub extern "C" fn clip_name_ptr(f: u32) -> *const u8 {
    query(std::ptr::null(), |e| {
        e.song()
            .clips
            .get(f as usize)
            .map_or(std::ptr::null(), |x| x.name.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn clip_name_len(f: u32) -> u32 {
    query(0, |e| {
        e.song()
            .clips
            .get(f as usize)
            .map_or(0, |x| x.name.len() as u32)
    })
}

/// The track clip `f` plays on.
#[unsafe(no_mangle)]
pub extern "C" fn clip_track(f: u32) -> u32 {
    query(0, |e| {
        e.song().clips.get(f as usize).map_or(0, |x| x.track as u32)
    })
}

/// Address and length of clip `f`'s line of notes as printed; empty for a
/// drum clip. `clip_bars` is how many bars before it repeats.
#[unsafe(no_mangle)]
pub extern "C" fn clip_notes_ptr(f: u32) -> *const u8 {
    query(std::ptr::null(), |e| {
        e.song()
            .clips
            .get(f as usize)
            .and_then(|x| x.notes.as_ref())
            .map_or(std::ptr::null(), |n| n.text.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn clip_notes_len(f: u32) -> u32 {
    query(0, |e| {
        e.song()
            .clips
            .get(f as usize)
            .and_then(|x| x.notes.as_ref())
            .map_or(0, |n| n.text.len() as u32)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn clip_bars(f: u32) -> u32 {
    query(0, |e| {
        e.song()
            .clips
            .get(f as usize)
            .and_then(|x| x.notes.as_ref())
            .map_or(0, |n| n.bars)
    })
}

/// Notes clip `f` plays, and the start (in ticks, 48 to a bar), length,
/// MIDI note and accent (0 or 1) of note `k`.
#[unsafe(no_mangle)]
pub extern "C" fn clip_events(f: u32) -> u32 {
    query(0, |e| e.clip_events(f as usize).len() as u32)
}

fn with_event(f: u32, k: u32, get: impl FnOnce(&crate::notes::Event) -> u32) -> u32 {
    query(0, |e| {
        e.clip_events(f as usize).get(k as usize).map_or(0, get)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn event_start(f: u32, k: u32) -> u32 {
    with_event(f, k, |ev| ev.start)
}

#[unsafe(no_mangle)]
pub extern "C" fn event_len(f: u32, k: u32) -> u32 {
    with_event(f, k, |ev| ev.len)
}

#[unsafe(no_mangle)]
pub extern "C" fn event_note(f: u32, k: u32) -> u32 {
    with_event(f, k, |ev| u32::from(ev.note))
}

#[unsafe(no_mangle)]
pub extern "C" fn event_accent(f: u32, k: u32) -> u32 {
    with_event(f, k, |ev| u32::from(ev.accent))
}

/// 1 when clip `f` is a generator call (euclid, arp, walk, markov, mutate):
/// its notes cannot be edited until it is frozen.
#[unsafe(no_mangle)]
pub extern "C" fn clip_generated(f: u32) -> u32 {
    query(0, |e| {
        e.song()
            .clips
            .get(f as usize)
            .and_then(|x| x.notes.as_ref())
            .map_or(0, |n| {
                u32::from(matches!(
                    n.seq,
                    crate::notes::Seq::Generated(_) | crate::notes::Seq::Euclid(..)
                ))
            })
    })
}

fn edit(f: u32, op: crate::notes::Edit) -> i32 {
    query(-1, |e| if e.edit_note(f as usize, op) { 0 } else { -1 })
}

/// Add a sixteenth note `note` at `tick` of clip `f`, make its length
/// `len` ticks, or remove it: 0 when done, −1 when the song does not take it.
#[unsafe(no_mangle)]
pub extern "C" fn note_add(f: u32, tick: u32, note: u32) -> i32 {
    match u8::try_from(note) {
        Ok(note) => edit(f, crate::notes::Edit::Add { tick, note }),
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn note_remove(f: u32, tick: u32, note: u32) -> i32 {
    match u8::try_from(note) {
        Ok(note) => edit(f, crate::notes::Edit::Remove { tick, note }),
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn note_len(f: u32, tick: u32, note: u32, len: u32) -> i32 {
    match u8::try_from(note) {
        Ok(note) => edit(f, crate::notes::Edit::Len { tick, note, len }),
        Err(_) => -1,
    }
}

/// Whether clip `f` makes new events every cycle.
#[unsafe(no_mangle)]
pub extern "C" fn clip_live(f: u32) -> u32 {
    query(0, |e| {
        e.song()
            .clips
            .get(f as usize)
            .map_or(0, |x| u32::from(x.live))
    })
}

/// Replace clip `f`'s generator call with the events it is playing, as
/// notes: 0 when done, −1 when it is not a generated clip or the events
/// do not fit the notation. The song is printed again (`song_text_*`).
#[unsafe(no_mangle)]
pub extern "C" fn freeze(f: u32) -> i32 {
    query(-1, |e| if e.freeze(f as usize) { 0 } else { -1 })
}

/// Steps to a bar of drum clip `f` (#353); 16 for an unknown one.
#[unsafe(no_mangle)]
pub extern "C" fn clip_grid(f: u32) -> u32 {
    query(16, |e| {
        e.song().clips.get(f as usize).map_or(16, |x| x.grid)
    })
}

/// Lanes of clip `f`.
#[unsafe(no_mangle)]
pub extern "C" fn clip_lanes(f: u32) -> u32 {
    query(0, |e| {
        e.song()
            .clips
            .get(f as usize)
            .map_or(0, |x| x.lanes.len() as u32)
    })
}

/// The pad (`Pad` id) lane `l` of clip `f` plays.
#[unsafe(no_mangle)]
pub extern "C" fn lane_pad(f: u32, l: u32) -> u32 {
    with_lane(0, f, l, |lane| lane.pad as u32)
}

/// Steps in lane `l` of clip `f`.
#[unsafe(no_mangle)]
pub extern "C" fn lane_steps(f: u32, l: u32) -> u32 {
    with_lane(0, f, l, |lane| lane.steps.len() as u32)
}

/// The words of the song's text playing now (#205); read their (start,
/// length) pairs, in UTF-16 units, from `lit_ptr`. 0 while the song is
/// stopped or a new one waits for its bar.
#[unsafe(no_mangle)]
pub extern "C" fn lit_count() -> u32 {
    query(0, |e| e.lit_count() as u32)
}

/// The spans of the last `lit_count`, two u32 each.
#[unsafe(no_mangle)]
pub extern "C" fn lit_ptr() -> *const u32 {
    query(std::ptr::null(), |e| e.lit_spans().as_ptr())
}

/// How often step `s` of lane `l` of clip `f` plays in its span (#242).
#[unsafe(no_mangle)]
pub extern "C" fn step_ratchet(f: u32, l: u32, s: u32) -> u32 {
    with_lane(1, f, l, |lane| u32::from(lane.ratchet(s as usize)))
}

/// Step `s` of lane `l` of clip `f`: 0 off, 1 hit, 2 accent.
#[unsafe(no_mangle)]
pub extern "C" fn step_level(f: u32, l: u32, s: u32) -> u32 {
    with_lane(0, f, l, |lane| {
        lane.steps.get(s as usize).map_or(0, |st| *st as u32)
    })
}

// --- Clock (spec 002, Req 5) -------------------------------------------------

/// The clock's tempo in BPM, 20..=300.
#[unsafe(no_mangle)]
pub extern "C" fn tempo(bpm: f32) {
    with_engine(|e| e.set_tempo(bpm));
}

/// The clock's swing in percent: 50 is straight, 75 the most.
#[unsafe(no_mangle)]
pub extern "C" fn swing(pct: f32) {
    with_engine(|e| e.set_swing(pct));
}

/// The last sixteenth step the clock fired, −1 before the first.
#[unsafe(no_mangle)]
pub extern "C" fn clock_step() -> i32 {
    query(-1, |e| {
        e.clock()
            .step()
            .map_or(-1, |k| i32::try_from(k).unwrap_or(i32::MAX))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drums::Pad;

    #[test]
    fn samples_load_through_the_abi() {
        init(48_000.0);
        assert!(sample_buf(sample::MAX_WAV as u32 + 1).is_null());
        // Three 16-bit mono frames.
        let mut file = b"RIFF".to_vec();
        let body: Vec<u8> = [
            &b"WAVEfmt "[..],
            &16u32.to_le_bytes(),
            &1u16.to_le_bytes(),
            &1u16.to_le_bytes(),
            &48_000u32.to_le_bytes(),
            &96_000u32.to_le_bytes(),
            &2u16.to_le_bytes(),
            &16u16.to_le_bytes(),
            &b"data"[..],
            &6u32.to_le_bytes(),
            &[0, 0, 0, 64, 0, 192],
        ]
        .concat();
        file.extend((body.len() as u32).to_le_bytes());
        file.extend(body);
        assert!(!sample_buf(file.len() as u32).is_null());
        query((), |e| {
            e.sample_buffer(file.len())
                .expect("fits")
                .copy_from_slice(&file)
        });
        assert_eq!(sample_load(2), 3);
        assert_eq!(sample_frames(2), 3);
        assert_eq!(sample_root(2), 60);
        assert_eq!(sample_used(), 3);
        assert_eq!(sample_load(sample_slots()), -7);
        assert_eq!(sample_peaks(2, 2), 4);
        assert!(!peaks_ptr().is_null());
        assert_eq!(sample_peaks(9, 2), 0, "an empty slot has no peaks");
        zone_set(0, 5, 0, 2.0);
        zone_set(0, 5, 1, 40.0);
        assert_eq!(zone_get(0, 5, 0), 2.0);
        assert_eq!(zone_get(0, 5, 1), 40.0);
        assert_eq!(zone_get(0, 6, 0), -1.0, "an untouched zone is empty");
        assert_eq!(zone_get(99, 0, 0), 0.0);
        assert_eq!(zone_get(0, 0, 99), 0.0);
        zones_clear(0);
        assert_eq!(zone_get(0, 5, 0), -1.0);
        assert_eq!((pad_count(), pad_fields()), (16, 10));
        pad_set(1, 3, 0, 2.0);
        pad_set(1, 3, 3, -0.5);
        assert_eq!(pad_get(1, 3, 0), 2.0);
        assert_eq!(pad_get(1, 3, 3), -0.5);
        assert_eq!(pad_get(1, 4, 0), -1.0, "an untouched pad has no sample");
        assert_eq!(pad_get(99, 0, 0), 0.0);
        assert_eq!(pad_get(1, 3, 99), 0.0);
        pads_clear(1);
        assert_eq!(pad_get(1, 3, 0), -1.0);
        sample_clear(2);
        assert_eq!(sample_frames(2), 0);
        assert_eq!(sample_root(2), 255);
        assert_eq!(sample_used(), 0);
    }

    #[test]
    fn exports_are_no_ops_before_init() {
        process(128);
        note_on(0, 60, 1.0);
        set_param(0, 0, 1.0);
        mono_preset(0, 1);
        assert_eq!(param_value(0, Param::Cutoff as u32), 0.0);
        assert_eq!(active_voices(), 0);
        assert!(out_ptr().is_null());
        assert!(meters_ptr().is_null());
        meters_clear();
    }

    #[test]
    fn parameters_and_presets_through_the_abi() {
        init(48_000.0);
        assert_eq!(param_count() as usize, Param::ALL.len());
        let cutoff = Param::Cutoff as u32;
        set_param(0, cutoff, 1.0e9);
        assert_eq!(param_value(0, cutoff), 20_000.0, "clamped");
        set_param(0, cutoff, f32::NAN);
        assert_eq!(param_value(0, cutoff), 20.0, "NaN is the lower bound");
        assert_eq!(param_value(0, u32::MAX), 0.0, "unknown id");
        let all = |synth| {
            Param::ALL
                .iter()
                .map(|(p, _)| param_value(synth, *p as u32))
                .collect::<Vec<_>>()
        };
        let before = all(0);
        mono_preset(0, u32::MAX);
        assert_eq!(all(0), before, "an unknown preset is ignored");
        mono_preset(0, Preset::Lead as u32);
        assert_ne!(all(0), before);
        assert_eq!(active_voices(), 0);
        note_on(0, 60, 1.0);
        note_on(1, 64, 1.0);
        process(128);
        assert_eq!(active_voices(), 2);
    }

    #[test]
    fn a_build_id_is_the_first_eight_hex_digits_or_zero() {
        assert_eq!(parse("abc12345", 16), 0xabc1_2345);
        assert_eq!(parse("ABC12345ffff", 16), 0xabc1_2345);
        assert_eq!(parse("7c6ffe4", 16), 0x07c6_ffe4);
        assert_eq!(parse("dirty", 16), 0);
        assert_eq!(parse("", 16), 0);
        assert_eq!(parse("30", 10), 30);
        assert_eq!(parse("3a", 10), 0);
    }

    #[test]
    fn exports_drive_the_engine() {
        init(48_000.0);
        assert!(!out_ptr().is_null());
        assert!(!meters_ptr().is_null());
        assert_eq!(meters_len() as usize, METERS);
        note_on(0, 69, 1.0);
        note_on(3, 69, 1.0);
        note_on(99, 69, 1.0); // unknown synth: ignored
        process(128);
        assert_eq!(query(0, |e| e.active_voices()), 2);
        assert_eq!(synth_count(), 16);
        assert_eq!(model_count() as usize, crate::mono::model::Model::ALL.len());
        assert_eq!(strip_count(), 24);
        let [major, minor, patch] = env!("CARGO_PKG_VERSION")
            .split('.')
            .map(|n| n.parse::<u32>().unwrap())
            .collect::<Vec<_>>()[..]
        else {
            panic!("a major.minor.patch version")
        };
        assert_eq!(version_code(), major * 10_000 + minor * 100 + patch);
        set_param(3, Param::Cutoff as u32, 300.0);
        assert_eq!(param_value(3, Param::Cutoff as u32), 300.0);
        assert_ne!(param_value(0, Param::Cutoff as u32), 300.0);
        synth_reset(3);
        assert_eq!(
            param_value(3, Param::Cutoff as u32),
            param_value(0, Param::Cutoff as u32)
        );
    }

    #[test]
    fn midi_import_and_the_transport_through_the_abi() {
        init(48_000.0);
        assert_eq!(midi_import(), -1); // empty buffer: not MIDI
        let bytes = include_bytes!("../../../web/public/demo.mid");
        let ptr = midi_buf(bytes.len() as u32);
        assert!(!ptr.is_null());
        query((), |e| {
            e.midi_buffer(bytes.len())
                .expect("fits")
                .copy_from_slice(bytes)
        });
        assert_eq!(midi_import(), 4);
        assert_eq!(song_routed(3), 3, "the fourth track plays on synth 3");
        song_play();
        process(128);
        assert_eq!((song_playing(), clock_step()), (1, 0));
        song_pause();
        process(128);
        assert_eq!(
            (song_playing(), clock_step()),
            (0, 0),
            "pause holds the place"
        );
        song_play();
        song_stop();
        assert_eq!(song_playing(), 0);
        assert!(midi_buf(u32::MAX).is_null());
    }

    /// #213: a track's patch and the song's settings through the ABI.
    #[test]
    fn track_patches_through_the_abi() {
        use crate::mono::model::Model;
        init(48_000.0);
        let text = b"track lead synth\n";
        query((), |e| {
            e.song_buffer(text.len())
                .expect("fits")
                .copy_from_slice(text)
        });
        assert_eq!(song_load(), 0);
        assert!(track_preset(0) >= 0, "a picked preset");
        assert_eq!((track_setting(0), song_settings()), (-1, 0));
        assert_eq!(track_edit(0, 0, Preset::MiniBass as u32), 0);
        assert_eq!(track_preset(0), Preset::MiniBass as i32);
        assert_eq!(
            track_edit(0, 0, Preset::Kit808 as u32),
            -1,
            "not on a synth track"
        );
        assert_eq!(track_edit(2, 0, 0), 0, "saved as a setting");
        assert_eq!((song_settings(), track_setting(0)), (1, 0));
        assert_eq!(setting_name_len(0), 4, "lead");
        assert!(!setting_name_ptr(0).is_null());
        assert_eq!(setting_preset(0), Preset::MiniBass as i32);
        assert_eq!(
            (setting_preset(5), track_preset(9), track_setting(9)),
            (-1, -1, -1)
        );
        assert_eq!(model_fits(1, Model::Minimoog as u32), 1);
        assert_eq!(model_fits(0, Model::Minimoog as u32), 0);
        assert_eq!(model_fits(0, Model::Tr909 as u32), 1);
        assert_eq!(model_fits(2, Model::PadSampler as u32), 1);
        assert_eq!((model_fits(3, 0), model_fits(1, 999)), (0, 0));
    }

    #[test]
    fn song_round_trip_through_the_abi() {
        init(48_000.0);
        let text = b"track kit drums\nclip b = kit\n  bd x.X.\n";
        assert!(!song_buf(text.len() as u32).is_null());
        query((), |e| {
            e.song_buffer(text.len())
                .expect("fits")
                .copy_from_slice(text)
        });
        assert_eq!(song_load(), 0);
        assert_eq!((song_error_line(), song_error_len()), (0, 0));
        assert_eq!(
            (song_tracks(), song_clips(), clip_lanes(0), clip_track(0)),
            (1, 1, 1, 0)
        );
        assert_eq!((track_name_len(0), clip_name_len(0)), (3, 1));
        assert_eq!(lane_pad(0, 0), Pad::Bd as u32);
        assert_eq!(lane_steps(0, 0), 4);
        assert_eq!(
            (
                step_level(0, 0, 0),
                step_level(0, 0, 2),
                step_level(0, 0, 9)
            ),
            (1, 2, 0)
        );
        assert_eq!((track_kind(0), clip_notes_len(0), clip_bars(0)), (0, 0, 0));
        assert_eq!((clip_events(0), clip_generated(0), clip_live(0)), (0, 0, 0));
        assert_eq!(note_add(0, 0, 60), -1, "a drum clip takes no notes");
        assert_eq!(song_routed(0), 0, "the first synth becomes the kit");
        song_route(0, 2);
        assert_eq!(song_routed(0), 2);
        assert_eq!(set_step(0, 0, 1, 1), 0);
        assert_eq!(set_step(0, 0, 4, 1), -1);
        assert_eq!(step_level(0, 0, 1), 1);
        assert_eq!((set_ratchet(0, 0, 1, 3), step_ratchet(0, 0, 1)), (0, 3));
        assert_eq!(
            (set_ratchet(0, 0, 3, 2), step_ratchet(0, 0, 3)),
            (-1, 1),
            "a rest"
        );
        assert!(song_text_len() > 0 && !song_text_ptr().is_null());
        query((), |e| {
            e.song_buffer(4).expect("fits").copy_from_slice(b"play")
        });
        assert_eq!(song_load(), -1);
        assert_eq!((song_error_line(), song_error_col()), (1, 1));
        assert!(song_error_len() > 0 && !song_error_ptr().is_null());
        assert_eq!(song_clips(), 1, "the old song stays");
        assert_eq!(
            (song_bars(), song_entry(), song_local()),
            (0, -1, -1),
            "no arrangement"
        );
        // The arranger's calls (#171): a scene, an entry, a toggle, a loop.
        assert_eq!(arr_edit(1, 2, 0, 0), 0, "a new scene of two bars");
        assert_eq!(
            (song_scenes(), arrange_len(), arrange_at(0), scene_bars(0)),
            (1, 1, 0, 2)
        );
        assert_eq!(scene_name_len(0), 5, "part1");
        assert_eq!(scene_has(0, 0, 0), 1, "the first scene holds the clip");
        assert_eq!(arr_edit(0, 0, 0, 0), 0, "the clip out of it");
        assert_eq!(scene_has(0, 0, 0), 0);
        assert_eq!(arr_edit(0, 0, 0, 0), 0, "and back");
        assert_eq!(scene_has(0, 0, 0), 1);
        assert_eq!(arr_edit(6, 1, 2, 0), 0);
        assert_eq!((loop_from(), loop_to(), song_bars()), (1, 2, 2));
        assert_eq!(arr_edit(6, 1, 3, 0), -1, "past the end");
        assert_eq!(arr_edit(9, 0, 0, 0), -1, "no such edit");
        assert_eq!((song_autos(), song_snapshots()), (0, 0));
        assert!(song_buf(u32::MAX).is_null());
        assert_eq!(song_playing(), 0);
        song_play();
        assert_eq!(song_playing(), 1);
        song_stop();
        assert_eq!(song_playing(), 0);
        song_tempo(97.0);
        song_swing(99.0);
        assert_eq!((clock_tempo(), clock_swing()), (97.0, 75.0));
    }
    #[test]
    fn notes_are_read_and_edited_through_the_abi() {
        init(48_000.0);
        let text = b"scale c minor\ntrack t synth\nclip a = t\n  c4:4 e4:4\nclip g = t\n  euclid(3,8) c4\n";
        assert!(!song_buf(text.len() as u32).is_null());
        query((), |e| {
            e.song_buffer(text.len())
                .expect("fits")
                .copy_from_slice(text)
        });
        assert_eq!(song_load(), 0);
        assert_eq!(
            (clip_events(0), clip_generated(0), clip_generated(1)),
            (2, 0, 1)
        );
        assert_eq!(
            (
                event_start(0, 1),
                event_len(0, 1),
                event_note(0, 1),
                event_accent(0, 1)
            ),
            (12, 12, 64, 0)
        );
        assert_eq!(note_add(0, 24, 67), 0);
        assert_eq!(clip_events(0), 3);
        assert_eq!(note_len(0, 24, 67, 9), 0);
        assert_eq!(event_len(0, 2), 9);
        assert_eq!(note_remove(0, 0, 60), 0);
        assert_eq!(note_remove(0, 0, 60), -1);
        assert_eq!(note_add(1, 0, 60), -1, "frozen first");
        assert_eq!(freeze(1), 0);
        assert_eq!((clip_generated(1), note_add(1, 3, 60)), (0, 0));
        let text = query(String::new(), |e| e.song_text().to_string());
        assert!(
            text.contains("clip a = t\n  \"~@12 e4@12 g4@9 ~@15\"\n"),
            "{text}"
        );
    }
}
