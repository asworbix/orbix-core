//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Data Control (Confidential Folders)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn list_confidential_folders(state: State<'_, AppState>) -> Vec<data_control::ConfidentialFolder> {
    state.connect.data_control.lock().unwrap_or_else(|e| e.into_inner()).folders.clone()
}

#[tauri::command]
pub(crate) fn add_confidential_folder(name: String, path: String, allowed_agents: Vec<String>, access_level: String, state: State<'_, AppState>) -> data_control::ConfidentialFolder {
    let level = if access_level == "read_write" { data_control::AccessLevel::ReadWrite } else { data_control::AccessLevel::ReadOnly };
    state.connect.data_control.lock().unwrap_or_else(|e| e.into_inner()).add_folder(name, path, allowed_agents, level)
}

#[tauri::command]
pub(crate) fn update_confidential_folder(id: String, name: Option<String>, allowed_agents: Option<Vec<String>>, access_level: Option<String>, state: State<'_, AppState>) -> Option<data_control::ConfidentialFolder> {
    let level = access_level.map(|l| if l == "read_write" { data_control::AccessLevel::ReadWrite } else { data_control::AccessLevel::ReadOnly });
    state.connect.data_control.lock().unwrap_or_else(|e| e.into_inner()).update_folder(&id, name, allowed_agents, level)
}

#[tauri::command]
pub(crate) fn remove_confidential_folder(id: String, state: State<'_, AppState>) -> bool {
    state.connect.data_control.lock().unwrap_or_else(|e| e.into_inner()).remove_folder(&id)
}

#[tauri::command]
pub(crate) fn list_folder_contents(folder_id: String, state: State<'_, AppState>) -> Result<data_control::FolderContents, String> {
    state.connect.data_control.lock().unwrap_or_else(|e| e.into_inner()).list_contents(&folder_id)
}

#[tauri::command]
pub(crate) fn get_folder_access_log(folder_id: Option<String>, state: State<'_, AppState>) -> Vec<data_control::FolderAccessLog> {
    let store = state.connect.data_control.lock().unwrap_or_else(|e| e.into_inner());
    match folder_id {
        Some(id) => store.access_log.iter().filter(|l| l.folder_id == id).cloned().collect(),
        None => store.access_log.clone(),
    }
}

#[tauri::command]
pub(crate) async fn pick_folder() -> Result<String, String> {
    
    // Fallback: use AppleScript for folder picker since dialog plugin may not be async-compatible
    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg("POSIX path of (choose folder with prompt \"Select a confidential folder\")")
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Folder selection cancelled".to_string());
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().trim_end_matches('/').to_string();
    if path.is_empty() {
        return Err("No folder selected".to_string());
    }
    Ok(path)
}

