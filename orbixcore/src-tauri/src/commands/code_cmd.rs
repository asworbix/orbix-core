//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Code Assistant commands (Code tab)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn code_scan_project(path: String, state: State<'_, AppState>) -> Result<code_assistant::CodeProject, String> {
    let (files, primary_lang) = code_assistant::scan_directory(&path)?;
    let total_lines: usize = files.iter().map(|f| f.lines).sum();

    let name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());

    let project = code_assistant::CodeProject {
        id: Uuid::new_v4().to_string(),
        name,
        path: path.clone(),
        language: primary_lang,
        files: files.len(),
        lines: total_lines,
        scanned_at: Utc::now().to_rfc3339(),
    };

    let mut store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
    // Replace if same path, else add
    store.projects.retain(|p| p.path != path);
    store.projects.push(project.clone());
    Ok(project)
}

#[tauri::command]
pub(crate) fn code_list_projects(state: State<'_, AppState>) -> Result<Vec<code_assistant::CodeProject>, String> {
    let store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.projects.clone())
}

#[tauri::command]
pub(crate) fn code_list_files(project_id: String, state: State<'_, AppState>) -> Result<Vec<code_assistant::CodeFile>, String> {
    let store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
    let project = store.projects.iter().find(|p| p.id == project_id)
        .ok_or("Project not found")?;
    let (files, _) = code_assistant::scan_directory(&project.path)?;
    Ok(files)
}

#[tauri::command]
pub(crate) fn code_read_file(file_path: String) -> Result<String, String> {
    std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read file: {}", e))
}

#[tauri::command]
pub(crate) async fn code_start_analysis(
    project_path: String,
    analysis_type: String,
    focus_files: Option<Vec<String>>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<code_assistant::AnalysisResult, String> {
    

    let agent_defs = code_assistant::default_agents_for_analysis(&analysis_type);

    // Spawn agents
    let mut spawned_ids = Vec::new();
    {
        let mut store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
        for (name, role) in &agent_defs {
            let agent = code_assistant::CodeAgent {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                role: role.to_string(),
                status: "working".to_string(),
                focus: focus_files.as_ref().and_then(|f| f.first().cloned()),
                findings: Vec::new(),
                created_at: Utc::now().to_rfc3339(),
            };
            spawned_ids.push(agent.id.clone());
            crate::event_bus::emit_ui(&app, "code-agent-update", &agent).ok();
            store.agents.push(agent);
        }
    }

    let result = code_assistant::AnalysisResult {
        id: Uuid::new_v4().to_string(),
        project_path: project_path.clone(),
        analysis_type: analysis_type.clone(),
        agents_spawned: spawned_ids.clone(),
        status: "running".to_string(),
        started_at: Utc::now().to_rfc3339(),
        completed_at: None,
    };

    // Get files to analyze - limit to 3 most important files to avoid overloading LLM
    let files_to_analyze: Vec<(String, String)> = if let Some(ref focus) = focus_files {
        focus.iter()
            .take(3)
            .filter_map(|f| std::fs::read_to_string(f).ok().map(|c| (f.clone(), c)))
            .collect()
    } else {
        // Scan and pick top 3 code files (skip configs, prefer source files)
        let (mut scanned, _) = code_assistant::scan_directory(&project_path)?;
        // Sort by lines descending - most substantial files first
        scanned.sort_by(|a, b| b.lines.cmp(&a.lines));
        scanned.iter()
            .filter(|f| {
                let ext = f.relative_path.rsplit('.').next().unwrap_or("");
                matches!(ext, "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "swift" | "cpp" | "c")
            })
            .take(3)
            .filter_map(|f| std::fs::read_to_string(&f.path).ok().map(|c| (f.relative_path.clone(), c)))
            .collect()
    };

    // Background: run agents SEQUENTIALLY (one at a time) to avoid overloading the LLM
    let app_handle = app.clone();
    let spawned = spawned_ids.clone();
    let agent_defs_owned: Vec<(String, String)> = agent_defs.iter().map(|(n, r)| (n.to_string(), r.to_string())).collect();
    let files_owned = files_to_analyze.clone();

    tokio::spawn(async move {
        let client = reqwest::Client::new();

        // Run agents one at a time - each finishes before the next starts
        for (idx, (agent_name, role)) in agent_defs_owned.iter().enumerate() {
            let agent_id = if idx < spawned.len() { spawned[idx].clone() } else { continue };

            let mut agent_findings = Vec::new();

            // Each agent analyzes one file at a time, sequentially
            for (file_path, content) in &files_owned {
                // Truncate to fit context window
                let max_chars = 1500;
                let truncated = if content.len() > max_chars { &content[..max_chars] } else { content.as_str() };

                // Direct prompt that forces the model to analyze the provided code
                let prompt = format!(
                    "Analyze this code file. Do NOT ask for code - it is provided below.\n\n\
                    File: {}\n\n```\n{}\n```\n\n\
                    Your role: {}\n\n\
                    Return a JSON object with this exact format:\n\
                    {{\"findings\": [{{\"severity\": \"info|suggestion|warning|critical\", \"category\": \"quality|performance|security|architecture|style\", \"title\": \"short title\", \"description\": \"explanation\", \"line_start\": null, \"line_end\": null, \"suggestion\": \"fix\"}}]}}\n\n\
                    Return ONLY valid JSON. No markdown. No explanation outside the JSON.",
                    file_path, truncated, role
                );

                let messages = vec![serde_json::json!({
                    "role": "user",
                    "content": prompt
                })];

                // Use agent server (E4B) for background analysis if available, else main server
                let agent_running = crate::local_llm::is_agent_server_running().await;
                let endpoint = if agent_running {
                    crate::local_llm::agent_server_endpoint()
                } else {
                    crate::local_llm::server_endpoint()
                };
                let url = format!("{}/v1/chat/completions", endpoint);

                let resp = client.post(&url)
                    .header("Content-Type", "application/json")
                    .json(&serde_json::json!({
                        "messages": messages,
                        "temperature": 0.2,
                        "max_tokens": 1024,
                        "stream": false,
                    }))
                    .send()
                    .await;

                if let Ok(resp) = resp {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        let msg = json.get("choices")
                            .and_then(|c| c.get(0))
                            .and_then(|c| c.get("message"));
                        let content_str = msg
                            .and_then(|m| {
                                let c = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
                                if c.is_empty() { m.get("reasoning_content").and_then(|c| c.as_str()) } else { Some(c) }
                            })
                            .unwrap_or("{}");

                        // Try to parse findings from JSON response
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content_str) {
                            if let Some(findings_arr) = parsed.get("findings").and_then(|f| f.as_array()) {
                                for f in findings_arr {
                                    let finding = code_assistant::CodeFinding {
                                        id: Uuid::new_v4().to_string(),
                                        agent_id: agent_id.clone(),
                                        severity: f.get("severity").and_then(|s| s.as_str()).unwrap_or("info").to_string(),
                                        category: f.get("category").and_then(|s| s.as_str()).unwrap_or("quality").to_string(),
                                        title: f.get("title").and_then(|s| s.as_str()).unwrap_or("Finding").to_string(),
                                        description: f.get("description").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                                        file_path: Some(file_path.clone()),
                                        line_start: f.get("line_start").and_then(|n| n.as_u64()).map(|n| n as usize),
                                        line_end: f.get("line_end").and_then(|n| n.as_u64()).map(|n| n as usize),
                                        suggestion: f.get("suggestion").and_then(|s| s.as_str()).map(|s| s.to_string()),
                                        created_at: Utc::now().to_rfc3339(),
                                    };
                                    crate::event_bus::emit_ui(&app_handle, "code-finding", &finding).ok();
                                    agent_findings.push(finding);
                                }
                            }
                        }
                    }
                }
            }

            // Mark agent as done
            let done_agent = code_assistant::CodeAgent {
                id: agent_id.clone(),
                name: agent_name.clone(),
                role: role.clone(),
                status: "done".to_string(),
                focus: None,
                findings: agent_findings.clone(),
                created_at: Utc::now().to_rfc3339(),
            };
            crate::event_bus::emit_ui(&app_handle, "code-agent-update", &done_agent).ok();

            // Store findings
            if let Some(app_state) = app_handle.try_state::<AppState>() {
                let mut store = app_state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
                store.findings.extend(agent_findings);
                if let Some(a) = store.agents.iter_mut().find(|a| a.id == agent_id) {
                    a.status = "done".to_string();
                }
            }

            // Brief pause between agents to let the system breathe
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    });

    Ok(result)
}

#[tauri::command]
pub(crate) fn code_list_agents(state: State<'_, AppState>) -> Result<Vec<code_assistant::CodeAgent>, String> {
    let store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.agents.clone())
}

#[tauri::command]
pub(crate) fn code_list_findings(agent_id: Option<String>, state: State<'_, AppState>) -> Result<Vec<code_assistant::CodeFinding>, String> {
    let store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(id) = agent_id {
        Ok(store.findings.iter().filter(|f| f.agent_id == id).cloned().collect())
    } else {
        Ok(store.findings.clone())
    }
}

#[tauri::command]
pub(crate) async fn code_send_chat(
    content: String,
    context_files: Option<Vec<String>>,
    state: State<'_, AppState>,
    _llama: State<'_, LlamaServerHolder>,
) -> Result<code_assistant::CodeChat, String> {
    let user_msg = code_assistant::CodeChat {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: content.clone(),
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
    };

    // Load context files
    let file_contents: Vec<(String, String)> = context_files.unwrap_or_default().iter()
        .filter_map(|f| std::fs::read_to_string(f).ok().map(|c| (f.clone(), c)))
        .collect();

    let history = {
        let mut store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
        store.chats.push(user_msg.clone());
        store.chats.clone()
    };

    let prompt = code_assistant::build_chat_prompt(&content, &file_contents, &history);

    let client = reqwest::Client::new();
    let messages = vec![
        serde_json::json!({
            "role": "system",
            "content": "Senior code assistant. Concise, technical, actionable. Use code blocks."
        }),
        serde_json::json!({
            "role": "user",
            "content": prompt
        }),
    ];

    let endpoint = local_llm::server_endpoint();
    let url = format!("{}/v1/chat/completions", endpoint);

    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "messages": messages,
            "temperature": 0.5,
            "max_tokens": 2048,
            "stream": false,
        }))
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {}. Make sure a local model is downloaded and selected.", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if body.contains("too big") || body.contains("KV size") {
            return Err("Context too large for current model. Try selecting the model again in Settings to restart the server.".to_string());
        }
        return Err(format!("Local model error ({}): {}", status, if body.len() > 200 { &body[..200] } else { &body }));
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let message = json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"));

    // Some models (Gemma 4) use thinking mode - response may be in reasoning_content
    let reply_content = message
        .and_then(|m| {
            let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
            if content.is_empty() {
                // Fallback to reasoning_content (thinking models)
                m.get("reasoning_content").and_then(|c| c.as_str())
            } else {
                Some(content)
            }
        })
        .unwrap_or("No response from model")
        .to_string();

    let reply = code_assistant::CodeChat {
        id: Uuid::new_v4().to_string(),
        role: "assistant".to_string(),
        content: reply_content,
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
    };

    let mut store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
    store.chats.push(reply.clone());
    Ok(reply)
}

#[tauri::command]
pub(crate) fn code_get_chats(state: State<'_, AppState>) -> Result<Vec<code_assistant::CodeChat>, String> {
    let store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.chats.clone())
}

#[tauri::command]
pub(crate) fn code_clear_chats(state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
    store.chats.clear();
    Ok(())
}

#[tauri::command]
pub(crate) fn code_spawn_agent(
    name: String,
    role: String,
    focus_files: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<code_assistant::CodeAgent, String> {
    let agent = code_assistant::CodeAgent {
        id: Uuid::new_v4().to_string(),
        name,
        role,
        status: "idle".to_string(),
        focus: focus_files.and_then(|f| f.first().cloned()),
        findings: Vec::new(),
        created_at: Utc::now().to_rfc3339(),
    };
    let mut store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
    store.agents.push(agent.clone());
    Ok(agent)
}

#[tauri::command]
pub(crate) fn code_dismiss_agent(agent_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.engine.code_assistant_store.lock().unwrap_or_else(|e| e.into_inner());
    store.agents.retain(|a| a.id != agent_id);
    store.findings.retain(|f| f.agent_id != agent_id);
    Ok(())
}

// ── Dynamic UI panels (Bob-extensible runtime) ─────────────
#[tauri::command]
pub(crate) fn list_dynamic_panels(state: State<'_, AppState>) -> Vec<dynamic_panels::DynamicPanel> {
    let store = state.automation.dynamic_panels_store.lock().unwrap_or_else(|e| e.into_inner());
    store.list()
}

#[tauri::command]
pub(crate) fn get_dynamic_panel(
    id: String,
    state: State<'_, AppState>,
) -> Option<dynamic_panels::DynamicPanel> {
    let store = state.automation.dynamic_panels_store.lock().unwrap_or_else(|e| e.into_inner());
    store.get(&id)
}

#[tauri::command]
pub(crate) fn save_dynamic_panel(
    panel: dynamic_panels::DynamicPanel,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<dynamic_panels::DynamicPanel, String> {
    let saved = {
        let mut store = state.automation.dynamic_panels_store.lock().unwrap_or_else(|e| e.into_inner());
        let saved = store.upsert(panel, "user");
        store.save()?;
        saved
    };
    crate::event_bus::emit_ui(&app, "data-changed", "dynamic-panel-saved").ok();
    Ok(saved)
}

#[tauri::command]
pub(crate) fn update_dynamic_panel(
    id: String,
    patch: dynamic_panels::DynamicPanelPatch,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<dynamic_panels::DynamicPanel, String> {
    let updated = {
        let mut store = state.automation.dynamic_panels_store.lock().unwrap_or_else(|e| e.into_inner());
        let updated = store.patch(&id, patch)?;
        store.save()?;
        updated
    };
    crate::event_bus::emit_ui(&app, "data-changed", "dynamic-panel-updated").ok();
    Ok(updated)
}

#[tauri::command]
pub(crate) fn delete_dynamic_panel(
    id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let removed = {
        let mut store = state.automation.dynamic_panels_store.lock().unwrap_or_else(|e| e.into_inner());
        let removed = store.delete(&id);
        store.save()?;
        removed
    };
    crate::event_bus::emit_ui(&app, "data-changed", "dynamic-panel-deleted").ok();
    Ok(removed)
}

/// Open a file (optionally at a line) in the user's editor (W6). Tries
/// Cursor, then VS Code, then $VISUAL/$EDITOR, then macOS `open`. Uses the
/// line_start a finding already carries so "open in editor" jumps to the
/// exact spot. Returns which editor was used.
#[tauri::command]
pub(crate) async fn code_open_in_editor(path: String, line: Option<usize>) -> Result<String, String> {
    if !std::path::Path::new(&path).exists() {
        return Err(format!("File not found: {}", path));
    }
    let ln = line.unwrap_or(1).max(1);
    // (command, uses -g <path>:<line> flag). code/cursor accept `-g file:line`.
    let candidates: [(&str, bool); 2] = [("cursor", true), ("code", true)];
    for (bin, goto) in candidates {
        // Resolve on PATH without spawning if missing.
        let found = std::process::Command::new("which")
            .arg(bin)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !found {
            continue;
        }
        let target = if goto { format!("{}:{}", path, ln) } else { path.clone() };
        let mut cmd = tokio::process::Command::new(bin);
        if goto {
            cmd.arg("-g");
        }
        cmd.arg(&target);
        return match cmd.spawn() {
            Ok(_) => Ok(bin.to_string()),
            Err(e) => Err(format!("Failed to launch {}: {}", bin, e)),
        };
    }
    // Editor env fallback.
    if let Ok(editor) = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")) {
        if !editor.trim().is_empty() {
            let _ = tokio::process::Command::new(editor.trim()).arg(&path).spawn();
            return Ok(editor);
        }
    }
    // Last resort on macOS: hand off to the default app.
    tokio::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map(|_| "open".to_string())
        .map_err(|e| format!("No editor found and `open` failed: {}", e))
}

/// IDE bridge (W6): apply a surgical edit to a file, then open it in the
/// user's editor at the change. Composes the file_edit primitive with
/// code_open_in_editor so Bob's generated change lands in the editor the
/// user is already in - a thin, editor-agnostic bridge (Cursor, VS Code,
/// $EDITOR, or `open`).
#[tauri::command]
pub(crate) async fn code_apply_and_open(
    path: String,
    old_string: String,
    new_string: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    use std::collections::HashMap;
    // Apply via the same guarded, atomic file_edit tool the agent uses.
    let mut args: HashMap<String, serde_json::Value> = HashMap::new();
    args.insert("path".into(), serde_json::Value::String(path.clone()));
    args.insert("old_string".into(), serde_json::Value::String(old_string));
    args.insert("new_string".into(), serde_json::Value::String(new_string));
    let ctx = tools::ToolCtx { state: &state, agent_id: "user" };
    let result = tools::execute_tool("file_edit", &args, &ctx).await;
    if !result.success {
        return Err(result.error.unwrap_or_else(|| "edit failed".into()));
    }
    // Open the edited file in the editor (best-effort; the edit already stuck).
    let opened = code_open_in_editor(path.clone(), None).await.unwrap_or_else(|_| "none".into());
    Ok(format!("Applied edit to {} and opened in {}", path, opened))
}
