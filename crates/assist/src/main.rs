//! `assist serve`: the assist server (#385, ADR-0028) on `ASSIST_BIND`
//! (default `127.0.0.1:6342`), offering the providers whose keys are in the
//! environment.
//!
//! `assist eval --provider ID --model ID [--only a,b] [--json]`: the eval
//! (#388) on that provider, graded by code; it calls the provider for every
//! case and costs money.
//!
//! `assist check|render|catalog [file]`: the song tools (#384) from a shell,
//! for people and for the eval (#388). A song is read from `file`, or from
//! standard input without one; the result is printed as JSON. Exits 1 when
//! a song does not check or render, 2 on a usage or I/O error.

use algo_assist::tools::{Limits, catalog, check, render};
use serde::Serialize;
use std::io::Read;
use std::process::ExitCode;

const USAGE: &str = "usage: assist serve | assist eval --provider ID --model ID [--only a,b] [--json] | assist check|render|catalog [file]";

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
        "eval" => eval(args.get(1..).unwrap_or_default()),
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

/// Run the eval on one provider and model; the report goes to standard
/// output, progress to standard error.
fn eval(args: &[String]) -> Result<ExitCode, String> {
    use algo_assist::assist::{LoopLimits, Request};
    use algo_assist::config::Config;
    use algo_assist::eval::{self, Row};
    use algo_assist::provider::Any;
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let (Some(provider), Some(model)) = (flag("--provider"), flag("--model")) else {
        return Err(USAGE.to_string());
    };
    let only: Option<Vec<String>> =
        flag("--only").map(|o| o.split(',').map(str::to_string).collect());
    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let config = Config::from_env(&env);
    let Some(p) = config.find(&provider, &model).cloned() else {
        return Err(format!(
            "{provider} / {model} is not offered: check its key and providers.json"
        ));
    };
    let cases: Vec<_> = eval::cases(eval::CASES)?
        .into_iter()
        .filter(|c| only.as_ref().is_none_or(|o| o.contains(&c.id)))
        .collect();
    let effort = env
        .get("ASSIST_EFFORT")
        .cloned()
        .unwrap_or_else(|| "high".into());
    eprintln!(
        "eval: {} cases on {provider} / {model}, effort {effort}; each calls the provider and costs money",
        cases.len()
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let any = Any::new(&p, &model, reqwest::Client::new(), &effort);
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let start = match (&case.start, &case.start_text) {
            (Some(path), _) => {
                std::fs::read_to_string(root.join(path)).map_err(|e| format!("{path}: {e}"))?
            }
            (None, Some(text)) => text.clone(),
            (None, None) => String::new(),
        };
        let req = Request {
            song: start.clone(),
            request: case.request.clone(),
            focus: None,
        };
        let out = rt.block_on(eval::run_case(&any, &req, LoopLimits::default()));
        let row = Row::new(case, &out, &eval::grade(&start, case, &out), &model);
        eprintln!(
            "[{}/{}] {} {}",
            i + 1,
            cases.len(),
            if row.pass { "pass" } else { "FAIL" },
            case.id
        );
        rows.push(row);
    }
    if args.iter().any(|a| a == "--json") {
        print(&rows, true)
    } else {
        println!("{}", eval::markdown(&provider, &model, &rows));
        Ok(ExitCode::SUCCESS)
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
