//! `orbix handoff` (FABLE B3): pipe a HandoffSpec into the Orbix app's
//! /handoff endpoint and stream run progress to the terminal.
//!
//!   orbix handoff plan.json
//!   cat plan.json | orbix handoff
//!   claude -p "emit an Orbix handoff for X" | orbix handoff
//!
//! Talks to the app server (default http://127.0.0.1:51231), not the
//! llama endpoint: the app owns the durable Run. Auth is the stable
//! endpoint token from Orbix Settings > Models, resolved from --token,
//! ORBIX_TOKEN, or the saved CLI config. Exit code 0 only when the run
//! completes.

use std::io::Read;
use std::path::PathBuf;

use anstyle::{AnsiColor, Style};

use crate::config::CliConfig;

const DEFAULT_APP_URL: &str = "http://127.0.0.1:51231";

pub struct HandoffOpts {
    pub file: Option<PathBuf>,
    pub app_url: Option<String>,
    pub token: Option<String>,
    pub detach: bool,
    pub save: bool,
}

fn ok_style() -> Style {
    Style::new().fg_color(Some(AnsiColor::Green.into()))
}
fn err_style() -> Style {
    Style::new().fg_color(Some(AnsiColor::Red.into()))
}
fn dim_style() -> Style {
    Style::new().fg_color(Some(AnsiColor::BrightBlack.into()))
}

fn print_step(status: &str, index: u64, label: &str) {
    let (mark, style) = match status {
        "done" => ("\u{2713}", ok_style()),
        "failed" => ("\u{2717}", err_style()),
        "running" => ("\u{25b8}", dim_style()),
        _ => ("\u{25b8}", dim_style()),
    };
    eprintln!(
        "{}{} [{}] {}{}",
        style.render(),
        mark,
        index + 1,
        label,
        style.render_reset()
    );
}

pub async fn cmd_handoff(opts: HandoffOpts, cfg: &mut CliConfig) -> std::process::ExitCode {
    // ── Resolve spec source ──
    let raw = match &opts.file {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("orbix handoff: cannot read {}: {}", path.display(), e);
                return std::process::ExitCode::from(1);
            }
        },
        None => {
            let mut s = String::new();
            if std::io::stdin().read_to_string(&mut s).is_err() || s.trim().is_empty() {
                eprintln!("orbix handoff: no spec. Pass a file or pipe JSON on stdin.");
                eprintln!("  orbix handoff plan.json");
                eprintln!("  cat plan.json | orbix handoff");
                return std::process::ExitCode::from(1);
            }
            s
        }
    };

    // Validate JSON locally so a malformed pipe fails with a pointer to
    // the schema instead of a server 400.
    let spec: serde_json::Value = match serde_json::from_str(raw.trim()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("orbix handoff: spec is not valid JSON ({})", e);
            eprintln!("  contract: docs/orbix-handoff.md in the Orbix repo");
            return std::process::ExitCode::from(1);
        }
    };

    // ── Resolve app URL + token ──
    let app_url = opts
        .app_url
        .clone()
        .or_else(|| std::env::var("ORBIX_APP_URL").ok())
        .or_else(|| {
            if cfg.app_url.is_empty() { None } else { Some(cfg.app_url.clone()) }
        })
        .unwrap_or_else(|| DEFAULT_APP_URL.to_string());
    let token = opts
        .token
        .clone()
        .or_else(|| std::env::var("ORBIX_TOKEN").ok())
        .or_else(|| {
            if cfg.app_token.is_empty() { None } else { Some(cfg.app_token.clone()) }
        });
    let Some(token) = token else {
        eprintln!("orbix handoff: no token.");
        eprintln!("  Turn on the endpoint in Orbix Settings > Models, then either:");
        eprintln!("    orbix handoff --token <token> --save   (stores it in the CLI config)");
        eprintln!("    export ORBIX_TOKEN=<token>");
        return std::process::ExitCode::from(1);
    };

    if opts.save {
        cfg.app_token = token.clone();
        cfg.app_url = app_url.clone();
        if let Err(e) = cfg.save() {
            eprintln!("orbix handoff: could not save config: {}", e);
        }
    }

    // ── POST ──
    let url = format!(
        "{}/handoff{}",
        app_url.trim_end_matches('/'),
        if opts.detach { "?wait=false" } else { "" }
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .json(&spec)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            eprintln!("orbix handoff: cannot reach {} ({})", app_url, e);
            eprintln!("  Is Orbix Core running?");
            return std::process::ExitCode::from(1);
        }
    };

    let status = resp.status();
    if status.as_u16() == 401 {
        eprintln!("orbix handoff: unauthorized. Check the token against Orbix Settings > Models.");
        return std::process::ExitCode::from(1);
    }
    if status.as_u16() == 400 {
        let body = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v["error"].as_str().map(|s| s.to_string()))
            .unwrap_or(body);
        eprintln!("orbix handoff: rejected: {}", msg);
        return std::process::ExitCode::from(1);
    }
    if !status.is_success() {
        eprintln!("orbix handoff: server returned {}", status);
        return std::process::ExitCode::from(1);
    }

    if opts.detach {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        println!(
            "accepted run {} (watch it in the Orbix app)",
            body["run_id"].as_str().unwrap_or("?")
        );
        return std::process::ExitCode::SUCCESS;
    }

    // ── Stream SSE progress ──
    use futures_util::StreamExt;
    let mut buf = String::new();
    let mut stream = resp.bytes_stream();
    let mut final_status: Option<String> = None;
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buf.find("\n\n") {
            let frame: String = buf.drain(..pos + 2).collect();
            let Some(data) = frame.trim().strip_prefix("data: ") else { continue };
            let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else { continue };
            match event["event"].as_str().unwrap_or("") {
                "accepted" => {
                    let dim = dim_style();
                    eprintln!(
                        "{}run {} accepted, {} step(s){}",
                        dim.render(),
                        event["run_id"].as_str().unwrap_or("?"),
                        event["steps"].as_u64().unwrap_or(0),
                        dim.render_reset()
                    );
                }
                "step" => {
                    print_step(
                        event["status"].as_str().unwrap_or(""),
                        event["index"].as_u64().unwrap_or(0),
                        event["label"].as_str().unwrap_or(""),
                    );
                }
                "done" => {
                    final_status = Some(event["status"].as_str().unwrap_or("unknown").to_string());
                    let result = event["result"].as_str().unwrap_or("");
                    if !result.trim().is_empty() {
                        println!("{}", result.trim());
                    }
                    if let Some(path) = event["deliverable"]["path"].as_str() {
                        let ok = ok_style();
                        eprintln!("{}deliverable: {}{}", ok.render(), path, ok.render_reset());
                    }
                    if let Some(panel) = event["deliverable"]["panel_id"].as_str() {
                        let ok = ok_style();
                        eprintln!("{}panel created: {} (My UI){}", ok.render(), panel, ok.render_reset());
                    }
                }
                _ => {}
            }
        }
    }

    match final_status.as_deref() {
        Some("completed") => std::process::ExitCode::SUCCESS,
        Some(other) => {
            eprintln!("{}run ended: {}{}", err_style().render(), other, err_style().render_reset());
            std::process::ExitCode::from(1)
        }
        None => {
            eprintln!("orbix handoff: stream ended without a result");
            std::process::ExitCode::from(1)
        }
    }
}
