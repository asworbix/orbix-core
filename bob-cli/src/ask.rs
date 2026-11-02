//! `bob "<task>"`: one Bob, asked from a terminal.
//!
//! The terminal is a window onto the same Bob that runs in the Orbix Core
//! app: same memory, tools, workflows and model contract. Each turn is a
//! POST to the app's loopback `/cli/peer/ask` with this terminal's folder as
//! `cwd`, so shell commands run here and relative paths resolve here. The
//! app streams the progress labels its chat ribbon shows, then the reply.
//! The message reaches Bob exactly as typed.
//!
//! When the app is not running, it is started headless first (the same
//! thing `orbix up --detach` does), so `bob` always works from a cold start.

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{json, Value};

pub const APP_URL: &str = "http://127.0.0.1:51231";

/// Where the turn comes from. Sent as `origin`, not `client`: the
/// terminal is the user, so the message is not prefixed "X asks:" (that
/// made Bob treat plain questions as a third party's and search the web).
const ORIGIN: &str = "Terminal";

/// True when the app's local server answers.
pub async fn app_is_up() -> bool {
    let quick = match reqwest::Client::builder().timeout(Duration::from_secs(2)).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    matches!(quick.get(format!("{APP_URL}/cli/warm")).send().await, Ok(r) if r.status().is_success())
}

/// Make sure the app is running: if it does not answer, start it headless
/// in the background and wait for its server. Never leaves the user with
/// homework when the fix is something we can do ourselves.
pub async fn ensure_app() -> Result<(), String> {
    if app_is_up().await {
        return Ok(());
    }
    let bin = crate::resolve_app_binary(None)?;
    crate::print_dim("Starting Orbix Core in the background…\n");
    std::process::Command::new(&bin)
        .arg("--headless")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start {}: {}", bin.display(), e))?;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if app_is_up().await {
            return Ok(());
        }
    }
    Err("Orbix Core did not come up after 30s. Open it once from Applications, then try again.".into())
}

/// One Bob turn from this terminal. Prints progress dimmed as it arrives and
/// the reply at the end. Ctrl-C stops the turn in the app, not just here.
pub async fn ask(message: &str, cwd: &Path, verbose: bool) -> Result<(), String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{APP_URL}/cli/peer/ask"))
        .json(&json!({
            "message": message,
            "origin": ORIGIN,
            "cwd": cwd.to_string_lossy(),
            "stream": true,
        }))
        .send()
        .await
        .map_err(|e| format!("cannot reach Orbix Core on {APP_URL} ({e})"))?;
    if !resp.status().is_success() {
        return Err(format!("Orbix Core answered {}", resp.status()));
    }
    // An app from before streaming answers with one JSON body (and runs
    // the turn in its default folder): show that reply rather than fail.
    let is_stream = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"));
    if !is_stream {
        let body: Value = resp.json().await.map_err(|e| format!("unreadable reply from Orbix Core ({e})"))?;
        if let Some(e) = body["error"].as_str() {
            return Err(e.to_string());
        }
        println!("{}", for_terminal(body["reply"].as_str().unwrap_or("")).trim());
        crate::print_dim("(this Orbix Core is older than this CLI: update the app so Bob works in this folder)\n");
        return Ok(());
    }

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut last_step = String::new();
    loop {
        let chunk = tokio::select! {
            c = stream.next() => c,
            _ = tokio::signal::ctrl_c() => {
                let _ = client.post(format!("{APP_URL}/cli/peer/cancel")).send().await;
                crate::print_dim("\nStopped.\n");
                return Ok(());
            }
        };
        let Some(chunk) = chunk else {
            return Err("Orbix Core closed the connection before Bob answered".into());
        };
        let chunk = chunk.map_err(|e| format!("lost the connection to Orbix Core ({e})"))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = buf.find("\n\n") {
            let frame: String = buf.drain(..end + 2).collect();
            let Some(event) = parse_frame(&frame) else { continue };
            if verbose {
                crate::print_dim(&format!("{event}\n"));
            }
            match event["event"].as_str().unwrap_or("") {
                "step" => {
                    let label = event["label"].as_str().unwrap_or("").trim();
                    if !label.is_empty() && label != last_step {
                        crate::print_dim(&format!("  · {label}\n"));
                        last_step = label.to_string();
                    }
                }
                "reply" => {
                    println!("{}", for_terminal(event["content"].as_str().unwrap_or("")).trim());
                    return Ok(());
                }
                "error" => return Err(event["error"].as_str().unwrap_or("Bob could not answer").to_string()),
                _ => {}
            }
        }
    }
}

/// The app's chat renders `[[kind:...]]` markers as UI. Here the step
/// trail was already streamed, so it goes; a document, panel or link
/// chip becomes its title, marked as living in the app.
fn for_terminal(reply: &str) -> String {
    let mut out = String::new();
    let mut rest = reply;
    while let Some(start) = rest.find("[[") {
        let Some(len) = rest[start..].find("]]") else { break };
        let inner = &rest[start + 2..start + len];
        out.push_str(&rest[..start]);
        if let Some((kind, fields)) = inner.split_once(':') {
            if kind != "steps" {
                let title = fields.rsplit('|').next().unwrap_or(fields).trim();
                out.push_str(&format!("{title} (in Orbix Core)"));
            }
        } else {
            out.push_str(&rest[start..start + len + 2]);
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

/// One SSE frame ("data: {...}") to its JSON payload.
fn parse_frame(frame: &str) -> Option<Value> {
    let data: String = frame
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim)
        .collect();
    serde_json::from_str(&data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_step_frame() {
        let v = parse_frame("data: {\"event\":\"step\",\"label\":\"Reading main.rs…\"}\n\n").unwrap();
        assert_eq!(v["event"], "step");
        assert_eq!(v["label"], "Reading main.rs…");
    }

    #[test]
    fn markers_read_as_plain_text() {
        assert_eq!(
            for_terminal("[[steps:Running `wc -l Cargo.toml`…]]\n\nIt has 44 lines."),
            "\n\nIt has 44 lines."
        );
        assert_eq!(
            for_terminal("Done: [[doc:abc-1|docx|Q3 Report]] and [[panel:p-9|Metrics]]."),
            "Done: Q3 Report (in Orbix Core) and Metrics (in Orbix Core)."
        );
        assert_eq!(for_terminal("a [[not a marker]] b"), "a [[not a marker]] b");
    }

    #[test]
    fn ignores_a_frame_without_data() {
        assert!(parse_frame(": keep-alive\n\n").is_none());
    }
}
