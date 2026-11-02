// Streams chat completions from llama-server's OpenAI-compatible
// /v1/chat/completions endpoint. Writes deltas to stdout as they
// arrive (so the user sees Bob "thinking out loud"), and returns
// the full assembled text so the agent loop can parse action tags
// off it.

use std::io::Write;
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::Value;

use crate::Msg;

pub async fn stream_chat(
    endpoint: &str,
    model: &str,
    history: &[Msg],
    verbose: bool,
) -> Result<String, String> {
    stream_chat_auth(endpoint, model, history, verbose, None).await
}

/// Same as `stream_chat`, but attaches a bearer token when `token` is set -
/// used by `orbix chat` to reach a remote Orbix app's authenticated
/// /v1/chat/completions endpoint over the network.
pub async fn stream_chat_auth(
    endpoint: &str,
    model: &str,
    history: &[Msg],
    verbose: bool,
    token: Option<&str>,
) -> Result<String, String> {
    // The app endpoint is shown as ".../v1"; llama-server's is bare. Accept
    // either by stripping a trailing /v1 before we append the path.
    let base = endpoint.trim_end_matches('/').trim_end_matches("/v1");
    let url = format!("{}/v1/chat/completions", base);
    let body = serde_json::json!({
        "model": model,
        "messages": history,
        "stream": true,
        "temperature": 0.2,
    });

    let client = reqwest::Client::builder()
        // Long total timeout so an agent that's running a big shell
        // command doesn't get a phantom HTTP error. The model itself
        // streams quickly; the wait is action latency between turns.
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| format!("http client: {}", e))?;

    let mut req = client.post(&url).json(&body);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await
        .map_err(|e| format!("post chat completions: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("llama-server HTTP {} - {}", status, tail(&body, 300)));
    }

    let mut stream = resp.bytes_stream();
    let mut accum = String::new();
    let mut printed_any = false;
    let mut buf = Vec::new();
    let mut stdout = std::io::stdout().lock();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("read stream: {}", e))?;
        buf.extend_from_slice(&bytes);

        // SSE frames separated by "\n\n". Parse complete frames out
        // of the buffer; leave a partial trailing frame for the next
        // chunk.
        while let Some(idx) = find_sep(&buf) {
            let frame = String::from_utf8_lossy(&buf[..idx]).to_string();
            buf.drain(..idx + 2);
            for line in frame.lines() {
                let line = line.trim_start();
                let Some(payload) = line.strip_prefix("data: ") else { continue };
                if payload == "[DONE]" { continue; }
                let Ok(v) = serde_json::from_str::<Value>(payload) else { continue };
                if let Some(delta) = v["choices"][0]["delta"]["content"].as_str() {
                    accum.push_str(delta);
                    // Stream to terminal only the prose the model
                    // wants the user to see. Hide <action> tag
                    // contents unless --verbose, otherwise the
                    // terminal fills with JSON.
                    let visible = if verbose { delta.to_string() }
                        else { strip_action_tag_chunks(delta, &accum) };
                    if !visible.is_empty() {
                        let _ = stdout.write_all(visible.as_bytes());
                        let _ = stdout.flush();
                        printed_any = true;
                    }
                }
            }
        }
    }
    if printed_any { let _ = stdout.write_all(b"\n"); let _ = stdout.flush(); }
    Ok(accum)
}

fn find_sep(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n")
}

/// Hide the contents of <action>...</action> tags from terminal
/// output. We track the accumulator's tag depth: if we're inside a
/// tag, write nothing; on a closing tag, optionally write a marker.
fn strip_action_tag_chunks(delta: &str, accum: &str) -> String {
    // Cheap heuristic: count open vs close tags in accum INCLUDING the
    // new delta. If we ended inside a tag, drop the visible portion
    // for this delta. The downside is we may swallow a leading
    // sliver of next prose if the close tag arrives mid-delta; the
    // accum's full text drives action parsing, so nothing is lost
    // semantically - just terminal-pretty.
    let open_count = accum.matches("<action>").count();
    let close_count = accum.matches("</action>").count();
    let inside = open_count > close_count;
    if inside {
        // If the delta CONTAINS a closing tag, emit a brief marker
        // so the user sees Bob just queued an action.
        if delta.contains("</action>") {
            return " · [action queued]\n".to_string();
        }
        return String::new();
    }
    // Outside any tag - but the delta might OPEN one. Cut at the
    // first '<action>' so we don't leak the JSON.
    if let Some(idx) = delta.find("<action>") {
        let prose = &delta[..idx];
        return prose.to_string();
    }
    delta.to_string()
}

fn tail(s: &str, n: usize) -> String {
    if s.len() <= n { return s.to_string() }
    let cut = s.len() - n;
    format!("…{}", &s[cut..])
}
