//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Scheduler Commands (Epic X)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_schedules(state: State<'_, AppState>) -> Result<Vec<scheduler::ScheduleEntry>, String> {
    let store = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list_schedules())
}

#[tauri::command]
pub(crate) async fn create_schedule(
    state: State<'_, AppState>,
    workflow_id: String,
    preset: scheduler::SchedulePreset,
    custom_cron: Option<String>,
    timezone: Option<String>,
) -> Result<scheduler::ScheduleEntry, String> {
    // Verify workflow exists
    {
        let wf_store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        if wf_store.get(&workflow_id).is_none() {
            return Err("Workflow not found".into());
        }
    }
    let mut store = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
    let r = store.create_schedule(workflow_id, preset, custom_cron, timezone);
    store.save();
    r
}

#[tauri::command]
pub(crate) async fn update_schedule(
    state: State<'_, AppState>,
    id: String,
    preset: Option<scheduler::SchedulePreset>,
    custom_cron: Option<String>,
    enabled: Option<bool>,
) -> Result<scheduler::ScheduleEntry, String> {
    let mut store = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
    let r = store.update_schedule(&id, preset, custom_cron, enabled);
    store.save();
    r
}

#[tauri::command]
pub(crate) async fn delete_schedule(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let mut store = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
    let removed = store.delete_schedule(&id);
    store.save();
    Ok(removed)
}

#[tauri::command]
pub(crate) async fn toggle_schedule(state: State<'_, AppState>, id: String) -> Result<scheduler::ScheduleEntry, String> {
    let mut store = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
    let r = store.toggle_schedule(&id);
    store.save();
    r
}

#[tauri::command]
pub(crate) async fn get_schedule_history(
    state: State<'_, AppState>,
    schedule_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<scheduler::ScheduleRunLog>, String> {
    let store = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.get_run_history(schedule_id.as_deref(), limit.unwrap_or(50)))
}

#[tauri::command]
pub(crate) async fn preview_schedule(cron_expr: String, count: Option<usize>) -> Result<Vec<String>, String> {
    scheduler::SchedulerStore::preview_next_runs(&cron_expr, count.unwrap_or(5))
}

