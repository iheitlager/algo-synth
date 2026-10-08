//! `assist check|render|catalog [file]`: the song tools (#384) from a shell,
//! for people and for the eval (#388). A song is read from `file`, or from
//! standard input without one; the result is printed as JSON. Exits 1 when
//! a song does not check or render, 2 on a usage or I/O error.

use algo_assist::tools::{Limits, catalog, check, render};
use serde::Serialize;
use std::io::Read;
use std::process::ExitCode;

const USAGE: &str =
    "usage: assist check|render|catalog [file]   (a song from file, or standard input)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(cmd), file) = (args.first(), args.get(1)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let song = || -> Result<String, String> {
        match file {
            Some(path) => std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}")),
            None => {
                let mut text = String::new();
                std::io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|e| format!("standard input: {e}"))?;
                Ok(text)
            }
        }
    };
    let result = match cmd.as_str() {
        "catalog" => print(&catalog(), true),
        "check" => song().and_then(|t| {
            let c = check(&t);
            print(&c, c.ok)
        }),
        "render" => song().and_then(|t| match render(&t, Limits::default()) {
            Ok(r) => print(&r, r.nonfinite == 0),
            Err(e) => print(&e, false),
        }),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(2)
        }
    }
}

/// Print `value` as JSON; exit 0 when `ok`, else 1.
fn print<T: Serialize>(value: &T, ok: bool) -> Result<ExitCode, String> {
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    println!("{json}");
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
