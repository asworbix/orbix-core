//! Brand kit + branded document commands (frontend surface for brand.rs
//! and docgen.rs; Bob reaches the same machinery through his tools).

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

#[tauri::command]
pub(crate) async fn list_brand_kits(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let store = state.engine.brand_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(serde_json::json!({ "kits": store.kits, "active_id": store.active_id }))
}

#[tauri::command]
pub(crate) async fn get_active_brand_kit(state: State<'_, AppState>) -> Result<brand::BrandKit, String> {
    Ok(brand::active_kit(&state))
}

#[tauri::command]
pub(crate) async fn set_active_brand_kit(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.engine.brand_store.lock().unwrap_or_else(|e| e.into_inner());
    if !store.kits.iter().any(|k| k.id == id) {
        return Err(format!("Brand kit {} not found", id));
    }
    store.active_id = Some(id);
    store.save();
    Ok(())
}

#[tauri::command]
pub(crate) async fn update_brand_kit(kit: brand::BrandKit, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.engine.brand_store.lock().unwrap_or_else(|e| e.into_inner());
    store.upsert(kit);
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_brand_kit(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.engine.brand_store.lock().unwrap_or_else(|e| e.into_inner());
    store.kits.retain(|k| k.id != id);
    if store.active_id.as_deref() == Some(id.as_str()) {
        store.active_id = store.kits.first().map(|k| k.id.clone());
    }
    store.save();
    Ok(())
}

/// Extract a kit from a guideline file (same path Bob's tool uses).
#[tauri::command]
pub(crate) async fn extract_brand_kit_from_file(
    path: String,
    state: State<'_, AppState>,
) -> Result<brand::BrandKit, String> {
    brand::extract_kit(&state, &path).await
}

/// Generate a branded document from the UI. Returns the output path.
#[tauri::command]
pub(crate) async fn create_branded_document(
    kind: String,
    spec: docgen::DocSpec,
    open_after: Option<bool>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let kit = brand::active_kit(&state);
    let path = docgen::generate(&kind, &spec, &kit)?;
    if open_after.unwrap_or(true) {
        let _ = open::that(&path);
    }
    Ok(path.display().to_string())
}


/// Base64 logo of the active kit for UI preview (data-URL ready).
#[tauri::command]
pub(crate) async fn get_brand_logo_data(state: State<'_, AppState>) -> Result<Option<serde_json::Value>, String> {
    let kit = brand::active_kit(&state);
    Ok(docgen::load_logo(&kit).map(|l| {
        use base64::Engine as _;
        serde_json::json!({
            "mime": format!("image/{}", l.ext),
            "base64": base64::engine::general_purpose::STANDARD.encode(&l.bytes),
        })
    }))
}
