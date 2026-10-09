//! Spike #420: a MIDI monitor and learner, first for the Akai MPK mini Plus.
//!
//! Every message is decoded twice: by the small decoder here (what the
//! engine's `midi_in` could keep) and by `wmidi` (the crate candidate), and a
//! disagreement is flagged. Nothing here writes to a device's memory: the only
//! SysEx sent is the universal Identity Request, and every byte sent is
//! printed first.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{Receiver, channel};
use std::thread::sleep;
use std::time::{Duration, Instant};

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

const USAGE: &str = "\
usage: midi-spike <command> [args]
  list                         ports in and out
  monitor [port] [secs] [log]  print every message (all inputs, or those whose
                               name contains <port>); stop after <secs> (0: never);
                               append to <log> as JSON lines
  identity [port]              send the Identity Request, print the reply
  learn [port] [toml] [log]    guided capture of every control, written as a map
  clock <bpm> [secs] [port]    send Start, 24 clocks a beat, then Stop
  stats <log>                  per control: count, values, rate; clock tempo and jitter";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    let result = match arg(0) {
        Some("list") => list(),
        Some("monitor") => monitor(
            arg(1).unwrap_or(""),
            arg(2).and_then(|s| s.parse().ok()).unwrap_or(0.0),
            arg(3),
        ),
        Some("identity") => identity(arg(1).unwrap_or("Port 1")),
        Some("learn") => learn(
            arg(1).unwrap_or("Port 1"),
            arg(2).unwrap_or("learn.toml"),
            arg(3).unwrap_or("learn.jsonl"),
        ),
        Some("clock") => match arg(1).and_then(|s| s.parse().ok()) {
            Some(bpm) => clock(
                bpm,
                arg(2).and_then(|s| s.parse().ok()).unwrap_or(8.0),
                arg(3).unwrap_or("Port 1"),
            ),
            None => Err("clock needs a tempo".into()),
        },
        Some("stats") => match arg(1) {
            Some(path) => stats(path),
            None => Err("stats needs a log".into()),
        },
        _ => Err(USAGE.into()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

type Res = Result<(), Box<dyn std::error::Error>>;
/// The open inputs (kept alive while listening) and what they hear.
type Inputs = (Vec<MidiInputConnection<()>>, Receiver<Event>);

/// A message as it arrived: the port, the driver's timestamp (µs) and the bytes.
#[derive(Clone, Debug)]
struct Event {
    port: String,
    us: u64,
    bytes: Vec<u8>,
}

fn list() -> Res {
    let input = MidiInput::new("midi-spike")?;
    println!("inputs:");
    for p in input.ports() {
        println!("  {}", input.port_name(&p)?);
    }
    let output = MidiOutput::new("midi-spike")?;
    println!("outputs:");
    for p in output.ports() {
        println!("  {}", output.port_name(&p)?);
    }
    Ok(())
}

/// Open every input whose name contains `filter`, all messages included
/// (SysEx, clock, active sensing), feeding one channel.
fn open_inputs(filter: &str) -> Result<Inputs, Box<dyn std::error::Error>> {
    let (tx, rx) = channel();
    let probe = MidiInput::new("midi-spike")?;
    let mut conns = Vec::new();
    for p in probe.ports() {
        let name = probe.port_name(&p)?;
        if !name.contains(filter) {
            continue;
        }
        let mut input = MidiInput::new("midi-spike")?;
        input.ignore(Ignore::None);
        let tx = tx.clone();
        let port = name.clone();
        conns.push(input.connect(
            &p,
            "midi-spike-in",
            move |us, bytes, _| {
                let _ = tx.send(Event {
                    port: port.clone(),
                    us,
                    bytes: bytes.to_vec(),
                });
            },
            (),
        )?);
        eprintln!("listening on {name}");
    }
    if conns.is_empty() {
        return Err(format!("no input matches {filter:?}").into());
    }
    Ok((conns, rx))
}

fn open_output(filter: &str) -> Result<MidiOutputConnection, Box<dyn std::error::Error>> {
    let output = MidiOutput::new("midi-spike")?;
    let port = output
        .ports()
        .into_iter()
        .find(|p| output.port_name(p).is_ok_and(|n| n.contains(filter)))
        .ok_or_else(|| format!("no output matches {filter:?}"))?;
    eprintln!("sending to {}", output.port_name(&port)?);
    Ok(output.connect(&port, "midi-spike-out")?)
}

fn send(out: &mut MidiOutputConnection, bytes: &[u8]) -> Res {
    eprintln!("send {}", hex(bytes));
    out.send(bytes)?;
    Ok(())
}

fn monitor(filter: &str, secs: f64, log: Option<&str>) -> Res {
    let (_conns, rx) = open_inputs(filter)?;
    let mut log = log
        .map(|p| File::options().create(true).append(true).open(p))
        .transpose()?;
    let start = Instant::now();
    let mut t0 = None;
    let mut clocks = Clock::default();
    loop {
        if secs > 0.0 && start.elapsed().as_secs_f64() > secs {
            return Ok(());
        }
        let Ok(e) = rx.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        if let Some(f) = log.as_mut() {
            writeln!(f, "{}", to_json(&e))?;
        }
        // Clock and active sensing would flood the screen: summarise them.
        match e.bytes.first() {
            Some(0xF8) => {
                if let Some(bpm) = clocks.tick(e.us) {
                    println!("{:>10.3}  {:<22}  clock  ~{bpm:.2} BPM", 0.0, e.port);
                }
                continue;
            }
            Some(0xFE) => continue,
            _ => {}
        }
        let t = e.us - *t0.get_or_insert(e.us);
        println!(
            "{:>10.3}  {:<22}  {:<24}  {}",
            t as f64 / 1e6,
            e.port,
            hex(&e.bytes),
            check(&e.bytes)
        );
    }
}

/// A running tempo from clock ticks, printed once a beat.
#[derive(Default)]
struct Clock {
    last: Option<u64>,
    beat: Vec<u64>,
}

impl Clock {
    fn tick(&mut self, us: u64) -> Option<f64> {
        if let Some(last) = self.last.replace(us) {
            self.beat.push(us - last);
        }
        (self.beat.len() == 24).then(|| {
            let sum: u64 = self.beat.drain(..).sum();
            60e6 / sum as f64
        })
    }
}

/// Both decodings of a message, and `!!` where they disagree.
fn check(bytes: &[u8]) -> String {
    let ours = describe(bytes);
    let theirs = match wmidi::MidiMessage::try_from(bytes) {
        Ok(m) => format!("{m:?}"),
        Err(e) => format!("wmidi error: {e:?}"),
    };
    let agree = agrees(bytes, &theirs);
    format!(
        "{ours}{}",
        if agree {
            String::new()
        } else {
            format!("   !! wmidi: {theirs}")
        }
    )
}

/// Whether wmidi read the message as the same kind ours did.
fn agrees(bytes: &[u8], theirs: &str) -> bool {
    let kind = match bytes.first().map(|b| if *b < 0xF0 { b & 0xF0 } else { *b }) {
        Some(0x80) => "NoteOff",
        Some(0x90) if bytes.get(2) == Some(&0) => "NoteOn",
        Some(0x90) => "NoteOn",
        Some(0xA0) => "PolyphonicKeyPressure",
        Some(0xB0) => "ControlChange",
        Some(0xC0) => "ProgramChange",
        Some(0xD0) => "ChannelPressure",
        Some(0xE0) => "PitchBendChange",
        Some(0xF0) => "SysEx",
        Some(0xF8) => "TimingClock",
        Some(0xFA) => "Start",
        Some(0xFB) => "Continue",
        Some(0xFC) => "Stop",
        Some(0xFE) => "ActiveSensing",
        _ => return true,
    };
    theirs.starts_with(kind)
}

const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A MIDI note as a name, middle C (60) being C4 as in the song language.
fn note(n: u8) -> String {
    format!("{}{}", NAMES[usize::from(n % 12)], i32::from(n / 12) - 1)
}

/// The decoder the engine could keep: a few lines per message type.
fn describe(b: &[u8]) -> String {
    let (Some(&s), d1, d2) = (
        b.first(),
        b.get(1).copied().unwrap_or(0),
        b.get(2).copied().unwrap_or(0),
    ) else {
        return "empty".into();
    };
    let ch = (s & 0x0F) + 1;
    match s & 0xF0 {
        0x80 => format!("ch{ch} note off {} vel {d2}", note(d1)),
        0x90 if d2 == 0 => format!("ch{ch} note on {} vel 0 (= off)", note(d1)),
        0x90 => format!("ch{ch} note on  {} vel {d2}", note(d1)),
        0xA0 => format!("ch{ch} poly pressure {} {d2}", note(d1)),
        0xB0 => format!("ch{ch} CC {d1} = {d2}{}", cc_name(d1)),
        0xC0 => format!("ch{ch} program {d1}"),
        0xD0 => format!("ch{ch} channel pressure {d1}"),
        0xE0 => {
            let v = (i32::from(d2) << 7 | i32::from(d1)) - 8192;
            format!("ch{ch} pitch bend {v:+} (lsb {d1})")
        }
        _ => match s {
            0xF0 => sysex(b),
            0xF2 => format!("song position {}", u16::from(d2) << 7 | u16::from(d1)),
            0xF8 => "clock".into(),
            0xFA => "start".into(),
            0xFB => "continue".into(),
            0xFC => "stop".into(),
            0xFE => "active sensing".into(),
            0xFF => "reset".into(),
            _ => format!("system {s:02X}"),
        },
    }
}

fn cc_name(n: u8) -> &'static str {
    match n {
        1 => " (mod wheel)",
        64 => " (sustain)",
        120..=127 => " (channel mode)",
        _ => "",
    }
}

/// SysEx, with the Identity Reply spelled out.
fn sysex(b: &[u8]) -> String {
    // Akai answers in its own layout (the MPK261's, nsmith-/mpk2, too): 47, a
    // product id, a 14-bit length, then that many bytes; the MPK mini Plus
    // sends 54, then a firmware version and its serial number in ASCII.
    if let [
        0xF0,
        0x7E,
        dev,
        0x06,
        0x02,
        0x47,
        product,
        hi,
        lo,
        data @ ..,
    ] = b
    {
        let len = usize::from(*hi) << 7 | usize::from(*lo);
        let data = &data[..len.min(data.len().saturating_sub(1))];
        let text: String = data
            .iter()
            .filter(|c| c.is_ascii_graphic())
            .map(|c| char::from(*c))
            .collect();
        return format!(
            "identity reply: device {dev:02X}, Akai product {product:02X}, {len} bytes: {} (text {text:?})",
            hex(data)
        );
    }
    if let [0xF0, 0x7E, dev, 0x06, 0x02, rest @ ..] = b {
        // One-byte manufacturer ids; 0x00 starts a three-byte one.
        let (mfr, rest) = match rest {
            [0x00, a, b2, rest @ ..] => (format!("00 {a:02X} {b2:02X}"), rest),
            [m, rest @ ..] => (
                format!("{m:02X}{}", if *m == 0x47 { " (Akai)" } else { "" }),
                rest,
            ),
            [] => return format!("identity reply, short: {}", hex(b)),
        };
        if let [f0, f1, m0, m1, v @ ..] = rest {
            let ver: Vec<String> = v
                .iter()
                .take_while(|x| **x != 0xF7)
                .map(|x| format!("{x}"))
                .collect();
            return format!(
                "identity reply: device {dev:02X}, manufacturer {mfr}, family {:04X}, model {:04X}, version {}",
                u16::from(*f1) << 7 | u16::from(*f0),
                u16::from(*m1) << 7 | u16::from(*m0),
                ver.join(".")
            );
        }
    }
    format!("sysex {} bytes", b.len())
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn to_json(e: &Event) -> String {
    let bytes: Vec<String> = e.bytes.iter().map(u8::to_string).collect();
    format!(
        "{{\"port\":\"{}\",\"us\":{},\"bytes\":[{}]}}",
        e.port,
        e.us,
        bytes.join(",")
    )
}

/// Reads back what `to_json` wrote; nothing more general.
fn from_json(line: &str) -> Option<Event> {
    let port = line
        .split("\"port\":\"")
        .nth(1)?
        .split('"')
        .next()?
        .to_string();
    let us = line
        .split("\"us\":")
        .nth(1)?
        .split(',')
        .next()?
        .parse()
        .ok()?;
    let inner = line.split("\"bytes\":[").nth(1)?.split(']').next()?;
    let bytes = inner
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().ok())
        .collect::<Option<_>>()?;
    Some(Event { port, us, bytes })
}

const IDENTITY_REQUEST: [u8; 6] = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];

fn identity(filter: &str) -> Res {
    let (_conns, rx) = open_inputs(filter)?;
    let mut out = open_output(filter)?;
    send(&mut out, &IDENTITY_REQUEST)?;
    let until = Instant::now() + Duration::from_secs(2);
    let mut heard = false;
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        let Ok(e) = rx.recv_timeout(left) else { break };
        if e.bytes.first() == Some(&0xF0) {
            println!("{}  {}", hex(&e.bytes), describe(&e.bytes));
            heard = true;
        }
    }
    if !heard {
        println!("no reply in 2 s");
    }
    Ok(())
}

fn clock(bpm: f64, secs: f64, filter: &str) -> Res {
    let mut out = open_output(filter)?;
    let tick = Duration::from_secs_f64(60.0 / bpm / 24.0);
    send(&mut out, &[0xFA])?;
    eprintln!("clock at {bpm} BPM for {secs} s (0xF8 not echoed)");
    let start = Instant::now();
    let mut next = start;
    while start.elapsed().as_secs_f64() < secs {
        out.send(&[0xF8])?;
        next += tick;
        // Sleep most of the way, then spin, for an even clock.
        if let Some(d) = next.checked_duration_since(Instant::now()) {
            if d > Duration::from_millis(2) {
                sleep(d - Duration::from_millis(1));
            }
            while Instant::now() < next {}
        }
    }
    send(&mut out, &[0xFC])?;
    Ok(())
}

/// One prompt of the learn walk-through: a key in the map, what to do, how long.
const STEPS: &[(&str, &str, u64)] = &[
    ("idle", "Hands off: what the MPK sends by itself", 4),
    ("key_low", "Play the lowest key, octave centred", 3),
    ("key_high", "Play the highest key", 3),
    (
        "velocity",
        "Play one key as softly as you can, then as hard",
        5,
    ),
    (
        "pitch_wheel",
        "Pitch wheel all the way down, then all the way up, let go",
        6,
    ),
    ("mod_wheel", "Mod wheel all the way up, then down", 5),
    ("joy_x", "Joystick fully left, then fully right, let go", 5),
    ("joy_y", "Joystick fully down, then fully up, let go", 5),
    ("knob1", "Turn knob 1 fully left, then fully right", 5),
    ("knob2", "Turn knob 2 fully left, then fully right", 5),
    ("knob3", "Turn knob 3 fully left, then fully right", 5),
    ("knob4", "Turn knob 4 fully left, then fully right", 5),
    ("knob5", "Turn knob 5 fully left, then fully right", 5),
    ("knob6", "Turn knob 6 fully left, then fully right", 5),
    ("knob7", "Turn knob 7 fully left, then fully right", 5),
    ("knob8", "Turn knob 8 fully left, then fully right", 5),
    ("knob_fast", "Spin knob 1 as fast as you can", 4),
    ("pads_a", "Bank A: hit pads 1 to 8 in order, one each", 8),
    (
        "pad_pressure",
        "Press pad 1 and push harder, then ease off",
        5,
    ),
    ("pad_roll", "Drum fast on pad 1", 4),
    (
        "bank_b",
        "Press Bank A/B for bank B, then hit pads 1 to 8",
        10,
    ),
    ("bank_a", "Press Bank A/B to return to bank A", 3),
    (
        "sustain",
        "Press and release the sustain pedal (wait if none)",
        4,
    ),
    ("rewind", "Press << once", 3),
    ("forward", "Press >> once", 3),
    ("stop", "Press Stop once", 3),
    ("play", "Press Play once", 3),
    ("rec", "Press Rec once", 3),
    ("octave", "Octave down once, octave up twice, down once", 5),
    (
        "seq_play",
        "Press Seq Play/Stop, let it run, press it again",
        8,
    ),
    ("arp", "Arp on, hold a chord, then arp off", 8),
    ("tap", "Tap Tempo four times", 4),
];

fn learn(filter: &str, toml: &str, log: &str) -> Res {
    let (_conns, rx) = open_inputs(filter)?;
    let mut logf = File::create(log)?;
    let mut map = String::from(
        "# The MPK mini Plus's default program, captured by `midi-spike learn` (#420).\n",
    );
    println!(
        "\n{} steps; follow each prompt while its bar runs.\n",
        STEPS.len()
    );
    for (i, (key, what, secs)) in STEPS.iter().enumerate() {
        println!("[{:>2}/{}] {what}", i + 1, STEPS.len());
        sleep(Duration::from_millis(1200));
        while rx.try_recv().is_ok() {}
        let mut got = Vec::new();
        let until = Instant::now() + Duration::from_secs(*secs);
        let mut shown = 0;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            let bar = ((*secs as f64 - left.as_secs_f64()) / *secs as f64 * 30.0) as usize;
            if bar > shown {
                print!("{}", "#".repeat(bar - shown));
                std::io::stdout().flush()?;
                shown = bar;
            }
            if let Ok(e) = rx.recv_timeout(left.min(Duration::from_millis(50))) {
                writeln!(logf, "{{\"step\":\"{key}\",{}", &to_json(&e)[1..])?;
                got.push(e);
            }
        }
        println!();
        let lines = summarise(&got);
        for l in &lines {
            println!("        {l}");
        }
        map.push_str(&format!("\n[{key}]\nprompt = \"{what}\"\n"));
        map.push_str(&format!(
            "messages = [\n{}]\n",
            lines
                .iter()
                .map(|l| format!("  \"{l}\",\n"))
                .collect::<String>()
        ));
    }
    File::create(toml)?.write_all(map.as_bytes())?;
    println!("\nwrote {toml} and {log}");
    Ok(())
}

/// What a set of events was: per port, kind, channel and number, the values
/// seen and how fast they came.
fn summarise(events: &[Event]) -> Vec<String> {
    #[derive(Default)]
    struct Group {
        count: usize,
        lo: i32,
        hi: i32,
        first: u64,
        last: u64,
        values: Vec<i32>,
    }
    let mut groups: BTreeMap<(String, String), Group> = BTreeMap::new();
    for e in events {
        let Some((kind, value)) = classify(&e.bytes) else {
            continue;
        };
        let g = groups.entry((e.port.clone(), kind)).or_insert(Group {
            lo: i32::MAX,
            hi: i32::MIN,
            first: e.us,
            ..Default::default()
        });
        g.count += 1;
        g.lo = g.lo.min(value);
        g.hi = g.hi.max(value);
        g.last = e.us;
        if g.values.len() < 12 {
            g.values.push(value);
        }
    }
    if groups.is_empty() {
        return vec!["nothing".into()];
    }
    groups
        .into_iter()
        .map(|((port, kind), g)| {
            let span = (g.last - g.first) as f64 / 1e6;
            let rate = if span > 0.0 {
                format!(", {:.0}/s", (g.count - 1) as f64 / span)
            } else {
                String::new()
            };
            let port = port.rsplit(' ').next().unwrap_or("").to_string();
            format!(
                "port {port} {kind}: {}x, {}..{}{rate}, first {:?}",
                g.count, g.lo, g.hi, g.values
            )
        })
        .collect()
}

/// A message's group (kind, channel, number) and its value.
fn classify(b: &[u8]) -> Option<(String, i32)> {
    let s = *b.first()?;
    let (d1, d2) = (
        i32::from(*b.get(1).unwrap_or(&0)),
        i32::from(*b.get(2).unwrap_or(&0)),
    );
    let ch = (s & 0x0F) + 1;
    Some(match s & 0xF0 {
        0x80 => (format!("ch{ch} note off"), d1),
        0x90 if d2 == 0 => (format!("ch{ch} note on vel 0"), d1),
        0x90 => (format!("ch{ch} note {}", note(d1 as u8)), d2),
        0xA0 => (format!("ch{ch} poly pressure {}", note(d1 as u8)), d2),
        0xB0 => (format!("ch{ch} CC {d1}"), d2),
        0xC0 => (format!("ch{ch} program"), d1),
        0xD0 => (format!("ch{ch} channel pressure"), d1),
        0xE0 => (format!("ch{ch} pitch bend"), (d2 << 7 | d1) - 8192),
        _ => (describe(b), 0),
    })
}

fn stats(path: &str) -> Res {
    let events: Vec<Event> = BufReader::new(File::open(path)?)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| from_json(&l))
        .collect();
    let wmidi =
        |e: &Event| wmidi::MidiMessage::try_from(e.bytes.as_slice()).map(|m| format!("{m:?}"));
    let wmidi_failed = events.iter().filter(|e| wmidi(e).is_err()).count();
    let differ = events
        .iter()
        .filter(|e| wmidi(e).is_ok_and(|w| !agrees(&e.bytes, &w)))
        .count();
    println!(
        "{} events; wmidi failed on {wmidi_failed}, read {differ} as another kind",
        events.len()
    );
    for l in summarise(&events) {
        println!("  {l}");
    }
    // Clock: the tempo and how evenly the ticks come.
    let ticks: Vec<u64> = events
        .iter()
        .filter(|e| e.bytes.first() == Some(&0xF8))
        .map(|e| e.us)
        .collect();
    if ticks.len() > 24 {
        let gaps: Vec<f64> = ticks.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
        let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
        let sd = (gaps.iter().map(|g| (g - mean).powi(2)).sum::<f64>() / gaps.len() as f64).sqrt();
        let worst = gaps.iter().map(|g| (g - mean).abs()).fold(0.0, f64::max);
        println!(
            "clock: {} ticks, {:.2} BPM, tick {mean:.0} µs ± {sd:.0} (worst {worst:.0})",
            ticks.len(),
            60e6 / (mean * 24.0)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_and_bend_decode() {
        assert_eq!(describe(&[0x90, 60, 100]), "ch1 note on  C4 vel 100");
        assert_eq!(describe(&[0x99, 36, 0]), "ch10 note on C2 vel 0 (= off)");
        assert_eq!(describe(&[0xE0, 0, 0x40]), "ch1 pitch bend +0 (lsb 0)");
        assert_eq!(
            describe(&[0xE0, 0x7F, 0x7F]),
            "ch1 pitch bend +8191 (lsb 127)"
        );
    }

    #[test]
    fn an_akai_identity_reply_decodes() {
        // As the MPK mini Plus sent it (#420), the serial number replaced.
        let mut reply = vec![0xF0, 0x7E, 0x7F, 0x06, 0x02, 0x47, 0x54, 0x00, 0x19];
        reply.extend([0x00, 0x01, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00]);
        reply.extend(b"SERIAL000000000");
        reply.extend([0x00, 0xF7]);
        let d = describe(&reply);
        assert!(d.contains("Akai product 54, 25 bytes"), "{d}");
        assert!(d.contains("\"SERIAL000000000\""), "{d}");
        // The generic layout: one-byte manufacturer, family, model, version.
        let generic = [
            0xF0, 0x7E, 0x00, 0x06, 0x02, 0x41, 0x10, 0x00, 0x20, 0x00, 1, 2, 3, 4, 0xF7,
        ];
        assert!(
            describe(&generic).contains("family 0010, model 0020"),
            "{}",
            describe(&generic)
        );
    }

    #[test]
    fn both_decoders_agree_on_the_common_messages() {
        for m in [
            &[0x90u8, 60, 1][..],
            &[0x80, 60, 0],
            &[0xB3, 74, 9],
            &[0xE0, 1, 2],
            &[0xD0, 5],
            &[0xF8],
            &[0xFA],
        ] {
            let theirs = format!(
                "{:?}",
                wmidi::MidiMessage::try_from(m).expect("wmidi decodes it")
            );
            assert!(agrees(m, &theirs), "{m:?}: {theirs}");
        }
    }

    #[test]
    fn the_log_reads_back() {
        let e = Event {
            port: "MPK mini Plus Port 1".into(),
            us: 42,
            bytes: vec![0xB0, 70, 3],
        };
        let back = from_json(&to_json(&e)).expect("parses");
        assert_eq!((back.port, back.us, back.bytes), (e.port, e.us, e.bytes));
    }
}
