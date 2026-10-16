//! Bounce a song to a stereo WAV offline (#705): its arrangement from the
//! top, then the tail until it rings out.
//!
//! `cargo run --release -p algo-dsp --example bounce -- <in.song> <out.wav> [--rate 96000] [--bits 16|24|32] [--bars N]`
//!
//! The defaults are 48 kHz, 24-bit and the song's own length: its
//! arrangement, up to the end of its `loop` (marked in the file), or five
//! minutes without an arrangement; `--bars` sets it. Tracks that play sample
//! files are silent here: only the app loads those.

use std::time::Instant;
use std::{env, fs, process};

use algo_dsp::bounce::{Bounce, Format, MAX_STEP};
use algo_dsp::engine::Engine;

const USAGE: &str = "usage: bounce <in.song> <out.wav> [--rate 96000] [--bits 16|24|32] [--bars N]";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(|v| {
                v.parse::<u32>()
                    .unwrap_or_else(|_| fail(&format!("{name} {v}: not a number")))
            })
    };
    let mut paths = args
        .iter()
        .filter(|a| a.ends_with(".song") || a.ends_with(".wav"));
    let (Some(song), Some(out)) = (paths.next(), paths.next()) else {
        eprintln!("{USAGE}");
        process::exit(2);
    };
    let rate = flag("--rate").unwrap_or(48_000);
    if !(8_000..=192_000).contains(&rate) {
        fail(&format!("--rate {rate}: 8000 to 192000"));
    }
    let bits = flag("--bits").unwrap_or(24);
    let format =
        Format::from_bits(bits).unwrap_or_else(|| fail(&format!("--bits {bits}: 16, 24 or 32")));
    let bars = flag("--bars").unwrap_or(0);

    let text = fs::read(song).unwrap_or_else(|e| fail(&format!("{song}: {e}")));
    let mut e = Engine::new(rate as f32);
    e.song_buffer(text.len())
        .unwrap_or_else(|| fail(&format!("{song}: too long")))
        .copy_from_slice(&text);
    if let Err(err) = e.load_song() {
        fail(&format!("{song}:{}:{}: {}", err.line, err.col, err.msg));
    }

    let started = Instant::now();
    let mut b = Bounce::start(&mut e, u64::from(bars), format)
        .unwrap_or_else(|err| fail(&format!("{err:?}")));
    while !b.done() {
        b.step(&mut e, MAX_STEP);
    }
    let took = started.elapsed().as_secs_f32();
    fs::write(out, b.wav()).unwrap_or_else(|e| fail(&format!("{out}: {e}")));
    let seconds = b.frames() as f32 / rate as f32;
    let tail = (b.frames() - b.end()) as f32 / rate as f32;
    println!(
        "{out}: {seconds:.1} s ({tail:.1} s tail), {rate} Hz, {bits}-bit, peak {:.1} dBFS, in {took:.1} s ({:.0}× real time)",
        20.0 * b.peak().max(1e-10).log10(),
        seconds / took.max(1e-6),
    );
}

fn fail(msg: &str) -> ! {
    eprintln!("bounce: {msg}");
    process::exit(1);
}
