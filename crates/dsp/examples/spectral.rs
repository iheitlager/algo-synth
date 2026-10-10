//! Take a WAV apart (spec 009 Req 6): its partial tracks, its fundamental in
//! harmonic mode, and how long the analysis took. With `--out`, put it back
//! together by additive resynthesis and write that as a WAV.
//!
//! `cargo run --release -p algo-dsp --example spectral -- <in.wav> [--out <out.wav>]`
//!
//! The file is read and resampled to 48 kHz by the engine's own WAV parser
//! (ADR-0013) and mixed to mono.

use std::io::Write;
use std::time::Instant;
use std::{env, fs, process};

use algo_dsp::analysis::{Settings, additive, analyse, harmonic};
use algo_dsp::sample;

const SR: f32 = 48_000.0;
/// Harmonics shown in harmonic mode.
const SHOWN: usize = 16;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1));
    let Some(path) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: spectral <in.wav> [--out <out.wav>]");
        process::exit(2);
    };
    let bytes = fs::read(path).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
    let s = sample::parse(&bytes, SR).unwrap_or_else(|e| fail(&format!("{path}: {e:?}")));
    let ch = usize::from(s.channels.max(1));
    let mono: Vec<f32> = s
        .data
        .chunks(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect();

    let started = Instant::now();
    let a = analyse(&mono, SR, &Settings::default()).unwrap_or_else(|e| fail(&format!("{e:?}")));
    let took = started.elapsed();
    let seconds = mono.len() as f32 / SR;
    println!(
        "{path}: {seconds:.2} s, {} frames, {} tracks, analysed in {:.0} ms ({:.1}× real time)",
        a.frames(),
        a.tracks.len(),
        took.as_secs_f32() * 1000.0,
        seconds / took.as_secs_f32().max(1e-6),
    );

    let mut longest: Vec<_> = a.tracks.iter().collect();
    longest.sort_by_key(|t| std::cmp::Reverse(t.len()));
    println!("\nlongest tracks:   start   frames   mean Hz    peak dB");
    for t in longest.iter().take(12) {
        let mean = t.freq.iter().sum::<f32>() / t.len().max(1) as f32;
        let peak = t.amp.iter().copied().fold(0.0, f32::max);
        println!(
            "                {:>7} {:>8} {:>9.1} {:>10.1}",
            t.start,
            t.len(),
            mean,
            20.0 * peak.max(1e-9).log10()
        );
    }

    if let Some(out) = out {
        let started = Instant::now();
        let y = additive::resynthesise(&a.tracks, a.frames(), a.hop as f32, SR, true);
        let took = started.elapsed();
        let energy = |v: &mut dyn Iterator<Item = f32>| v.map(|e| e * e).sum::<f32>();
        let err = energy(&mut mono.iter().zip(&y).map(|(a, b)| a - b));
        let level = energy(&mut mono.iter().copied());
        println!(
            "\nresynthesis: {:.0} ms, residual {:.1} dB under the original, written to {out}",
            took.as_secs_f32() * 1000.0,
            10.0 * (level.max(1e-12) / err.max(1e-12)).log10(),
        );
        write_wav(out, &y).unwrap_or_else(|e| fail(&format!("{out}: {e}")));
    }

    let h = harmonic::harmonics(&a, SHOWN);
    let voiced: Vec<f32> = h.f0.iter().copied().filter(|f| *f > 0.0).collect();
    if voiced.is_empty() {
        println!("\nharmonic mode: unvoiced");
        return;
    }
    let mut sorted = voiced.clone();
    sorted.sort_by(f32::total_cmp);
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
    println!(
        "\nharmonic mode: {} of {} frames voiced, median f0 {median:.2} Hz",
        voiced.len(),
        h.f0.len()
    );
    // The loudest voiced frame's harmonics, relative to its first.
    let loudest = (0..h.f0.len())
        .max_by(|&x, &y| {
            let sum = |f| h.frame(f).iter().sum::<f32>();
            sum(x).total_cmp(&sum(y))
        })
        .unwrap_or(0);
    let row = h.frame(loudest);
    let first = row.first().copied().unwrap_or(0.0).max(1e-9);
    let levels: Vec<String> = row
        .iter()
        .map(|a| format!("{:.1}", 20.0 * (a.max(1e-9) / first).log10()))
        .collect();
    println!(
        "frame {loudest} harmonics (dB re k=1): {}",
        levels.join(" ")
    );
}

/// Mono 16-bit PCM at `SR`.
fn write_wav(path: &str, samples: &[f32]) -> std::io::Result<()> {
    let data = (samples.len() * 2) as u32;
    let rate = SR as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes()); // bytes per second
    b.extend_from_slice(&2u16.to_le_bytes()); // block align
    b.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        let s = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        b.extend_from_slice(&s.to_le_bytes());
    }
    fs::File::create(path)?.write_all(&b)
}

fn fail(msg: &str) -> ! {
    eprintln!("spectral: {msg}");
    process::exit(1);
}
