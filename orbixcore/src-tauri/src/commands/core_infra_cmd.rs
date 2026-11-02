//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Core Infrastructure (Architecture v2)
// ─────────────────────────────────────────────

// ── Event Bus ──

#[tauri::command]
pub(crate) async fn core_event_bus_stats(state: State<'_, AppState>) -> Result<event_bus::EventBusStats, String> {
    let bus = state.core.core_event_bus.read().await;
    Ok(bus.stats().await)
}

#[tauri::command]
pub(crate) async fn core_publish_event(
    event_type: String,
    source: String,
    payload: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<event_bus::CoreEvent, String> {
    let event = event_bus::CoreEvent::new(event_type, source, payload);
    let bus = state.core.core_event_bus.read().await;
    bus.publish(event.clone()).await;
    Ok(event)
}

// ── Config ──

#[tauri::command]
pub(crate) fn core_config_get(key: String, state: State<'_, AppState>) -> Result<Option<config::ConfigValue>, String> {
    Ok(state.core.core_config.get(&key))
}

#[tauri::command]
pub(crate) fn core_config_set(key: String, value: String, state: State<'_, AppState>) -> Result<(), String> {
    state.core.core_config.set(&key, config::ConfigValue::String(value));
    Ok(())
}

#[tauri::command]
pub(crate) fn core_config_all(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    Ok(state.core.core_config.to_json())
}

// ── Job Scheduler ──

#[tauri::command]
pub(crate) fn core_submit_job(input: job_scheduler::JobInput, state: State<'_, AppState>) -> Result<job_scheduler::Job, String> {
    let mut sched = state.core.core_job_scheduler.lock().map_err(|e| e.to_string())?;
    sched.submit(input)
}

#[tauri::command]
pub(crate) fn core_get_job(job_id: String, state: State<'_, AppState>) -> Result<Option<job_scheduler::Job>, String> {
    let sched = state.core.core_job_scheduler.lock().map_err(|e| e.to_string())?;
    Ok(sched.get(&job_id).cloned())
}

#[tauri::command]
pub(crate) fn core_cancel_job(job_id: String, state: State<'_, AppState>) -> Result<Option<job_scheduler::Job>, String> {
    let mut sched = state.core.core_job_scheduler.lock().map_err(|e| e.to_string())?;
    Ok(sched.cancel(&job_id))
}

#[tauri::command]
pub(crate) fn core_job_stats(state: State<'_, AppState>) -> Result<job_scheduler::SchedulerStats, String> {
    let sched = state.core.core_job_scheduler.lock().map_err(|e| e.to_string())?;
    Ok(sched.stats().clone())
}

#[tauri::command]
pub(crate) fn core_list_jobs(status: Option<String>, state: State<'_, AppState>) -> Result<Vec<job_scheduler::Job>, String> {
    let sched = state.core.core_job_scheduler.lock().map_err(|e| e.to_string())?;
    let status_filter = status.map(|s| match s.as_str() {
        "pending" => job_scheduler::JobStatus::Pending,
        "running" => job_scheduler::JobStatus::Running,
        "completed" => job_scheduler::JobStatus::Completed,
        "failed" => job_scheduler::JobStatus::Failed,
        "retrying" => job_scheduler::JobStatus::Retrying,
        "dead" => job_scheduler::JobStatus::Dead,
        "cancelled" => job_scheduler::JobStatus::Cancelled,
        _ => job_scheduler::JobStatus::Pending,
    });
    Ok(sched.list(status_filter.as_ref(), 100).into_iter().cloned().collect())
}

#[tauri::command]
pub(crate) fn core_dead_letter_queue(state: State<'_, AppState>) -> Result<Vec<job_scheduler::Job>, String> {
    let sched = state.core.core_job_scheduler.lock().map_err(|e| e.to_string())?;
    Ok(sched.dead_letter_queue().into_iter().cloned().collect())
}

// ── State Machine ──

#[tauri::command]
pub(crate) fn core_sm_entity_types(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let sm = state.core.core_state_machine.lock().map_err(|e| e.to_string())?;
    Ok(sm.entity_types())
}

#[tauri::command]
pub(crate) fn core_sm_get_graph(entity_type: String, state: State<'_, AppState>) -> Result<Option<state_machine::StateGraph>, String> {
    let sm = state.core.core_state_machine.lock().map_err(|e| e.to_string())?;
    Ok(sm.get_graph(&entity_type).cloned())
}

#[tauri::command]
pub(crate) fn core_sm_init_entity(entity_id: String, entity_type: String, state: State<'_, AppState>) -> Result<state_machine::EntityState, String> {
    let mut sm = state.core.core_state_machine.lock().map_err(|e| e.to_string())?;
    sm.init_entity(entity_id, entity_type)
}

#[tauri::command]
pub(crate) fn core_sm_transition(
    entity_id: String,
    to_state: String,
    reason: Option<String>,
    actor: Option<String>,
    state: State<'_, AppState>,
) -> Result<state_machine::EntityState, String> {
    let mut sm = state.core.core_state_machine.lock().map_err(|e| e.to_string())?;
    sm.transition(&entity_id, to_state, reason, actor)
}

#[tauri::command]
pub(crate) fn core_sm_get_state(entity_id: String, state: State<'_, AppState>) -> Result<Option<state_machine::EntityState>, String> {
    let sm = state.core.core_state_machine.lock().map_err(|e| e.to_string())?;
    Ok(sm.get_state(&entity_id).cloned())
}

#[tauri::command]
pub(crate) fn core_sm_next_states(entity_id: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let sm = state.core.core_state_machine.lock().map_err(|e| e.to_string())?;
    sm.next_states(&entity_id)
}

// ── Session Manager ──

#[tauri::command]
pub(crate) fn core_create_session(
    principal: session_manager::Principal,
    state: State<'_, AppState>,
) -> Result<session_manager::Session, String> {
    let mut mgr = state.core.core_session_manager.lock().map_err(|e| e.to_string())?;
    Ok(mgr.create_session(principal, std::collections::HashMap::new()))
}

#[tauri::command]
pub(crate) fn core_validate_session(token: String, state: State<'_, AppState>) -> Result<Option<session_manager::Session>, String> {
    let mut mgr = state.core.core_session_manager.lock().map_err(|e| e.to_string())?;
    Ok(mgr.validate_session(&token))
}

#[tauri::command]
pub(crate) fn core_revoke_session(token: String, state: State<'_, AppState>) -> Result<bool, String> {
    let mut mgr = state.core.core_session_manager.lock().map_err(|e| e.to_string())?;
    Ok(mgr.revoke_session(&token))
}

#[tauri::command]
pub(crate) fn core_session_stats(state: State<'_, AppState>) -> Result<session_manager::SessionStats, String> {
    let mgr = state.core.core_session_manager.lock().map_err(|e| e.to_string())?;
    Ok(mgr.stats())
}

#[tauri::command]
pub(crate) fn core_active_sessions(state: State<'_, AppState>) -> Result<Vec<session_manager::Session>, String> {
    let mgr = state.core.core_session_manager.lock().map_err(|e| e.to_string())?;
    Ok(mgr.active_sessions().into_iter().cloned().collect())
}

// ── Observability ──

#[tauri::command]
pub(crate) fn core_recent_logs(
    limit: Option<usize>,
    level: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<observability::LogEntry>, String> {
    let level_filter = level.map(|l| observability::LogLevel::from_str(&l));
    Ok(state.core.core_observability.recent_logs(limit.unwrap_or(100), level_filter.as_ref()))
}

#[tauri::command]
pub(crate) fn core_metrics(state: State<'_, AppState>) -> Result<Vec<observability::MetricSnapshot>, String> {
    Ok(state.core.core_observability.snapshot_metrics())
}

#[tauri::command]
pub(crate) fn core_prometheus_metrics(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.core.core_observability.prometheus_export())
}

#[tauri::command]
pub(crate) fn core_recent_traces(limit: Option<usize>, state: State<'_, AppState>) -> Result<Vec<observability::TraceContext>, String> {
    Ok(state.core.core_observability.recent_traces(limit.unwrap_or(50)))
}

// ── Gateway / Router ──

#[tauri::command]
pub(crate) fn core_list_routes(state: State<'_, AppState>) -> Result<Vec<gateway::Route>, String> {
    let router = state.core.core_gateway.lock().map_err(|e| e.to_string())?;
    Ok(router.list_routes().into_iter().cloned().collect())
}

#[tauri::command]
pub(crate) fn core_health_check(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let sched = state.core.core_job_scheduler.lock().map_err(|e| e.to_string())?;
    let sess_mgr = state.core.core_session_manager.lock().map_err(|e| e.to_string())?;
    let stats = sched.stats();
    let sess_stats = sess_mgr.stats();

    Ok(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "components": {
            "event_bus": "up",
            "job_scheduler": {
                "status": "up",
                "queue_depth": stats.queue_depth,
                "running_jobs": stats.running_count,
            },
            "session_manager": {
                "status": "up",
                "active_sessions": sess_stats.active,
            },
            "plugin_host": "up",
            "gateway": "up",
        }
    }))
}

