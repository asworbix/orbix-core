use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use warp::Filter;
use uuid::Uuid;
use chrono::Utc;
use tauri::Manager;

use crate::AppState;

/// The endpoint server holds only the Tauri AppHandle so every request
/// reads/writes the *same* state the desktop UI uses.
pub struct MobileApiState {
    pub app_handle: tauri::AppHandle,
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

/// Start the mobile HTTP API on port 51231
pub async fn start_mobile_api(state: MobileApiState) {
    let state = Arc::new(state);


    // POST /hook/<id>  Webhook trigger endpoint (W2.6)
    // Matches an enabled WebhookEndpoint by its path, acks immediately, and
    // fires the bound workflow in the background (correct webhook semantics:
    // do not make the caller wait for the run). Previously WebhookEndpoints
    // existed but no route ever received them, so they never fired.
    let webhook_hook = {
        let st = Arc::clone(&state);
        warp::path("hook")
            .and(warp::path::tail())
            .and(warp::post())
            // The POST body IS the trigger data (a JSON form/record/event).
            // Accept anything up to 1MB; parse to JSON, or wrap raw text so
            // a non-JSON caller still hands the workflow {{trigger.raw}}.
            .and(warp::body::content_length_limit(1024 * 1024))
            .and(warp::body::bytes())
            .map(move |tail: warp::path::Tail, body: warp::hyper::body::Bytes| {
                let hook_path = format!("/hook/{}", tail.as_str());
                let payload: serde_json::Value = serde_json::from_slice(&body).unwrap_or_else(|_| {
                    serde_json::json!({ "raw": String::from_utf8_lossy(&body).to_string() })
                });
                let app_state = st.app_handle.state::<AppState>();
                let matched = {
                    let mut store = app_state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(w) = store.webhooks.iter_mut().find(|w| w.enabled && w.path == hook_path) {
                        w.received_count += 1;
                        let wf_id = w.workflow_id.clone();
                        let name = w.name.clone();
                        let entry = crate::triggers::EventLogEntry::new(
                            crate::triggers::TriggerSource::Webhook,
                            hook_path.clone(),
                            format!("Webhook '{}' received", name),
                        );
                        store.event_log.push(entry);
                        store.save();
                        Some(wf_id)
                    } else {
                        None
                    }
                };
                match matched {
                    Some(wf_id) if !wf_id.is_empty() => {
                        let fire_handle = st.app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = fire_handle.state::<AppState>();
                            let _ = crate::commands::workflows_cmd::run_workflow_with_trigger(&wf_id, payload, &state, &fire_handle).await;
                        });
                        warp::reply::json(&ApiResponse { success: true, message: "Webhook accepted".to_string() })
                    }
                    Some(_) => warp::reply::json(&ApiResponse { success: false, message: "Webhook has no workflow bound".to_string() }),
                    None => warp::reply::json(&ApiResponse { success: false, message: "No matching webhook".to_string() }),
                }
            })
    };


    // ── CLI Bridge (no device auth, localhost only) ──
    // These endpoints let Bob (Claude Code terminal) interact with the running app.

    // POST /cli/email/connect - start OAuth flow for email provider
    let cli_email_connect = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "email" / "connect")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |body: HashMap<String, String>| {
                let app = st.app_handle.clone();
                async move {
                    let provider_str = body.get("provider").map(|s| s.as_str()).unwrap_or("microsoft");
                    let connector_id = match provider_str {
                        "gmail" | "google" => {
                            return Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                                "success": false,
                                "error": "Google mail is not supported yet - connect a Microsoft account instead."
                            })));
                        }
                        "outlook" | "microsoft" => "microsoft",
                        other => other,
                    };

                    match crate::oauth::start_oauth_flow(connector_id).await {
                        Ok(tokens) => {
                            // Determine email from provider
                            let email = body.get("email").cloned().unwrap_or_default();
                            let display_name = body.get("display_name").cloned().unwrap_or_else(|| email.clone());

                            let provider = crate::triggers::EmailProvider::Outlook;

                            // Create email account with tokens
                            let app_state = app.state::<AppState>();
                            let mut store = app_state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                            let mut account = crate::triggers::EmailAccount::new(provider, email, display_name);
                            account.access_token = Some(tokens.access_token.clone());
                            account.refresh_token = tokens.refresh_token.clone();
                            account.status = crate::triggers::EmailAccountStatus::Connected;
                            if let Some(exp) = tokens.expires_in {
                                account.token_expires_at = Some(
                                    (chrono::Utc::now() + chrono::Duration::seconds(exp as i64)).to_rfc3339()
                                );
                            }
                            let account_id = account.id.clone();
                            store.email_accounts.push(account);
                            store.save();
                            drop(store);

                            crate::event_bus::emit_ui(&app, "data-changed", "email-connected").ok();

                            Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                                "success": true,
                                "account_id": account_id,
                                "message": "Email connected via OAuth"
                            })))
                        }
                        Err(e) => {
                            Ok(warp::reply::json(&serde_json::json!({
                                "success": false,
                                "error": e
                            })))
                        }
                    }
                }
            })
    };

    // GET /cli/email/accounts - list connected email accounts
    let cli_email_accounts = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "email" / "accounts")
            .and(warp::get())
            .map(move || {
                let app_state = st.app_handle.state::<AppState>();
                let store = app_state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                let accounts: Vec<serde_json::Value> = store.email_accounts.iter().map(|a| {
                    serde_json::json!({
                        "id": a.id,
                        "provider": format!("{:?}", a.provider).to_lowercase(),
                        "email": a.email,
                        "status": format!("{:?}", a.status).to_lowercase(),
                        "last_polled": a.last_polled,
                    })
                }).collect();
                warp::reply::json(&serde_json::json!({ "accounts": accounts }))
            })
    };

    // POST /cli/email/fetch - fetch recent emails from a connected account
    let cli_email_fetch = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "email" / "fetch")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |body: HashMap<String, String>| {
                let app = st.app_handle.clone();
                async move {
                    let account = {
                        let app_state = app.state::<AppState>();
                        let store = app_state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                        let account_id = body.get("account_id").cloned().unwrap_or_default();
                        store.email_accounts.iter().find(|a| a.id == account_id).cloned()
                    };

                    match account {
                        Some(acct) => {
                            let token = match &acct.access_token {
                                Some(t) => t.clone(),
                                None => return Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                                    "success": false, "error": "No access token"
                                }))),
                            };
                            let limit = body.get("limit").and_then(|l| l.parse::<u32>().ok()).unwrap_or(10);
                            let result = match acct.provider {
                                crate::triggers::EmailProvider::Outlook =>
                                    crate::triggers::fetch_outlook_emails(&token, None, limit).await,
                            };
                            match result {
                                Ok(emails) => Ok(warp::reply::json(&serde_json::json!({
                                    "success": true,
                                    "count": emails.len(),
                                    "emails": emails,
                                }))),
                                Err(e) => Ok(warp::reply::json(&serde_json::json!({
                                    "success": false, "error": e
                                }))),
                            }
                        }
                        None => Ok(warp::reply::json(&serde_json::json!({
                            "success": false, "error": "Account not found"
                        }))),
                    }
                }
            })
    };

    // POST /cli/email/device-start - get device code immediately (non-blocking)
    let cli_device_start = {
        warp::path!("cli" / "email" / "device-start")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |body: HashMap<String, String>| {
                async move {
                    let provider_str = body.get("provider").map(|s| s.as_str()).unwrap_or("microsoft");
                    let connector_id = match provider_str {
                        "gmail" | "google" => {
                            return Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                                "success": false,
                                "error": "Google mail is not supported yet - connect a Microsoft account instead."
                            })));
                        }
                        "outlook" | "microsoft" => "microsoft",
                        other => other,
                    };

                    match crate::oauth::start_device_code_flow(connector_id).await {
                        Ok(dcr) => {
                            // Open browser automatically
                            open::that(&dcr.verification_uri).ok();

                            Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                                "success": true,
                                "user_code": dcr.user_code,
                                "verification_uri": dcr.verification_uri,
                                "device_code": dcr.device_code,
                                "message": dcr.message,
                                "expires_in": dcr.expires_in,
                                "connector_id": connector_id,
                            })))
                        }
                        Err(e) => Ok(warp::reply::json(&serde_json::json!({
                            "success": false, "error": e
                        }))),
                    }
                }
            })
    };

    // POST /cli/email/device-complete - poll for token after user signs in
    let cli_device_complete = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "email" / "device-complete")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |body: HashMap<String, String>| {
                let app = st.app_handle.clone();
                async move {
                    let connector_id = body.get("connector_id").map(|s| s.as_str()).unwrap_or("microsoft");
                    if connector_id == "google" {
                        return Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                            "success": false,
                            "error": "Google mail is not supported yet - connect a Microsoft account instead."
                        })));
                    }
                    let device_code = match body.get("device_code") {
                        Some(dc) => dc.clone(),
                        None => return Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                            "success": false, "error": "Missing device_code"
                        }))),
                    };

                    match crate::oauth::poll_device_code(connector_id, &device_code).await {
                        Ok(tokens) => {
                            let (name, email) = crate::oauth::fetch_user_info(connector_id, &tokens.access_token).await;
                            let email = email.unwrap_or_default();
                            let display_name = name.unwrap_or_else(|| email.clone());

                            let provider = crate::triggers::EmailProvider::Outlook;

                            let app_state = app.state::<AppState>();
                            let account_id = {
                                let mut store = app_state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                                let mut account = crate::triggers::EmailAccount::new(provider, email.clone(), display_name.clone());
                                account.access_token = Some(tokens.access_token);
                                account.refresh_token = tokens.refresh_token;
                                account.status = crate::triggers::EmailAccountStatus::Connected;
                                if let Some(exp) = tokens.expires_in {
                                    account.token_expires_at = Some(
                                        (chrono::Utc::now() + chrono::Duration::seconds(exp as i64)).to_rfc3339()
                                    );
                                }
                                let id = account.id.clone();
                                store.email_accounts.push(account);
                                store.save();
                                id
                            };

                            crate::event_bus::emit_ui(&app, "data-changed", "email-connected").ok();

                            Ok(warp::reply::json(&serde_json::json!({
                                "success": true,
                                "account_id": account_id,
                                "email": email,
                                "display_name": display_name,
                                "message": "Email connected"
                            })))
                        }
                        Err(e) => Ok(warp::reply::json(&serde_json::json!({
                            "success": false, "error": e
                        }))),
                    }
                }
            })
    };

    // ── CLI: Window Intelligence ──

    // GET /cli/warm - load the model server if it isn't up. Loopback
    // only: this is the CLI's self-heal hook ('orbix' launches the app,
    // then warms it), and it carries no data, but remote peers still
    // have no business starting compute on this machine.
    let cli_warm = {
        let warm_handle = state.app_handle.clone();
        warp::path!("cli" / "warm")
            .and(warp::get())
            .and(warp::addr::remote())
            .and_then(move |addr: Option<std::net::SocketAddr>| {
                let handle = warm_handle.clone();
                async move {
                    let loopback = addr.map(|a| a.ip().is_loopback()).unwrap_or(false);
                    if !loopback {
                        return Ok::<_, warp::Rejection>(warp::reply::json(
                            &serde_json::json!({ "error": "loopback only" }),
                        ));
                    }
                    match crate::ensure_model_loaded(&handle).await {
                        Ok(model) => Ok(warp::reply::json(
                            &serde_json::json!({ "status": "ready", "model": model }),
                        )),
                        Err(e) => Ok(warp::reply::json(&serde_json::json!({ "error": e }))),
                    }
                }
            })
    };

    // GET /cli/window/active - get active window info
    let cli_window_active = {
        warp::path!("cli" / "window" / "active")
            .and(warp::get())
            .and_then(|| async {
                match crate::os_auto::get_active_window().await {
                    Ok(w) => Ok::<_, warp::Rejection>(warp::reply::json(&w)),
                    Err(e) => Ok(warp::reply::json(&serde_json::json!({ "error": e }))),
                }
            })
    };

    // GET /cli/window/list - list all windows
    let cli_window_list = {
        warp::path!("cli" / "window" / "list")
            .and(warp::get())
            .and_then(|| async {
                match crate::os_auto::list_windows().await {
                    Ok(w) => Ok::<_, warp::Rejection>(warp::reply::json(&w)),
                    Err(e) => Ok(warp::reply::json(&serde_json::json!({ "error": e }))),
                }
            })
    };

    // POST /cli/window/profile - profile a window by PID
    let cli_window_profile = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "window" / "profile")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |body: HashMap<String, serde_json::Value>| {
                let st = Arc::clone(&st);
                async move {
                    let pid = body.get("pid").and_then(|v| v.as_i64()).unwrap_or(0);
                    let app_name = body.get("app_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if pid == 0 {
                        return Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({ "error": "pid required" })));
                    }
                    let app_name_clone = app_name.clone();
                    let profile_result = tokio::task::spawn_blocking(move || {
                        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                        let output = std::process::Command::new(&exe)
                            .arg("--wi-profile")
                            .arg(pid.to_string())
                            .arg(&app_name_clone)
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .output()
                            .map_err(|e| format!("spawn failed: {}", e))?;
                        if !output.status.success() {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            return Err(format!("profiling failed: {}", &stderr[..stderr.len().min(200)]));
                        }
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let profile: crate::window_intelligence::WindowProfile = serde_json::from_str(&stdout)
                            .map_err(|e| format!("parse failed: {}", e))?;
                        Ok::<_, String>(profile)
                    }).await.unwrap_or_else(|e| Err(e.to_string()));

                    match profile_result {
                        Ok(p) => {
                            let app_state = st.app_handle.state::<AppState>();
                            if let Ok(mut store) = app_state.connect.window_intelligence_store.lock() {
                                store.profiles.retain(|x| x.pid != pid);
                                store.profiles.push(p.clone());
                                store.save();
                            }
                            Ok::<_, warp::Rejection>(warp::reply::json(&p))
                        }
                        Err(e) => Ok(warp::reply::json(&serde_json::json!({ "error": e }))),
                    }
                }
            })
    };

    // ── CLI: OS Operations ──

    // GET /cli/os/apps - list running apps
    let cli_os_apps = {
        warp::path!("cli" / "os" / "apps")
            .and(warp::get())
            .and_then(|| async {
                match crate::os_auto::list_running_apps().await {
                    Ok(apps) => Ok::<_, warp::Rejection>(warp::reply::json(&apps)),
                    Err(e) => Ok(warp::reply::json(&serde_json::json!({ "error": e }))),
                }
            })
    };

    // GET /cli/os/clipboard - read clipboard
    let cli_os_clipboard_read = {
        warp::path!("cli" / "os" / "clipboard")
            .and(warp::get())
            .and_then(|| async {
                match crate::os_auto::get_clipboard().await {
                    Ok(text) => Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({ "text": text }))),
                    Err(e) => Ok(warp::reply::json(&serde_json::json!({ "error": e }))),
                }
            })
    };

    // POST /cli/os/clipboard - write clipboard
    let cli_os_clipboard_write = {
        warp::path!("cli" / "os" / "clipboard")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(|body: HashMap<String, String>| async move {
                let text = body.get("text").cloned().unwrap_or_default();
                let result = crate::os_auto::set_clipboard(&text).await;
                Ok::<_, warp::Rejection>(warp::reply::json(&result))
            })
    };

    // POST /cli/os/open-url - open URL in browser
    let cli_os_open_url = {
        warp::path!("cli" / "os" / "open-url")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(|body: HashMap<String, String>| async move {
                let url = body.get("url").cloned().unwrap_or_default();
                let result = crate::os_auto::open_url(&url).await;
                Ok::<_, warp::Rejection>(warp::reply::json(&result))
            })
    };

    // POST /cli/os/open-app - open application
    let cli_os_open_app = {
        warp::path!("cli" / "os" / "open-app")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(|body: HashMap<String, String>| async move {
                let name = body.get("name").cloned().unwrap_or_default();
                let result = crate::os_auto::open_application(&name).await;
                Ok::<_, warp::Rejection>(warp::reply::json(&result))
            })
    };

    // POST /cli/os/notification - send notification
    let cli_os_notify = {
        let app = state.app_handle.clone();
        warp::path!("cli" / "os" / "notification")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |body: HashMap<String, String>| {
                let app = app.clone();
                async move {
                    let title = body.get("title").cloned().unwrap_or_default();
                    let msg = body.get("body").cloned().unwrap_or_default();
                    let result = crate::os_auto::send_notification(&app, &title, &msg).await;
                    Ok::<_, warp::Rejection>(warp::reply::json(&result))
                }
            })
    };

    // ── CLI: Memory ──

    // GET /cli/memory - list all memories
    let cli_memory_list = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "memory")
            .and(warp::get())
            .map(move || {
                let app_state = st.app_handle.state::<AppState>();
                let mem = app_state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
                warp::reply::json(&mem.entries)
            })
    };

    // POST /cli/memory - add memory entry
    let cli_memory_add = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "memory")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |body: HashMap<String, String>| {
                let key = body.get("key").cloned().unwrap_or_default();
                let value = body.get("value").cloned().unwrap_or_default();
                let category = body.get("category").cloned().unwrap_or("fact".into());
                let source = body.get("source").cloned().unwrap_or("bob".into());
                let cat = match category.as_str() {
                    "preference" => crate::memory::MemoryCategory::Preference,
                    "context" => crate::memory::MemoryCategory::Context,
                    "skill" => crate::memory::MemoryCategory::Skill,
                    _ => crate::memory::MemoryCategory::Fact,
                };
                let app_state = st.app_handle.state::<AppState>();
                let mut mem = app_state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
                let entry = mem.add(key, value, cat, source);
                warp::reply::json(&entry)
            })
    };

    // GET /cli/memory/search?q=<query> - search memories
    let cli_memory_search = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "memory" / "search")
            .and(warp::get())
            .and(warp::query::<HashMap<String, String>>())
            .map(move |params: HashMap<String, String>| {
                let query = params.get("q").cloned().unwrap_or_default();
                let app_state = st.app_handle.state::<AppState>();
                let mem = app_state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
                let results: Vec<_> = mem.search(&query).into_iter().cloned().collect();
                warp::reply::json(&results)
            })
    };

    // ── CLI: Agents ──

    // GET /cli/agents - list agents
    let cli_agents_list = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "agents")
            .and(warp::get())
            .map(move || {
                let app_state = st.app_handle.state::<AppState>();
                let agents = app_state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                warp::reply::json(&*agents)
            })
    };

    // POST /cli/agents - create agent
    let cli_agents_create = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "agents")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |body: serde_json::Value| {
                let app_state = st.app_handle.state::<AppState>();
                let provider = match body["provider"].as_str().unwrap_or("anthropic") {
                    "openai" => crate::AgentProvider::OpenAI,
                    "local" => crate::AgentProvider::Local,
                    _ => crate::AgentProvider::Anthropic,
                };
                let agent = crate::Agent { charter: String::new(), granted_tools: Vec::new(), schedule_minutes: None, last_run_at: None,
                    id: Uuid::new_v4().to_string(),
                    name: body["name"].as_str().unwrap_or("CLI Agent").to_string(),
                    provider,
                    model: body["model"].as_str().unwrap_or("claude-sonnet-4-6").to_string(),
                    endpoint: body["endpoint"].as_str().unwrap_or("").to_string(),
                    api_key: body["api_key"].as_str().unwrap_or("").to_string(),
                    enabled: true,
                    is_default: false,
                    created_at: Utc::now().to_rfc3339(),
                    connector_ids: Vec::new(),
                    workflow_ids: Vec::new(),
                    capabilities: Default::default(),
                };
                let mut agents = app_state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                agents.push(agent.clone());
                crate::storage::save_agents(&agents).ok();
                crate::event_bus::emit_ui(&st.app_handle, "data-changed", "cli-agent-created").ok();
                warp::reply::json(&agent)
            })
    };

    // DELETE /cli/agents/:id - delete agent by id or name
    let cli_agents_delete = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "agents" / String)
            .and(warp::delete())
            .map(move |id: String| {
                let app_state = st.app_handle.state::<AppState>();
                let mut agents = app_state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                let before = agents.len();
                agents.retain(|a| a.id != id && a.name.to_lowercase() != id.to_lowercase());
                crate::storage::save_agents(&agents).ok();
                crate::event_bus::emit_ui(&st.app_handle, "data-changed", "cli-agent-deleted").ok();
                let removed = before - agents.len();
                warp::reply::json(&serde_json::json!({ "success": true, "removed": removed }))
            })
    };

    // GET /cli/workflows - list workflows
    let cli_workflows_list = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "workflows")
            .and(warp::get())
            .map(move || {
                let app_state = st.app_handle.state::<AppState>();
                let store = app_state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                warp::reply::json(&store.workflows)
            })
    };

    // POST /cli/workflows - create workflow
    let cli_workflows_create = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "workflows")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |body: serde_json::Value| {
                let app_state = st.app_handle.state::<AppState>();
                let mut store = app_state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                let name = body["name"].as_str().unwrap_or("CLI Workflow").to_string();
                let desc = body["description"].as_str().unwrap_or("").to_string();
                let wf = store.create(name, desc);
                store.save();
                crate::event_bus::emit_ui(&st.app_handle, "data-changed", "cli-workflow-created").ok();
                warp::reply::json(&wf)
            })
    };

    // DELETE /cli/workflows/:id - delete workflow by id or name
    let cli_workflows_delete = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "workflows" / String)
            .and(warp::delete())
            .map(move |id: String| {
                let app_state = st.app_handle.state::<AppState>();
                let mut store = app_state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                let before = store.workflows.len();
                store.workflows.retain(|w| w.id != id && w.name.to_lowercase() != id.to_lowercase());
                store.save();
                crate::event_bus::emit_ui(&st.app_handle, "data-changed", "cli-workflow-deleted").ok();
                let removed = before - store.workflows.len();
                warp::reply::json(&serde_json::json!({ "success": true, "removed": removed }))
            })
    };

    // ── Peer routes: what `orbix mcp` (running inside Claude, Cursor, ...)
    // calls on Bob's behalf. Loopback-only like the rest of /cli. ──

    // POST /cli/peer/ask {message, client?, origin?, cwd?, stream?} - a full
    // Bob turn (router, tools, memory), the same as typing in the chat.
    // `client` is another app asking on the user's behalf (Claude, Cursor
    // via `orbix mcp`): its name prefixes the message so the user sees who
    // asked. `origin` is one of the user's own surfaces (the terminal):
    // the message stays exactly as typed. A "Terminal asks:" prefix made
    // Bob web-search timeless questions (4/20 vs 0/20, Gemma 4 E4B,
    // 2026-10-22), reading them as a third party's request. `cwd`
    // is the caller's folder: shell commands run there and relative paths
    // resolve there for this turn. With `stream: true` the reply is SSE:
    // {"event":"step","label"} as Bob works, then one {"event":"reply",
    // "content","id"} or {"event":"error","error"}. A caller that hangs up
    // does not stop the turn; /cli/peer/cancel does.
    let cli_peer_ask = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "peer" / "ask")
            .and(warp::post())
            .and(warp::body::content_length_limit(512 * 1024))
            .and(warp::body::json())
            .and_then(move |body: serde_json::Value| {
                let app = st.app_handle.clone();
                async move {
                    let message = body["message"].as_str().unwrap_or("").trim().to_string();
                    let client = body["client"].as_str().unwrap_or("").trim().to_string();
                    let workdir = body["cwd"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(std::path::PathBuf::from);
                    let stream = body["stream"].as_bool().unwrap_or(false);
                    if message.is_empty() {
                        return Ok::<_, warp::Rejection>(json_reply(200, &serde_json::json!({ "error": "message is required" })));
                    }
                    let origin = body["origin"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
                        .unwrap_or_else(|| if client.is_empty() { "peer".to_string() } else { client.clone() });
                    let content = if client.is_empty() { message } else { format!("{} asks: {}", client, message) };

                    if !stream {
                        let scope = crate::turn_scope::TurnScope { workdir, steps: None, origin: Some(origin) };
                        let state = app.state::<AppState>();
                        let result = crate::turn_scope::scoped(scope, crate::bob_core::bob_send(content, None, state, app.clone())).await;
                        return Ok(match result {
                            Ok(reply) => json_reply(200, &serde_json::json!({ "reply": reply.content, "id": reply.id })),
                            Err(e) => json_reply(200, &serde_json::json!({ "error": e })),
                        });
                    }

                    let (step_tx, mut step_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                    let (mut tx, body) = warp::hyper::Body::channel();
                    tokio::spawn(async move {
                        let scope = crate::turn_scope::TurnScope { workdir, steps: Some(step_tx), origin: Some(origin) };
                        let turn_app = app.clone();
                        let turn = tokio::spawn(crate::turn_scope::scoped(scope, async move {
                            let state = turn_app.state::<AppState>();
                            crate::bob_core::bob_send(content, None, state, turn_app.clone()).await
                        }));
                        // The step channel closes when the turn (and its scope) ends.
                        let mut caller_here = true;
                        while let Some(label) = step_rx.recv().await {
                            if caller_here {
                                let frame = serde_json::json!({ "event": "step", "label": label });
                                caller_here = tx.send_data(sse_bytes(&frame)).await.is_ok();
                            }
                        }
                        let last = match turn.await {
                            Ok(Ok(reply)) => serde_json::json!({ "event": "reply", "content": reply.content, "id": reply.id }),
                            Ok(Err(e)) => serde_json::json!({ "event": "error", "error": e }),
                            Err(e) => serde_json::json!({ "event": "error", "error": format!("Bob's turn stopped unexpectedly: {}", e) }),
                        };
                        if caller_here {
                            let _ = tx.send_data(sse_bytes(&last)).await;
                        }
                    });
                    Ok(warp::http::Response::builder()
                        .status(200)
                        .header("content-type", "text/event-stream")
                        .header("cache-control", "no-cache")
                        .body(body)
                        .unwrap_or_else(|_| warp::http::Response::new(warp::hyper::Body::empty())))
                }
            })
    };

    // POST /cli/show - bring the window forward. A second launch of the
    // app calls this and exits (always_on::hand_over_to_running_instance).
    let cli_show = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "show")
            .and(warp::post())
            .map(move || {
                crate::always_on::show_main_window(&st.app_handle);
                warp::reply::json(&serde_json::json!({ "orbix": true }))
            })
    };

    // POST /cli/peer/cancel - stop Bob's current turn (Ctrl-C in `bob`).
    let cli_peer_cancel = warp::path!("cli" / "peer" / "cancel")
        .and(warp::post())
        .map(|| {
            crate::BOB_CANCEL_FLAG.store(true, std::sync::atomic::Ordering::SeqCst);
            warp::reply::json(&serde_json::json!({ "success": true }))
        });

    // POST /cli/peer/run-workflow {name} - run a saved workflow by name or id.
    let cli_peer_run_workflow = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "peer" / "run-workflow")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |body: serde_json::Value| {
                let app = st.app_handle.clone();
                async move {
                    let name = body["name"].as_str().unwrap_or("").trim().to_lowercase();
                    let id = {
                        let state = app.state::<AppState>();
                        let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                        store.workflows.iter()
                            .find(|w| w.id.to_lowercase() == name || w.name.to_lowercase() == name)
                            .map(|w| w.id.clone())
                    };
                    let Some(id) = id else {
                        return Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({ "error": format!("no workflow called \"{}\"", name) })));
                    };
                    let state = app.state::<AppState>();
                    match crate::commands::workflows_cmd::run_workflow_core(&id, &state, &app).await {
                        Ok(run) => Ok(warp::reply::json(&run)),
                        Err(e) => Ok(warp::reply::json(&serde_json::json!({ "error": e }))),
                    }
                }
            })
    };

    // GET /cli/peer/status - what a peer may want to know about this Orbix.
    let cli_peer_status = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "peer" / "status")
            .and(warp::get())
            .map(move || {
                let app_state = st.app_handle.state::<AppState>();
                let workflows: Vec<serde_json::Value> = {
                    let store = app_state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                    store.workflows.iter().map(|w| serde_json::json!({
                        "id": w.id, "name": w.name, "description": w.description, "steps": w.steps.len(),
                    })).collect()
                };
                let agents: Vec<String> = {
                    let agents = app_state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                    agents.iter().filter(|a| a.enabled).map(|a| a.name.clone()).collect()
                };
                let model = app_state.engine.selected_local_model.lock().unwrap_or_else(|e| e.into_inner()).clone();
                warp::reply::json(&serde_json::json!({
                    "app": "Orbix Core",
                    "model": model,
                    "agents": agents,
                    "workflows": workflows,
                    "peers": crate::peer::list(),
                }))
            })
    };

    // ── CLI: Review - Bob submits output for user approval ──

    // POST /cli/review - push a result to the app for review
    let cli_review = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "review")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |body: serde_json::Value| {
                crate::event_bus::emit_ui(&st.app_handle, "bob-review", body.clone()).ok();
                warp::reply::json(&serde_json::json!({ "success": true, "message": "review submitted" }))
            })
    };

    // GET /cli/review/response/<id> - Bob polls for user's decision
    let cli_review_response = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "review" / "response" / String)
            .and(warp::get())
            .map(move |id: String| {
                let app_state = st.app_handle.state::<AppState>();
                if let Ok(responses) = app_state.chat.review_responses.lock() {
                    if let Some(resp) = responses.get(&id) {
                        return warp::reply::json(resp);
                    }
                }
                warp::reply::json(&serde_json::json!({ "status": "pending" }))
            })
    };

    // POST /cli/review/respond - frontend submits accept/decline
    let cli_review_respond = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "review" / "respond")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |body: serde_json::Value| {
                let id = body["id"].as_str().unwrap_or("").to_string();
                let app_state = st.app_handle.state::<AppState>();
                if let Ok(mut responses) = app_state.chat.review_responses.lock() {
                    responses.insert(id, body.clone());
                }
                warp::reply::json(&serde_json::json!({ "success": true }))
            })
    };


    // POST /cli/reload - reload app data from disk
    let cli_reload = {
        let st = Arc::clone(&state);
        warp::path!("cli" / "reload")
            .and(warp::post())
            .map(move || {
                let app_state = st.app_handle.state::<AppState>();
                let new_agents = crate::storage::load_agents().unwrap_or_default();
                if let Ok(mut agents) = app_state.chat.agents.lock() {
                    *agents = new_agents;
                }
                let new_ws = crate::workflow::WorkflowStore::load();
                if let Ok(mut ws) = app_state.automation.workflow_store.lock() {
                    ws.workflows = new_ws.workflows;
                }
                crate::event_bus::emit_ui(&st.app_handle, "data-changed", "cli-reload").ok();
                warp::reply::json(&serde_json::json!({ "success": true, "message": "reloaded" }))
            })
    };


    // ── OpenAI-compatible local endpoint (FABLE B1) ──
    // Own bearer-token auth (stable token from Settings), separate from
    // device pairing: external tools store it in their config.
    let v1_auth = warp::header::optional::<String>("authorization")
        .and(warp::header::optional::<String>("x-api-key"))
        .and_then(|auth: Option<String>, key: Option<String>| async move {
            match crate::endpoint::authorize(auth.as_deref(), key.as_deref()) {
                Ok(()) => Ok(()),
                Err(reason) => Err(warp::reject::custom(EndpointDenied(reason))),
            }
        })
        .untuple_one();

    let v1_chat = warp::path!("v1" / "chat" / "completions")
        .and(warp::post())
        .and(v1_auth.clone())
        .and(warp::body::content_length_limit(4 * 1024 * 1024))
        .and(warp::body::bytes())
        .and_then(|body: warp::hyper::body::Bytes| async move {
            proxy_llama(reqwest::Method::POST, "/v1/chat/completions", Some(body)).await
        });

    let v1_models = warp::path!("v1" / "models")
        .and(warp::get())
        .and(v1_auth.clone())
        .and_then(|| async move { proxy_llama(reqwest::Method::GET, "/v1/models", None).await });

    // ── Task handoff (FABLE B2): frontier model plans, Orbix executes ──
    let handoff_route = {
        let st = Arc::clone(&state);
        warp::path!("handoff")
            .and(warp::post())
            .and(v1_auth.clone())
            .and(warp::body::content_length_limit(1024 * 1024))
            .and(warp::body::json())
            .and(warp::query::<HashMap<String, String>>())
            .and_then(move |spec: crate::handoff::HandoffSpec, q: HashMap<String, String>| {
                let app = st.app_handle.clone();
                async move { crate::handoff::handle(app, spec, q).await }
            })
    };

    let v1_routes = v1_chat
        .or(v1_models)
        .or(handoff_route)
        .recover(endpoint_recover)
        .boxed();

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST", "PUT", "DELETE"])
        .allow_headers(vec!["content-type", "authorization", "x-api-key"]);

    // CLI routes - no device auth required, localhost only
    // Split into groups with .boxed() to avoid warp type recursion overflow
    let cli_email_group = cli_email_connect
        .or(cli_device_start)
        .or(cli_device_complete)
        .or(cli_email_accounts)
        .or(cli_email_fetch)
        .or(cli_reload)
        .boxed();

    let cli_window_group = cli_window_active
        .or(cli_warm)
        .or(cli_window_list)
        .or(cli_window_profile)
        .or(cli_os_apps)
        .or(cli_os_clipboard_read)
        .or(cli_os_clipboard_write)
        .boxed();

    let cli_os_group = cli_os_open_url
        .or(cli_os_open_app)
        .or(cli_os_notify)
        .or(cli_memory_list)
        .or(cli_memory_add)
        .or(cli_memory_search)
        .boxed();

    let cli_agent_review_group = cli_agents_list
        .or(cli_agents_create)
        .or(cli_agents_delete)
        .or(cli_review)
        .or(cli_review_response)
        .or(cli_review_respond)
        .boxed();

    let cli_workflow_group = cli_workflows_list
        .or(cli_workflows_create)
        .or(cli_workflows_delete)
        .boxed();

    let cli_peer_group = cli_peer_ask
        .or(cli_peer_cancel)
        .or(cli_show)
        .or(cli_peer_run_workflow)
        .or(cli_peer_status)
        .boxed();

    // The webhook receiver is the one deliberately-inbound route: external
    // services POST /hook/<path> to fire a bound workflow, and the
    // unguessable path IS the credential. It must stay reachable off-box, so
    // it is kept OUT of the loopback-gated /cli group below.
    let webhook_route = webhook_hook.boxed();

    // The /cli bridge carries NO auth token yet exposes OS control (clipboard,
    // open app/url, notifications), memories, agents, and workflows. The server
    // binds 0.0.0.0 so the token-authed /v1 endpoint is reachable over the LAN
    // for `orbix connect`; the /cli family must never be. Gate the whole group
    // on a loopback client address - a missing/undecodable peer address is
    // treated as remote and denied. This is the real enforcement behind the
    // "localhost only" comments; the inline check in /cli/warm is now redundant
    // but harmless.
    let loopback_only = warp::addr::remote()
        .and_then(|addr: Option<std::net::SocketAddr>| async move {
            if addr.map(|a| a.ip().is_loopback()).unwrap_or(false) {
                Ok(())
            } else {
                Err(warp::reject::custom(EndpointDenied("loopback only")))
            }
        })
        .untuple_one();

    let cli_routes = loopback_only
        .and(
            cli_email_group
                .or(cli_window_group)
                .or(cli_os_group)
                .or(cli_agent_review_group)
                .or(cli_workflow_group)
                .or(cli_peer_group),
        )
        .recover(endpoint_recover)
        .boxed();

    // Two route families now: the token-authed OpenAI-compatible /v1 endpoint
    // (for other devices via `orbix connect`) and the localhost /cli bridge
    // (for the Claude Code terminal). The old device-paired /api/* + /ws
    // family for the iOS companion has been removed. The webhook receiver
    // rides alongside, deliberately reachable off-box.
    let routes = v1_routes.or(webhook_route).or(cli_routes).with(cors);

    log::info!("Local endpoint server (/v1 + /cli) starting on port 51231");
    warp::serve(routes).run(([0, 0, 0, 0], 51231)).await;
}

fn json_reply(status: u16, v: &serde_json::Value) -> warp::http::Response<warp::hyper::Body> {
    warp::http::Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(warp::hyper::Body::from(v.to_string()))
        .unwrap_or_else(|_| warp::http::Response::new(warp::hyper::Body::empty()))
}

fn sse_bytes(v: &serde_json::Value) -> warp::hyper::body::Bytes {
    warp::hyper::body::Bytes::from(format!("data: {}\n\n", v))
}

// ─────────────────────────────────────────────
//  OpenAI-compatible local endpoint (FABLE B1)
// ─────────────────────────────────────────────

/// Rejection carrying the auth-failure reason for /v1 and /handoff.
#[derive(Debug)]
pub(crate) struct EndpointDenied(pub &'static str);
impl warp::reject::Reject for EndpointDenied {}

pub(crate) async fn endpoint_recover(err: warp::Rejection) -> Result<impl warp::Reply, warp::Rejection> {
    if let Some(EndpointDenied(reason)) = err.find::<EndpointDenied>() {
        let body = serde_json::json!({
            "error": { "message": format!("Unauthorized: {}", reason), "type": "invalid_request_error" }
        });
        Ok(warp::reply::with_status(
            warp::reply::json(&body),
            warp::http::StatusCode::UNAUTHORIZED,
        ))
    } else {
        Err(err)
    }
}

/// Pass a request through to the local llama-server unchanged. Piping
/// the byte stream handles streaming and non-streaming alike.
async fn proxy_llama(
    method: reqwest::Method,
    path: &str,
    body: Option<warp::hyper::body::Bytes>,
) -> Result<warp::http::Response<warp::hyper::Body>, warp::Rejection> {
    let url = format!("{}{}", crate::local_llm::server_endpoint(), path);
    let client = reqwest::Client::new();
    let mut req = client
        .request(method, &url)
        .header("content-type", "application/json");
    if let Some(b) = body {
        req = req.body(b.to_vec());
    }
    match req.send().await {
        Ok(resp) => {
            let status = warp::http::StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(warp::http::StatusCode::OK);
            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/json")
                .to_string();
            let stream_body = warp::hyper::Body::wrap_stream(resp.bytes_stream());
            Ok(warp::http::Response::builder()
                .status(status)
                .header("content-type", content_type)
                .body(stream_body)
                .unwrap_or_else(|_| warp::http::Response::new(warp::hyper::Body::empty())))
        }
        Err(_) => {
            let msg = serde_json::json!({
                "error": {
                    "message": "Local model server is not running. Open Orbix Core and select a model in Settings > Models.",
                    "type": "server_error"
                }
            })
            .to_string();
            Ok(warp::http::Response::builder()
                .status(warp::http::StatusCode::SERVICE_UNAVAILABLE)
                .header("content-type", "application/json")
                .body(warp::hyper::Body::from(msg))
                .unwrap_or_else(|_| warp::http::Response::new(warp::hyper::Body::empty())))
        }
    }
}
