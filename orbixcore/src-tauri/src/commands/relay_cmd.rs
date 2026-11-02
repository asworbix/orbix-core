//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Secure Relay (privacy bridge)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_relay_sessions(state: State<'_, AppState>) -> Result<Vec<secure_relay::RelaySession>, String> {
    let store = state.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.sessions.clone())
}

#[tauri::command]
pub(crate) async fn create_relay_session(
    name: String,
    local_agent_id: String,
    cloud_agent_id: String,
    policy: Option<secure_relay::RedactionPolicy>,
    state: State<'_, AppState>,
) -> Result<secure_relay::RelaySession, String> {
    let session = secure_relay::create_session(&name, &local_agent_id, &cloud_agent_id, policy);
    let mut store = state.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner());
    store.sessions.push(session.clone());
    store.save();
    Ok(session)
}

#[tauri::command]
pub(crate) async fn delete_relay_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut store = state.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner());
    store.sessions.retain(|s| s.id != session_id);
    store.save();
    Ok(())
}

#[tauri::command]
pub(crate) async fn update_relay_policy(
    session_id: String,
    policy: secure_relay::RedactionPolicy,
    state: State<'_, AppState>,
) -> Result<secure_relay::RelaySession, String> {
    let mut store = state.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner());
    let session = store.sessions.iter_mut().find(|s| s.id == session_id)
        .ok_or("Session not found")?;
    session.policy = policy;
    session.updated_at = chrono::Utc::now().to_rfc3339();
    let result = session.clone();
    store.save();
    Ok(result)
}

#[tauri::command]
pub(crate) async fn relay_sanitise_text(
    session_id: String,
    text: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let policy = {
        let store = state.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner());
        let session = store.sessions.iter().find(|s| s.id == session_id)
            .ok_or("Session not found")?;
        session.policy.clone()
    };
    let result = secure_relay::sanitise(&text, &policy);

    // Update redaction log in session (store redaction metadata, NOT original content)
    {
        let mut store = state.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = store.sessions.iter_mut().find(|s| s.id == session_id) {
            session.redaction_log.extend(result.redactions.clone());
            session.updated_at = chrono::Utc::now().to_rfc3339();
        }
        store.save();
    }

    Ok(serde_json::json!({
        "sanitised_text": result.sanitised_text,
        "redaction_count": result.redaction_count,
        "redactions": result.redactions,
    }))
}

/// Run the full relay pipeline: local analyse → sanitise → cloud query → local compare
#[tauri::command]
pub(crate) async fn run_relay_session(
    session_id: String,
    document_content: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    

    let (local_agent, cloud_agent, policy) = {
        let store = state.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner());
        let session = store.sessions.iter().find(|s| s.id == session_id)
            .ok_or("Session not found")?;
        let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
        let local = agents.iter().find(|a| a.id == session.local_agent_id)
            .ok_or("Local agent not found")?.clone();
        let cloud = agents.iter().find(|a| a.id == session.cloud_agent_id)
            .ok_or("Cloud agent not found")?.clone();
        (local, cloud, session.policy.clone())
    };

    // Helper to update session step status
    let update_step = |step_idx: usize, status: secure_relay::RelayStepStatus, output: Option<String>, redactions: u32| {
        let mut store = state.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = store.sessions.iter_mut().find(|s| s.id == session_id) {
            if let Some(step) = session.steps.get_mut(step_idx) {
                step.status = status;
                step.output = output;
                step.redaction_count = redactions;
                step.timestamp = chrono::Utc::now().to_rfc3339();
            }
        }
        store.save();
    };

    let update_session_status = |status: secure_relay::RelayStatus| {
        let mut store = state.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = store.sessions.iter_mut().find(|s| s.id == session_id) {
            session.status = status;
            session.updated_at = chrono::Utc::now().to_rfc3339();
        }
        store.save();
    };

    // ── Step 1: Local Analyse ──
    update_session_status(secure_relay::RelayStatus::LocalAnalysing);
    update_step(0, secure_relay::RelayStepStatus::Running, None, 0);
    crate::event_bus::emit_ui(&app, "relay-step-change", serde_json::json!({ "session_id": session_id, "step": 0, "status": "running" })).ok();

    let analyse_prompt = secure_relay::local_analyse_prompt(&document_content);
    let analyse_history = vec![Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: analyse_prompt,
        context: None,
        agent_id: Some(local_agent.id.clone()),
        timestamp: chrono::Utc::now().to_rfc3339(),
        tool_turns: None,
    }];

    let local_summary = call_agent_api(&local_agent, &analyse_history).await
        .map_err(|e| { update_step(0, secure_relay::RelayStepStatus::Failed, Some(e.clone()), 0); e })?;

    update_step(0, secure_relay::RelayStepStatus::Completed, Some(local_summary.clone()), 0);
    crate::event_bus::emit_ui(&app, "relay-step-change", serde_json::json!({ "session_id": session_id, "step": 0, "status": "completed" })).ok();

    // ── Step 2: Sanitise ──
    update_session_status(secure_relay::RelayStatus::Sanitising);
    update_step(1, secure_relay::RelayStepStatus::Running, None, 0);
    crate::event_bus::emit_ui(&app, "relay-step-change", serde_json::json!({ "session_id": session_id, "step": 1, "status": "running" })).ok();

    let sanitise_result = secure_relay::sanitise(&local_summary, &policy);
    let sanitised_query = sanitise_result.sanitised_text.clone();

    // Persist redaction log
    {
        let mut store = state.engine.relay_store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = store.sessions.iter_mut().find(|s| s.id == session_id) {
            session.redaction_log.extend(sanitise_result.redactions);
        }
        store.save();
    }

    update_step(1, secure_relay::RelayStepStatus::Completed, Some(sanitised_query.clone()), sanitise_result.redaction_count);
    crate::event_bus::emit_ui(&app, "relay-step-change", serde_json::json!({ "session_id": session_id, "step": 1, "status": "completed", "redaction_count": sanitise_result.redaction_count })).ok();

    // ── Step 3: Cloud Query ──
    update_session_status(secure_relay::RelayStatus::CloudQuerying);
    update_step(2, secure_relay::RelayStepStatus::Running, None, 0);
    crate::event_bus::emit_ui(&app, "relay-step-change", serde_json::json!({ "session_id": session_id, "step": 2, "status": "running" })).ok();

    let cloud_prompt = secure_relay::cloud_search_prompt(&sanitised_query);
    let cloud_history = vec![Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: cloud_prompt,
        context: None,
        agent_id: Some(cloud_agent.id.clone()),
        timestamp: chrono::Utc::now().to_rfc3339(),
        tool_turns: None,
    }];

    let cloud_results = call_agent_api(&cloud_agent, &cloud_history).await
        .map_err(|e| { update_step(2, secure_relay::RelayStepStatus::Failed, Some(e.clone()), 0); e })?;

    update_step(2, secure_relay::RelayStepStatus::Completed, Some(cloud_results.clone()), 0);
    crate::event_bus::emit_ui(&app, "relay-step-change", serde_json::json!({ "session_id": session_id, "step": 2, "status": "completed" })).ok();

    // ── Step 4: Local Compare ──
    update_session_status(secure_relay::RelayStatus::LocalComparing);
    update_step(3, secure_relay::RelayStepStatus::Running, None, 0);
    crate::event_bus::emit_ui(&app, "relay-step-change", serde_json::json!({ "session_id": session_id, "step": 3, "status": "running" })).ok();

    let compare_prompt = secure_relay::local_compare_prompt(&document_content, &cloud_results);
    let compare_history = vec![Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: compare_prompt,
        context: None,
        agent_id: Some(local_agent.id.clone()),
        timestamp: chrono::Utc::now().to_rfc3339(),
        tool_turns: None,
    }];

    let comparison = call_agent_api(&local_agent, &compare_history).await
        .map_err(|e| { update_step(3, secure_relay::RelayStepStatus::Failed, Some(e.clone()), 0); e })?;

    update_step(3, secure_relay::RelayStepStatus::Completed, Some(comparison.clone()), 0);
    update_session_status(secure_relay::RelayStatus::Completed);
    crate::event_bus::emit_ui(&app, "relay-step-change", serde_json::json!({ "session_id": session_id, "step": 3, "status": "completed" })).ok();

    Ok(serde_json::json!({
        "local_summary": local_summary,
        "sanitised_query": sanitised_query,
        "cloud_results": cloud_results,
        "comparison": comparison,
    }))
}

