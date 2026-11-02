//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Semantic Twin + Time Travel Commands
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn get_twin_graph(state: State<'_, AppState>) -> semantic_twin::SemanticGraph {
    state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner()).graph.clone()
}

#[tauri::command]
pub(crate) fn get_twin_stats(state: State<'_, AppState>) -> semantic_twin::TwinStats {
    state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner()).stats()
}

#[tauri::command]
pub(crate) fn add_twin_entity(entity: semantic_twin::TwinEntity, state: State<'_, AppState>, app: tauri::AppHandle) -> semantic_twin::TwinEntity {
    let mut store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
    let created = store.add_entity(entity.clone());
    // Record event
    let mut ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
    ledger.append_simple(
        &created.id, &created.name, &format!("{:?}", created.entity_type).to_lowercase(),
        event_ledger::EventType::EntityCreated,
        None,
        serde_json::to_value(&created).ok(),
        "user",
    );
    // Persist + notify
    storage::save_twin_graph(&store.graph).ok();
    storage::save_twin_events(&ledger).ok();
    drop(ledger);
    drop(store);
    crate::event_bus::emit_ui(&app, "data-changed", "twin").ok();
    created
}

#[tauri::command]
pub(crate) fn update_twin_entity(id: String, updates: HashMap<String, serde_json::Value>, state: State<'_, AppState>, app: tauri::AppHandle) -> Result<semantic_twin::TwinEntity, String> {
    let mut store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
    let old_state = store.get_entity(&id).map(|e| format!("{:?}", e.state).to_lowercase());
    match store.update_entity(&id, updates.clone()) {
        Some(updated) => {
            let new_state = format!("{:?}", updated.state).to_lowercase();
            let mut ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
            let event_type = if updates.contains_key("state") {
                event_ledger::EventType::StateChanged
            } else if updates.contains_key("health") {
                event_ledger::EventType::HealthChanged
            } else if updates.contains_key("lat") || updates.contains_key("lng") {
                event_ledger::EventType::PositionUpdated
            } else {
                event_ledger::EventType::EntityUpdated
            };
            ledger.append_simple(
                &updated.id, &updated.name, &format!("{:?}", updated.entity_type).to_lowercase(),
                event_type,
                old_state.map(|s| serde_json::json!(s)),
                Some(serde_json::json!(new_state)),
                "user",
            );
            storage::save_twin_graph(&store.graph).ok();
            storage::save_twin_events(&ledger).ok();
            drop(ledger);
            drop(store);
            crate::event_bus::emit_ui(&app, "data-changed", "twin").ok();
            Ok(updated)
        }
        None => Err("Entity not found".into()),
    }
}

#[tauri::command]
pub(crate) fn remove_twin_entity(id: String, state: State<'_, AppState>, app: tauri::AppHandle) -> bool {
    let mut store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
    let name = store.get_entity(&id).map(|e| e.name.clone()).unwrap_or_default();
    let etype = store.get_entity(&id).map(|e| format!("{:?}", e.entity_type).to_lowercase()).unwrap_or_default();
    let removed = store.remove_entity(&id);
    if removed {
        let mut ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
        ledger.append_simple(&id, &name, &etype, event_ledger::EventType::EntityDeleted, None, None, "user");
        storage::save_twin_graph(&store.graph).ok();
        storage::save_twin_events(&ledger).ok();
        drop(ledger);
        drop(store);
        crate::event_bus::emit_ui(&app, "data-changed", "twin").ok();
    }
    removed
}

#[tauri::command]
pub(crate) fn add_twin_relation(relation: semantic_twin::TwinRelation, state: State<'_, AppState>, app: tauri::AppHandle) -> semantic_twin::TwinRelation {
    let mut store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
    let created = store.add_relation(relation);
    let mut ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
    ledger.append_simple(
        &created.id, &created.label, "relation",
        event_ledger::EventType::RelationCreated, None, None, "user",
    );
    storage::save_twin_graph(&store.graph).ok();
    storage::save_twin_events(&ledger).ok();
    drop(ledger);
    drop(store);
    crate::event_bus::emit_ui(&app, "data-changed", "twin").ok();
    created
}

#[tauri::command]
pub(crate) fn remove_twin_relation(id: String, state: State<'_, AppState>, app: tauri::AppHandle) -> bool {
    let mut store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
    let removed = store.remove_relation(&id);
    if removed {
        let mut ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
        ledger.append_simple(&id, "", "relation", event_ledger::EventType::RelationRemoved, None, None, "user");
        storage::save_twin_graph(&store.graph).ok();
        storage::save_twin_events(&ledger).ok();
        drop(ledger);
        drop(store);
        crate::event_bus::emit_ui(&app, "data-changed", "twin").ok();
    }
    removed
}

#[tauri::command]
pub(crate) fn get_twin_subgraph(entity_id: String, depth: usize, state: State<'_, AppState>) -> semantic_twin::SemanticGraph {
    let store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
    store.get_subgraph(&entity_id, depth.max(1).min(5))
}

// Event Ledger commands

#[tauri::command]
pub(crate) fn get_twin_events(start: Option<String>, end: Option<String>, entity_id: Option<String>, limit: Option<usize>, state: State<'_, AppState>) -> Vec<event_ledger::LedgerEvent> {
    let ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(eid) = entity_id {
        let events = ledger.get_events_for_entity(&eid);
        let lim = limit.unwrap_or(200);
        events.into_iter().rev().take(lim).cloned().collect()
    } else if let (Some(s), Some(e)) = (start, end) {
        ledger.get_events_in_range(&s, &e).into_iter().cloned().collect()
    } else {
        let lim = limit.unwrap_or(200);
        ledger.recent_events(lim).into_iter().cloned().collect()
    }
}

#[tauri::command]
pub(crate) fn get_twin_event_count(state: State<'_, AppState>) -> usize {
    state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner()).event_count()
}

#[tauri::command]
pub(crate) fn get_twin_snapshots(state: State<'_, AppState>) -> Vec<event_ledger::SnapshotInfo> {
    state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner()).snapshot_infos()
}

#[tauri::command]
pub(crate) fn create_twin_snapshot(state: State<'_, AppState>) -> event_ledger::SnapshotInfo {
    let graph = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner()).graph.clone();
    let mut ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
    ledger.create_snapshot(&graph)
}

// Time Travel commands

#[tauri::command]
pub(crate) fn twin_state_at(timestamp: String, state: State<'_, AppState>) -> semantic_twin::SemanticGraph {
    let ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
    let graph = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner()).graph.clone();
    time_travel::state_at(&ledger, &graph, &timestamp)
}

#[tauri::command]
pub(crate) fn twin_diff(t1: String, t2: String, state: State<'_, AppState>) -> time_travel::TwinDiff {
    let ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
    time_travel::diff(&ledger, &t1, &t2)
}

#[tauri::command]
pub(crate) fn twin_replay(t1: String, t2: String, max_frames: Option<usize>, state: State<'_, AppState>) -> Vec<time_travel::ReplayFrame> {
    let ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
    let graph = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner()).graph.clone();
    time_travel::replay_frames(&ledger, &graph, &t1, &t2, max_frames.unwrap_or(50))
}

#[tauri::command]
pub(crate) fn twin_query_at(timestamp: String, entity_type: Option<String>, state: State<'_, AppState>) -> Vec<semantic_twin::TwinEntity> {
    let ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
    let graph = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner()).graph.clone();
    time_travel::query_at(&ledger, &graph, &timestamp, entity_type.as_deref())
}

