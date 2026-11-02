//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Reload app data from disk
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn reload_app_data(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<String, String> {
    // Reload agents
    let new_agents = storage::load_agents().unwrap_or_default();
    {
        let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
        *agents = new_agents;
    }
    // Reload workflows
    let new_ws = WorkflowStore::load();
    {
        let mut ws = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        ws.workflows = new_ws.workflows;
    }
    // Notify frontend
    crate::event_bus::emit_ui(&app, "data-changed", "reload").ok();
    Ok("reloaded".to_string())
}


// ─────────────────────────────────────────────
//  Commands - Bob Review
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn bob_review_respond(id: String, action: String, edits: Option<String>, state: State<'_, AppState>) -> Result<String, String> {
    let response = serde_json::json!({
        "id": id,
        "action": action,
        "edits": edits,
        "responded_at": chrono::Utc::now().to_rfc3339(),
    });
    let mut responses = state.chat.review_responses.lock().map_err(|e| e.to_string())?;
    responses.insert(id, response);
    Ok("ok".to_string())
}

