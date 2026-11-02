//! Workspace profiles: run several Orbix Cores side by side - a work
//! workspace, a private one, a client sandbox - each with its own
//! conversations, agents, workflows, memory and policy. Models, runtime
//! binaries and the login are shared. Switching restarts the app so
//! every store loads from the new profile by construction.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

use crate::storage::{
    load_profile_registry, profile_dir, save_profile_registry, WorkspaceProfile,
    DEFAULT_PROFILE_ID,
};

#[derive(serde::Serialize)]
pub struct ProfilesView {
    pub profiles: Vec<WorkspaceProfile>,
    pub active: String,
}

#[tauri::command]
pub(crate) async fn list_workspace_profiles() -> Result<ProfilesView, String> {
    Ok(ProfilesView {
        profiles: load_profile_registry().profiles,
        active: crate::storage::active_profile_id().to_string(),
    })
}

/// Names that never belong in a new profile directory: shared data and
/// the profile tree itself.
const SHARED_ENTRIES: &[&str] = &[
    "models", "bin", "profiles", "profiles.json",
    "oauth_client_ids.json", "onboarding.json",
];

/// Heavyweight or history-bound entries skipped unless the user asks to
/// bring history along.
const HISTORY_ENTRIES: &[&str] = &[
    "conversations", "audit_log.json", "ledger.jsonl", "runs.json", "recordings",
];

fn copy_recursively(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_recursively(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn create_workspace_profile(
    name: String,
    clone_current: bool,
    include_history: bool,
) -> Result<WorkspaceProfile, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Give the workspace a name.".into());
    }
    let mut reg = load_profile_registry();
    if reg.profiles.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
        return Err(format!("A workspace named '{}' already exists.", name));
    }
    let profile = WorkspaceProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let dir = profile_dir(&profile.id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    if clone_current {
        // Template the CURRENT workspace: copy its profile-scoped entries
        // so the new one starts with the same agents, workflows, memory,
        // brand kits and policy.
        let src_dir = crate::storage::data_dir();
        for entry in std::fs::read_dir(&src_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let file_name = entry.file_name();
            let n = file_name.to_string_lossy().to_string();
            if SHARED_ENTRIES.contains(&n.as_str()) {
                continue;
            }
            if !include_history && HISTORY_ENTRIES.contains(&n.as_str()) {
                continue;
            }
            let src = entry.path();
            let dst = dir.join(&file_name);
            if src.is_dir() {
                copy_recursively(&src, &dst)?;
            } else {
                std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
            }
        }
    }

    reg.profiles.push(profile.clone());
    save_profile_registry(&reg)?;
    Ok(profile)
}

/// Switch and restart. The restart is the design, not a limitation:
/// every store (conversations, agents, workflows, memory, policy, kb)
/// is loaded at startup, so a clean relaunch is the one mechanism that
/// can never leave half the app pointing at the old workspace.
#[tauri::command]
pub(crate) async fn switch_workspace_profile(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let mut reg = load_profile_registry();
    if !reg.profiles.iter().any(|p| p.id == id) {
        return Err("That workspace does not exist.".into());
    }
    if id == crate::storage::active_profile_id() {
        return Ok(());
    }
    reg.active = Some(id);
    save_profile_registry(&reg)?;
    app.restart();
}

#[tauri::command]
pub(crate) async fn rename_workspace_profile(id: String, name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Give the workspace a name.".into());
    }
    let mut reg = load_profile_registry();
    match reg.profiles.iter_mut().find(|p| p.id == id) {
        Some(p) => p.name = name,
        None => return Err("That workspace does not exist.".into()),
    }
    save_profile_registry(&reg)
}

#[tauri::command]
pub(crate) async fn delete_workspace_profile(id: String) -> Result<(), String> {
    if id == DEFAULT_PROFILE_ID {
        return Err("The default workspace cannot be deleted.".into());
    }
    if id == crate::storage::active_profile_id() {
        return Err("Switch to another workspace before deleting this one.".into());
    }
    let mut reg = load_profile_registry();
    let existed = reg.profiles.iter().any(|p| p.id == id);
    if !existed {
        return Err("That workspace does not exist.".into());
    }
    reg.profiles.retain(|p| p.id != id);
    save_profile_registry(&reg)?;
    // The directory is a strict child of base/profiles - never the base.
    let dir = profile_dir(&id);
    if dir.starts_with(crate::storage::base_dir().join("profiles")) {
        std::fs::remove_dir_all(&dir).ok();
    }
    Ok(())
}
