//! `assist serve`: the assist server (#385, ADR-0028) on `ASSIST_BIND`
//! (default `127.0.0.1:6342`), offering the providers whose keys are in the
//! environment.
//!
//! `assist check|render|catalog [file]`: the song tools (#384) from a shell,
//! for people and for the eval (#388). A song is read from `file`, or from
//! standard input without one; the result is printed as JSON. Exits 1 when
//! a song does not check or render, 2 on a usage or I/O error.

use algo_assist::tools::{Limits, catalog, check, render};
use serde::Serialize;
use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "usage: assist serve | assist check|render|catalog [file]   (a song from file, or standard input)";

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
        "serve" => serve(),
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

/// Run the server until Ctrl-C.
fn serve() -> Result<ExitCode, String> {
    use algo_assist::assist::LoopLimits;
    use algo_assist::config::Config;
    use algo_assist::server::{AppState, Rates, router};
    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let bind = env
        .get("ASSIST_BIND")
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:6342".into());
    let effort = env
        .get("ASSIST_EFFORT")
        .cloned()
        .unwrap_or_else(|| "high".into());
    let config = Config::from_env(&env);
    // Which providers are offered, never a key.
    let offered: Vec<String> = config
        .providers
        .iter()
        .map(|p| format!("{} ({})", p.id, p.models.join(", ")))
        .collect();
    eprintln!(
        "assist: http://{bind}  providers: {}",
        if offered.is_empty() {
            "none (set a key, see README)".into()
        } else {
            offered.join("; ")
        }
    );
    let state = std::sync::Arc::new(AppState::new(
        config,
        effort,
        LoopLimits::default(),
        Rates::default(),
    ));
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&bind)
            .await
            .map_err(|e| format!("{bind}: {e}"))?;
        axum::serve(listener, router(state))
            .with_graceful_shutdown(async {
                tokio::signal::ctrl_c().await.ok();
            })
            .await
            .map_err(|e| e.to_string())
    })?;
    Ok(ExitCode::SUCCESS)
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
