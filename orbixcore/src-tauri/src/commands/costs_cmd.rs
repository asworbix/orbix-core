//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Code Assistant commands (Code tab)
// ─────────────────────────────────────────────


// ─────────────────────────────────────────────
//  Cost Ticker + RBAC + Supervisor commands
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn get_session_cost(state: State<'_, AppState>) -> Result<SessionCost, String> {
    let sc = state.chat.session_cost.lock().unwrap_or_else(|e| e.into_inner());
    Ok(sc.clone())
}

#[tauri::command]
pub(crate) fn reset_session_cost(state: State<'_, AppState>) -> Result<(), String> {
    let mut sc = state.chat.session_cost.lock().unwrap_or_else(|e| e.into_inner());
    *sc = SessionCost::default();
    Ok(())
}

#[tauri::command]
pub(crate) fn get_agent_scopes(agent_id: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    let agent = agents.iter().find(|a| a.id == agent_id).ok_or("Agent not found")?;
    let scopes = agent_scopes(agent);
    Ok(scopes.iter().map(|s| format!("{:?}", s)).collect())
}

#[tauri::command]
pub(crate) fn route_model(task: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    let (model, location, is_local) = route_model_for_task(&task, &agents);
    Ok(serde_json::json!({
        "model": model,
        "location": location,
        "is_local": is_local,
    }))
}

#[tauri::command]
pub(crate) async fn supervisor_decompose(
    goal: String,
    _state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    

    let prompt = format!(
        "You are a Supervisor Agent. Decompose this goal into 2-4 sub-tasks.\n\n\
        Goal: {}\n\n\
        Return JSON ONLY:\n\
        {{\"subtasks\": [\n\
          {{\"role\": \"researcher|writer|analyst|coder|reviewer\",\n\
           \"task\": \"specific sub-task description\",\n\
           \"tools_needed\": [\"shell_exec\"|\"file_read\"|\"file_write\"|\"call_api\"]}}\n\
        ]}}\n\n\
        Return ONLY the JSON. No markdown.",
        goal
    );

    let client = reqwest::Client::new();
    let history = vec![Message {
        id: String::new(), role: "user".to_string(),
        content: prompt, context: None, agent_id: None, timestamp: String::new(), tool_turns: None,
    }];

    let raw = call_local_llm(&client, &history).await?;

    // Parse the decomposition
    let parsed = serde_json::from_str::<serde_json::Value>(&raw)
        .or_else(|_| {
            // Try to extract JSON from markdown
            let start = raw.find('{').unwrap_or(0);
            let end = raw.rfind('}').map(|i| i + 1).unwrap_or(raw.len());
            serde_json::from_str(&raw[start..end])
        })
        .unwrap_or_else(|_| serde_json::json!({"subtasks": [{"role": "analyst", "task": goal, "tools_needed": []}]}));

    crate::event_bus::emit_ui(&app, "supervisor-decomposition", &parsed).ok();
    Ok(parsed)
}

