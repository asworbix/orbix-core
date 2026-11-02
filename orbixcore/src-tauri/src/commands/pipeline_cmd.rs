//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Inbox & Process Pipeline (Epic W)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn list_pipelines(state: State<'_, AppState>) -> Vec<pipeline::ProcessBoard> {
    let mut store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    store.ensure_seed_pipelines();
    store.pipelines.clone()
}

#[tauri::command]
pub(crate) fn create_pipeline(
    state: State<'_, AppState>,
    name: String,
    stages: Vec<(String, String)>,
) -> pipeline::ProcessBoard {
    let p = pipeline::ProcessBoard::new(name, "custom".into(), stages);
    let mut store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    store.pipelines.push(p.clone());
    store.save();
    p
}

#[tauri::command]
pub(crate) fn delete_pipeline(state: State<'_, AppState>, pipeline_id: String) {
    let mut store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    store.pipelines.retain(|p| p.id != pipeline_id);
    // Remove items from deleted pipeline
    for item in &mut store.items {
        if item.pipeline_id.as_deref() == Some(&pipeline_id) {
            item.pipeline_id = None;
            item.stage_id = None;
        }
    }
    store.save();
}

#[tauri::command]
pub(crate) fn list_inbox_items(
    state: State<'_, AppState>,
    source_filter: Option<pipeline::ItemSource>,
    limit: Option<usize>,
) -> Vec<pipeline::BoardItem> {
    let store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    store.inbox_items(source_filter.as_ref(), limit.unwrap_or(200))
        .into_iter().cloned().collect()
}

#[tauri::command]
pub(crate) fn list_pipeline_items(
    state: State<'_, AppState>,
    pipeline_id: String,
) -> Vec<pipeline::BoardItem> {
    let store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    store.pipeline_items(&pipeline_id).into_iter().cloned().collect()
}

#[tauri::command]
pub(crate) fn create_inbox_item(
    state: State<'_, AppState>,
    title: String,
    snippet: String,
    source: pipeline::ItemSource,
    pipeline_id: Option<String>,
    data: Option<serde_json::Value>,
) -> pipeline::BoardItem {
    let mut item = pipeline::BoardItem::new(title, snippet, source);
    item.data = data;

    if let Some(pid) = &pipeline_id {
        let store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = store.pipelines.iter().find(|p| &p.id == pid) {
            if let Some(first) = p.stages.first() {
                item.pipeline_id = Some(pid.clone());
                item.stage_id = Some(first.id.clone());
                item.entered_stage_at = Some(Utc::now().to_rfc3339());
                item.history.push(pipeline::StageTransition {
                    from_stage: None,
                    to_stage: first.id.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    note: Some("Item created".into()),
                });
            }
        }
    }

    let mut store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    store.items.insert(0, item.clone());
    store.save();
    item
}

#[tauri::command]
pub(crate) fn move_pipeline_item(
    state: State<'_, AppState>,
    item_id: String,
    stage_id: String,
    note: Option<String>,
) -> Result<pipeline::BoardItem, String> {
    let mut store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    let item = store.items.iter_mut().find(|i| i.id == item_id)
        .ok_or("Item not found")?;
    item.move_to_stage(stage_id, note);
    let result = item.clone();
    store.save();
    Ok(result)
}

#[tauri::command]
pub(crate) fn assign_item_to_pipeline(
    state: State<'_, AppState>,
    item_id: String,
    pipeline_id: String,
) -> Result<pipeline::BoardItem, String> {
    let mut store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    let first_stage = store.pipelines.iter()
        .find(|p| p.id == pipeline_id)
        .and_then(|p| p.stages.first())
        .map(|s| s.id.clone())
        .ok_or("Pipeline not found or has no stages")?;

    let item = store.items.iter_mut().find(|i| i.id == item_id)
        .ok_or("Item not found")?;
    item.pipeline_id = Some(pipeline_id);
    item.move_to_stage(first_stage, Some("Assigned to pipeline".into()));
    let result = item.clone();
    store.save();
    Ok(result)
}

#[tauri::command]
pub(crate) fn update_item_status(
    state: State<'_, AppState>,
    item_id: String,
    status: pipeline::ItemStatus,
) -> Result<pipeline::BoardItem, String> {
    let mut store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    let item = store.items.iter_mut().find(|i| i.id == item_id)
        .ok_or("Item not found")?;
    item.status = status;
    item.updated_at = Utc::now().to_rfc3339();
    let result = item.clone();
    store.save();
    Ok(result)
}

#[tauri::command]
pub(crate) fn bulk_update_items(
    state: State<'_, AppState>,
    item_ids: Vec<String>,
    action: String,
    pipeline_id: Option<String>,
) -> Result<usize, String> {
    let mut store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    let mut count = 0;

    // Pre-compute first stage for assign action to avoid borrow conflict
    let first_stage_id = pipeline_id.as_ref().and_then(|pid| {
        store.pipelines.iter()
            .find(|p| &p.id == pid)
            .and_then(|p| p.stages.first())
            .map(|s| s.id.clone())
    });

    for item in store.items.iter_mut() {
        if !item_ids.contains(&item.id) { continue; }
        match action.as_str() {
            "archive" => {
                item.status = pipeline::ItemStatus::Archived;
                item.updated_at = Utc::now().to_rfc3339();
                count += 1;
            }
            "read" => {
                item.status = pipeline::ItemStatus::Read;
                item.updated_at = Utc::now().to_rfc3339();
                count += 1;
            }
            "assign" => {
                if let (Some(ref pid), Some(ref sid)) = (&pipeline_id, &first_stage_id) {
                    item.pipeline_id = Some(pid.clone());
                    item.move_to_stage(sid.clone(), Some("Bulk assigned".into()));
                    count += 1;
                }
            }
            "delete" => {
                item.status = pipeline::ItemStatus::Archived;
                count += 1;
            }
            _ => {}
        }
    }

    if action == "delete" {
        store.items.retain(|i| !item_ids.contains(&i.id) || i.status != pipeline::ItemStatus::Archived);
    }

    store.save();
    Ok(count)
}

#[tauri::command]
pub(crate) fn delete_inbox_item(state: State<'_, AppState>, item_id: String) {
    let mut store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    store.items.retain(|i| i.id != item_id);
    store.save();
}

#[tauri::command]
pub(crate) fn get_stale_items(state: State<'_, AppState>) -> Vec<pipeline::BoardItem> {
    let store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
    store.stale_items().into_iter().cloned().collect()
}

#[tauri::command]
pub(crate) async fn summarize_item(
    state: State<'_, AppState>,
    item_id: String,
) -> Result<String, String> {
    let (title, snippet, data) = {
        let store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
        let item = store.items.iter().find(|i| i.id == item_id)
            .ok_or("Item not found")?;
        (item.title.clone(), item.snippet.clone(), item.data.clone())
    };

    let summary = pipeline::ai_summarize_item(&title, &snippet, data.as_ref()).await?;

    // Store summary on item
    {
        let mut store = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(item) = store.items.iter_mut().find(|i| i.id == item_id) {
            item.ai_summary = Some(summary.clone());
            store.save();
        }
    }

    Ok(summary)
}

