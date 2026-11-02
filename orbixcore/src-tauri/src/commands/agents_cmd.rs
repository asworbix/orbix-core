//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Agents
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_agents(state: State<'_, AppState>) -> Result<Vec<Agent>, String> {
    let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    Ok(agents.clone())
}

#[tauri::command]
pub(crate) async fn add_agent(
    input: AgentInput,
    state: State<'_, AppState>,
) -> Result<Agent, String> {
    let agent = Agent { charter: String::new(), granted_tools: Vec::new(), schedule_minutes: None, last_run_at: None,
        id: Uuid::new_v4().to_string(),
        name: input.name,
        provider: input.provider,
        model: input.model,
        endpoint: input.endpoint,
        api_key: input.api_key,
        enabled: true,
        is_default: input.is_default,
        created_at: Utc::now().to_rfc3339(),
        connector_ids: input.connector_ids,
        workflow_ids: input.workflow_ids,
        capabilities: input.capabilities,
    };
    let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    if agent.is_default {
        for a in agents.iter_mut() {
            a.is_default = false;
        }
    }
    agents.push(agent.clone());
    storage::save_agents(&agents).ok();
    drop(agents);
    Ok(agent)
}

#[tauri::command]
pub(crate) async fn remove_agent(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    agents.retain(|a| a.id != id);
    storage::save_agents(&agents).ok();
    Ok(())
}

#[tauri::command]
pub(crate) async fn update_agent(
    id: String,
    input: AgentInput,
    state: State<'_, AppState>,
) -> Result<Agent, String> {
    let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    if input.is_default {
        for a in agents.iter_mut() {
            a.is_default = false;
        }
    }
    let agent = agents.iter_mut().find(|a| a.id == id)
        .ok_or_else(|| format!("Agent {} not found", id))?;
    agent.name = input.name;
    agent.provider = input.provider;
    agent.model = input.model;
    agent.endpoint = input.endpoint;
    if !input.api_key.is_empty() {
        agent.api_key = input.api_key;
    }
    agent.is_default = input.is_default;
    agent.connector_ids = input.connector_ids;
    agent.workflow_ids = input.workflow_ids;
    agent.capabilities = input.capabilities;
    let result = agent.clone();
    storage::save_agents(&agents).ok();
    Ok(result)
}

#[tauri::command]
pub(crate) async fn toggle_agent(id: String, state: State<'_, AppState>) -> Result<Agent, String> {
    let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(agent) = agents.iter_mut().find(|a| a.id == id) {
        agent.enabled = !agent.enabled;
        let result = agent.clone();
        storage::save_agents(&agents).ok();
        Ok(result)
    } else {
        Err(format!("Agent {} not found", id))
    }
}

#[tauri::command]
pub(crate) async fn set_default_agent(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    for agent in agents.iter_mut() {
        agent.is_default = agent.id == id;
    }
    storage::save_agents(&agents).ok();
    Ok(())
}

// ─────────────────────────────────────────────
//  Commands - Agent Routing (Epic 4)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn update_agent_capabilities(
    agent_id: String,
    capabilities: agent_router::AgentCapabilities,
    state: State<'_, AppState>,
) -> Result<Agent, String> {
    let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    let agent = agents.iter_mut().find(|a| a.id == agent_id)
        .ok_or("Agent not found")?;
    agent.capabilities = capabilities;
    let result = agent.clone();
    storage::save_agents(&agents).ok();
    Ok(result)
}

#[tauri::command]
pub(crate) async fn get_route_suggestion(
    message: String,
    state: State<'_, AppState>,
) -> Result<Option<agent_router::RouteMatch>, String> {
    let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    let routable: Vec<agent_router::RoutableAgent> = agents.iter().map(|a| {
        agent_router::RoutableAgent {
            id: &a.id,
            name: &a.name,
            enabled: a.enabled,
            is_default: a.is_default,
            capabilities: &a.capabilities,
            connector_ids: &a.connector_ids,
        }
    }).collect();
    Ok(agent_router::route_message(&routable, &message))
}

#[tauri::command]
pub(crate) async fn list_capability_presets() -> Result<Vec<(String, agent_router::AgentCapabilities)>, String> {
    let roles = ["orchestrator", "crm", "finance", "erp", "ppm", "support"];
    Ok(roles.iter().map(|r| (r.to_string(), agent_router::preset_capabilities(r))).collect())
}



/// P2: configure an agent's employee fields (charter, tool grants,
/// wake schedule). Separate from update_agent so plain model-connection
/// edits and employee config don't fight over one input shape.
#[tauri::command]
pub(crate) async fn update_agent_employee(
    id: String,
    charter: String,
    granted_tools: Vec<String>,
    schedule_minutes: Option<u64>,
    state: State<'_, AppState>,
) -> Result<Agent, String> {
    let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    let agent = agents.iter_mut().find(|a| a.id == id)
        .ok_or_else(|| format!("Agent {} not found", id))?;
    agent.charter = charter;
    agent.granted_tools = granted_tools;
    agent.schedule_minutes = schedule_minutes;
    let result = agent.clone();
    storage::save_agents(&agents).ok();
    Ok(result)
}

/// P2: run one shift right now. Returns the shift report.
#[tauri::command]
pub(crate) async fn run_agent_shift(
    id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    crate::agent_shifts::run_shift(&id, &state, &app).await
}

/// P2: the grantable tool catalog (name + description) for the UI.
#[tauri::command]
pub(crate) async fn list_agent_grantable_tools() -> Result<Vec<serde_json::Value>, String> {
    Ok(crate::agent_shifts::grantable_tools())
}

// ─────────────────────────────────────────────
//  Shift monitor - watch, steer, stop a running shift
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_active_shifts() -> Result<Vec<crate::shift_monitor::ShiftSessionWire>, String> {
    Ok(crate::shift_monitor::list_wire())
}

/// Queue a user message into a running shift; the agent sees it before
/// its next turn. Echoed as shift-user-message so every monitor view
/// shows it immediately.
#[tauri::command]
pub(crate) async fn send_to_agent_shift(
    shift_id: String,
    content: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("Message is empty.".into());
    }
    crate::shift_monitor::push_user_message(&shift_id, &content)?;
    crate::event_bus::emit_ui(&app, "shift-user-message", serde_json::json!({
        "shift_id": shift_id,
        "content": content,
    })).ok();
    Ok(())
}

/// Stop a running shift after its current step. Only THIS shift stops;
/// Bob and other shifts are untouched.
#[tauri::command]
pub(crate) async fn stop_agent_shift(shift_id: String, app: tauri::AppHandle) -> Result<(), String> {
    crate::shift_monitor::request_stop(&shift_id)?;
    crate::event_bus::emit_ui(&app, "shift-step", serde_json::json!({
        "shift_id": shift_id,
        "label": "Stop requested…",
    })).ok();
    Ok(())
}

/// Open (or surface) the Run monitor window.
#[tauri::command]
pub(crate) async fn open_run_monitor(app: tauri::AppHandle) -> Result<(), String> {
    crate::shift_monitor::open_window(&app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_monitor_settings() -> Result<crate::shift_monitor::MonitorSettings, String> {
    Ok(crate::shift_monitor::settings())
}

#[tauri::command]
pub(crate) async fn set_monitor_settings(settings: crate::shift_monitor::MonitorSettings) -> Result<(), String> {
    crate::shift_monitor::save_settings(settings)
}

#[tauri::command]
pub(crate) async fn get_always_on_settings() -> Result<crate::always_on::AlwaysOnSettings, String> {
    Ok(crate::always_on::settings())
}

#[tauri::command]
pub(crate) async fn set_always_on_settings(settings: crate::always_on::AlwaysOnSettings) -> Result<crate::always_on::AlwaysOnSettings, String> {
    crate::always_on::save_settings(settings)
}

#[tauri::command]
pub(crate) async fn quick_bar_ask(app: tauri::AppHandle, content: String) -> Result<crate::Message, String> {
    crate::quick_bar::ask(app, content).await
}

#[tauri::command]
pub(crate) async fn quick_bar_hide(app: tauri::AppHandle) -> Result<(), String> {
    crate::quick_bar::hide(&app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn quick_bar_open_main(app: tauri::AppHandle) -> Result<(), String> {
    crate::quick_bar::hide(&app);
    crate::always_on::show_main_window(&app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_quick_bar_status(app: tauri::AppHandle) -> Result<crate::quick_bar::QuickBarStatus, String> {
    Ok(crate::quick_bar::status(&app))
}

#[tauri::command]
pub(crate) async fn set_quick_bar_settings(app: tauri::AppHandle, settings: crate::quick_bar::QuickBarSettings) -> Result<crate::quick_bar::QuickBarStatus, String> {
    Ok(crate::quick_bar::save_settings(&app, settings))
}

/// The last `limit` run-log entries, so a monitor opened mid-run backfills.
#[tauri::command]
pub(crate) async fn run_log_recent(limit: Option<usize>) -> Result<Vec<crate::run_log::RunLogEntry>, String> {
    Ok(crate::run_log::recent(limit.unwrap_or(400).min(2000)))
}
