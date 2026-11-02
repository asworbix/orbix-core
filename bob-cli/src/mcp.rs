//! `orbix mcp`: Orbix Core as an MCP tool server for other apps.
//!
//! Pair mode (the desktop app's peer.rs) writes this command into the
//! MCP config of apps that take one (Claude Desktop, Cursor, ...). The
//! app then spawns `orbix mcp --client <name>` and talks JSON-RPC over
//! stdio; every tool call becomes a loopback HTTP request to the Orbix
//! app's /cli/peer routes, so Bob answers with the user's real context.
//!
//! Kept deliberately small: the protocol handshake, four tools, and
//! honest errors when Orbix is not running. Logs go to stderr only;
//! stdout is the wire.

use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::config::CliConfig;

const DEFAULT_APP_URL: &str = "http://127.0.0.1:51231";
const LATEST_PROTOCOL: &str = "2025-06-18";
const KNOWN_PROTOCOLS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

pub struct McpOpts {
    pub app_url: Option<String>,
    pub client: Option<String>,
}

fn tools() -> Value {
    json!([
        {
            "name": "bob",
            "description": "Ask Bob, the Orbix Core agent running on this computer. Bob works locally with the user's files, desktop apps, memory, saved workflows and paired apps, and either answers or does the task. Use it for anything on the user's machine, anything Orbix already knows about the user's work, or to hand Bob a job.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "What to ask or tell Bob, in plain language." }
                },
                "required": ["message"]
            }
        },
        {
            "name": "orbix_status",
            "description": "What Orbix Core is running right now: local model, agents on shift, saved workflows, paired apps.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "orbix_run_workflow",
            "description": "Run one of the user's saved Orbix workflows by name. Use orbix_status to see the names.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": { "type": "string", "description": "Workflow name or id." } },
                "required": ["name"]
            }
        },
        {
            "name": "orbix_search_memory",
            "description": "Search what Bob remembers about the user, their preferences and their work.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }
        }
    ])
}

fn text_result(text: impl Into<String>, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "isError": is_error })
}

fn offline() -> Value {
    text_result("Orbix Core is not running on this computer. Open Orbix Core and try again.", true)
}

async fn call_tool(
    http: &reqwest::Client,
    app_url: &str,
    client: &str,
    params: &Value,
) -> Result<Value, (i64, String)> {
    let name = params["name"].as_str().unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    match name {
        "bob" => {
            let message = args["message"].as_str().unwrap_or("").trim().to_string();
            if message.is_empty() {
                return Ok(text_result("message is required", true));
            }
            let res = http
                .post(format!("{}/cli/peer/ask", app_url))
                .json(&json!({ "message": message, "client": client }))
                .send()
                .await;
            match res {
                Err(_) => Ok(offline()),
                Ok(r) => {
                    let body: Value = r.json().await.unwrap_or_else(|_| json!({}));
                    if let Some(e) = body["error"].as_str() {
                        Ok(text_result(format!("Bob could not answer: {}", e), true))
                    } else {
                        Ok(text_result(body["reply"].as_str().unwrap_or("").to_string(), false))
                    }
                }
            }
        }
        "orbix_status" => match http.get(format!("{}/cli/peer/status", app_url)).send().await {
            Err(_) => Ok(offline()),
            Ok(r) => {
                let body: Value = r.json().await.unwrap_or_else(|_| json!({}));
                let model = body["model"].as_str().unwrap_or("(none selected)");
                let agents: Vec<String> = body["agents"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let workflows: Vec<String> = body["workflows"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|w| {
                                let d = w["description"].as_str().unwrap_or("");
                                if d.is_empty() {
                                    w["name"].as_str().unwrap_or("").to_string()
                                } else {
                                    format!("{} ({})", w["name"].as_str().unwrap_or(""), d)
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let peers: Vec<String> = body["peers"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|p| format!("{}: {}", p["app"].as_str().unwrap_or(""), p["bob_line"].as_str().unwrap_or("")))
                            .collect()
                    })
                    .unwrap_or_default();
                let text = format!(
                    "Orbix Core is running.\nModel: {}\nAgents on shift: {}\nWorkflows: {}\nPaired apps: {}",
                    model,
                    if agents.is_empty() { "none".to_string() } else { agents.join(", ") },
                    if workflows.is_empty() { "none".to_string() } else { workflows.join("; ") },
                    if peers.is_empty() { "none".to_string() } else { peers.join("; ") },
                );
                Ok(text_result(text, false))
            }
        },
        "orbix_run_workflow" => {
            let wf = args["name"].as_str().unwrap_or("").trim().to_string();
            if wf.is_empty() {
                return Ok(text_result("name is required", true));
            }
            match http
                .post(format!("{}/cli/peer/run-workflow", app_url))
                .json(&json!({ "name": wf }))
                .send()
                .await
            {
                Err(_) => Ok(offline()),
                Ok(r) => {
                    let body: Value = r.json().await.unwrap_or_else(|_| json!({}));
                    if let Some(e) = body["error"].as_str() {
                        Ok(text_result(e.to_string(), true))
                    } else {
                        let status = body["status"].as_str().unwrap_or("done");
                        let out = body["output"].as_str().or(body["result"].as_str()).unwrap_or("");
                        Ok(text_result(
                            if out.is_empty() { format!("Workflow \"{}\": {}", wf, status) } else { format!("Workflow \"{}\": {}\n{}", wf, status, out) },
                            status == "failed",
                        ))
                    }
                }
            }
        }
        "orbix_search_memory" => {
            let q = args["query"].as_str().unwrap_or("").trim().to_string();
            match http
                .get(format!("{}/cli/memory/search", app_url))
                .query(&[("q", q.as_str())])
                .send()
                .await
            {
                Err(_) => Ok(offline()),
                Ok(r) => {
                    let body: Value = r.json().await.unwrap_or_else(|_| json!([]));
                    let lines: Vec<String> = body
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .take(20)
                                .map(|m| {
                                    let k = m["key"].as_str().unwrap_or("");
                                    let v = m["value"].as_str().unwrap_or("");
                                    if k.is_empty() { v.to_string() } else { format!("{}: {}", k, v) }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Ok(text_result(
                        if lines.is_empty() { "Bob has nothing remembered that matches.".to_string() } else { lines.join("\n") },
                        false,
                    ))
                }
            }
        }
        other => Err((-32602, format!("unknown tool: {}", other))),
    }
}

/// Serve MCP over stdio until the client closes the pipe.
pub async fn cmd_mcp(opts: McpOpts, cfg: &CliConfig) -> std::process::ExitCode {
    let app_url = opts
        .app_url
        .or_else(|| std::env::var("ORBIX_APP_URL").ok().filter(|s| !s.is_empty()))
        .or_else(|| if cfg.app_url.is_empty() { None } else { Some(cfg.app_url.clone()) })
        .unwrap_or_else(|| DEFAULT_APP_URL.to_string());
    let app_url = app_url.trim_end_matches('/').to_string();
    let client = opts.client.unwrap_or_else(|| "an app".to_string());
    let http = match reqwest::Client::builder().timeout(Duration::from_secs(240)).build() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("orbix mcp: http client: {}", e);
            return std::process::ExitCode::from(1);
        }
    };
    eprintln!("orbix mcp: serving {} for {} (stdio)", app_url, client);

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let out = json!({ "jsonrpc": "2.0", "id": Value::Null, "error": { "code": -32700, "message": format!("parse error: {}", e) } });
                if write_line(&mut stdout, &out).await.is_err() {
                    break;
                }
                continue;
            }
        };
        let messages: Vec<Value> = match msg {
            Value::Array(batch) => batch,
            single => vec![single],
        };
        for m in messages {
            if let Some(out) = handle(&http, &app_url, &client, &m).await {
                if write_line(&mut stdout, &out).await.is_err() {
                    return std::process::ExitCode::SUCCESS;
                }
            }
        }
    }
    std::process::ExitCode::SUCCESS
}

async fn write_line(stdout: &mut tokio::io::Stdout, v: &Value) -> std::io::Result<()> {
    stdout.write_all(format!("{}\n", v).as_bytes()).await?;
    stdout.flush().await
}

/// One JSON-RPC message in, at most one out (notifications get none).
pub async fn handle(http: &reqwest::Client, app_url: &str, client: &str, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned().filter(|v| !v.is_null());
    let method = msg["method"].as_str().unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    let id = match id {
        Some(id) => id,
        None => {
            // A notification (initialized, cancelled, progress): nothing to say.
            return None;
        }
    };
    let result: Result<Value, (i64, String)> = match method {
        "initialize" => {
            let requested = params["protocolVersion"].as_str().unwrap_or(LATEST_PROTOCOL);
            let version = if KNOWN_PROTOCOLS.contains(&requested) { requested } else { LATEST_PROTOCOL };
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "orbix", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Orbix Core runs on this computer as Bob, a local agent with the user's files, desktop apps, memory and saved workflows. Call `bob` for anything on the user's machine or anything Orbix already knows; it answers or does the job locally."
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(http, app_url, client, &params).await,
        "resources/list" => Ok(json!({ "resources": [] })),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        "logging/setLevel" => Ok(json!({})),
        other => Err((-32601, format!("method not found: {}", other))),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> reqwest::Client {
        reqwest::Client::builder().timeout(Duration::from_millis(300)).build().unwrap()
    }

    #[tokio::test]
    async fn initialize_negotiates_a_known_version() {
        let out = handle(&client(), "http://127.0.0.1:9", "Claude", &json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } }
        })).await.unwrap();
        assert_eq!(out["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(out["result"]["serverInfo"]["name"], "orbix");
        let out = handle(&client(), "http://127.0.0.1:9", "Claude", &json!({
            "jsonrpc": "2.0", "id": 2, "method": "initialize", "params": { "protocolVersion": "1999-01-01" }
        })).await.unwrap();
        assert_eq!(out["result"]["protocolVersion"], LATEST_PROTOCOL);
    }

    #[tokio::test]
    async fn notifications_get_no_reply_and_tools_are_listed() {
        assert!(handle(&client(), "http://127.0.0.1:9", "x", &json!({
            "jsonrpc": "2.0", "method": "notifications/initialized"
        })).await.is_none());
        let out = handle(&client(), "http://127.0.0.1:9", "x", &json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/list"
        })).await.unwrap();
        let names: Vec<&str> = out["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["bob", "orbix_status", "orbix_run_workflow", "orbix_search_memory"]);
    }

    #[tokio::test]
    async fn offline_app_is_a_tool_error_not_a_protocol_error() {
        // Port 9 (discard) refuses connections: the tool result must say
        // Orbix is not running, with isError, and the RPC itself succeeds.
        let out = handle(&client(), "http://127.0.0.1:9", "Claude", &json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "bob", "arguments": { "message": "hi" } }
        })).await.unwrap();
        assert!(out.get("error").is_none());
        assert_eq!(out["result"]["isError"], true);
        assert!(out["result"]["content"][0]["text"].as_str().unwrap().contains("not running"));
        let out = handle(&client(), "http://127.0.0.1:9", "Claude", &json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "nope" }
        })).await.unwrap();
        assert_eq!(out["error"]["code"], -32602);
        let out = handle(&client(), "http://127.0.0.1:9", "Claude", &json!({
            "jsonrpc": "2.0", "id": 6, "method": "does/notexist"
        })).await.unwrap();
        assert_eq!(out["error"]["code"], -32601);
    }
}
