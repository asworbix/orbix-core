//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Tools (Epic E)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_tools(state: State<'_, AppState>) -> Result<Vec<ToolDef>, String> {
    let registry = state.engine.tool_registry.lock().unwrap_or_else(|e| e.into_inner());
    Ok(registry.tools.clone())
}

#[tauri::command]
pub(crate) async fn toggle_tool(tool_id: String, state: State<'_, AppState>) -> Result<ToolDef, String> {
    let mut registry = state.engine.tool_registry.lock().unwrap_or_else(|e| e.into_inner());
    registry.toggle_tool(&tool_id).ok_or_else(|| format!("Tool {} not found", tool_id))
}

#[tauri::command]
pub(crate) async fn set_tool_auto_approve(
    tool_name: String,
    auto_approve: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut registry = state.engine.tool_registry.lock().unwrap_or_else(|e| e.into_inner());
    registry.set_auto_approve(&tool_name, auto_approve);
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_pending_tool_calls(state: State<'_, AppState>) -> Result<Vec<ToolCall>, String> {
    let registry = state.engine.tool_registry.lock().unwrap_or_else(|e| e.into_inner());
    Ok(registry.pending_calls.clone())
}

#[tauri::command]
pub(crate) async fn approve_tool_call(
    call_id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ToolCall, String> {
    let mut call = {
        let mut registry = state.engine.tool_registry.lock().unwrap_or_else(|e| e.into_inner());
        registry.approve_call(&call_id).ok_or_else(|| format!("Call {} not found", call_id))?
    };

    call.status = ToolCallStatus::Running;
    crate::event_bus::emit_ui(&app, "tool-call-update", call.clone()).ok();

    let result = tools::execute_tool(&call.tool_name, &call.arguments, &tools::ToolCtx { state: &state, agent_id: "user" }).await;
    call.status = if result.success {
        ToolCallStatus::Completed
    } else {
        ToolCallStatus::Failed
    };
    call.result = Some(result);
    call.resolved_at = Some(Utc::now().to_rfc3339());

    crate::event_bus::emit_ui(&app, "tool-call-update", call.clone()).ok();

    let result_content = if let Some(ref r) = call.result {
        if r.success {
            format!("[Tool: {}] {}", call.tool_name, r.output)
        } else {
            format!("[Tool: {} failed] {}", call.tool_name, r.error.as_deref().unwrap_or("Unknown error"))
        }
    } else {
        format!("[Tool: {}] No result", call.tool_name)
    };

    let tool_msg = Message {
        id: Uuid::new_v4().to_string(),
        role: "assistant".to_string(),
        content: result_content,
        context: None,
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    };

    {
        let mut msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
        msgs.push(tool_msg.clone());
    }
    crate::event_bus::emit_ui(&app, "new-message", tool_msg).ok();

    Ok(call)
}

#[tauri::command]
pub(crate) async fn deny_tool_call(
    call_id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ToolCall, String> {
    let mut registry = state.engine.tool_registry.lock().unwrap_or_else(|e| e.into_inner());
    let call = registry.deny_call(&call_id).ok_or_else(|| format!("Call {} not found", call_id))?;
    crate::event_bus::emit_ui(&app, "tool-call-update", call.clone()).ok();
    Ok(call)
}

#[tauri::command]
pub(crate) async fn request_tool_call(
    tool_name: String,
    arguments: HashMap<String, serde_json::Value>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ToolCall, String> {
    let call = {
        let mut registry = state.engine.tool_registry.lock().unwrap_or_else(|e| e.into_inner());
        registry.create_tool_call(&tool_name, arguments)?
    };

    if call.status == ToolCallStatus::Pending {
        crate::event_bus::emit_ui(&app, "tool-call-pending", call.clone()).ok();
        Ok(call)
    } else {
        let mut call = call;
        call.status = ToolCallStatus::Running;
        crate::event_bus::emit_ui(&app, "tool-call-update", call.clone()).ok();

        let result = tools::execute_tool(&call.tool_name, &call.arguments, &tools::ToolCtx { state: &state, agent_id: "user" }).await;
        call.status = if result.success {
            ToolCallStatus::Completed
        } else {
            ToolCallStatus::Failed
        };
        call.result = Some(result);
        call.resolved_at = Some(Utc::now().to_rfc3339());

        crate::event_bus::emit_ui(&app, "tool-call-update", call.clone()).ok();
        Ok(call)
    }
}

