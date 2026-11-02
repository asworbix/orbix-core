//! MCP client: Orbix consuming the tool servers apps and services publish.
//!
//! Door three of pair mode, and the other half of `orbix mcp`. A server
//! is one `McpServer` entry (the MCP panel's list): a local command
//! spoken to over stdio, or a URL spoken to over streamable HTTP. Servers
//! start lazily on first use and stay up; tools are listed once per
//! start and cached. Bob gets `mcp_tools` and `mcp_call`; pairing adds a
//! server when a recipe names one for the app and turns its tools into a
//! peer capability. Server-to-client requests (ping, roots) get minimal
//! answers; sampling is declined.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};

use crate::{AppState, McpServer, McpStatus, McpTransport};

const PROTOCOL: &str = "2025-06-18";
const CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
const START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolSummary {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

// ─────────────────────────────────────────────
//  Stdio sessions
// ─────────────────────────────────────────────

struct StdioSession {
    child: tokio::process::Child,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pending: Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next_id: AtomicU64,
    tools: Vec<ToolSummary>,
    server_name: String,
}

impl StdioSession {
    async fn request(&self, method: &str, params: Value, timeout: std::time::Duration) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        {
            let mut stdin = self.stdin.lock().await;
            stdin
                .write_all(format!("{}\n", msg).as_bytes())
                .await
                .map_err(|e| format!("{}: write: {}", self.server_name, e))?;
            stdin.flush().await.map_err(|e| format!("{}: flush: {}", self.server_name, e))?;
        }
        let reply = tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| format!("{} did not answer {} in time", self.server_name, method))?
            .map_err(|_| format!("{} closed the connection", self.server_name))?;
        unwrap_rpc(reply)
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(format!("{}\n", msg).as_bytes())
            .await
            .map_err(|e| format!("{}: write: {}", self.server_name, e))?;
        stdin.flush().await.map_err(|e| e.to_string())
    }
}

/// A JSON-RPC reply: the result, or the error's message.
fn unwrap_rpc(reply: Value) -> Result<Value, String> {
    if let Some(err) = reply.get("error") {
        let msg = err["message"].as_str().unwrap_or("unknown error");
        let code = err["code"].as_i64().unwrap_or(0);
        return Err(if code == 0 { msg.to_string() } else { format!("{} ({})", msg, code) });
    }
    Ok(reply.get("result").cloned().unwrap_or(Value::Null))
}

/// What to answer when the server asks us something.
fn answer_server_request(msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg["method"].as_str()?;
    Some(match method {
        "ping" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
        "roots/list" => json!({ "jsonrpc": "2.0", "id": id, "result": { "roots": [] } }),
        _ => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("{} is not supported by this client", method) } }),
    })
}

fn tools_from_list(result: &Value) -> Vec<ToolSummary> {
    result["tools"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    Some(ToolSummary {
                        name: t["name"].as_str()?.to_string(),
                        description: t["description"].as_str().unwrap_or("").to_string(),
                        input_schema: t.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object" })),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

type SessionMap = HashMap<String, Arc<Mutex<StdioSession>>>;
static SESSIONS: OnceLock<Mutex<SessionMap>> = OnceLock::new();

fn sessions() -> &'static Mutex<SessionMap> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn start_stdio(server: &McpServer) -> Result<Arc<Mutex<StdioSession>>, String> {
    if server.command.trim().is_empty() {
        return Err(format!("{} has no command to run", server.name));
    }
    let mut cmd = crate::process_ext::tokio_command(&server.command);
    cmd.args(&server.args)
        .envs(server.env.iter())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start {} ({}): {}", server.name, server.command, e))?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stdin = Arc::new(Mutex::new(stdin));
    let pending: Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Arc::new(std::sync::Mutex::new(HashMap::new()));

    // Reader: route replies by id, answer the server's own requests.
    {
        let pending = pending.clone();
        let stdin = stdin.clone();
        let name = server.name.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Ok(msg) = serde_json::from_str::<Value>(line) else {
                    log::debug!("[mcp:{}] non-JSON line: {}", name, crate::truncate_str(line, 200));
                    continue;
                };
                let items: Vec<Value> = match msg {
                    Value::Array(a) => a,
                    m => vec![m],
                };
                for m in items {
                    let is_reply = m.get("id").is_some() && (m.get("result").is_some() || m.get("error").is_some());
                    if is_reply {
                        if let Some(id) = m["id"].as_u64() {
                            if let Some(tx) = pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
                                let _ = tx.send(m);
                            }
                        }
                    } else if m.get("id").is_some() {
                        if let Some(reply) = answer_server_request(&m) {
                            let mut w = stdin.lock().await;
                            let _ = w.write_all(format!("{}\n", reply).as_bytes()).await;
                            let _ = w.flush().await;
                        }
                    }
                    // Notifications (progress, logging) are not surfaced yet.
                }
            }
            // Pipe closed: fail every waiter.
            pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
        });
    }

    let session = StdioSession {
        child,
        stdin,
        pending,
        next_id: AtomicU64::new(1),
        tools: Vec::new(),
        server_name: server.name.clone(),
    };
    let init = session
        .request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": { "name": "orbix-core", "version": env!("CARGO_PKG_VERSION") }
            }),
            START_TIMEOUT,
        )
        .await?;
    let _ = init;
    session.notify("notifications/initialized", json!({})).await?;
    let tools = session.request("tools/list", json!({}), START_TIMEOUT).await?;
    let mut session = session;
    session.tools = tools_from_list(&tools);
    Ok(Arc::new(Mutex::new(session)))
}

async fn stdio_session(server: &McpServer) -> Result<Arc<Mutex<StdioSession>>, String> {
    let mut map = sessions().lock().await;
    if let Some(existing) = map.get(&server.id) {
        // Still alive?
        let alive = {
            let mut s = existing.lock().await;
            matches!(s.child.try_wait(), Ok(None))
        };
        if alive {
            return Ok(existing.clone());
        }
        map.remove(&server.id);
    }
    let session = start_stdio(server).await?;
    map.insert(server.id.clone(), session.clone());
    Ok(session)
}

/// Stop a stdio server (the MCP panel's toggle off, or unpair).
pub async fn stop(server_id: &str) {
    if let Some(s) = sessions().lock().await.remove(server_id) {
        let mut s = s.lock().await;
        let _ = s.child.kill().await;
    }
}

// ─────────────────────────────────────────────
//  Streamable HTTP
// ─────────────────────────────────────────────

struct HttpSession {
    session_id: Option<String>,
    tools: Vec<ToolSummary>,
}

static HTTP: OnceLock<Mutex<HashMap<String, HttpSession>>> = OnceLock::new();

fn http_sessions() -> &'static Mutex<HashMap<String, HttpSession>> {
    HTTP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Pull the JSON-RPC reply with `id` out of an SSE body.
pub fn reply_from_sse(body: &str, id: u64) -> Option<Value> {
    let mut data = String::new();
    let mut found = None;
    for line in body.lines().chain(std::iter::once("")) {
        if let Some(rest) = line.strip_prefix("data:") {
            data.push_str(rest.trim_start());
            data.push('\n');
        } else if line.trim().is_empty() && !data.trim().is_empty() {
            if let Ok(v) = serde_json::from_str::<Value>(data.trim()) {
                if v["id"].as_u64() == Some(id) {
                    found = Some(v);
                    break;
                }
            }
            data.clear();
        }
    }
    found
}

async fn http_bearer(server: &McpServer, state: Option<&AppState>) -> Option<String> {
    if let Some(slug) = server.bearer_from_service.as_deref().filter(|s| !s.is_empty()) {
        if let Some(st) = state {
            if let Ok(t) = crate::services::bearer_token(st, slug).await {
                return Some(t);
            }
        }
    }
    server
        .env
        .get("AUTHORIZATION")
        .or_else(|| server.env.get("BEARER_TOKEN"))
        .map(|v| v.trim().trim_start_matches("Bearer ").to_string())
        .filter(|v| !v.is_empty())
}

async fn http_post(
    server: &McpServer,
    bearer: Option<&str>,
    session_id: Option<&str>,
    id: u64,
    method: &str,
    params: Value,
) -> Result<(Value, Option<String>), String> {
    let url = server.url.clone().filter(|u| !u.trim().is_empty()).ok_or_else(|| format!("{} has no url", server.name))?;
    let client = reqwest::Client::builder().timeout(CALL_TIMEOUT).build().map_err(|e| e.to_string())?;
    let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", PROTOCOL)
        .json(&body);
    if let Some(b) = bearer {
        req = req.bearer_auth(b);
    }
    if let Some(sid) = session_id {
        req = req.header("Mcp-Session-Id", sid);
    }
    let resp = req.send().await.map_err(|e| format!("{}: {}", server.name, e))?;
    let status = resp.status();
    let new_session = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()).map(String::from);
    let content_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(format!("{} refused the sign-in ({}). It needs a valid token.", server.name, status.as_u16()));
    }
    if !status.is_success() {
        return Err(format!("{} answered {}: {}", server.name, status.as_u16(), crate::truncate_str(&text, 300)));
    }
    let reply = if content_type.contains("text/event-stream") {
        reply_from_sse(&text, id).ok_or_else(|| format!("{} streamed no reply to {}", server.name, method))?
    } else if text.trim().is_empty() {
        json!({ "result": {} })
    } else {
        serde_json::from_str::<Value>(&text).map_err(|e| format!("{}: bad JSON: {}", server.name, e))?
    };
    Ok((reply, new_session))
}

async fn http_ensure(server: &McpServer, state: Option<&AppState>) -> Result<(Option<String>, Vec<ToolSummary>), String> {
    {
        let map = http_sessions().lock().await;
        if let Some(s) = map.get(&server.id) {
            return Ok((s.session_id.clone(), s.tools.clone()));
        }
    }
    let bearer = http_bearer(server, state).await;
    let (init, sid) = http_post(
        server,
        bearer.as_deref(),
        None,
        1,
        "initialize",
        json!({
            "protocolVersion": PROTOCOL,
            "capabilities": {},
            "clientInfo": { "name": "orbix-core", "version": env!("CARGO_PKG_VERSION") }
        }),
    )
    .await?;
    unwrap_rpc(init)?;
    // initialized notification (no id; servers may answer 202)
    let _ = {
        let url = server.url.clone().unwrap_or_default();
        let client = reqwest::Client::new();
        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized", "params": {} }));
        if let Some(b) = &bearer {
            req = req.bearer_auth(b);
        }
        if let Some(s) = &sid {
            req = req.header("Mcp-Session-Id", s);
        }
        req.send().await
    };
    let (list, _) = http_post(server, bearer.as_deref(), sid.as_deref(), 2, "tools/list", json!({})).await?;
    let tools = tools_from_list(&unwrap_rpc(list)?);
    http_sessions().lock().await.insert(server.id.clone(), HttpSession { session_id: sid.clone(), tools: tools.clone() });
    Ok((sid, tools))
}

// ─────────────────────────────────────────────
//  Public API
// ─────────────────────────────────────────────

fn find_server(state: &AppState, name_or_id: &str) -> Result<McpServer, String> {
    let needle = name_or_id.trim().to_lowercase();
    let servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
    servers
        .iter()
        .find(|s| s.id == needle || s.name.to_lowercase() == needle)
        .or_else(|| servers.iter().find(|s| s.name.to_lowercase().contains(&needle)))
        .cloned()
        .ok_or_else(|| {
            let known: Vec<String> = servers.iter().map(|s| s.name.clone()).collect();
            if known.is_empty() {
                "no MCP servers are set up. Add one in the MCP panel, or pair an app that publishes one.".to_string()
            } else {
                format!("no MCP server called \"{}\". Servers: {}", name_or_id, known.join(", "))
            }
        })
}

fn set_status(state: &AppState, id: &str, status: McpStatus) {
    let mut servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = servers.iter_mut().find(|s| s.id == id) {
        s.status = status;
    }
}

/// Start (if needed) and list a server's tools.
pub async fn list_tools(state: &AppState, name_or_id: &str) -> Result<(McpServer, Vec<ToolSummary>), String> {
    let server = find_server(state, name_or_id)?;
    if !server.enabled {
        return Err(format!("{} is switched off in the MCP panel", server.name));
    }
    let result = match server.transport {
        McpTransport::Stdio => stdio_session(&server).await.map(|s| s).and_then(|s| Ok(s)),
        McpTransport::Http | McpTransport::Sse => Err(String::new()),
    };
    let tools = match (&server.transport, result) {
        (McpTransport::Stdio, Ok(session)) => session.lock().await.tools.clone(),
        (McpTransport::Stdio, Err(e)) => {
            set_status(state, &server.id, McpStatus::Error);
            return Err(e);
        }
        _ => match http_ensure(&server, Some(state)).await {
            Ok((_, tools)) => tools,
            Err(e) => {
                set_status(state, &server.id, McpStatus::Error);
                return Err(e);
            }
        },
    };
    set_status(state, &server.id, McpStatus::Running);
    Ok((server, tools))
}

/// Call one tool. Returns the text content joined, plus the raw result.
pub async fn call_tool(state: &AppState, name_or_id: &str, tool: &str, args: Value) -> Result<(String, Value), String> {
    let server = find_server(state, name_or_id)?;
    if !server.enabled {
        return Err(format!("{} is switched off in the MCP panel", server.name));
    }
    let params = json!({ "name": tool, "arguments": if args.is_null() { json!({}) } else { args } });
    let result = match server.transport {
        McpTransport::Stdio => {
            let session = stdio_session(&server).await?;
            let s = session.lock().await;
            s.request("tools/call", params, CALL_TIMEOUT).await?
        }
        McpTransport::Http | McpTransport::Sse => {
            let (sid, _) = http_ensure(&server, Some(state)).await?;
            let bearer = http_bearer(&server, Some(state)).await;
            let (reply, _) = http_post(&server, bearer.as_deref(), sid.as_deref(), 3, "tools/call", params).await?;
            unwrap_rpc(reply)?
        }
    };
    let is_error = result["isError"].as_bool().unwrap_or(false);
    let text: Vec<String> = result["content"]
        .as_array()
        .map(|c| {
            c.iter()
                .filter_map(|item| match item["type"].as_str() {
                    Some("text") => item["text"].as_str().map(String::from),
                    Some("image") => Some("[image]".to_string()),
                    Some("resource") => item["resource"]["text"].as_str().map(String::from).or(Some("[resource]".to_string())),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let text = if text.is_empty() { result.to_string() } else { text.join("\n") };
    if is_error {
        return Err(text);
    }
    Ok((text, result))
}

/// Add (or refresh) a server entry. Returns it. `id` is stable so a
/// recipe adding the same server twice does not duplicate it.
pub fn upsert_server(state: &AppState, server: McpServer) -> McpServer {
    let mut servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = servers.iter_mut().find(|s| s.id == server.id) {
        existing.name = server.name.clone();
        existing.transport = server.transport.clone();
        existing.command = server.command.clone();
        existing.args = server.args.clone();
        existing.env = server.env.clone();
        existing.url = server.url.clone();
        existing.bearer_from_service = server.bearer_from_service.clone();
        let out = existing.clone();
        crate::storage::save_mcp_servers(&servers).ok();
        return out;
    }
    servers.push(server.clone());
    crate::storage::save_mcp_servers(&servers).ok();
    server
}

/// Prompt lines: enabled servers and the tools of the ones already up.
/// Never starts a server from here.
pub async fn prompt_digest(state: &AppState) -> String {
    let servers: Vec<McpServer> = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|s| s.enabled).cloned().collect();
    if servers.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nMCP SERVERS (mcp_tools lists a server's tools; mcp_call runs one):\n");
    let stdio = sessions().lock().await;
    let http = http_sessions().lock().await;
    for s in servers {
        let tools: Option<Vec<String>> = match s.transport {
            McpTransport::Stdio => match stdio.get(&s.id) {
                Some(sess) => Some(sess.lock().await.tools.iter().map(|t| t.name.clone()).collect()),
                None => None,
            },
            _ => http.get(&s.id).map(|h| h.tools.iter().map(|t| t.name.clone()).collect()),
        };
        match tools {
            Some(names) if !names.is_empty() => out.push_str(&format!("- {}: {}\n", s.name, names.iter().take(20).cloned().collect::<Vec<_>>().join(", "))),
            Some(_) => out.push_str(&format!("- {}: (no tools)\n", s.name)),
            None => out.push_str(&format!("- {}: not started yet (mcp_tools starts it)\n", s.name)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_reply_is_found_by_id() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"a\":1}}\n\n: keepalive\n\ndata: {\"jsonrpc\":\"2.0\",\"id\":8,\"result\":{}}\n\n";
        assert_eq!(reply_from_sse(body, 8).unwrap()["id"], 8);
        assert_eq!(reply_from_sse(body, 7).unwrap()["result"]["a"], 1);
        assert!(reply_from_sse(body, 9).is_none());
        // Multi-line data frames concatenate.
        let body = "data: {\"jsonrpc\":\"2.0\",\ndata: \"id\":1,\"result\":{}}\n\n";
        assert_eq!(reply_from_sse(body, 1).unwrap()["id"], 1);
    }

    #[test]
    fn rpc_errors_and_server_requests() {
        assert_eq!(unwrap_rpc(json!({"result": {"x": 1}})).unwrap()["x"], 1);
        let e = unwrap_rpc(json!({"error": {"code": -32601, "message": "nope"}})).unwrap_err();
        assert!(e.contains("nope") && e.contains("-32601"));
        let ping = answer_server_request(&json!({"jsonrpc":"2.0","id":4,"method":"ping"})).unwrap();
        assert_eq!(ping["id"], 4);
        assert!(ping.get("result").is_some());
        let sampling = answer_server_request(&json!({"jsonrpc":"2.0","id":5,"method":"sampling/createMessage"})).unwrap();
        assert_eq!(sampling["error"]["code"], -32601);
        assert!(answer_server_request(&json!({"jsonrpc":"2.0","method":"notifications/progress"})).is_none());
    }

    #[test]
    fn tools_parse_with_defaults() {
        let t = tools_from_list(&json!({"tools": [{"name": "a", "description": "A"}, {"name": "b", "inputSchema": {"type": "object", "properties": {}}}, {"noname": true}]}));
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].input_schema["type"], "object");
        assert_eq!(t[0].description, "A");
    }
}

#[cfg(test)]
mod live {
    //! Talks to our own `orbix mcp` binary over stdio, which needs no
    //! running app for initialize/tools/list. Ignored unless the release
    //! CLI is built:
    //!   cargo test --bin orbixcore live_stdio_roundtrip -- --ignored --nocapture
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn live_stdio_roundtrip() {
        let bin = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bob-cli/target/release/orbix");
        if !bin.exists() {
            eprintln!("[skip] {} not built", bin.display());
            return;
        }
        let server = McpServer {
            id: "test-orbix".into(),
            name: "orbix self".into(),
            transport: McpTransport::Stdio,
            command: bin.to_string_lossy().into_owned(),
            args: vec!["mcp".into(), "--client".into(), "self-test".into()],
            env: HashMap::new(),
            url: None,
            enabled: true,
            status: McpStatus::Unknown,
            created_at: String::new(),
            bearer_from_service: None,
        };
        let session = stdio_session(&server).await.expect("start");
        let tools: Vec<String> = session.lock().await.tools.iter().map(|t| t.name.clone()).collect();
        eprintln!("tools: {:?}", tools);
        assert!(tools.contains(&"bob".to_string()));
        let r = session.lock().await.request("ping", json!({}), START_TIMEOUT).await.expect("ping");
        assert!(r.is_object());
        stop("test-orbix").await;
    }
}
