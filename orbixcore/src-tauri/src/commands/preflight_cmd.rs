//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Pre-Flight Simulation
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn preflight_submit(action: semantic_twin::PreflightAction, state: State<'_, AppState>) -> Result<semantic_twin::PreflightAction, String> {
    let mut store = state.core.preflight_store.lock().map_err(|e| e.to_string())?;
    Ok(store.submit(action))
}

#[tauri::command]
pub(crate) fn preflight_list(state: State<'_, AppState>) -> Vec<semantic_twin::PreflightAction> {
    let store = state.core.preflight_store.lock().unwrap_or_else(|e| e.into_inner());
    store.pending().into_iter().cloned().collect()
}

#[tauri::command]
pub(crate) fn preflight_simulate(state: State<'_, AppState>) -> Result<semantic_twin::SimulationResult, String> {
    let graph = state.core.twin_store.lock().map_err(|e| e.to_string())?.graph.clone();
    let mut store = state.core.preflight_store.lock().map_err(|e| e.to_string())?;
    Ok(store.simulate(&graph))
}

#[tauri::command]
pub(crate) fn preflight_approve(state: State<'_, AppState>) -> Result<String, String> {
    let mut store = state.core.preflight_store.lock().map_err(|e| e.to_string())?;
    store.approve_all();
    Ok("approved".to_string())
}

#[tauri::command]
pub(crate) fn preflight_reject(state: State<'_, AppState>) -> Result<String, String> {
    let mut store = state.core.preflight_store.lock().map_err(|e| e.to_string())?;
    store.reject_all();
    Ok("rejected".to_string())
}

#[tauri::command]
pub(crate) fn preflight_commit(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<Vec<semantic_twin::PreflightAction>, String> {
    let mut preflight = state.core.preflight_store.lock().map_err(|e| e.to_string())?;
    let mut twin = state.core.twin_store.lock().map_err(|e| e.to_string())?;
    let committed = preflight.commit(&mut twin);

    // Log committed actions to event ledger
    if let Ok(mut ledger) = state.core.event_ledger.lock() {
        for action in &committed {
            ledger.append_simple(
                action.target_entity_id.as_deref().unwrap_or(""),
                action.target_entity_name.as_deref().unwrap_or(""),
                "",
                event_ledger::EventType::EntityUpdated,
                None,
                Some(serde_json::json!({ "preflight": action.description })),
                &action.agent_name,
            );
        }
    }

    crate::event_bus::emit_ui(&app, "data-changed", "preflight-committed").ok();
    Ok(committed)
}

