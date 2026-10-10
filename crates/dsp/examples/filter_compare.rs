//! Compare the 4-pole ladder voicings (#319): a harmonic table per voicing
//! and input, and with `--wav <dir>` one short WAV each to A/B by ear.
//!
//! `cargo run --release -p algo-dsp --example filter_compare -- [--stages-only] [--wav <dir>]`
//!
//! Every voicing comes from the models' own filters at every panel switch,
//! so a new one shows up here by itself. `--stages-only` runs each `Stages`
//! through one neutral voicing instead, for the stage types alone.

use std::f32::consts::TAU;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use algo_dsp::mono::ladder::{Ladder, LadderTables, MAX_K, hz_to_note};
use algo_dsp::mono::model::{Filter, LadderVoicing, Model, Setting, Stages};

const SR: f32 = 48_000.0;
/// The test tone: 240 samples a period, so a second holds whole periods.
const F0: f32 = 200.0;
const CUTOFF_HZ: f32 = 1_000.0;
/// Let the filter settle before measuring.
const SETTLE: usize = 12_000;
const MEASURE: usize = 48_000;
const HARMONICS: [usize; 4] = [3, 5, 7, 9];

/// A voicing with nothing of its own: unit drive, no bass compensation,
/// the full range, the plain taper.
const NEUTRAL: LadderVoicing = LadderVoicing {
    drive: 1.0,
    comp: 0.0,
    k_scale: 1.0,
    stages: Stages::Linear,
    onset: 0.8,
    loop_hp: 0.0,
};

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Saw,
}

/// An input: the wave, its amplitude into the filter and the resonance knob.
struct Input {
    name: &'static str,
    wave: Wave,
    amp: f32,
    res: f32,
}

const INPUTS: [Input; 6] = [
    Input {
        name: "sine soft",
        wave: Wave::Sine,
        amp: 0.1,
        res: 0.0,
    },
    Input {
        name: "sine hot",
        wave: Wave::Sine,
        amp: 2.0,
        res: 0.0,
    },
    Input {
        name: "sine hot res .7",
        wave: Wave::Sine,
        amp: 2.0,
        res: 0.7,
    },
    Input {
        name: "saw soft",
        wave: Wave::Saw,
        amp: 0.1,
        res: 0.0,
    },
    Input {
        name: "saw hot",
        wave: Wave::Saw,
        amp: 2.0,
        res: 0.0,
    },
    Input {
        name: "saw hot res .7",
        wave: Wave::Saw,
        amp: 2.0,
        res: 0.7,
    },
];

fn main() -> io::Result<()> {
    let mut stages_only = false;
    let mut wav_dir = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stages-only" => stages_only = true,
            "--wav" => wav_dir = args.next(),
            _ => {
                eprintln!("usage: filter_compare [--stages-only] [--wav <dir>]");
                std::process::exit(2);
            }
        }
    }

    let voicings = if stages_only {
        stage_voicings()
    } else {
        model_voicings()
    };
    let tables = LadderTables::new(SR);
    let period = period_tables();

    println!(
        "{F0} Hz into a {CUTOFF_HZ} Hz cutoff; harmonics in dB under the fundamental, \
         fundamental in dBFS; osc = full-resonance pitch error in cents\n"
    );
    println!(
        "{:<32} {:<11} {:<16} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7}",
        "voicing", "stages", "input", "fund", "3rd", "5th", "7th", "9th", "peak", "osc"
    );
    for (label, v) in &voicings {
        let osc = match self_oscillation(&tables, v) {
            Some(cents) => format!("{cents:+.1}"),
            None => "-".to_string(),
        };
        for (i, input) in INPUTS.iter().enumerate() {
            let m = measure(&tables, v, input, &period);
            let osc = if i == 0 { osc.as_str() } else { "" };
            let label = if i == 0 { label.as_str() } else { "" };
            let stages = if i == 0 {
                format!("{:?}", v.stages)
            } else {
                String::new()
            };
            print!(
                "{label:<32} {stages:<11} {:<16} {:>6.1}",
                input.name, m.fund
            );
            for h in m.harmonics {
                print!(" {h:>6.1}");
            }
            println!(" {:>6.2} {osc:>7}", m.peak);
        }
    }

    if let Some(dir) = wav_dir {
        let dir = Path::new(&dir);
        fs::create_dir_all(dir)?;
        for (label, v) in &voicings {
            let path = dir.join(format!("{}.wav", slug(label)));
            write_wav(&path, &sweep(&tables, v))?;
            println!("wrote {}", path.display());
        }
    }
    Ok(())
}

/// Every distinct ladder voicing a model has, at its own filter and at
/// each panel switch that changes it, labelled by the models and switches
/// that share it. Only the models whose voice runs the filter: the Mono
/// voice and the D-50's partials.
fn model_voicings() -> Vec<(String, LadderVoicing)> {
    let mut out: Vec<(String, LadderVoicing)> = Vec::new();
    let settings = std::iter::once(Setting::OWN).chain(Setting::SWITCHED);
    let settings: Vec<Setting> = settings.collect();
    for (model, name) in Model::ALL {
        if !model.uses_mono_voice() && !model.uses_la() {
            continue;
        }
        let own = model.low_pass(Setting::OWN);
        for s in &settings {
            let filter = model.low_pass(*s);
            let Filter::Ladder(v) = filter else {
                continue;
            };
            let label = if *s == Setting::OWN {
                name.to_string()
            } else if filter != own {
                format!("{name} rev{}", s.rev)
            } else {
                continue;
            };
            match out.iter_mut().find(|(_, o)| *o == v) {
                Some((l, _)) if !l.starts_with(name) => *l = format!("{l}, {label}"),
                Some(_) => {}
                None => out.push((label, v)),
            }
        }
    }
    out
}

/// Each stage type through the neutral voicing.
fn stage_voicings() -> Vec<(String, LadderVoicing)> {
    [
        Stages::Linear,
        Stages::Transistor,
        Stages::Ota,
        Stages::Cem3320,
        Stages::Ssm2040,
    ]
    .into_iter()
    .map(|stages| (format!("{stages:?}"), LadderVoicing { stages, ..NEUTRAL }))
    .collect()
}

/// One period of each wave at `F0`, the saw band-limited (additive up to
/// Nyquist) so its own aliasing doesn't show up as distortion.
fn period_tables() -> [Vec<f32>; 2] {
    let n = (SR / F0) as usize;
    let sine = (0..n).map(|i| (TAU * i as f32 / n as f32).sin()).collect();
    let top = (SR / 2.0 / F0) as usize;
    let saw = (0..n)
        .map(|i| {
            let ph = TAU * i as f32 / n as f32;
            let sum: f32 = (1..top).map(|h| (h as f32 * ph).sin() / h as f32).sum();
            sum * 2.0 / std::f32::consts::PI
        })
        .collect();
    [sine, saw]
}

struct Measured {
    fund: f32,
    harmonics: [f32; 4],
    peak: f32,
}

fn measure(t: &LadderTables, v: &LadderVoicing, input: &Input, period: &[Vec<f32>; 2]) -> Measured {
    let [sine, saw] = period;
    let wave = match input.wave {
        Wave::Sine => sine,
        Wave::Saw => saw,
    };
    let mut ladder = Ladder::new();
    let cutoff = hz_to_note(CUTOFF_HZ);
    let k = input.res * MAX_K;
    let out: Vec<f32> = wave
        .iter()
        .cycle()
        .take(SETTLE + MEASURE)
        .map(|x| ladder.voiced(t, v, input.amp * x, cutoff, k, 1.0))
        .skip(SETTLE)
        .collect();
    let fund = amplitude(&out, F0);
    let db = |a: f32| 20.0 * (a.max(1e-9)).log10();
    Measured {
        fund: db(fund),
        harmonics: HARMONICS.map(|h| db(amplitude(&out, F0 * h as f32)) - db(fund)),
        peak: out.iter().fold(0.0, |p, y| p.max(y.abs())),
    }
}

/// The amplitude of `hz` in `x`: one DFT bin, exact when `x` holds whole
/// periods of it.
fn amplitude(x: &[f32], hz: f32) -> f32 {
    let w = TAU * hz / SR;
    let (re, im) = x
        .iter()
        .enumerate()
        .fold((0.0f64, 0.0f64), |(re, im), (i, y)| {
            let ph = f64::from(w) * i as f64;
            (re + f64::from(*y) * ph.cos(), im + f64::from(*y) * ph.sin())
        });
    (2.0 * (re * re + im * im).sqrt() / x.len() as f64) as f32
}

/// The pitch error in cents of the self-oscillation at full resonance and
/// no input, from the rising zero crossings; `None` if it doesn't ring up.
fn self_oscillation(t: &LadderTables, v: &LadderVoicing) -> Option<f32> {
    let mut ladder = Ladder::new();
    let cutoff = hz_to_note(CUTOFF_HZ);
    let out: Vec<f32> = (0..SETTLE * 4 + MEASURE)
        .map(|_| ladder.voiced(t, v, 0.0, cutoff, MAX_K, 1.0))
        .skip(SETTLE * 4)
        .collect();
    if out.iter().fold(0.0f32, |p, y| p.max(y.abs())) < 0.01 {
        return None;
    }
    // Interpolated rising zero crossings: first and last give the period.
    let crossings: Vec<f32> = out
        .iter()
        .zip(out.iter().skip(1))
        .enumerate()
        .filter(|(_, (a, b))| **a < 0.0 && **b >= 0.0)
        .map(|(i, (a, b))| i as f32 + a / (a - b))
        .collect();
    let (first, last) = (crossings.first()?, crossings.last()?);
    let cycles = crossings.len() as f32 - 1.0;
    if cycles < 1.0 {
        return None;
    }
    let hz = SR * cycles / (last - first);
    Some(1200.0 * (hz / CUTOFF_HZ).log2())
}

/// A 110 Hz saw, driven, through a resonant cutoff swept up and back over
/// four seconds: the same input and gain for every voicing.
fn sweep(t: &LadderTables, v: &LadderVoicing) -> Vec<f32> {
    let n = (SR * 4.0) as usize;
    let mut ladder = Ladder::new();
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            phase = (phase + 110.0 / SR).fract();
            let saw = 2.0 * phase - 1.0;
            let pos = i as f32 / n as f32;
            let tri = 1.0 - (2.0 * pos - 1.0).abs();
            let cutoff = 36.0 + 72.0 * tri;
            0.5 * ladder.voiced(t, v, 1.5 * saw, cutoff, 0.7 * MAX_K, 1.0)
        })
        .collect()
}

/// 16-bit mono PCM at `SR`, with a hand-written RIFF header.
fn write_wav(path: &Path, samples: &[f32]) -> io::Result<()> {
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

fn slug(label: &str) -> String {
    let s: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    s.split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
