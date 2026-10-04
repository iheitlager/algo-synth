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

use crate::engine::{BLOCK, Engine, METERS, SYNTHS};
use crate::mixer::STRIPS;
use crate::mono::preset::Preset;
use crate::padsampler::{PADS, PadField};
use crate::params::Param;
use crate::player::Part;
use crate::sample;
use crate::sampler::{ZONES, ZoneField};

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
        with_engine(|e| e.set_param(synth as usize, p, value));
    }
}

/// Strips the mixer holds: the synths, then the group buses.
#[unsafe(no_mangle)]
pub extern "C" fn strip_count() -> u32 {
    STRIPS as u32
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
        with_engine(|e| e.preset(synth as usize, p));
    }
}

/// Start `note` (MIDI 0..=127) on `synth`'s live voice.
#[unsafe(no_mangle)]
pub extern "C" fn note_on(synth: u32, note: u32, velocity: f32) {
    let note = u8::try_from(note.min(127)).unwrap_or(127);
    with_engine(|e| e.note_on(synth as usize, note, velocity));
}

/// Release `note` on `synth`'s live voice.
#[unsafe(no_mangle)]
pub extern "C" fn note_off(synth: u32, note: u32) {
    if let Ok(n) = u8::try_from(note) {
        with_engine(|e| e.note_off(synth as usize, n));
    }
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
    query(false, |e| e.apply_sysex(synth as usize, i as usize))
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

/// Play MIDI `channel` on `synth`; an unknown synth (e.g. 255) mutes it.
#[unsafe(no_mangle)]
pub extern "C" fn route(channel: u32, synth: u32) {
    if let Ok(ch) = u8::try_from(channel) {
        with_engine(|e| e.route(ch, Some(synth as usize)));
    }
}

/// The synth MIDI `channel` plays on, or 255 if muted.
#[unsafe(no_mangle)]
pub extern "C" fn routed(channel: u32) -> u32 {
    let ch = u8::try_from(channel).unwrap_or(u8::MAX);
    query(255, |e| e.routed(ch).map_or(255, |s| s as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!((pad_count(), pad_fields()), (16, 9));
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
        assert!(out_ptr().is_null());
        assert!(meters_ptr().is_null());
        meters_clear();
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
        assert_eq!(strip_count(), 24);
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
        assert_eq!(midi_load(), 4);
        assert_eq!(part_channel(3), 3);
        assert_eq!(routed(3), 3, "the fourth part plays on synth 3");
        route(3, 255);
        assert_eq!(routed(3), 255);
        route(3, 99);
        assert_eq!(routed(3), 255, "an unknown synth mutes");
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
        seek(0.0);
        tempo(120.0);
        swing(50.0);
        assert_eq!(clock_step(), -1);
        play();
        process(128);
        assert_eq!(clock_step(), 0);
        stop();
        assert!(midi_buf(u32::MAX).is_null());
    }
}
