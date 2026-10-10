//! `song-lsp`: a language server for song files over stdio (#482).
//! `song-lsp tokens <file>` prints the engine's spans instead, a line each
//! (`line col len class`), which the Vim and tree-sitter tests compare with.

use std::collections::HashMap;
use std::error::Error;
use std::process::ExitCode;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{Request as _, SemanticTokensFullRequest};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    PublishDiagnosticsParams, SemanticTokens, SemanticTokensFullOptions, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use song_lsp::{diagnostics, legend, name_of, semantic_tokens, tokens};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let run = match args.as_slice() {
        [] => serve(),
        [cmd, path] if cmd == "tokens" => print_tokens(path),
        _ => {
            eprintln!(
                "usage: song-lsp            serve over stdio\n       song-lsp tokens FILE  print the engine's spans"
            );
            return ExitCode::from(2);
        }
    };
    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("song-lsp: {e}");
            ExitCode::FAILURE
        }
    }
}

fn print_tokens(path: &str) -> Result<()> {
    let text = std::fs::read_to_string(path)?;
    for t in tokens(&text) {
        println!("{} {} {} {}", t.line, t.col, t.len, name_of(t.class));
    }
    Ok(())
}

fn serve() -> Result<()> {
    let (connection, io) = Connection::stdio();
    let caps = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: legend(),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                ..SemanticTokensOptions::default()
            },
        )),
        ..ServerCapabilities::default()
    };
    connection.initialize(serde_json::to_value(caps)?)?;
    run(&connection)?;
    drop(connection);
    io.join()?;
    Ok(())
}

/// The open documents' texts, answered from and checked on every change.
fn run(connection: &Connection) -> Result<()> {
    // By the URI's text: `Uri` caches inside a `Cell`, no key for a map.
    let mut docs: HashMap<String, String> = HashMap::new();
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                let response = respond(&docs, req)?;
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(n) => {
                if let Some((uri, text)) = changed(n)? {
                    let diagnostics = text.as_deref().map(diagnostics).unwrap_or_default();
                    let params = PublishDiagnosticsParams::new(uri.clone(), diagnostics, None);
                    let note = Notification::new(PublishDiagnostics::METHOD.to_string(), params);
                    connection.sender.send(Message::Notification(note))?;
                    match text {
                        Some(text) => docs.insert(uri.as_str().to_string(), text),
                        None => docs.remove(uri.as_str()),
                    };
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn respond(docs: &HashMap<String, String>, req: Request) -> Result<Response> {
    if req.method != SemanticTokensFullRequest::METHOD {
        let msg = format!("song-lsp does not handle {}", req.method);
        return Ok(Response::new_err(
            req.id,
            ErrorCode::MethodNotFound as i32,
            msg,
        ));
    }
    let (id, params) = req.extract::<SemanticTokensParams>(SemanticTokensFullRequest::METHOD)?;
    let data = docs
        .get(params.text_document.uri.as_str())
        .map(|t| semantic_tokens(t))
        .unwrap_or_default();
    let result = SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    });
    Ok(Response::new_ok(id, result))
}

/// A document opened or changed, with its whole text, or closed (`None`).
fn changed(n: Notification) -> Result<Option<(Uri, Option<String>)>> {
    Ok(match n.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let p: DidOpenTextDocumentParams = serde_json::from_value(n.params)?;
            Some((p.text_document.uri, Some(p.text_document.text)))
        }
        DidChangeTextDocument::METHOD => {
            // Full sync: the last change is the whole text.
            let p: DidChangeTextDocumentParams = serde_json::from_value(n.params)?;
            let text = p.content_changes.into_iter().last().map(|c| c.text);
            text.map(|t| (p.text_document.uri, Some(t)))
        }
        DidCloseTextDocument::METHOD => {
            let p: DidCloseTextDocumentParams = serde_json::from_value(n.params)?;
            Some((p.text_document.uri, None))
        }
        _ => None,
    })
}
